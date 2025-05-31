// src/seccomp.rs 

use thiserror::Error;
use libseccomp::{
    ScmpAction,
    ScmpArch,
    ScmpFilterContext,
    ScmpSyscall,
    ALLOW_SYSCALLS_COUNT_MAX
};

#[derive(Error, Debug)]
pub enum SeccompError {
    #[error("Libseccomp error: {0}")]
    Libseccomp(#[from] libseccomp::error::Error),
    #[error("Invalid syscall name: {0}")]
    InvalidSyscallName(String),
    #[error("Failed to load seccomp filter")]
    LoadFilterFailed,
}

/// Represents a Seccomp profile configuration.
pub struct SeccompProfile {
    default_action: ScmpAction,
    allow_syscalls: Vec<String>, // Names of syscalls to allow
    // Potentially add more specific rules, e.g., syscalls with argument checks
}

impl SeccompProfile {
    /// Creates a new profile that denies all syscalls by default
    /// and allows a predefined set of essential syscalls.
    pub fn default_safe() -> Self {
        SeccompProfile {
            default_action: ScmpAction::Errno(libc::EPERM as u32), // Deny by default
            allow_syscalls: vec![
                // Essential syscalls for most programs
                "accept".to_string(),
                "access".to_string(),
                "arch_prctl".to_string(),
                "bind".to_string(),
                "brk".to_string(),
                "capget".to_string(),
                "capset".to_string(),
                "chdir".to_string(),
                "chmod".to_string(), // Often needed, consider restricting args
                "clock_gettime".to_string(),
                "clone".to_string(), // Needed for creating threads/processes
                "close".to_string(),
                "connect".to_string(),
                "dup".to_string(),
                "dup2".to_string(),
                "dup3".to_string(),
                "epoll_create1".to_string(),
                "epoll_ctl".to_string(),
                "epoll_pwait".to_string(),
                "epoll_wait".to_string(),
                "eventfd2".to_string(),
                "execve".to_string(), // Crucial for running commands
                "exit".to_string(),
                "exit_group".to_string(),
                "faccessat".to_string(),
                "fadvise64".to_string(),
                "fallocate".to_string(),
                "fchdir".to_string(),
                "fchmod".to_string(),
                "fchown".to_string(),
                "fcntl".to_string(),
                "fdatasync".to_string(),
                "flock".to_string(),
                "fork".to_string(), // Consider if needed, often covered by clone
                "fstat".to_string(),
                "fstatfs".to_string(),
                "fsync".to_string(),
                "ftruncate".to_string(),
                "futex".to_string(),
                "getcwd".to_string(),
                "getdents64".to_string(),
                "getegid".to_string(),
                "geteuid".to_string(),
                "getgid".to_string(),
                "getgroups".to_string(),
                "getitimer".to_string(),
                "getpeername".to_string(),
                "getpgid".to_string(),
                "getpgrp".to_string(),
                "getpid".to_string(),
                "getppid".to_string(),
                "getpriority".to_string(),
                "getrandom".to_string(),
                "getresgid".to_string(),
                "getresuid".to_string(),
                "getrlimit".to_string(),
                "getrusage".to_string(),
                "getsid".to_string(),
                "getsockname".to_string(),
                "getsockopt".to_string(),
                "gettid".to_string(),
                "gettimeofday".to_string(),
                "getuid".to_string(),
                "ioctl".to_string(), // Often needed, but can be risky. Consider restricting args.
                "kill".to_string(),  // Needed for signaling
                "lseek".to_string(),
                "lstat".to_string(),
                "madvise".to_string(),
                "mincore".to_string(),
                "mkdirat".to_string(),
                "mmap".to_string(), // Essential for memory management
                "mprotect".to_string(),
                "munmap".to_string(),
                "nanosleep".to_string(),
                "newfstatat".to_string(),
                "openat".to_string(),
                "pipe2".to_string(),
                "poll".to_string(),
                "prctl".to_string(),
                "pread64".to_string(),
                "prlimit64".to_string(),
                "pselect6".to_string(),
                "pwrite64".to_string(),
                "read".to_string(),
                "readlinkat".to_string(),
                "recvfrom".to_string(),
                "recvmmsg".to_string(),
                "recvmsg".to_string(),
                "restart_syscall".to_string(),
                "rt_sigaction".to_string(),
                "rt_sigpending".to_string(),
                "rt_sigprocmask".to_string(),
                "rt_sigqueueinfo".to_string(),
                "rt_sigreturn".to_string(),
                "rt_sigsuspend".to_string(),
                "rt_sigtimedwait".to_string(),
                "rt_tgsigqueueinfo".to_string(),
                "sched_getaffinity".to_string(),
                "sched_getattr".to_string(),
                "sched_getparam".to_string(),
                "sched_getscheduler".to_string(),
                "sched_setaffinity".to_string(),
                "sched_setattr".to_string(),
                "sched_setparam".to_string(),
                "sched_setscheduler".to_string(),
                "sched_yield".to_string(),
                "seccomp".to_string(),
                "select".to_string(),
                "sendmmsg".to_string(),
                "sendmsg".to_string(),
                "sendto".to_string(),
                "set_robust_list".to_string(),
                "set_tid_address".to_string(),
                "setgid".to_string(),
                "setgroups".to_string(),
                "setitimer".to_string(),
                "setpgid".to_string(),
                "setpriority".to_string(),
                "setregid".to_string(),
                "setresgid".to_string(),
                "setresuid".to_string(),
                "setreuid".to_string(),
                "setsid".to_string(),
                "setsockopt".to_string(),
                "setuid".to_string(),
                "sigaltstack".to_string(),
                "socket".to_string(),
                "splice".to_string(),
                "stat".to_string(),
                "statfs".to_string(),
                "sysinfo".to_string(),
                "tgkill".to_string(),
                "time".to_string(),
                "timerfd_create".to_string(),
                "timerfd_gettime".to_string(),
                "timerfd_settime".to_string(),
                "times".to_string(),
                "tkill".to_string(),
                "uname".to_string(),
                "unlinkat".to_string(),
                "wait4".to_string(),
                "waitid".to_string(),
                "write".to_string(),
                "writev".to_string(),
            ],
        }
    }

