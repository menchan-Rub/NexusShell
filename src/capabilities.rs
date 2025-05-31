// src/capabilities.rs 

use thiserror::Error;
use caps::{CapSet, Capability, CapsHashSet, raise, drop, get_pid_caps};
use nix::unistd::Pid;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    #[error("Caps error: {0}")]
    Caps(#[from] caps::errors::Error),
    #[error("Failed to get capabilities for PID {pid}: {source}")]
    GetCapsError {
        pid: Option<u32>,
        source: caps::errors::Error,
    },
    #[error("Capability {cap} not found in string lookup")]
    UnknownCapabilityName { cap: String },
}

/// Gets all capabilities (effective, permitted, inheritable) for the current process.
pub fn get_current_caps() -> Result<CapsHashSet, CapabilityError> {
    caps::read(None, CapSet::Effective).map_err(|e| CapabilityError::GetCapsError{ pid: None, source: e }) // Example for one set, usually we want all
    // For a more complete representation, one might want to return a struct with all sets.
    // CapsHashSet combines all, which is useful for checking if *any* set has a cap.
    // Let's get all capabilities that are in any set.
    // However, caps::read only reads one set at a time.
    // get_pid_caps(None) gets all for current process
    // Ok(caps::all()) // This gets all *possible* caps, not current process's ones.
    // Let's fetch all three main sets and combine them or return them separately.

    // For simplicity, let's use get_pid_caps(None) which returns a struct with all sets.
    // However, the return type of get_pid_caps is ProcessCaps, not CapsHashSet directly.
    // Let's re-evaluate. The request is for *current* capabilities. caps::all() is not it.
    // A common use case is to check if a capability is present. So a CapsHashSet is good.

    // Let's try to build a CapsHashSet representing all capabilities currently active in any way.
    let mut all_current_caps = CapsHashSet::new();
    let effective = caps::read(None, CapSet::Effective).map_err(|e| CapabilityError::GetCapsError{ pid: None, source: e })?;
    let permitted = caps::read(None, CapSet::Permitted).map_err(|e| CapabilityError::GetCapsError{ pid: None, source: e })?;
    let inheritable = caps::read(None, CapSet::Inheritable).map_err(|e| CapabilityError::GetCapsError{ pid: None, source: e })?;
    
    all_current_caps.extend(effective);
    all_current_caps.extend(permitted);
    all_current_caps.extend(inheritable);
    // Bounding set is not read by caps::read, it's a different mechanism (prctl or specific cap calls)
    // For now, focusing on E, P, I.
    Ok(all_current_caps)
}


/// Drops a list of specified capabilities from all capability sets (Effective, Permitted, Inheritable)
/// for the current process. It also attempts to drop them from the Bounding set.
///
/// Args:
///   caps_to_drop: A slice of Capability to drop.
pub fn drop_capabilities(caps_to_drop: &[Capability]) -> Result<(), CapabilityError> {
    for &cap_to_drop in caps_to_drop {
        // Drop from Effective, Permitted, and Inheritable sets
        drop(None, CapSet::Effective, cap_to_drop)?;
        drop(None, CapSet::Permitted, cap_to_drop)?;
        drop(None, CapSet::Inheritable, cap_to_drop)?;
        
        // Dropping from Bounding set is a bit different. 
        // It needs CAP_SETPCAP or to be done before capabilities are locked.
        // The `caps::drop` function with CapSet::Bounding might not work as expected
        // or might require special conditions. The standard way is prctl(PR_CAPBSET_DROP).
        // The `caps` crate handles this internally if possible when dropping from CapSet::Bounding.
        // However, this can fail if the capability is not in the bounding set or due to permissions.
        // We'll attempt it, but failure here might not be critical if the goal is to reduce E/P/I.
        let _ = caps::drop(None, CapSet::Bounding, cap_to_drop); // Ignore error for bounding set for now
    }
    Ok(())
}

