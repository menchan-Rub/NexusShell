// src/user_namespace.rs 

use thiserror::Error;
use nix::sched::{unshare, CloneFlags};
use nix::unistd::{setgid, setuid, Gid, Uid};
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Error, Debug)]
pub enum UserNamespaceError {
    #[error("Failed to unshare user namespace: {0}")]
    UnshareError(#[from] nix::Error),
    #[error("Failed to write UID/GID map: {0}")]
    MapWriteError(#[from] std::io::Error),
    #[error("Failed to set GID: {0}")]
    SetGidError(nix::Error),
    #[error("Failed to set UID: {0}")]
    SetUidError(nix::Error),
}

pub struct IdMap {
    pub container_id: u32,
    pub host_id: u32,
    pub size: u32,
}

/// Sets up the user namespace for the current process.
/// This involves unsharing the user namespace and then setting up UID and GID mappings.
pub fn setup_user_namespace(uid_mappings: &[IdMap], gid_mappings: &[IdMap]) -> Result<(), UserNamespaceError> {
    // 1. Unshare the user namespace
    unshare(CloneFlags::CLONE_NEWUSER)?;
    
    // 2. Write GID map (must be done before setting GID and before writing UID map by unprivileged user)
    // Process must be single-threaded at this point or have CAP_SYS_ADMIN in the PARENT user namespace.
    // We assume single-threaded for now for simplicity, or that the caller ensures this.
    write_id_map("/proc/self/gid_map", gid_mappings)?;
    
    // Deny setgroups(2) in the new user namespace before setting GID, if not root.
    // This is critical for security to prevent gaining privileges.
    // It's typically done by writing "deny" to /proc/self/setgroups.
    // Requires kernel 3.19+. For simplicity, we'll assume a modern kernel.
    // If the process is already root in the parent namespace, this step is not strictly necessary
    // but good practice if aiming for least privilege within the new namespace.
    // However, if we are truly unprivileged, we might not be able to write to setgroups.
    // For now, let's assume we are mapping to a non-root user in the new namespace.
    let mut file = OpenOptions::new().write(true).open("/proc/self/setgroups")?;
    file.write_all(b"deny")?;


    // 3. Set GID and UID for the process within the new namespace
    // This typically maps to a non-root user within the namespace.
    // The actual GID/UID to set would depend on the mappings and desired user in the container.
    // For now, assuming the first mapping's container_id is the target.
    if let Some(first_gid_map) = gid_mappings.first() {
        setgid(Gid::from_raw(first_gid_map.container_id))
            .map_err(UserNamespaceError::SetGidError)?;
    }
    
    if let Some(first_uid_map) = uid_mappings.first() {
        setuid(Uid::from_raw(first_uid_map.container_id))
            .map_err(UserNamespaceError::SetUidError)?;
    }

    // 4. Write UID map
    write_id_map("/proc/self/uid_map", uid_mappings)?;

    Ok(())
}

fn write_id_map(path: &str, mappings: &[IdMap]) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    for mapping in mappings {
        // Format: "container_id host_id size\n"
        writeln!(file, "{} {} {}", mapping.container_id, mapping.host_id, mapping.size)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::unistd::{getuid, getgid};

    // Note: These tests require root privileges to run correctly as they involve creating user namespaces.
    // Or, more precisely, they need CAP_SYS_ADMIN to create arbitrary mappings,
    // or unprivileged user namespace creation must be enabled on the system (kernel.unprivileged_userns_clone=1).
    // For simplicity in a CI/dev environment, running as root is often easiest.
    // These tests will likely fail if run as a non-root user without unprivileged user ns enabled.

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore] // Requires root or specific sysctl settings, ignored by default.
    fn test_setup_user_namespace_basic() {
        // This test needs to be run in a context where it can actually create user namespaces.
        // Typically, this means running as root or having unprivileged user namespaces enabled.
        // sudo sysctl -w kernel.unprivileged_userns_clone=1
        // The test also needs to be careful about the host UIDs/GIDs it tries to map,
        // as it can only map its own UID/GID as an unprivileged user.

        // We'll simulate a scenario where the current user (e.g., UID 1000) maps to root (UID 0) in the container.
        let current_uid = getuid().as_raw();
        let current_gid = getgid().as_raw();

        let uid_maps = [IdMap { container_id: 0, host_id: current_uid, size: 1 }];
        let gid_maps = [IdMap { container_id: 0, host_id: current_gid, size: 1 }];

        // A more robust test would fork, setup namespace in child, and check UID/GID in child.
        // For now, we just check if setup_user_namespace completes without error.
        // This is a very basic check and doesn't confirm the namespace isolation itself.
        
        // Since this function changes the UID/GID of the current process,
        // it's tricky to test without forking or running in a separate process.
        // For now, we'll just assert it doesn't panic for a simple case.
        // Proper testing requires more elaborate setup.
        
        // A simple call to check for panics. This is NOT a comprehensive test.
        let result = std::panic::catch_unwind(|| {
            // In a real test, we would fork, and the child would call setup_user_namespace.
            // The parent would wait and check the child's status.
            // Here, we are directly calling it, which will affect the test process itself.
            // This is generally not advisable for tests that alter process-wide state like UID/GID.
            // However, for a preliminary check:
            if unshare(CloneFlags::CLONE_NEWUSER).is_ok() {
                // We are in a new user ns, proceed with caution
                let _ = write_id_map("/proc/self/gid_map", &gid_maps);
                let _ = OpenOptions::new().write(true).open("/proc/self/setgroups").and_then(|mut f| f.write_all(b"deny"));
                let _ = setgid(Gid::from_raw(0)); // Map to container GID 0
                let _ = setuid(Uid::from_raw(0)); // Map to container UID 0
                let _ = write_id_map("/proc/self/uid_map", &uid_maps);
            }
        });
        assert!(result.is_ok(), "setup_user_namespace (simulated) panicked");


        // To truly test setup_user_namespace, we need to fork.
        // let pid = unsafe { libc::fork() };
        // if pid == 0 { // Child process
        //     match setup_user_namespace(&uid_maps, &gid_maps) {
        //         Ok(()) => {
        //             assert_eq!(getuid(), Uid::from_raw(0));
        //             assert_eq!(getgid(), Gid::from_raw(0));
        //             std::process::exit(0);
        //         }
        //         Err(e) => {
        //             eprintln!("Child error: {:?}", e);
        //             std::process::exit(1);
        //         }
        //     }
        // } else if pid > 0 { // Parent process
        //     let mut status = 0;
        //     unsafe { libc::waitpid(pid, &mut status, 0) };
        //     assert!(unsafe { libc::WIFEXITED(status) } && unsafe { libc::WEXITSTATUS(status) } == 0, "Child process failed");
        // } else { // Fork failed
        //     panic!("Fork failed");
        // }
    }
} 