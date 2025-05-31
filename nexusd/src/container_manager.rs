use anyhow::{Result, anyhow};
use libnexuscontainer::{Container, ContainerState};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetadata {
    pub id: String,
    pub name: String,
    pub image: String,
    pub created: DateTime<Utc>,
    pub state: String,
    pub labels: HashMap<String, String>,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub log_path: PathBuf,
    pub config: ContainerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ContainerConfig {
    pub env: Vec<String>,
    pub cmd: Vec<String>,
    pub working_dir: Option<String>,
    pub user: Option<String>,
    pub hostname: Option<String>,
    pub entrypoint: Vec<String>,
    pub exposed_ports: Vec<String>,
    pub volumes: HashMap<String, String>,
}


#[derive(Debug)]
pub struct ContainerManager {
    containers: Arc<RwLock<HashMap<String, ContainerMetadata>>>,
    data_root: PathBuf,
}

impl ContainerManager {
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            containers: Arc::new(RwLock::new(HashMap::new())),
            data_root,
        }
    }

    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing container manager");
        tokio::fs::create_dir_all(&self.data_root).await?;
        self.load_existing_containers().await?;
        info!("Container manager initialized successfully");
        Ok(())
    }

    async fn load_existing_containers(&self) -> Result<()> {
        let metadata_dir = self.data_root.join("containers");
        if !metadata_dir.exists() {
            tokio::fs::create_dir_all(&metadata_dir).await?;
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&metadata_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if let Some(ext) = entry.path().extension() {
                if ext == "json" {
                    if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                        if let Ok(metadata) = serde_json::from_str::<ContainerMetadata>(&content) {
                            let mut containers = self.containers.write().await;
                            containers.insert(metadata.id.clone(), metadata);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn save_container_metadata(&self, metadata: &ContainerMetadata) -> Result<()> {
        let metadata_dir = self.data_root.join("containers");
        tokio::fs::create_dir_all(&metadata_dir).await?;
        
        let metadata_file = metadata_dir.join(format!("{}.json", metadata.id));
        let content = serde_json::to_string_pretty(metadata)?;
        tokio::fs::write(metadata_file, content).await?;
        
        Ok(())
    }

    pub async fn create_container(&self, name: &str, image: &str, config: Option<ContainerConfig>) -> Result<String> {
        let container_id = Uuid::new_v4().to_string();
        info!("Creating container {} with name: {} image: {}", container_id, name, image);

        let config = config.unwrap_or_default();
        let metadata = ContainerMetadata {
            id: container_id.clone(),
            name: name.to_string(),
            image: image.to_string(),
            created: Utc::now(),
            state: "created".to_string(),
            labels: HashMap::new(),
            pid: None,
            exit_code: None,
            log_path: self.data_root.join("logs").join(format!("{}.log", container_id)),
            config,
        };

        // メタデータを保存
        self.save_container_metadata(&metadata).await?;

        {
            let mut containers = self.containers.write().await;
            containers.insert(container_id.clone(), metadata);
        }

        info!("Container {} created successfully", container_id);
        Ok(container_id)
    }

    pub async fn start_container(&self, container_id: &str) -> Result<()> {
        info!("Starting container: {}", container_id);

        let mut containers = self.containers.write().await;
        if let Some(metadata) = containers.get_mut(container_id) {
            if metadata.state == "running" {
                return Err(anyhow!("Container {} is already running", container_id));
            }

            // コンテナプロセスを起動（モック実装）
            metadata.state = "running".to_string();
            metadata.pid = Some(std::process::id()); // 現在のプロセスIDをモックとして使用
            
            // メタデータを更新
            self.save_container_metadata(metadata).await?;
            
            info!("Container {} started successfully (PID: {:?})", container_id, metadata.pid);
            Ok(())
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn stop_container(&self, container_id: &str, timeout: u64) -> Result<()> {
        info!("Stopping container: {} (timeout: {}s)", container_id, timeout);

        let mut containers = self.containers.write().await;
        if let Some(metadata) = containers.get_mut(container_id) {
            if metadata.state != "running" {
                return Err(anyhow!("Container {} is not running", container_id));
            }

            metadata.state = "exited".to_string();
            metadata.exit_code = Some(0);
            metadata.pid = None;
            
            // メタデータを更新
            self.save_container_metadata(metadata).await?;
            
            info!("Container {} stopped successfully", container_id);
            Ok(())
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn remove_container(&self, container_id: &str, force: bool, remove_volumes: bool) -> Result<()> {
        info!("Removing container: {} (force: {}, remove_volumes: {})", container_id, force, remove_volumes);

        let mut containers = self.containers.write().await;
        if let Some(metadata) = containers.get(container_id) {
            if metadata.state == "running" && !force {
                return Err(anyhow!("Cannot remove running container {}. Stop the container or use force=true", container_id));
            }

            // メタデータファイルを削除
            let metadata_file = self.data_root.join("containers").join(format!("{}.json", container_id));
            if metadata_file.exists() {
                tokio::fs::remove_file(metadata_file).await?;
            }

            // ログファイルを削除
            if metadata.log_path.exists() {
                tokio::fs::remove_file(&metadata.log_path).await?;
            }

            containers.remove(container_id);
            info!("Container {} removed successfully", container_id);
            Ok(())
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn list_containers(&self, all: bool, _size: bool, _filters: HashMap<String, String>) -> Result<Vec<ContainerMetadata>> {
        let containers = self.containers.read().await;
        let mut result: Vec<ContainerMetadata> = containers.values().cloned().collect();
        
        if !all {
            result.retain(|c| c.state == "running");
        }

        // TODO: フィルター適用
        // TODO: サイズ情報計算

        Ok(result)
    }

    pub async fn inspect_container(&self, container_id: &str, _size: bool) -> Result<ContainerMetadata> {
        let containers = self.containers.read().await;
        if let Some(metadata) = containers.get(container_id) {
            Ok(metadata.clone())
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn pause_container(&self, container_id: &str) -> Result<()> {
        info!("Pausing container: {}", container_id);

        let mut containers = self.containers.write().await;
        if let Some(metadata) = containers.get_mut(container_id) {
            if metadata.state != "running" {
                return Err(anyhow!("Container {} is not running", container_id));
            }

            metadata.state = "paused".to_string();
            self.save_container_metadata(metadata).await?;
            
            info!("Container {} paused successfully", container_id);
            Ok(())
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn unpause_container(&self, container_id: &str) -> Result<()> {
        info!("Unpausing container: {}", container_id);

        let mut containers = self.containers.write().await;
        if let Some(metadata) = containers.get_mut(container_id) {
            if metadata.state != "paused" {
                return Err(anyhow!("Container {} is not paused", container_id));
            }

            metadata.state = "running".to_string();
            self.save_container_metadata(metadata).await?;
            
            info!("Container {} unpaused successfully", container_id);
            Ok(())
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn get_container_logs(&self, container_id: &str, _follow: bool, tail: Option<usize>) -> Result<Vec<String>> {
        let containers = self.containers.read().await;
        if let Some(metadata) = containers.get(container_id) {
            if !metadata.log_path.exists() {
                return Ok(vec![]);
            }

            let content = tokio::fs::read_to_string(&metadata.log_path).await?;
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
            
            let result = if let Some(n) = tail {
                let len = lines.len();
                if len > n {
                    lines.into_iter().skip(len - n).collect()
                } else {
                    lines
                }
            } else {
                lines
            };

            // TODO: follow実装（ファイル監視）
            Ok(result)
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn get_container_stats(&self, container_id: &str) -> Result<HashMap<String, serde_json::Value>> {
        let containers = self.containers.read().await;
        if let Some(metadata) = containers.get(container_id) {
            if metadata.state != "running" {
                return Err(anyhow!("Container {} is not running", container_id));
            }

            // モック統計情報
            let mut stats = HashMap::new();
            stats.insert("cpu_percent".to_string(), serde_json::json!(0.5));
            stats.insert("memory_usage".to_string(), serde_json::json!(1024 * 1024 * 100)); // 100MB
            stats.insert("memory_limit".to_string(), serde_json::json!(1024 * 1024 * 512)); // 512MB
            stats.insert("network_rx_bytes".to_string(), serde_json::json!(1024));
            stats.insert("network_tx_bytes".to_string(), serde_json::json!(2048));
            
            Ok(stats)
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    pub async fn exec_in_container(&self, container_id: &str, cmd: Vec<String>, _env: Vec<String>, _workdir: Option<String>) -> Result<String> {
        let containers = self.containers.read().await;
        if let Some(metadata) = containers.get(container_id) {
            if metadata.state != "running" {
                return Err(anyhow!("Container {} is not running", container_id));
            }

            let exec_id = Uuid::new_v4().to_string();
            info!("Executing command in container {}: {:?}", container_id, cmd);
            
            // TODO: 実際のexec実装
            // 現在はモック実装
            Ok(exec_id)
        } else {
            Err(anyhow!("Container not found: {}", container_id))
        }
    }

    /// コンテナを強制終了
    pub async fn kill_container(&self, id: &str) -> Result<()> {
        log::info!("Killing container: {}", id);
        
        let mut containers = self.containers.write().await;
        if let Some(container) = containers.get_mut(id) {
            container.state = ContainerState::Exited;
            log::info!("Container killed: {}", id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Container not found: {}", id))
        }
    }
}
