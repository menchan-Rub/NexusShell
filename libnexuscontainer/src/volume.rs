use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// ボリューム情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mount_point: PathBuf,
    pub labels: HashMap<String, String>,
    pub created_at: std::time::SystemTime,
}

/// ボリュームマネージャー
pub struct VolumeManager {
    root_path: PathBuf,
    volumes: HashMap<String, VolumeInfo>,
}

impl VolumeManager {
    pub fn new<P: AsRef<Path>>(root_path: P) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
            volumes: HashMap::new(),
        }
    }

    pub fn create_volume(&mut self, name: &str, driver: &str) -> Result<VolumeInfo> {
        if self.volumes.contains_key(name) {
            return Err(ContainerError::Runtime(format!("Volume '{}' already exists", name)));
        }

        let mount_point = self.root_path.join("volumes").join(name);
        std::fs::create_dir_all(&mount_point)?;

        let volume = VolumeInfo {
            name: name.to_string(),
            driver: driver.to_string(),
            mount_point,
            labels: HashMap::new(),
            created_at: std::time::SystemTime::now(),
        };

        self.volumes.insert(name.to_string(), volume.clone());
        Ok(volume)
    }

    pub fn remove_volume(&mut self, name: &str) -> Result<()> {
        if let Some(volume) = self.volumes.remove(name) {
            if volume.mount_point.exists() {
                std::fs::remove_dir_all(&volume.mount_point)?;
            }
            Ok(())
        } else {
            Err(ContainerError::Runtime(format!("Volume '{}' not found", name)))
        }
    }

    pub fn get_volume(&self, name: &str) -> Option<&VolumeInfo> {
        self.volumes.get(name)
    }

    pub fn list_volumes(&self) -> Vec<&VolumeInfo> {
        self.volumes.values().collect()
    }
} 