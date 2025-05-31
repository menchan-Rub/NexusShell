// This module will manage Inter-Process Communication between the supervisor (parent)
// and the containerized process (child). This is crucial for synchronization,
// error reporting, and passing file descriptors.

use crate::errors::{ContainerError, Result};

// Unix固有のインポートを条件付きに
#[cfg(unix)]
use nix::unistd::{pipe, read, write, close};

/// Represents the file descriptors for a synchronization pipe.
/// Parent writes to `write_fd`, child reads from `read_fd`.
#[derive(Debug)]
pub struct SyncPipe {
    pub read_fd: i32,
    pub write_fd: i32,
}

impl SyncPipe {
    /// Close the read end of the pipe.
    #[cfg(unix)]
    pub fn close_read(&self) -> Result<()> {
        close(self.read_fd).map_err(|e| ContainerError::PipeError(e.to_string()))
    }

    /// Close the write end of the pipe.
    #[cfg(unix)]
    pub fn close_write(&self) -> Result<()> {
        close(self.write_fd).map_err(|e| ContainerError::PipeError(e.to_string()))
    }
    
    /// Windows用のプレースホルダー実装
    #[cfg(not(unix))]
    pub fn close_read(&self) -> Result<()> {
        Err(ContainerError::UnsupportedFeature("Pipe operations not supported on this platform".to_string()))
    }

    #[cfg(not(unix))]
    pub fn close_write(&self) -> Result<()> {
        Err(ContainerError::UnsupportedFeature("Pipe operations not supported on this platform".to_string()))
    }
}

/// Creates a new synchronization pipe.
/// Returns a SyncPipe containing the read and write file descriptors.
#[cfg(unix)]
pub fn create_sync_pipe() -> Result<SyncPipe> {
    let (read_fd, write_fd) = pipe().map_err(|e| ContainerError::PipeError(e.to_string()))?;
    Ok(SyncPipe { read_fd, write_fd })
}

#[cfg(not(unix))]
pub fn create_sync_pipe() -> Result<SyncPipe> {
    Err(ContainerError::UnsupportedFeature("Pipe operations not supported on this platform".to_string()))
}

/// Signals the child process by writing a byte to the pipe.
/// This should be called by the parent process.
#[cfg(unix)]
pub fn signal_child(pipe_write_fd: i32) -> Result<()> {
    match write(pipe_write_fd, &[0u8]) { // Write a single sync byte
        Ok(1) => Ok(()),
        Ok(_) => Err(ContainerError::Ipc("Unexpected number of bytes written to sync pipe".to_string())),
        Err(e) => Err(ContainerError::PipeError(e.to_string())),
    }
}

#[cfg(not(unix))]
pub fn signal_child(_pipe_write_fd: i32) -> Result<()> {
    Err(ContainerError::UnsupportedFeature("Pipe operations not supported on this platform".to_string()))
}

/// Waits for a signal from the parent process by reading a byte from the pipe.
/// This should be called by the child process.
#[cfg(unix)]
pub fn wait_for_parent_signal(pipe_read_fd: i32) -> Result<()> {
    let mut buf = [0u8; 1];
    match read(pipe_read_fd, &mut buf) {
        Ok(0) => Err(ContainerError::Ipc("Parent closed pipe before writing sync byte".to_string())),
        Ok(1) => Ok(()), // Successfully read the sync byte
        Ok(_) => Err(ContainerError::Ipc("Unexpected number of bytes read from sync pipe".to_string())),
        Err(e) => Err(ContainerError::PipeError(e.to_string())),
    }
}

#[cfg(not(unix))]
pub fn wait_for_parent_signal(_pipe_read_fd: i32) -> Result<()> {
    Err(ContainerError::UnsupportedFeature("Pipe operations not supported on this platform".to_string()))
} 