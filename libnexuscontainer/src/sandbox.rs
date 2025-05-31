use crate::errors::{ContainerError, Result};
use crate::config::ContainerConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::ffi::CString;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub hostname: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub capabilities: Vec<String>,
    pub readonly_rootfs: bool,
    pub no_new_privileges: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            hostname: "container".to_string(),
            command: "/bin/sh".to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
            user: None,
            group: None,
            capabilities: Vec::new(),
            readonly_rootfs: false,
            no_new_privileges: true,
        }
    }
}

#[derive(Debug)]
pub struct Sandbox {
    config: SandboxConfig,
    container_config: ContainerConfig,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            container_config: ContainerConfig {
                id: "default".to_string(),
                image: "default".to_string(),
                command: vec!["/bin/sh".to_string()],
                env: vec![],
                working_dir: None,
                user: None,
                hostname: Some("container".to_string()),
                privileged: false,
                read_only: false,
                network_mode: "bridge".to_string(),
                volumes: vec![],
                ports: vec![],
                labels: HashMap::new(),
                annotations: HashMap::new(),
            },
        }
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub fn container_config(&self) -> &ContainerConfig {
        &self.container_config
    }
}

// Helper functions
fn to_cstring(s: &str) -> Result<CString> {
    CString::new(s.as_bytes()).map_err(|e| ContainerError::CStringError{ 
        original: s.to_string(), 
        source: e
    })
}

// Non-Unix stub implementations
#[cfg(not(unix))]
pub fn run_container(_config: &ContainerConfig) -> Result<u32> {
    Err(ContainerError::UnsupportedFeature("Container execution not supported on this platform".to_string()))
}

// Main container execution function
#[cfg(unix)]
pub fn run_container(config: &ContainerConfig) -> Result<nix::unistd::Pid> {
    log::info!("Starting container: {}", config.id);
    
    // 簡易実装
    Ok(nix::unistd::Pid::from_raw(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sandbox_config_creation() {
        let config = SandboxConfig::default();
        assert_eq!(config.hostname, "container");
        assert_eq!(config.command, "/bin/sh");
    }

    #[test]
    fn test_to_cstring() {
        let result = to_cstring("test");
        assert!(result.is_ok());
        
        let result = to_cstring("test\0with\0nulls");
        assert!(result.is_err());
    }
} 