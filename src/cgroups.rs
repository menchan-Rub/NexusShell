// src/cgroups.rs 

use thiserror::Error;
use cgroups_rs::{
    cgroup_builder::CgroupBuilder,
    hierarchies::V2,
    Cgroup,
    MaxValue,
    Resources,
    cpu::CpuController,
    memory::MemoryController,
    pids::PidsController,
    Controller,
};
use cgroups_rs::error::Error as CgroupsError;
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum CgroupError {
    #[error("Cgroups operation failed: {0}")]
    Cgroups(#[from] CgroupsError),
    #[error("Failed to join cgroup: {0}")]
    JoinError(CgroupsError),
    #[error("Invalid cgroup path: {0}")]
    InvalidPath(String),
    #[error("Controller not found: {0}")]
    ControllerNotFound(String),
}

pub struct CgroupManager {
    cgroup: Cgroup,
}

impl CgroupManager {
    /// Creates or loads a cgroup v2.
    /// The path is relative to the root of the cgroupfs v2 hierarchy (e.g., /sys/fs/cgroup).
    /// Example path: "nexus_container/my_container_id"
    pub fn new(path_under_cgroupfs_root: PathBuf) -> Result<Self, CgroupError> {
        let name = path_under_cgroupfs_root.to_str()
            .ok_or_else(|| CgroupError::InvalidPath(format!("Path {:?} is not valid UTF-8", path_under_cgroupfs_root)))?;
        
        // We assume cgroup v2 is mounted at /sys/fs/cgroup
        // The CgroupBuilder takes a path *including* the cgroupfs root.
        let full_cgroup_path = PathBuf::from("/sys/fs/cgroup").join(name);

        // TODO: Consider how to handle existing cgroups. For now, CgroupBuilder might recreate or fail.
        // It might be better to try to load first, then create if not exists.
        // For simplicity, we'll rely on CgroupBuilder's behavior.

        let h = V2::new(full_cgroup_path.clone());

        // Define some basic resources. These can be expanded later.
        let resources = Resources {
            memory: Some(cgroups_rs::memory::MemResources {
                limit_in_bytes: Some(MaxValue::Value(1024 * 1024 * 512)), // 512MB limit
                ..Default::default()
            }),
            cpu: Some(cgroups_rs::cpu::CpuResources {
                quota: Some(MaxValue::Value(50_000)), // 50% of one CPU core (50000 out of 100000 period)
                period: Some(MaxValue::Value(100_000)),
                ..Default::default()
            }),
            pids: Some(cgroups_rs::pids::PidsResources {
                max: Some(MaxValue::Value(100)), // Max 100 processes/threads
                ..Default::default()
            }),
            hugepages: None, // Add if needed
            network: None,   // Add if needed
            blkio: None,     // Add if needed (blkio is v1, for v2 use io controller)
            devices: None,   // Add if needed
            rdma: None,      // Add if needed
            misc: Default::default(),
        };

        let cgroup = CgroupBuilder::new(name, &h)
            .set_resources(resources)
            .build()?; 
            // Note: CgroupBuilder internally creates the cgroup path if it doesn't exist.
            // It also writes to the interface files like memory.max, cpu.max, pids.max etc.

        Ok(CgroupManager { cgroup })
    }

    /// Adds the current process (or a specific PID) to this cgroup.
    pub fn add_task(&self, pid: u64) -> Result<(), CgroupError> {
        self.cgroup.add_task_by_tgid(pid.into()).map_err(CgroupError::JoinError)
    }

    /// Applies resource limits defined in the Resources struct.
    /// Note: `CgroupBuilder::build()` already applies the resources set via `set_resources()`.
    /// This method could be used for dynamic updates if needed, but `cgroups-rs` controllers
    /// are the primary way to do this after creation.
    pub fn apply_resources(&self, resources: &Resources) -> Result<(), CgroupError> {
        self.cgroup.apply(resources)?;
        Ok(())
    }

    /// Deletes the cgroup.
    /// This usually requires that the cgroup has no processes in it and no child cgroups.
    pub fn delete(self) -> Result<(), CgroupError> {
        self.cgroup.delete()?;
        Ok(())
    }

    // Example methods to interact with specific controllers
    pub fn set_memory_limit(&self, limit_bytes: i64) -> Result<(), CgroupError> {
        let mem_controller: &MemoryController = self.cgroup.controller_of()
            .ok_or_else(|| CgroupError::ControllerNotFound("MemoryController".to_string()))?;
        mem_controller.set_limit(limit_bytes)?;
        Ok(())
    }

    pub fn get_memory_usage(&self) -> Result<i64, CgroupError> {
        let mem_controller: &MemoryController = self.cgroup.controller_of()
            .ok_or_else(|| CgroupError::ControllerNotFound("MemoryController".to_string()))?;
        Ok(mem_controller.get_current()?.unwrap_or(0))
    }

     pub fn set_cpu_quota(&self, quota_us: i64, period_us: i64) -> Result<(), CgroupError> {
        let cpu_controller: &CpuController = self.cgroup.controller_of()
            .ok_or_else(|| CgroupError::ControllerNotFound("CpuController".to_string()))?;
        cpu_controller.set_quota(quota_us)?;
        cpu_controller.set_period(period_us)?;
        Ok(())
    }

    pub fn set_pids_max(&self, max_pids: u64) -> Result<(), CgroupError> {
        let pids_controller: &PidsController = self.cgroup.controller_of()
            .ok_or_else(|| CgroupError::ControllerNotFound("PidsController".to_string()))?;
        pids_controller.set_max(MaxValue::Value(max_pids as i64))?; // cgroups-rs uses i64 for MaxValue
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::unistd::{fork, ForkResult, getpid, getuid};
    use nix::sys::wait::waitpid;
    use std::thread;
    use std::time::Duration;
    use crate::user_namespace::{IdMap, setup_user_namespace};
    use nix::sched::{unshare, CloneFlags};

    // Helper to ensure cgroup is cleaned up
    struct TestCgroupGuard(Option<CgroupManager>);
    impl Drop for TestCgroupGuard {
        fn drop(&mut self) {
            if let Some(manager) = self.0.take() {
                let _ = manager.delete(); // Ignore error on cleanup
            }
        }
    }

    // These tests require root privileges or CAP_SYS_ADMIN for cgroup manipulation.
    // They also might require user namespaces to be set up if run by a non-root user
    // who needs to enter a cgroup owned by root (though typically cgroups are created
    // by the process that will manage them).

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore] // Requires root, ignored by default.
    fn test_cgroup_creation_and_deletion() {
         if unsafe { libc::geteuid() } != 0 {
            eprintln!("This test must be run as root.");
            return;
        }

        let cgroup_path = PathBuf::from("test_nexus/test_create_delete");
        let manager = CgroupManager::new(cgroup_path.clone()).expect("Failed to create cgroup");
        // Check if cgroup dir exists
        assert!(Path::new("/sys/fs/cgroup").join(&cgroup_path).exists());
        manager.delete().expect("Failed to delete cgroup");
        assert!(!Path::new("/sys/fs/cgroup").join(&cgroup_path).exists());
    }

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore] // Requires root, ignored by default.
    fn test_add_task_to_cgroup_and_check_resources() {
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("This test must be run as root.");
            return;
        }

        let cgroup_path = PathBuf::from("test_nexus/test_task_resources");
        let manager = CgroupManager::new(cgroup_path.clone()).expect("Failed to create cgroup");
        let _guard = TestCgroupGuard(Some(manager)); // Ensure cleanup

        // Fork to have a child process to add to the cgroup
        match unsafe{ fork() } {
            Ok(ForkResult::Parent { child, .. }) => {
                waitpid(child, None).expect("Waitpid failed");
                // Parent can check cgroup stats if desired, but tasks file is tricky for terminated processes
            }
            Ok(ForkResult::Child) => {
                // Child process
                let current_pid = getpid().as_raw() as u64;
                let cg_manager = CgroupManager::new(cgroup_path).expect("Child: Failed to load cgroup");
                cg_manager.add_task(current_pid).expect("Child: Failed to add self to cgroup");
                
                // Verify pids.current (or other stats) after joining
                // Reading /sys/fs/cgroup/.../cgroup.procs should show current_pid
                let procs_path = PathBuf::from("/sys/fs/cgroup").join(cg_manager.cgroup.path()).join("cgroup.procs");
                let procs_content = std::fs::read_to_string(procs_path).unwrap_or_default();
                assert!(procs_content.lines().any(|line| line.trim() == current_pid.to_string()));

                // Example: try to allocate memory that would exceed a limit (if one was set and enforced strictly)
                // For simplicity, we just check the task is in the cgroup.
                
                // Exit child successfully
                std::process::exit(0);
            }
            Err(_) => panic!("Fork failed"),
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore] // Requires root, ignored by default.
    fn test_set_and_get_memory_limit() {
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("This test must be run as root.");
            return;
        }
        let cgroup_path = PathBuf::from("test_nexus/test_mem_limit");
        let manager = CgroupManager::new(cgroup_path).expect("Failed to create cgroup");
        let _guard = TestCgroupGuard(Some(manager)); // Ensure cleanup

        let test_limit = 100 * 1024 * 1024; // 100MB
        let loaded_manager = CgroupManager::new(_guard.0.as_ref().unwrap().cgroup.path().strip_prefix("/sys/fs/cgroup/").unwrap().to_path_buf()).unwrap();
        loaded_manager.set_memory_limit(test_limit).expect("Failed to set memory limit");
        
        // cgroups-rs might cache, so re-fetch or check interface file directly for test robustness
        let mem_controller: &MemoryController = loaded_manager.cgroup.controller_of().unwrap();
        let actual_limit = mem_controller.get_limit().unwrap();
        assert_eq!(actual_limit, test_limit, "Memory limit was not set correctly.");
    }
} 