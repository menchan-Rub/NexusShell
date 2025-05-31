use anyhow::{Result, anyhow};
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::DaemonConfig;
use crate::container_manager::ContainerManager;
use crate::image_manager::ImageManager;
use crate::volume_manager::VolumeManager;
use crate::network_manager::NetworkManager;
use crate::event_manager::EventManager;
use crate::metrics::MetricsCollector;
use crate::grpc::GrpcServer;
use crate::http::HttpServer;

#[derive(Debug, Clone)]
pub struct NexusDaemon {
    config: Arc<RwLock<DaemonConfig>>,
    pub container_manager: Arc<ContainerManager>,
    pub image_manager: Arc<ImageManager>,
    pub volume_manager: Arc<VolumeManager>,
    pub _network_manager: Arc<NetworkManager>,
    pub event_manager: Arc<EventManager>,
    pub metrics_collector: Arc<MetricsCollector>,
    _grpc_server: Option<Arc<GrpcServer>>,
    _http_server: Option<Arc<HttpServer>>,
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImageStats {
    pub total: u32,
    pub size: u64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VolumeStats {
    pub total: u32,
    pub size: u64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NetworkStats {
    pub total: u32,
    pub active: u32,
}

impl NexusDaemon {
    pub async fn new(config: DaemonConfig) -> Result<Self> {
        info!("Initializing NexusContainer daemon");
        
        // データディレクトリの作成
        tokio::fs::create_dir_all(&config.data_root).await?;
        tokio::fs::create_dir_all(&config.storage_config.images_dir).await?;
        tokio::fs::create_dir_all(&config.storage_config.containers_dir).await?;
        tokio::fs::create_dir_all(&config.storage_config.volumes_dir).await?;
        tokio::fs::create_dir_all(&config.storage_config.tmp_dir).await?;
        
        // マネージャーの初期化
        let container_manager = Arc::new(ContainerManager::new(config.storage_config.containers_dir.clone()));
        container_manager.initialize().await?;
        
        let image_manager = Arc::new(ImageManager::new(config.storage_config.images_dir.clone()));
        image_manager.initialize().await?;
        
        let volume_manager = Arc::new(VolumeManager::new(config.storage_config.volumes_dir.clone()));
        volume_manager.initialize().await?;
        
        let network_manager = Arc::new(NetworkManager::new());
        network_manager.initialize().await?;
        
        let event_manager = Arc::new(EventManager::new());
        event_manager.initialize().await?;
        
        let metrics_collector = Arc::new(MetricsCollector::new());
        metrics_collector.initialize().await?;

        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel(1);
        
        let daemon = Self {
            config: Arc::new(RwLock::new(config)),
            container_manager,
            image_manager,
            volume_manager,
            _network_manager: network_manager,
            event_manager,
            metrics_collector,
            _grpc_server: None,
            _http_server: None,
            shutdown_tx: Some(shutdown_tx),
        };

        // デーモン開始イベントを発行
        daemon.event_manager.emit_daemon_event("start", "NexusContainer daemon starting").await?;
        
        Ok(daemon)
    }
    
    pub async fn run(&self) -> Result<()> {
        info!("Starting NexusContainer daemon services");
        
        // 設定を読み込み
        let config = {
            let config = self.config.read().await;
            config.clone()
        };
        
        // gRPCサーバーの開始
        let grpc_addr = config.grpc_listen
            .parse()
            .map_err(|e| anyhow!("Invalid gRPC address: {}", e))?;
        
        let grpc_server = Arc::new(GrpcServer::new(
            Arc::new(RwLock::new(self.clone())),
            grpc_addr,
        ));
        
        let grpc_handle = {
            let server = grpc_server.clone();
            tokio::spawn(async move {
                if let Err(e) = server.serve().await {
                    error!("gRPC server error: {}", e);
                }
            })
        };
        
        // HTTPサーバーの開始
        let http_addr = config.http_listen
            .parse()
            .map_err(|e| anyhow!("Invalid HTTP address: {}", e))?;
        
        let http_server = Arc::new(HttpServer::new(
            Arc::new(RwLock::new(self.clone())),
            http_addr,
        ));
        
        let http_handle = {
            let server = http_server.clone();
            tokio::spawn(async move {
                if let Err(e) = server.serve().await {
                    error!("HTTP server error: {}", e);
                }
            })
        };
        
        // メトリクス収集の開始
        let metrics_handle = {
            let metrics = self.metrics_collector.clone();
            tokio::spawn(async move {
                if let Err(e) = metrics.start_collection().await {
                    error!("Metrics collection error: {}", e);
                }
            })
        };
        
        // イベント処理の開始
        let event_handle = {
            let events = self.event_manager.clone();
            tokio::spawn(async move {
                if let Err(e) = events.start_processing().await {
                    error!("Event processing error: {}", e);
                }
            })
        };
        
        // シグナルハンドリング
        let shutdown_rx = self.shutdown_tx.as_ref().unwrap().subscribe();
        let signal_handle = {
            let event_manager = self.event_manager.clone();
            let shutdown_tx = self.shutdown_tx.as_ref().unwrap().clone();
            
            tokio::spawn(async move {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sigterm = signal(SignalKind::terminate()).unwrap();
                    let mut sigint = signal(SignalKind::interrupt()).unwrap();
                    
                    tokio::select! {
                        _ = sigterm.recv() => {
                            info!("Received SIGTERM, shutting down gracefully");
                            let _ = event_manager.emit_daemon_event("shutdown", "Received SIGTERM").await;
                        }
                        _ = sigint.recv() => {
                            info!("Received SIGINT, shutting down gracefully");
                            let _ = event_manager.emit_daemon_event("shutdown", "Received SIGINT").await;
                        }
                    }
                }
                
                #[cfg(windows)]
                {
                    use tokio::signal::ctrl_c;
                    if (ctrl_c().await).is_ok() {
                        log::info!("Received SIGINT, shutting down gracefully...");
                    }
                }
                
                let _ = shutdown_tx.send(());
            })
        };
        
        info!("All services started, daemon is ready");
        self.event_manager.emit_daemon_event("ready", "All services started successfully").await?;
        
        // シャットダウンシグナルを待機
        let mut shutdown_rx = shutdown_rx;
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received");
            }
            _ = grpc_handle => {
                warn!("gRPC server terminated unexpectedly");
            }
            _ = http_handle => {
                warn!("HTTP server terminated unexpectedly");
            }
            _ = metrics_handle => {
                warn!("Metrics collector terminated unexpectedly");
            }
            _ = event_handle => {
                warn!("Event manager terminated unexpectedly");
            }
            _ = signal_handle => {
                info!("Signal handler completed");
            }
        }
        
        // グレースフルシャットダウン
        self.graceful_shutdown().await?;
        
        Ok(())
    }
    
