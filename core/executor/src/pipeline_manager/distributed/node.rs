/**
 * 分散ノードモジュール
 * 
 * 分散パイプライン実行におけるノード管理を担当するモジュール
 */

use std::collections::HashSet;
use std::time::Instant;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use tokio::sync::{Mutex, RwLock};
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn, error};

use crate::pipeline_manager::stages::StageKind;

/// ノード能力の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeCapability {
    /// 計算処理
    Compute,
    /// メモリ集約処理
    Memory,
    /// ネットワーク処理
    Network,
    /// ストレージ処理
    Storage,
    /// GPUアクセラレーション
    Gpu,
    /// 特権実行
    Privileged,
}

/// ノードの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// 利用可能
    Available,
    /// ビジー
    Busy,
    /// オフライン
    Offline,
    /// 障害発生
    Failed,
    /// メンテナンス中
    Maintenance,
}

/// 分散実行ノード
pub struct Node {
    /// ノードID
    pub id: String,
    /// ノードの現在の状態
    pub state: NodeState,
    /// ノードの能力セット
    pub capabilities: HashSet<NodeCapability>,
    /// 最後の応答時刻
    pub last_heartbeat: Instant,
    /// 現在のロード（実行中タスク数）
    pub current_load: usize,
    /// 最大同時タスク数
    pub max_concurrent_tasks: usize,
    /// リソースメトリクス
    pub metrics: NodeMetrics,
}

/// ノードリソースメトリクス
#[derive(Debug, Clone, Default)]
pub struct NodeMetrics {
    /// CPU使用率 (0.0-1.0)
    pub cpu_usage: f64,
    /// メモリ使用率 (0.0-1.0)
    pub memory_usage: f64,
    /// ディスク使用率 (0.0-1.0)
    pub disk_usage: f64,
    /// ネットワーク使用量 (Mbps)
    pub network_usage: f64,
}

impl Node {
    /// 新しいノードを作成
    pub fn new(id: String, capabilities: HashSet<NodeCapability>) -> Self {
        Self {
            id,
            state: NodeState::Available,
            capabilities,
            last_heartbeat: Instant::now(),
            current_load: 0,
            max_concurrent_tasks: 10, // デフォルト値
            metrics: NodeMetrics::default(),
        }
    }
    
    /// ノードの状態を更新
    pub fn set_state(&mut self, state: NodeState) {
        self.state = state;
    }
    
    /// ハートビートを更新
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }
    
    /// リソースメトリクスを更新
    pub fn update_metrics(&mut self, metrics: NodeMetrics) {
        self.metrics = metrics;
        
        // 負荷に応じて状態を自動調整
        if self.metrics.cpu_usage > 0.9 || self.metrics.memory_usage > 0.9 {
            self.state = NodeState::Busy;
        } else if self.state == NodeState::Busy {
            self.state = NodeState::Available;
        }
    }
    
    /// ノードが利用可能かチェック
    pub fn is_available(&self) -> bool {
        self.state == NodeState::Available && self.current_load < self.max_concurrent_tasks
    }
    
    /// ノードの健全性スコアを計算 (0.0-1.0)
    pub fn health_score(&self) -> f64 {
        if self.state != NodeState::Available && self.state != NodeState::Busy {
            return 0.0;
        }
        
        let load_factor = 1.0 - (self.current_load as f64 / self.max_concurrent_tasks as f64);
        let resource_factor = 1.0 - (self.metrics.cpu_usage + self.metrics.memory_usage) / 2.0;
        
        0.4 * load_factor + 0.6 * resource_factor
    }
}

/// 分散ノードの識別子
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    /// 新しいノードIDを生成
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// 特定の文字列からノードIDを作成
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// ノードIDを文字列として取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// ノードの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// オンライン
    Online,
    /// ビジー状態
    Busy,
    /// アイドル状態
    Idle,
    /// オフライン
    Offline,
    /// エラー状態
    Error,
    /// メンテナンス中
    Maintenance,
}