/// Sets the capabilities for the current process to only the ones specified.
/// All other capabilities will be dropped from Effective, Permitted, and Inheritable sets.
/// It will also attempt to restrict the Bounding set to *at least* these capabilities,
/// meaning it won't add to the bounding set, but will try to ensure caps not in `desired_caps`
/// are dropped from it if possible.
///
/// This is a complex operation. A robust implementation often involves:
/// 1. Ensuring the process has CAP_SETPCAP.
/// 2. Raising necessary caps to Permitted (e.g., CAP_SETPCAP itself if needed to change other sets).
/// 3. Setting Inheritable, Permitted, Effective sets.
/// 4. Restricting the Bounding set.
/// 5. Dropping CAP_SETPCAP from all sets if it was only temporarily raised.
///
/// Args:
///   desired_caps: A HashSet of capabilities to be the *only* ones active.
pub fn set_capabilities(desired_caps: &CapsHashSet) -> Result<(), CapabilityError> {
    // 1. Get all known capabilities to find out which ones to drop.
    let mut all_known_caps = CapsHashSet::new();
    for cap in Capability::iter() { // Iterate over all possible capabilities
        all_known_caps.insert(cap);
    }

    let caps_to_drop: Vec<Capability> = all_known_caps.difference(desired_caps).cloned().collect();
    drop_capabilities(&caps_to_drop)?;

    // 2. Raise the desired capabilities to all sets (E, P, I)
    // This assumes that if a capability is in `desired_caps`, it should be in E, P, and I.
    // This might not always be the desired outcome (e.g., some only in P or I).
    // For simplicity, we make them active in all three.
    // The process must have them in its Permitted set (or be able to gain them, e.g. via User NS mapping or CAP_SETPCAP)
    // for `raise` to work for E and P. For I, it just sets the flag.

    for &cap_to_set in desired_caps.iter() {
        // Raise to Permitted first, then Effective. No, raise can do both.
        // raise will add to E, P, and I if it can.
        // This will fail if the capability is not in the current permitted set and cannot be gained.
        // Or if it's not in the bounding set.
        raise(None, CapSet::Effective, cap_to_set)?;
        raise(None, CapSet::Permitted, cap_to_set)?;
        raise(None, CapSet::Inheritable, cap_to_set)?;
    }
    
    // 3. Ensure capabilities *not* in desired_caps are *not* in E, P, I.
    // (This should have been handled by drop_capabilities already, but double-check)
    // Re-checking and explicitly dropping might be needed if `raise` added something unexpectedly
    // (though it shouldn't for caps not in desired_caps).
    let current_effective = caps::read(None, CapSet::Effective)?;
    for cap in current_effective.difference(desired_caps) {
        drop(None, CapSet::Effective, *cap)?;
    }
    let current_permitted = caps::read(None, CapSet::Permitted)?;
    for cap in current_permitted.difference(desired_caps) {
        drop(None, CapSet::Permitted, *cap)?;
    }
    let current_inheritable = caps::read(None, CapSet::Inheritable)?;
    for cap in current_inheritable.difference(desired_caps) {
        drop(None, CapSet::Inheritable, *cap)?;
    }

    // Note: Managing the bounding set to *exactly* match desired_caps is hard.
    // `drop_capabilities` attempts to remove unwanted caps from bounding.
    // Adding to bounding set is not possible after it's restricted.

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use caps::{Capability::*, CapSet, CapsHashSet};
    use nix::unistd::{fork, ForkResult, getuid};
    use nix::sys::wait::waitpid;

    // Helper to run code in a child process. Child exits 0 on success, 1 on error.
    fn run_in_child<F>(child_fn: F) -> bool
    where F: FnOnce() -> Result<(), anyhow::Error> + std::panic::UnwindSafe + Copy
    {
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child, .. }) => {
                let mut status = 0;
                waitpid(child, Some(&mut status)).expect("waitpid failed");
                unsafe { libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0 }
            }
            Ok(ForkResult::Child) => {
                match child_fn() {
                    Ok(_) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("Child error: {:?}", e);
                        std::process::exit(1);
                    }
                }
            }
            Err(_) => panic!("Fork failed"),
        }
    }

    // Most capability tests require root (or CAP_SETPCAP initially).
    // Running these tests usually means `sudo cargo test`.

    #[test]
    #[cfg(target_os = "linux")]
    fn test_get_current_caps_not_empty_as_root() {
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("Skipping test_get_current_caps_not_empty_as_root: requires root privileges.");
            return;
        }
        let caps = get_current_caps().expect("Failed to get current caps");
        // As root, we expect to have many capabilities.
        assert!(!caps.is_empty(), "Current capabilities should not be empty for root.");
        assert!(caps.contains(CAP_SYS_ADMIN), "Root should have CAP_SYS_ADMIN");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_drop_single_capability() {
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("Skipping test_drop_single_capability: requires root privileges.");
            return;
        }

        let child_success = run_in_child(|| {
            // Ensure we have the capability before dropping (e.g. CAP_SYS_TIME)
            // This test assumes the process starts with certain caps (like when run as root).
            // If not root, this test needs more setup (e.g. user ns + map root, then grant cap).
            // For simplicity, assume root for now.
            raise(None, CapSet::Effective, CAP_SYS_TIME).map_err(|e| anyhow::anyhow!(e))?;
            raise(None, CapSet::Permitted, CAP_SYS_TIME).map_err(|e| anyhow::anyhow!(e))?;
            
            assert!(caps::read(None, CapSet::Effective)?.contains(CAP_SYS_TIME), "CAP_SYS_TIME should be effective before drop");

            drop_capabilities(&[CAP_SYS_TIME]).map_err(|e| anyhow::anyhow!(e))?;
            
            let current_eff_caps = caps::read(None, CapSet::Effective).map_err(|e| anyhow::anyhow!(e))?;
            assert!(!current_eff_caps.contains(CAP_SYS_TIME), "CAP_SYS_TIME should not be effective after drop");
            
            let current_perm_caps = caps::read(None, CapSet::Permitted).map_err(|e| anyhow::anyhow!(e))?;
            assert!(!current_perm_caps.contains(CAP_SYS_TIME), "CAP_SYS_TIME should not be permitted after drop");
            Ok(())
        });
        assert!(child_success, "Child process for drop_single_capability failed.");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_set_capabilities_to_minimal() {
         if unsafe { libc::geteuid() } != 0 {
            eprintln!("Skipping test_set_capabilities_to_minimal: requires root privileges.");
            return;
        }

        let child_success = run_in_child(|| {
            let mut desired = CapsHashSet::new();
            desired.insert(CAP_NET_BIND_SERVICE); // A relatively harmless capability to test with
            // For the process to exit cleanly after setting caps, it might need more, like CAP_SYS_ADMIN 
            // to fork/exec or even basic ones depending on what it does. 
            // Or it needs to ensure it calls exit() or similar which doesn't require special caps.
            // For `set_capabilities` to work robustly, especially to raise caps, the process might need CAP_SETPCAP in Permitted.
            // Let's assume for this test we start as root, so we have all caps initially.
            
            // As root, we have CAP_SETPCAP, so we can change all capability sets.
            set_capabilities(&desired).map_err(|e| anyhow::anyhow!(e))?;

            let effective_caps = caps::read(None, CapSet::Effective).map_err(|e| anyhow::anyhow!(e))?;
            let permitted_caps = caps::read(None, CapSet::Permitted).map_err(|e| anyhow::anyhow!(e))?;
            
            assert_eq!(effective_caps.len(), 1, "Effective set should only contain CAP_NET_BIND_SERVICE");
            assert!(effective_caps.contains(CAP_NET_BIND_SERVICE), "Effective set missing CAP_NET_BIND_SERVICE");
            
            assert_eq!(permitted_caps.len(), 1, "Permitted set should only contain CAP_NET_BIND_SERVICE");
            assert!(permitted_caps.contains(CAP_NET_BIND_SERVICE), "Permitted set missing CAP_NET_BIND_SERVICE");
            
            // Check that a dropped capability (e.g. CAP_SYS_ADMIN) is gone
            assert!(!effective_caps.contains(CAP_SYS_ADMIN), "CAP_SYS_ADMIN should be dropped from effective");
            assert!(!permitted_caps.contains(CAP_SYS_ADMIN), "CAP_SYS_ADMIN should be dropped from permitted");
            
            Ok(())
        });
        assert!(child_success, "Child process for test_set_capabilities_to_minimal failed.");
    }
} 