    async fn graceful_shutdown(&self) -> Result<()> {
        info!("Starting graceful shutdown");
        
        // 実行中のコンテナを停止
        let containers = self.container_manager.list_containers(false, false, std::collections::HashMap::new()).await?;
        for container in containers {
            if container.state == "running" {
                info!("Stopping container: {}", container.id);
                if let Err(e) = self.container_manager.stop_container(&container.id, 10).await {
                    warn!("Failed to stop container {}: {}", container.id, e);
                }
            }
        }
        
        // 未使用リソースのクリーンアップ
        if let Err(e) = self.image_manager.cleanup_unused_images().await {
            warn!("Failed to cleanup unused images: {}", e);
        }
        
        if let Err(e) = self.volume_manager.cleanup_unused_volumes().await {
            warn!("Failed to cleanup unused volumes: {}", e);
        }
        
        // メトリクスのエクスポート
        if let Ok(metrics) = self.metrics_collector.export_metrics("json").await {
            debug!("Final metrics: {}", metrics);
        }
        
        // 最終イベントの発行
        self.event_manager.emit_daemon_event("stop", "NexusContainer daemon stopped").await?;
        
        info!("Graceful shutdown completed");
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down NexusContainer daemon");
        
        // シャットダウンシグナルを送信
        if let Some(ref shutdown_tx) = self.shutdown_tx {
            let _ = shutdown_tx.send(());
        }
        
        info!("Daemon shutdown completed");
        Ok(())
    }

