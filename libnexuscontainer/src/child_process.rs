// This module handles the specifics of the child process execution,
// including command conversion, argument preparation, environment variable setup,
// and the actual execve call.

use crate::errors::{ContainerError, Result};
use std::ffi::{CString, NulError};

// Unix固有のインポートを条件付きに
#[cfg(unix)]
use nix::unistd::execvpe;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

fn string_to_cstring(s: &str) -> std::result::Result<CString, NulError> {
    CString::new(s.as_bytes())
}

pub fn prepare_command(command: &str, args: &[String]) -> Result<(CString, Vec<CString>)> {
    let c_command = string_to_cstring(command)
        .map_err(|e| ContainerError::CStringError { original: command.to_string(), source: e })?;

    let mut c_args: Vec<CString> = Vec::new();
    c_args.push(c_command.clone()); // argv[0] is the command itself

    for arg in args {
        let c_arg = string_to_cstring(arg)
            .map_err(|e| ContainerError::CStringError { original: arg.to_string(), source: e })?;
        c_args.push(c_arg);
    }
    Ok((c_command, c_args))
}

pub fn prepare_env(custom_envs: Option<&Vec<String>>) -> Result<Vec<CString>> {
    let mut c_envs: Vec<CString> = Vec::new();
    if let Some(envs) = custom_envs {
        for env_var in envs {
            let c_env = string_to_cstring(env_var)
                .map_err(|e| ContainerError::CStringError { original: env_var.to_string(), source: e })?;
            c_envs.push(c_env);
        }
    } else {
        // Inherit environment if custom_envs is None
        for (key, value) in std::env::vars_os() {
            let env_str = format!("{}={}", key.to_string_lossy(), value.to_string_lossy());
            match string_to_cstring(&env_str) {
                Ok(c_env) => c_envs.push(c_env),
                Err(_) => {
                    // Log or handle individual env var conversion error if necessary
                    // eprintln!("Warning: Could not convert env var to CString: {}", env_str);
                }
            }
        }
    }
    Ok(c_envs)
}

/// Executes the specified command with the given arguments and environment variables.
/// This function will replace the current process image.
#[cfg(unix)]
pub fn execute_command(
    command_path: &CString, 
    args: &[CString], 
    env: &[CString]
) -> Result<()> {
    execvpe(command_path, args, env)
        .map_err(|e| ContainerError::ExecError { 
            command: command_path.to_string_lossy().into_owned(), 
            message: e.to_string(),
        })?;
    // execvpe only returns on error. If it returns, it means an error occurred.
    // The error is already mapped above, so we theoretically shouldn't reach here directly.
    // However, to satisfy the function signature that implies it can return Ok:
    Ok(())
}

#[cfg(not(unix))]
pub fn execute_command(
    _command_path: &CString, 
    _args: &[CString], 
    _env: &[CString]
) -> Result<()> {
    Err(ContainerError::UnsupportedFeature("Process execution not supported on this platform".to_string()))
} 