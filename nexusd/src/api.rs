// API定義とハンドラー（スタブ）

pub mod container {
    // コンテナAPI
}

pub mod image {
    // イメージAPI
}

pub mod volume {
    // ボリュームAPI
}

pub mod network {
    // ネットワークAPI
}

pub mod system {
    // システムAPI
}

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::daemon::NexusDaemon;

// 一時的なスタブ実装（protobuf生成待ち）
#[derive(Debug)]
#[allow(dead_code)]
pub struct ApiServer {
    daemon: Arc<RwLock<NexusDaemon>>,
}

impl ApiServer {
    #[allow(dead_code)]
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>) -> Self {
        Self { daemon }
    }

    #[allow(dead_code)]
    pub async fn start(&self, addr: std::net::SocketAddr) -> anyhow::Result<()> {
        info!("API server starting on {}...", addr);
        
        // HTTP APIサーバー
        let app = axum::Router::new()
            .route("/health", axum::routing::get(health_check))
            .route("/version", axum::routing::get(version_info));

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        
        Ok(())
    }
}

#[allow(dead_code)]
async fn health_check() -> &'static str {
    "OK"
}

#[allow(dead_code)]
async fn version_info() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": "1.0",
        "build_time": chrono::Utc::now().to_rfc3339()
    }))
}