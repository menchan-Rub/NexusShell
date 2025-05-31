// src/pivot_root.rs 

use thiserror::Error;
use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::unistd::{chown, Gid, Uid, chdir, mkdir};
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Error, Debug)]
pub enum PivotRootError {
    #[error("Nix error: {0}")]
    Nix(#[from] nix::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to create directory {path:?}: {source}")]
    CreateDirError {
        path: PathBuf,
        source: nix::Error,
    },
    #[error("Old root mount point {path:?} does not exist or is not a directory after pivot_root")]
    OldRootNotDir { path: PathBuf },
    #[error("Path conversion error for {path:?}")]
    PathConversionError { path: PathBuf },
}

/// Performs a pivot_root operation to change the root filesystem of the current process.
/// 
/// Args:
///   new_root: Path to the new root filesystem.
///   old_root_put_inside: Path within new_root where the old root filesystem will be mounted.
/// 
/// This function implements the standard pivot_root procedure:
/// 1. Ensure `new_root` is a mount point. Often, this means bind-mounting `new_root` onto itself.
/// 2. Create `old_root_put_inside` directory if it doesn't exist.
/// 3. Call `pivot_root(new_root, old_root_put_inside)`.
/// 4. Change current directory to "/".
/// 5. Unmount the old root from `old_root_put_inside` (relative to the new root).
/// 6. Remove the `old_root_put_inside` directory.
pub fn pivot_root_utils(new_root: &Path, old_root_name: &str) -> Result<(), PivotRootError> {
    // 1. Make new_root a mount point if it's not already one.
    // This is typically done by bind mounting new_root onto itself.
    // MS_BIND makes it a bind mount.
    // MS_REC makes it recursive if new_root itself has sub-mounts (usually not the case for a simple chroot).
    // We also make it private to prevent mount/unmount events from propagating.
    mount(
        Some(new_root),
        new_root,
        None, // Fstype: not needed for bind mount
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None, // Data: not needed for bind mount
    )?;
    mount(
        None, // Source: not needed when changing propagation type
        new_root,
        None, // Fstype: not needed
        MsFlags::MS_PRIVATE | MsFlags::MS_REC, // Make it a private mount
        None, // Data: not needed
    )?;

    let old_root_put_inside = new_root.join(old_root_name);

    // 2. Create the directory where the old root will be put.
    // It must exist before pivot_root.
    mkdir(&old_root_put_inside, nix::sys::stat::Mode::S_IRWXU)?;

    // 3. Perform pivot_root.
    // The first argument is new_root, the second is the directory *inside* new_root
    // where the old root filesystem will be mounted.
    nix::unistd::pivot_root(new_root, &old_root_put_inside)?;

    // 4. Change current directory to the new root ("/").
    chdir("/")?;

    // 5. Unmount the old root.
    // The path to unmount is now relative to the new root.
    let old_root_relative_path = PathBuf::from("/").join(old_root_name);
    umount2(&old_root_relative_path, MntFlags::MNT_DETACH)?;

    // 6. Remove the temporary directory used for the old root.
    fs::remove_dir(&old_root_relative_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};
    use nix::sched::{unshare, CloneFlags};
    use nix::unistd::getuid;
    use std::fs::{File, create_dir_all};
    use std::io::Write;
    use crate::user_namespace::{setup_user_namespace, IdMap};

    // Helper function to create a dummy rootfs structure for testing
    fn create_dummy_rootfs(base_path: &Path) -> std::io::Result<()> {
        create_dir_all(base_path.join("bin"))?;
        create_dir_all(base_path.join("old_root_mnt"))?;
        File::create(base_path.join("bin/sh"))?.write_all(b"#!/bin/sh\necho hello")?;
        // Add more essential files/dirs if needed for specific tests
        Ok(())
    }

    // These tests generally require root privileges (CAP_SYS_ADMIN)
    // because they involve mount operations and pivot_root.
    // They also often need a new mount namespace (CLONE_NEWNS)
    // and sometimes a new user namespace (CLONE_NEWUSER) to avoid permission issues.

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore] // Requires root and namespace setup, ignored by default.
    fn test_pivot_root_basic_flow() {
        // This test needs to be run in an isolated environment (new mount and user namespace).
        // It's best executed by a test runner that can set up these namespaces.
        // sudo sysctl -w kernel.unprivileged_userns_clone=1 might be needed for unprivileged user ns.
        
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("This test must be run as root (or in a user namespace mapping to root).");
            return;
        }

        let result = std::panic::catch_unwind(|| {
            // 0. Setup namespaces: new user ns, new mount ns
            // This order is important: user ns first, then others.
            let host_uid = getuid().as_raw();
            let host_gid = nix::unistd::getgid().as_raw();
            let uid_map = [IdMap { container_id: 0, host_id: host_uid, size: 1 }];
            let gid_map = [IdMap { container_id: 0, host_id: host_gid, size: 1 }];

            if unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS).is_err() {
                // Attempt without user ns if unprivileged user ns are not available
                // but we are already root.
                if getuid().is_root() && unshare(CloneFlags::CLONE_NEWNS).is_err() {
                    panic!("Failed to unshare mount namespace even as root");
                }
                // If not root and NEWUSER failed, this test can't proceed meaningfully.
                if !getuid().is_root() {
                    eprintln!("Failed to create new user namespace. Try with sudo or enable unprivileged user namespaces.");
                    return
                }
                // If root, but only NEWNS succeeded, we don't call setup_user_namespace
            } else {
                // Successfully created NEWUSER, proceed to map.
                 if let Err(e) = setup_user_namespace(&uid_map, &gid_map) {
                    panic!("Failed to setup user namespace: {:?}", e);
                 }
            }

            // Make sure root is private before creating new mounts
            // to prevent mounts from propagating to the parent mount namespace.
            mount(None, Path::new("/"), None, MsFlags::MS_REC | MsFlags::MS_PRIVATE, None).unwrap();


            let new_root_dir = tempdir().expect("Failed to create temp dir for new_root");
            let new_root_path = new_root_dir.path();
            create_dummy_rootfs(new_root_path).expect("Failed to create dummy rootfs");
            
            let old_root_mount_name = "oldroot";

            // The actual pivot_root call
            pivot_root_utils(new_root_path, old_root_mount_name).expect("pivot_root_utils failed");

            // After pivot_root, verify we are in the new root
            assert!(Path::new("/bin/sh").exists(), "/bin/sh should exist in new root");
            assert!(!Path::new(&format!("/../{}",old_root_mount_name)).exists(), "Old root mount point should be gone");
            assert!(Path::new("/").is_dir());
            // Check current directory is "/"
            assert_eq!(std::env::current_dir().unwrap(), Path::new("/"));
        });

        if let Err(e) = result {
            panic!("Test panicked: {:?}", e);
        }
    }
} 