/// ノード能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// 利用可能なメモリ (バイト)
    pub available_memory: u64,
    /// 利用可能なCPUコア数
    pub available_cores: u32,
    /// ディスク容量 (バイト)
    pub disk_space: u64,
    /// ネットワーク帯域幅 (Mbps)
    pub network_bandwidth: u32,
    /// 特殊ハードウェア機能
    pub special_hardware: Vec<String>,
    /// サポートするステージ種類
    pub supported_stages: Vec<StageKind>,
    /// ノードの優先度 (1-100)
    pub priority: u8,
    /// 最大同時実行パイプライン数
    pub max_concurrent_pipelines: u32,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            available_memory: 0,
            available_cores: 0,
            disk_space: 0,
            network_bandwidth: 0,
            special_hardware: Vec::new(),
            supported_stages: Vec::new(),
            priority: 50,
            max_concurrent_pipelines: 10,
        }
    }
}

/// ノード情報
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// ノードID
    pub id: NodeId,
    /// ノード名
    pub name: String,
    /// ホスト名またはIPアドレス
    pub host: String,
    /// ポート
    pub port: u16,
    /// 状態
    pub status: NodeStatus,
    /// 能力
    pub capabilities: NodeCapabilities,
    /// 最終ハートビート時間
    pub last_heartbeat: Instant,
    /// 実行中のパイプライン数
    pub active_pipelines: u32,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl NodeInfo {
    /// 新しいノード情報を作成
    pub fn new(name: String, host: String, port: u16) -> Self {
        Self {
            id: NodeId::new(),
            name,
            host,
            port,
            status: NodeStatus::Idle,
            capabilities: NodeCapabilities::default(),
            last_heartbeat: Instant::now(),
            active_pipelines: 0,
            metadata: HashMap::new(),
        }
    }
    
    /// ノードが利用可能かどうか確認
    pub fn is_available(&self) -> bool {
        match self.status {
            NodeStatus::Online | NodeStatus::Idle => true,
            _ => false,
        }
    }
    
    /// ノードのアドレスを取得
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
    
    /// ノードのハートビートを更新
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }
    
    /// ノードの状態を更新
    pub fn update_status(&mut self, status: NodeStatus) {
        self.status = status;
    }
    
    /// メタデータを設定
    pub fn set_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }
    
    /// メタデータを取得
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// ノード負荷情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLoad {
    /// CPU使用率 (%)
    pub cpu_usage: f32,
    /// メモリ使用率 (%)
    pub memory_usage: f32,
    /// ディスク使用率 (%)
    pub disk_usage: f32,
    /// ネットワーク使用率 (%)
    pub network_usage: f32,
}

impl Default for NodeLoad {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_usage: 0.0,
            network_usage: 0.0,
        }
    }
}

/// ノードマネージャー
pub struct NodeManager {
    /// ノードレジストリ
    nodes: Arc<RwLock<HashMap<NodeId, NodeInfo>>>,
    /// ノード接続状態
    connections: Arc<RwLock<HashMap<NodeId, NodeConnectionState>>>,
    /// ローカルノードID
    local_node_id: NodeId,
}

/// ノード接続状態
#[derive(Debug, Clone)]
struct NodeConnectionState {
    /// 接続中フラグ
    connected: bool,
    /// 最終接続時間
    last_connection: Instant,
    /// 試行回数
    retry_count: u32,
    /// 接続エラー
    last_error: Option<String>,
}

impl NodeManager {
    /// 新しいノードマネージャーを作成
    pub fn new(local_node: NodeInfo) -> Self {
        let local_node_id = local_node.id.clone();
        let mut nodes = HashMap::new();
        nodes.insert(local_node.id.clone(), local_node);
        
        Self {
            nodes: Arc::new(RwLock::new(nodes)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            local_node_id,
        }
    }
    
    /// ノードを追加
    pub async fn add_node(&self, node: NodeInfo) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        
        if nodes.contains_key(&node.id) {
            return Err(anyhow!("ノードID {} は既に存在します", node.id));
        }
        
        info!("新しいノードを追加: {} ({})", node.name, node.address());
        nodes.insert(node.id.clone(), node);
        
        // 接続状態も初期化
        let mut connections = self.connections.write().await;
        connections.insert(node.id.clone(), NodeConnectionState {
            connected: false,
            last_connection: Instant::now(),
            retry_count: 0,
            last_error: None,
        });
        
        Ok(())
    }
    
