use crate::errors::{ContainerError, Result};
use crate::container::{Container, ContainerState};
use crate::config::ContainerConfig;
use std::collections::HashMap;
use std::path::PathBuf;

/// ランタイム設定
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub root_dir: PathBuf,
    pub state_dir: PathBuf,
    pub runtime_path: PathBuf,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("/var/lib/nexuscontainer"),
            state_dir: PathBuf::from("/run/nexuscontainer"),
            runtime_path: PathBuf::from("/usr/bin/nexus-runtime"),
        }
    }
}

/// コンテナランタイム
pub struct Runtime {
    config: RuntimeConfig,
    containers: HashMap<String, Container>,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            containers: HashMap::new(),
        }
    }

    pub fn create_container(&mut self, container_config: ContainerConfig) -> Result<()> {
        let container_id = container_config.id.clone();
        
        if self.containers.contains_key(&container_id) {
            return Err(ContainerError::Runtime(format!("Container '{}' already exists", container_id)));
        }

        let container = Container::new(container_config)?;
        self.containers.insert(container_id, container);
        
        Ok(())
    }

    pub fn start_container(&mut self, container_id: &str) -> Result<()> {
        let container = self.containers.get_mut(container_id)
            .ok_or_else(|| ContainerError::Runtime(format!("Container '{}' not found", container_id)))?;
        
        container.start()
    }

    pub fn stop_container(&mut self, container_id: &str) -> Result<()> {
        let container = self.containers.get_mut(container_id)
            .ok_or_else(|| ContainerError::Runtime(format!("Container '{}' not found", container_id)))?;
        
        container.stop()
    }

    pub fn remove_container(&mut self, container_id: &str) -> Result<()> {
        if let Some(mut container) = self.containers.remove(container_id) {
            container.remove()
        } else {
            Err(ContainerError::Runtime(format!("Container '{}' not found", container_id)))
        }
    }

    pub fn get_container_state(&self, container_id: &str) -> Result<ContainerState> {
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::Runtime(format!("Container '{}' not found", container_id)))?;
        
        container.get_state()
    }

    pub fn list_containers(&self) -> Vec<&Container> {
        self.containers.values().collect()
    }
} 