    // 統計情報取得
    #[allow(dead_code)]
    pub async fn get_daemon_stats(&self) -> Result<std::collections::HashMap<String, serde_json::Value>> {
        let mut stats = std::collections::HashMap::new();
        
        // コンテナ統計
        let containers = self.container_manager.list_containers(true, false, std::collections::HashMap::new()).await?;
        let running_containers = containers.iter().filter(|c| c.state == "running").count();
        let paused_containers = containers.iter().filter(|c| c.state == "paused").count();
        let stopped_containers = containers.iter().filter(|c| c.state == "exited").count();
        
        stats.insert("containers_total".to_string(), serde_json::json!(containers.len()));
        stats.insert("containers_running".to_string(), serde_json::json!(running_containers));
        stats.insert("containers_paused".to_string(), serde_json::json!(paused_containers));
        stats.insert("containers_stopped".to_string(), serde_json::json!(stopped_containers));
        
        // イメージ統計
        let image_stats = self.image_manager.get_stats().await?;
        stats.insert("images_total".to_string(), serde_json::json!(image_stats.total));
        stats.insert("images_size".to_string(), serde_json::json!(image_stats.size));
        
        // ボリューム統計
        let volume_stats = self.volume_manager.get_stats().await?;
        stats.insert("volumes_total".to_string(), serde_json::json!(volume_stats.total));
        stats.insert("volumes_size".to_string(), serde_json::json!(volume_stats.size));
        
        // ネットワーク統計
        let network_stats = self._network_manager.get_stats().await?;
        stats.insert("networks_total".to_string(), serde_json::json!(network_stats.total));
        stats.insert("networks_active".to_string(), serde_json::json!(network_stats.active));
        
        // システム統計
        if let Some(system_metrics) = self.metrics_collector.get_system_metrics().await {
            stats.insert("system_metrics".to_string(), serde_json::to_value(system_metrics)?);
        }
        
        // イベント統計
        let event_stats = self.event_manager.get_event_stats().await;
        stats.insert("events".to_string(), serde_json::to_value(event_stats)?);
        
        Ok(stats)
    }

    // ヘルスチェック
    #[allow(dead_code)]
    pub async fn health_check(&self) -> Result<std::collections::HashMap<String, serde_json::Value>> {
        let mut health = std::collections::HashMap::new();
        
        // 各コンポーネントのヘルスチェック
        health.insert("daemon".to_string(), serde_json::json!("healthy"));
        health.insert("container_manager".to_string(), serde_json::json!("healthy"));
        health.insert("image_manager".to_string(), serde_json::json!("healthy"));
        health.insert("volume_manager".to_string(), serde_json::json!("healthy"));
        health.insert("network_manager".to_string(), serde_json::json!("healthy"));
        health.insert("event_manager".to_string(), serde_json::json!("healthy"));
        health.insert("metrics_collector".to_string(), serde_json::json!("healthy"));
        
        // システムリソースチェック
        if let Some(system_metrics) = self.metrics_collector.get_system_metrics().await {
            let memory_usage_percent = (system_metrics.memory_usage as f64 / system_metrics.memory_total as f64) * 100.0;
            let disk_usage_percent = (system_metrics.disk_usage as f64 / system_metrics.disk_total as f64) * 100.0;
            
            health.insert("memory_usage_percent".to_string(), serde_json::json!(memory_usage_percent));
            health.insert("disk_usage_percent".to_string(), serde_json::json!(disk_usage_percent));
            health.insert("cpu_usage_percent".to_string(), serde_json::json!(system_metrics.cpu_usage));
            health.insert("load_average".to_string(), serde_json::json!(system_metrics.load_average));
        }
        
        health.insert("timestamp".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
        
        Ok(health)
    }

    // 設定の再読み込み
    #[allow(dead_code)]
    pub async fn reload_config(&self, new_config: DaemonConfig) -> Result<()> {
        info!("Reloading daemon configuration");
        
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }
        
        self.event_manager.emit_daemon_event("config_reload", "Configuration reloaded").await?;
        
        info!("Configuration reloaded successfully");
        Ok(())
    }

    // デバッグ情報取得
    #[allow(dead_code)]
    pub async fn get_debug_info(&self) -> Result<std::collections::HashMap<String, serde_json::Value>> {
        let mut debug_info = std::collections::HashMap::new();
        
        // 設定情報
        let config = self.config.read().await;
        debug_info.insert("config".to_string(), serde_json::to_value(&*config)?);
        
        // 統計情報
        let stats = self.get_daemon_stats().await?;
        debug_info.insert("stats".to_string(), serde_json::to_value(stats)?);
        
        // ヘルス情報
        let health = self.health_check().await?;
        debug_info.insert("health".to_string(), serde_json::to_value(health)?);
        
        // 最近のイベント
        let recent_events = self.event_manager.get_events(crate::event_manager::EventFilter::default()).await?;
        let recent_events: Vec<_> = recent_events.into_iter().take(10).collect();
        debug_info.insert("recent_events".to_string(), serde_json::to_value(recent_events)?);
        
        Ok(debug_info)
    }
} 