    /// ノードを削除
    pub async fn remove_node(&self, node_id: &NodeId) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        
        if !nodes.contains_key(node_id) {
            return Err(anyhow!("ノードID {} は存在しません", node_id));
        }
        
        if node_id == &self.local_node_id {
            return Err(anyhow!("ローカルノードは削除できません"));
        }
        
        info!("ノードを削除: {}", node_id);
        nodes.remove(node_id);
        
        // 接続状態も削除
        let mut connections = self.connections.write().await;
        connections.remove(node_id);
        
        Ok(())
    }
    
    /// ノード情報を取得
    pub async fn get_node(&self, node_id: &NodeId) -> Option<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.get(node_id).cloned()
    }
    
    /// 利用可能なノードのリストを取得
    pub async fn get_available_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|node| node.is_available())
            .cloned()
            .collect()
    }
    
    /// すべてのノードのリストを取得
    pub async fn get_all_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values().cloned().collect()
    }
    
    /// ローカルノード情報を取得
    pub async fn get_local_node(&self) -> NodeInfo {
        let nodes = self.nodes.read().await;
        nodes.get(&self.local_node_id)
            .cloned()
            .expect("ローカルノード情報が見つかりません")
    }
    
    /// ノード状態を更新
    pub async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        
        if let Some(node) = nodes.get_mut(node_id) {
            info!("ノード {} の状態を更新: {:?}", node_id, status);
            node.update_status(status);
            Ok(())
        } else {
            Err(anyhow!("ノードID {} は存在しません", node_id))
        }
    }
    
    /// ノードハートビートを更新
    pub async fn update_node_heartbeat(&self, node_id: &NodeId) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        
        if let Some(node) = nodes.get_mut(node_id) {
            node.update_heartbeat();
            Ok(())
        } else {
            Err(anyhow!("ノードID {} は存在しません", node_id))
        }
    }
    
    /// 接続状態を更新
    pub async fn set_node_connected(&self, node_id: &NodeId, connected: bool, error: Option<String>) -> Result<()> {
        let mut connections = self.connections.write().await;
        
        if let Some(state) = connections.get_mut(node_id) {
            state.connected = connected;
            state.last_connection = Instant::now();
            
            if connected {
                state.retry_count = 0;
                state.last_error = None;
            } else {
                state.retry_count += 1;
                state.last_error = error;
            }
            
            Ok(())
        } else {
            Err(anyhow!("ノードID {} の接続情報が存在しません", node_id))
        }
    }
    
    /// タイムアウトしたノードを検出
    pub async fn detect_timed_out_nodes(&self, timeout: Duration) -> Vec<NodeId> {
        let now = Instant::now();
        let nodes = self.nodes.read().await;
        
        nodes.iter()
            .filter(|(id, node)| {
                // ローカルノードは除外
                *id != &self.local_node_id && 
                // タイムアウト判定
                now.duration_since(node.last_heartbeat) > timeout
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_id() {
        let node_id = NodeId::new();
        assert!(!node_id.as_str().is_empty());
        
        let node_id2 = NodeId::from_string("test-node".to_string());
        assert_eq!(node_id2.as_str(), "test-node");
    }
    
    #[test]
    fn test_node_info() {
        let node = NodeInfo::new(
            "test-node".to_string(), 
            "localhost".to_string(), 
            8080
        );
        
        assert_eq!(node.name, "test-node");
        assert_eq!(node.host, "localhost");
        assert_eq!(node.port, 8080);
        assert_eq!(node.status, NodeStatus::Idle);
        assert_eq!(node.address(), "localhost:8080");
        assert!(node.is_available());
    }
} 