use crate::errors::{ContainerError, Result};

/// Windows固有のコンテナ操作
pub struct WindowsContainer;

impl Default for WindowsContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsContainer {
    pub fn new() -> Self {
        Self
    }

    pub fn create_container(&self, _config: &crate::config::ContainerConfig) -> Result<()> {
        Err(ContainerError::UnsupportedFeature(
            "Windows containers not yet implemented".to_string()
        ))
    }

    pub fn start_container(&self, _container_id: &str) -> Result<()> {
        Err(ContainerError::UnsupportedFeature(
            "Windows containers not yet implemented".to_string()
        ))
    }

    pub fn stop_container(&self, _container_id: &str) -> Result<()> {
        Err(ContainerError::UnsupportedFeature(
            "Windows containers not yet implemented".to_string()
        ))
    }
} 