    /// Applies the seccomp profile to the current process.
    pub fn apply(&self) -> Result<(), SeccompError> {
        // Determine the native architecture for the filter.
        // Using ScmpArch::Native might be too restrictive if the binary could be, e.g., 32-bit on a 64-bit kernel.
        // ScmpArch::X86_64 is a common default for server environments.
        // For broader compatibility, one might need to add rules for multiple architectures
        // (e.g., X86, X32, ARM, AARCH64) or use a more dynamic approach.
        let arch = ScmpArch::X86_64; // Assuming x86-64 for now.
                                      // On other systems, this needs to be ScmpArch::Native or the correct arch.
                                      // Or detect at runtime: ScmpArch::native() (requires libseccomp >= 2.5.0)

        let mut filter = ScmpFilterContext::new_filter(self.default_action, arch)?;
        // Add additional architectures if necessary (e.g., for 32-bit syscalls on 64-bit kernel)
        // filter.add_arch(ScmpArch::X86)?;
        
        // Ensure TSYNC is enabled so the filter applies to all threads of the process.
        filter.set_filter_attr(libseccomp::ScmpFilterAttr::Tsync, 1)?;

        for syscall_name in &self.allow_syscalls {
            let syscall_num = ScmpSyscall::from_name_in_arch(syscall_name, arch)
                .map_err(|_| SeccompError::InvalidSyscallName(syscall_name.clone()))?;
            // Add a rule to allow this specific syscall.
            // For more fine-grained control, rules can have conditions on arguments.
            filter.add_rule(ScmpAction::Allow, syscall_num)?;
        }
        
        // Load the filter into the kernel.
        filter.load().map_err(|_| SeccompError::LoadFilterFailed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::unistd::{fork, ForkResult, getpid, write};
    use nix::sys::wait::waitpid;
    use nix::errno::Errno;

    // Helper function to run a piece of code in a child process with seccomp applied.
    // Returns true if the child exited successfully (status 0),
    // false if it exited with a non-zero status (e.g., killed by seccomp).
    fn run_in_seccomp_child<F>(profile: &SeccompProfile, child_fn: F) -> bool 
    where F: FnOnce() -> () + std::panic::UnwindSafe + Copy {
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child, .. }) => {
                let mut status = 0;
                waitpid(child, Some(&mut status)).unwrap();
                unsafe { libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0 }
            }
            Ok(ForkResult::Child) => {
                // Apply seccomp filter in the child
                if let Err(e) = profile.apply() {
                    eprintln!("Child: Failed to apply seccomp profile: {:?}", e);
                    std::process::exit(125); // Arbitrary non-zero exit code
                }
                // Execute the provided function
                child_fn();
                std::process::exit(0); // Success
            }
            Err(e) => {
                panic!("Fork failed: {:?}", e);
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_default_safe_profile_allows_essentials() {
        let profile = SeccompProfile::default_safe();
        
        // Test a syscall that should be allowed (e.g., getpid)
        let getpid_allowed = run_in_seccomp_child(&profile, || {
            let _pid = getpid(); // This should be allowed
        });
        assert!(getpid_allowed, "getpid should be allowed by default_safe profile");

        // Test a syscall that should be allowed (e.g., write to stdout)
        let write_allowed = run_in_seccomp_child(&profile, || {
            let _ = write(libc::STDOUT_FILENO, b"hello\n"); // This should be allowed
        });
        assert!(write_allowed, "write to stdout should be allowed by default_safe profile");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_profile_denies_unlisted_syscall() {
        // Create a profile that allows only getpid and exit_group, and denies others (e.g., uname)
        let profile = SeccompProfile {
            default_action: ScmpAction::Errno(libc::EPERM as u32),
            allow_syscalls: vec!["getpid".to_string(), "exit_group".to_string(), "write".to_string()], // write for eprintln in child
        };

        // Test uname, which should be denied and cause the child to be killed or error out.
        // The child process should exit with a non-zero status.
        let uname_denied = run_in_seccomp_child(&profile, || {
            let mut uts_name = unsafe { std::mem::zeroed() };
            // This syscall should be blocked by seccomp, leading to an error or signal.
            // The child process will likely be killed by the kernel (SIGSYS).
            // For this test, we rely on the child exiting non-zero due to SIGSYS.
            let ret = unsafe { libc::uname(&mut uts_name) };
            if ret == -1 {
                 // If uname returns -1, it might be due to seccomp (EPERM or another error)
                 // or the syscall itself failed for other reasons. The key is that the process
                 // doesn't terminate normally if seccomp is truly blocking it.
                 // However, the kernel usually sends SIGSYS.
                 std::process::exit(1); // Indicate failure explicitly if not killed
            }
        });
        // If uname was denied, run_in_seccomp_child returns false (child exited non-zero).
        assert!(!uname_denied, "uname syscall should be denied and cause non-zero exit");
    }

     #[test]
    #[cfg(target_os = "linux")]
    fn test_empty_allow_list_denies_all_except_exit() {
        // Profile that denies everything by default, allows nothing explicitly.
        // libseccomp usually ensures exit_group is allowed for filter to load,
        // or one must handle it carefully.
        // For a truly minimal filter, one might need to allow exit/exit_group.
        let profile = SeccompProfile {
            default_action: ScmpAction::KillThread, // Kill the thread for any syscall
            allow_syscalls: vec!["exit_group".to_string()], // MUST allow exit_group for process to terminate cleanly
        };

        let getpid_killed = run_in_seccomp_child(&profile, || {
            // This getpid call should cause the thread/process to be killed.
            let _pid = getpid(); 
            // If we reach here, seccomp didn't work as expected.
            eprintln!("Child: getpid was not blocked by KILL action!");
            std::process::exit(1);
        });
        // If getpid was blocked and killed, run_in_seccomp_child returns false.
        assert!(!getpid_killed, "getpid should be blocked and killed by restrictive profile");
    }
} 