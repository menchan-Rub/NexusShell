/*!
# 分散パイプライン実行モジュール

複数のノードにまたがる分散パイプライン実行を可能にする高度なモジュール。
大規模データ処理やハイパフォーマンスコンピューティングに対応した設計になっています。

## 主な機能

- ノード間でのシームレスなパイプライン分散
- 動的ロードバランシング
- 自動フェイルオーバー
- データシャーディング
- リソース使用の最適化
- グローバルモニタリング
*/

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicUsize};

use anyhow::{Result, anyhow, Context as AnyhowContext};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, trace, warn};
use uuid::Uuid;

use crate::pipeline_manager::{
    PipelineId, PipelineContext, PipelineResult, PipelineManagerConfig,
    stages::{Stage, StageId, StageKind, DataType},
    pipeline::{Pipeline, PipelineConfig, PipelineEvent, PipelineStatus},
    error::{PipelineError, ErrorCategory}
};

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

/// 分散パイプライン設定
#[derive(Debug, Clone)]
pub struct DistributedConfig {
    /// クラスタ名
    pub cluster_name: String,
    /// マスターノード
    pub master_node: Option<NodeId>,
    /// ハートビート間隔
    pub heartbeat_interval: Duration,
    /// ノードタイムアウト
    pub node_timeout: Duration,
    /// 自動再接続を試みる
    pub auto_reconnect: bool,
    /// データレプリケーション係数
    pub replication_factor: u8,
    /// 通信暗号化を有効にする
    pub enable_encryption: bool,
    /// サービス検出方法
    pub discovery_method: DiscoveryMethod,
}

impl Default for DistributedConfig {
    fn default() -> Self {
        Self {
            cluster_name: "nexusshell-cluster".to_string(),
            master_node: None,
            heartbeat_interval: Duration::from_secs(5),
            node_timeout: Duration::from_secs(30),
            auto_reconnect: true,
            replication_factor: 1,
            enable_encryption: true,
            discovery_method: DiscoveryMethod::MulticastDns,
        }
    }
}

/// サービス検出方法
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// 静的ノードリスト
    StaticList,
    /// マルチキャストDNS
    MulticastDns,
    /// 中央レジストリ
    CentralRegistry,
    /// Kubernetes
    Kubernetes,
    /// コンスル
    Consul,
    /// ZooKeeper
    ZooKeeper,
    /// カスタム
    Custom(String),
}

/// 分散実行戦略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistributionStrategy {
    /// ラウンドロビン
    RoundRobin,
    /// 最小負荷
    LeastLoaded,
    /// データの近接性
    DataLocality,
    /// 能力ベース
    CapabilityBased,
    /// コスト最適化
    CostOptimized,
    /// カスタム
    Custom(String),
}

/// パーティション戦略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionStrategy {
    /// ハッシュベース
    Hash,
    /// レンジベース
    Range,
    /// リストベース
    List,
    /// カスタム
    Custom(String),
}

/// 分散タスク
#[derive(Debug, Clone)]
pub struct DistributedTask {
    /// タスクID
    pub id: String,
    /// パイプラインID
    pub pipeline_id: PipelineId,
    /// ステージID
    pub stage_id: StageId,
    /// 割り当てられたノード
    pub assigned_node: Option<NodeId>,
    /// 入力データパーティション
    pub input_partition: Option<DataPartition>,
    /// 状態
    pub status: TaskStatus,
    /// 開始時間
    pub start_time: Option<Instant>,
    /// 終了時間
    pub end_time: Option<Instant>,
    /// エラー
    pub error: Option<String>,
    /// 再試行回数
    pub retry_count: u32,
    /// 優先度
    pub priority: u8,
}

/// データパーティション
#[derive(Debug, Clone)]
pub struct DataPartition {
    /// パーティションID
    pub id: String,
    /// データ
    pub data: DataType,
    /// パーティション範囲
    pub range: Option<PartitionRange>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

/// パーティション範囲
#[derive(Debug, Clone)]
pub struct PartitionRange {
    /// 開始インデックス
    pub start_index: u64,
    /// 終了インデックス
    pub end_index: u64,
    /// キー範囲（文字列ベース）
    pub key_range: Option<(String, String)>,
}

/// タスク状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// 作成済み
    Created,
    /// キューイング中
    Queued,
    /// 割り当て済み
    Assigned,
    /// 実行中
    Running,
    /// 完了
    Completed,
    /// 失敗
    Failed,
    /// タイムアウト
    TimedOut,
    /// キャンセル済み
    Cancelled,
}

/// フェイルオーバー戦略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailoverStrategy {
    /// 即時再割り当て
    ImmediateReassignment,
    /// リカバリーポイントから再開
    RecoveryPointRestart,
    /// 特定ノードに限定
    LimitedToNodes(Vec<NodeId>),
    /// レプリケーション基準値
    ReplicationBasedOnCriticality(u8),
    /// カスタム戦略
    Custom(String),
}

/// 高可用性設定
#[derive(Debug, Clone)]
pub struct HighAvailabilityConfig {
    /// フェイルオーバー戦略
    pub failover_strategy: FailoverStrategy,
    /// 最大再試行回数
    pub max_retries: u32,
    /// 再試行待機時間
    pub retry_delay: Duration,
    /// タスク再開方法
    pub task_resumption: TaskResumptionStrategy,
    /// クリティカルタスクの識別条件
    pub critical_task_criteria: Option<Box<dyn Fn(&DistributedTask) -> bool + Send + Sync>>,
    /// ノードクォーラム数（最小動作ノード数）
    pub node_quorum: usize,
}

/// タスク再開戦略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskResumptionStrategy {
    /// 最初から再実行
    Restart,
    /// チェックポイントから再開
    CheckpointBased,
    /// 部分結果を保持
    KeepPartialResults,
}

impl Default for HighAvailabilityConfig {
    fn default() -> Self {
        Self {
            failover_strategy: FailoverStrategy::ImmediateReassignment,
            max_retries: 3,
            retry_delay: Duration::from_secs(2),
            task_resumption: TaskResumptionStrategy::Restart,
            critical_task_criteria: None,
            node_quorum: 1,
        }
    }
}

/// チェックポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    /// タスクID
    pub task_id: String,
    /// チェックポイント時間
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 進捗率 (0.0-1.0)
    pub progress: f32,
    /// 中間結果
    pub intermediate_results: Option<DataType>,
    /// 実行状態
    pub execution_state: Vec<u8>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

/// ノードフェイルオーバーマネージャー
pub struct FailoverManager {
    /// HA設定
    config: HighAvailabilityConfig,
    /// タスクチェックポイント
    checkpoints: Arc<RwLock<HashMap<String, TaskCheckpoint>>>,
    /// タスク再試行カウンター
    retry_counters: Arc<RwLock<HashMap<String, u32>>>,
    /// フェイルオーバーイベント履歴
    failover_history: Arc<Mutex<Vec<FailoverEvent>>>,
}

/// フェイルオーバーイベント
#[derive(Debug, Clone)]
struct FailoverEvent {
    /// イベントID
    id: String,
    /// 失敗したノード
    failed_node: NodeId,
    /// 引き継いだノード
    takeover_node: Option<NodeId>,
    /// 影響を受けたタスク
    affected_tasks: Vec<String>,
    /// イベント発生時間
    timestamp: chrono::DateTime<chrono::Utc>,
    /// 復旧時間
    recovery_time: Option<Duration>,
    /// フェイルオーバー成功フラグ
    success: bool,
}

impl FailoverManager {
    /// 新しいフェイルオーバーマネージャーを作成
    pub fn new(config: HighAvailabilityConfig) -> Self {
        Self {
            config,
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            retry_counters: Arc::new(RwLock::new(HashMap::new())),
            failover_history: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// ノード障害を処理
    pub async fn handle_node_failure(
        &self,
        failed_node: &NodeId,
        active_nodes: &[NodeInfo],
        affected_tasks: &[DistributedTask],
    ) -> Result<HashMap<String, NodeId>> {
        info!("ノード {} の障害を処理中...", failed_node);
        
        let event_id = Uuid::new_v4().to_string();
        let event_time = chrono::Utc::now();
        
        // 再割り当て結果
        let mut reassignments = HashMap::new();
        
        // 再割り当てを実行
        let strategy = &self.config.failover_strategy;
        let available_nodes = self.filter_eligible_nodes(active_nodes, strategy);
        
        if available_nodes.is_empty() {
            error!("利用可能なノードがありません - フェイルオーバー失敗");
            
            // フェイルオーバーイベントを記録
            let event = FailoverEvent {
                id: event_id,
                failed_node: failed_node.clone(),
                takeover_node: None,
                affected_tasks: affected_tasks.iter().map(|t| t.id.clone()).collect(),
                timestamp: event_time,
                recovery_time: None,
                success: false,
            };
            
            let mut history = self.failover_history.lock().await;
            history.push(event);
            
            return Err(anyhow!("フェイルオーバーに利用可能なノードがありません"));
        }
        
        // タスクを再割り当て
        for task in affected_tasks {
            // 再試行カウンターをチェック
            let retry_exceeded = {
                let mut counters = self.retry_counters.write().await;
                let counter = counters.entry(task.id.clone()).or_insert(0);
                *counter += 1;
                *counter > self.config.max_retries
            };
            
            if retry_exceeded {
                warn!("タスク {} の最大再試行回数を超過", task.id);
                continue;
            }
            
            // ノードを選択
            let selected_node = self.select_replacement_node(available_nodes, task);
            
            if let Some(node) = selected_node {
                info!("タスク {} をノード {} からノード {} に再割り当て", 
                      task.id, failed_node, node.id);
                
                // チェックポイントがあるか確認
                let checkpoint = {
                    let checkpoints = self.checkpoints.read().await;
                    checkpoints.get(&task.id).cloned()
                };
                
                // 再割り当て情報を記録
                reassignments.insert(task.id.clone(), node.id.clone());
            } else {
                warn!("タスク {} の再割り当て先が見つかりません", task.id);
            }
        }
        
        // フェイルオーバーイベントを記録
        let takeover_node = if !reassignments.is_empty() {
            // 簡略化のため、最初の再割り当て先を記録
            let first_task_id = reassignments.keys().next().unwrap();
            Some(reassignments.get(first_task_id).unwrap().clone())
        } else {
            None
        };
        
        let event = FailoverEvent {
            id: event_id,
            failed_node: failed_node.clone(),
            takeover_node,
            affected_tasks: affected_tasks.iter().map(|t| t.id.clone()).collect(),
            timestamp: event_time,
            recovery_time: Some(chrono::Utc::now().signed_duration_since(event_time).to_std().unwrap_or(Duration::from_secs(0))),
            success: !reassignments.is_empty(),
        };
        
        let mut history = self.failover_history.lock().await;
        history.push(event);
        
        Ok(reassignments)
    }
    
    /// フェイルオーバー戦略に基づいて対象ノードをフィルタリング
    fn filter_eligible_nodes<'a>(
        &self,
        active_nodes: &'a [NodeInfo],
        strategy: &FailoverStrategy,
    ) -> Vec<&'a NodeInfo> {
        match strategy {
            FailoverStrategy::ImmediateReassignment => {
                // すべてのオンラインノードを使用
                active_nodes.iter()
                    .filter(|n| n.status == NodeStatus::Online || n.status == NodeStatus::Idle)
                    .collect()
            },
            FailoverStrategy::LimitedToNodes(node_ids) => {
                // 指定されたノードIDのみを使用
                active_nodes.iter()
                    .filter(|n| node_ids.contains(&n.id) && 
                           (n.status == NodeStatus::Online || n.status == NodeStatus::Idle))
                    .collect()
            },
            _ => {
                // デフォルトですべてのオンラインノードを使用
                active_nodes.iter()
                    .filter(|n| n.status == NodeStatus::Online || n.status == NodeStatus::Idle)
                    .collect()
            }
        }
    }
    
    /// 代替ノードを選択
    fn select_replacement_node<'a>(
        &self,
        available_nodes: Vec<&'a NodeInfo>,
        task: &DistributedTask,
    ) -> Option<&'a NodeInfo> {
        if available_nodes.is_empty() {
            return None;
        }
        
        // 最も負荷の少ないノードを選択
        available_nodes.iter()
            .min_by_key(|n| n.active_pipelines)
            .copied()
    }
    
    /// チェックポイントを作成
    pub async fn create_checkpoint(&self, task: &DistributedTask, progress: f32, intermediate_results: Option<DataType>) -> Result<()> {
        let checkpoint = TaskCheckpoint {
            task_id: task.id.clone(),
            timestamp: chrono::Utc::now(),
            progress,
            intermediate_results,
            execution_state: Vec::new(), // 実際の実装ではタスク状態をシリアライズ
            metadata: HashMap::new(),
        };
        
        let mut checkpoints = self.checkpoints.write().await;
        checkpoints.insert(task.id.clone(), checkpoint);
        
        Ok(())
    }
    
    /// チェックポイントを取得
    pub async fn get_checkpoint(&self, task_id: &str) -> Option<TaskCheckpoint> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints.get(task_id).cloned()
    }
    
    /// フェイルオーバー履歴を取得
    pub async fn get_failover_history(&self) -> Vec<FailoverEvent> {
        let history = self.failover_history.lock().await;
        history.clone()
    }
}

/// 分散パイプラインマネージャー
pub struct DistributedPipelineManager {
    /// 設定
    config: DistributedConfig,
    /// ローカルノードID
    local_node_id: NodeId,
    /// 全ノード情報
    nodes: Arc<RwLock<HashMap<NodeId, NodeInfo>>>,
    /// 分散タスク
    tasks: Arc<RwLock<HashMap<String, DistributedTask>>>,
    /// タスクキュー
    task_queue: Arc<Mutex<Vec<String>>>,
    /// タスク結果
    task_results: Arc<RwLock<HashMap<String, TaskResult>>>,
    /// 分散戦略
    distribution_strategy: DistributionStrategy,
    /// パーティション戦略
    partition_strategy: PartitionStrategy,
    /// ハートビート送信チャネル
    heartbeat_tx: mpsc::Sender<HeartbeatMessage>,
    /// ノード検出サービス
    discovery_service: Arc<dyn DiscoveryService>,
    /// シャットダウンシグナル
    shutdown_tx: mpsc::Sender<()>,
    /// フェイルオーバーマネージャー
    failover_manager: Arc<FailoverManager>,
    /// クラスタマネージャー
    cluster_manager: Option<Arc<ClusterManager>>,
}

/// タスク結果
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// タスクID
    pub task_id: String,
    /// 出力データ
    pub output: Option<DataType>,
    /// 実行時間
    pub execution_time: Duration,
    /// エラー
    pub error: Option<String>,
    /// メトリクス
    pub metrics: HashMap<String, f64>,
}

/// ハートビートメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    /// 送信元ノードID
    pub from_node: String,
    /// タイムスタンプ
    pub timestamp: u64,
    /// アクティブパイプライン数
    pub active_pipelines: u32,
    /// ノード状態
    pub status: NodeStatus,
    /// 負荷情報
    pub load: NodeLoad,
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

/// サービス検出インターフェース
#[async_trait]
pub trait DiscoveryService: Send + Sync {
    /// ノードを登録
    async fn register_node(&self, node: &NodeInfo) -> Result<()>;
    
    /// ノードの登録を解除
    async fn unregister_node(&self, node_id: &NodeId) -> Result<()>;
    
    /// ノードを検出
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>>;
    
    /// ノードの状態を更新
    async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()>;
}

/// マルチキャストDNSサービス検出
pub struct MulticastDnsDiscovery {
    /// クラスタ名
    cluster_name: String,
    /// サービスポート
    port: u16,
    /// 内部状態
    state: Arc<Mutex<MulticastDnsState>>,
}

/// マルチキャストDNS状態
struct MulticastDnsState {
    /// 登録済みノード
    registered_nodes: HashMap<NodeId, NodeInfo>,
    /// サービス名
    service_name: String,
}

impl MulticastDnsDiscovery {
    /// 新しいマルチキャストDNS検出サービスを作成
    pub fn new(cluster_name: String, port: u16) -> Self {
        let service_name = format!("_nexusshell-{}._{}.local", cluster_name, if port == 80 { "http" } else { "tcp" });
        
        let state = Arc::new(Mutex::new(MulticastDnsState {
            registered_nodes: HashMap::new(),
            service_name,
        }));
        
        Self {
            cluster_name,
            port,
            state,
        }
    }
}

#[async_trait]
impl DiscoveryService for MulticastDnsDiscovery {
    async fn register_node(&self, node: &NodeInfo) -> Result<()> {
        let mut state = self.state.lock().await;
        state.registered_nodes.insert(node.id.clone(), node.clone());
        
        // TODO: 実際のmDNSレジストレーション
        debug!("mDNSサービスに登録: {}.{}", node.name, state.service_name);
        
        Ok(())
    }
    
    async fn unregister_node(&self, node_id: &NodeId) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.registered_nodes.remove(node_id).is_some() {
            // TODO: 実際のmDNS登録解除
            debug!("mDNSサービスから登録解除: {}", node_id);
        }
        
        Ok(())
    }
    
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        let state = self.state.lock().await;
        
        // TODO: 実際のmDNS検出
        debug!("mDNSで検出中: {}", state.service_name);
        
        // 現在は登録されているノードを返すだけ
        Ok(state.registered_nodes.values().cloned().collect())
    }
    
    async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()> {
        let mut state = self.state.lock().await;
        if let Some(node) = state.registered_nodes.get_mut(node_id) {
            node.status = status;
            // TODO: 実際のmDNSアップデート
            debug!("mDNSでノード状態を更新: {} -> {:?}", node_id, status);
        }
        
        Ok(())
    }
}

impl DistributedPipelineManager {
    /// 新しい分散パイプラインマネージャーを作成
    pub fn new(
        config: DistributedConfig,
        local_node_info: NodeInfo,
        ha_config: Option<HighAvailabilityConfig>,
    ) -> Result<Self> {
        let (heartbeat_tx, heartbeat_rx) = mpsc::channel(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        // サービス検出方法に基づいて検出サービスを作成
        let discovery_service: Arc<dyn DiscoveryService> = match &config.discovery_method {
            DiscoveryMethod::MulticastDns => {
                Arc::new(MulticastDnsDiscovery::new(
                    config.cluster_name.clone(),
                    local_node_info.port,
                ))
            },
            // TODO: 他の検出方法の実装
            _ => return Err(anyhow!("未対応のサービス検出方法: {:?}", config.discovery_method)),
        };
        
        // フェイルオーバーマネージャーを作成
        let failover_manager = Arc::new(FailoverManager::new(
            ha_config.unwrap_or_default()
        ));
        
        let manager = Self {
            config,
            local_node_id: local_node_info.id.clone(),
            nodes: Arc::new(RwLock::new({
                let mut nodes = HashMap::new();
                nodes.insert(local_node_info.id.clone(), local_node_info);
                nodes
            })),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(Mutex::new(Vec::new())),
            task_results: Arc::new(RwLock::new(HashMap::new())),
            distribution_strategy: DistributionStrategy::LeastLoaded,
            partition_strategy: PartitionStrategy::Hash,
            heartbeat_tx,
            discovery_service,
            shutdown_tx,
            failover_manager,
            cluster_manager: None,
        };
        
        // バックグラウンドサービスを開始
        manager.start_services(heartbeat_rx, shutdown_rx);
        
        Ok(manager)
    }
    
    /// バックグラウンドサービスを開始
    fn start_services(
        &self,
        mut heartbeat_rx: mpsc::Receiver<HeartbeatMessage>,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        let nodes = self.nodes.clone();
        let discovery = self.discovery_service.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let node_timeout = self.config.node_timeout;
        let local_node_id = self.local_node_id.clone();
        
        // ハートビート送信タスク
        let heartbeat_nodes = nodes.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let nodes_read = heartbeat_nodes.read().await;
                        if let Some(local_node) = nodes_read.get(&local_node_id) {
                            let message = HeartbeatMessage {
                                from_node: local_node_id.to_string(),
                                timestamp: chrono::Utc::now().timestamp() as u64,
                                active_pipelines: local_node.active_pipelines,
                                status: local_node.status,
                                load: NodeLoad {
                                    cpu_usage: 0.0, // TODO: 実際の値を取得
                                    memory_usage: 0.0,
                                    disk_usage: 0.0,
                                    network_usage: 0.0,
                                },
                            };
                            
                            // ハートビートを他のノードにブロードキャスト
                            // TODO: 実際のブロードキャスト実装
                            debug!("ハートビート送信: ノード {}", local_node_id);
                        }
                    },
                    _ = shutdown_rx.recv() => {
                        info!("シャットダウン信号を受信、ハートビートタスクを終了します");
                        break;
                    }
                }
            }
        });
        
        // ハートビート受信タスク
        let heartbeat_nodes = nodes.clone();
        tokio::spawn(async move {
            while let Some(message) = heartbeat_rx.recv().await {
                let from_node_id = NodeId::from_string(message.from_node.clone());
                
                let mut nodes_write = heartbeat_nodes.write().await;
                if let Some(node) = nodes_write.get_mut(&from_node_id) {
                    // 既存ノードの状態を更新
                    node.status = message.status;
                    node.last_heartbeat = Instant::now();
                    node.active_pipelines = message.active_pipelines;
                    debug!("ハートビート受信: ノード {} (アクティブパイプライン: {})", 
                           from_node_id, message.active_pipelines);
                }
                // 注: 未知のノードからのハートビートは無視（検出時に追加）
            }
        });
        
        // ノードタイムアウト検出タスク
        let timeout_nodes = nodes.clone();
        tokio::spawn(async move {
            let check_interval = node_timeout / 2;
            let mut interval = tokio::time::interval(check_interval);
            
            loop {
                interval.tick().await;
                
                let mut nodes_write = timeout_nodes.write().await;
                let now = Instant::now();
                
                // タイムアウトしたノードを検出
                for (node_id, node) in nodes_write.iter_mut() {
                    if *node_id != local_node_id && node.status != NodeStatus::Offline {
                        let elapsed = now.duration_since(node.last_heartbeat);
                        if elapsed > node_timeout {
                            warn!("ノード {} がタイムアウトしました (最終ハートビートから {:.2}秒)", 
                                  node_id, elapsed.as_secs_f64());
                            node.status = NodeStatus::Offline;
                            
                            // オフラインノードの処理
                            // TODO: このノードに割り当てられていたタスクを再割り当て
                        }
                    }
                }
            }
        });
        
        // ノード検出タスク
        let discovery_nodes = nodes.clone();
        let discovery_service = discovery.clone();
        tokio::spawn(async move {
            let discovery_interval = Duration::from_secs(60); // 1分ごとに検出
            let mut interval = tokio::time::interval(discovery_interval);
            
            loop {
                interval.tick().await;
                
                match discovery_service.discover_nodes().await {
                    Ok(discovered_nodes) => {
                        let mut nodes_write = discovery_nodes.write().await;
                        
                        for discovered in discovered_nodes {
                            if discovered.id != local_node_id {
                                let is_new = !nodes_write.contains_key(&discovered.id);
                                
                                if is_new {
                                    info!("新しいノードを検出しました: {} ({})", discovered.name, discovered.id);
                                    nodes_write.insert(discovered.id.clone(), discovered);
                                }
                            }
                        }
                    },
                    Err(e) => {
                        error!("ノード検出中にエラーが発生しました: {}", e);
                    }
                }
            }
        });
    }
    
    /// ローカルノードの情報を取得
    pub async fn get_local_node(&self) -> Result<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.get(&self.local_node_id)
            .cloned()
            .ok_or_else(|| anyhow!("ローカルノード情報が見つかりません"))
    }
    
    /// ノード情報を更新
    pub async fn update_local_node_info(&self, status: NodeStatus, active_pipelines: u32) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        
        if let Some(node) = nodes.get_mut(&self.local_node_id) {
            node.status = status;
            node.active_pipelines = active_pipelines;
            node.last_heartbeat = Instant::now();
            
            // 検出サービスも更新
            self.discovery_service.update_node_status(&self.local_node_id, status).await?;
            
            Ok(())
        } else {
            Err(anyhow!("ローカルノード情報が見つかりません"))
        }
    }
    
    /// すべてのノード情報を取得
    pub async fn get_all_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values().cloned().collect()
    }
    
    /// オンラインノードを取得
    pub async fn get_online_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|n| n.status == NodeStatus::Online || n.status == NodeStatus::Idle)
            .cloned()
            .collect()
    }
    
    /// 分散パイプラインを実行
    pub async fn execute_distributed_pipeline(
        &self,
        pipeline: Arc<Pipeline>,
        context: PipelineContext,
        partition_count: u32,
    ) -> Result<PipelineResult> {
        info!("分散パイプライン {} を実行 (パーティション数: {})", context.pipeline_id, partition_count);
        
        // オンラインノードのチェック
        let online_nodes = self.get_online_nodes().await;
        if online_nodes.is_empty() {
            return Err(anyhow!("利用可能なノードがありません"));
        }
        
        // データをパーティション化
        let partitions = self.create_data_partitions(pipeline.clone(), partition_count).await?;
        
        // 各パーティションのタスクを作成
        let mut task_ids = Vec::new();
        
        for (i, partition) in partitions.iter().enumerate() {
            let task_id = Uuid::new_v4().to_string();
            
            let task = DistributedTask {
                id: task_id.clone(),
                pipeline_id: context.pipeline_id.clone(),
                stage_id: StageId::from_string(format!("stage-{}", i)), // TODO: 実際のステージID
                assigned_node: None, // スケジューラーが割り当て
                input_partition: Some(partition.clone()),
                status: TaskStatus::Created,
                start_time: None,
                end_time: None,
                error: None,
                retry_count: 0,
                priority: 50, // デフォルト優先度
            };
            
            // タスクを登録
            {
                let mut tasks = self.tasks.write().await;
                tasks.insert(task_id.clone(), task);
            }
            
            // タスクをキューに追加
            {
                let mut queue = self.task_queue.lock().await;
                queue.push(task_id.clone());
            }
            
            task_ids.push(task_id);
        }
        
        // すべてのタスクが完了するのを待機
        let result = self.wait_for_tasks_completion(task_ids, context.clone()).await?;
        
        info!("分散パイプライン {} の実行が完了しました", context.pipeline_id);
        
        Ok(result)
    }
    
    /// データパーティションを作成
    async fn create_data_partitions(
        &self,
        pipeline: Arc<Pipeline>,
        partition_count: u32,
    ) -> Result<Vec<DataPartition>> {
        // TODO: 実際のデータパーティション作成ロジック
        // 簡単な例として、空のパーティションを作成
        
        let mut partitions = Vec::new();
        
        for i in 0..partition_count {
            let partition = DataPartition {
                id: format!("partition-{}", i),
                data: DataType::Empty,
                range: Some(PartitionRange {
                    start_index: (i as u64) * 1000,
                    end_index: ((i + 1) as u64) * 1000 - 1,
                    key_range: None,
                }),
                metadata: HashMap::new(),
            };
            
            partitions.push(partition);
        }
        
        Ok(partitions)
    }
    
    /// すべてのタスクが完了するのを待機
    async fn wait_for_tasks_completion(
        &self,
        task_ids: Vec<String>,
        context: PipelineContext,
    ) -> Result<PipelineResult> {
        let start_time = Instant::now();
        
        // タスクの完了を監視
        let mut completed_count = 0;
        let total_tasks = task_ids.len();
        let mut failed_tasks = Vec::new();
        
        // タイムアウト設定
        let timeout_duration = context.timeout.unwrap_or(Duration::from_secs(3600));
        
        // 進捗報告間隔
        let progress_interval = Duration::from_secs(10);
        let mut last_progress_report = Instant::now();
        
        while completed_count < total_tasks {
            // タイムアウトチェック
            if start_time.elapsed() > timeout_duration {
                return Err(anyhow!("分散パイプライン実行がタイムアウトしました"));
            }
            
            // 進捗報告
            if last_progress_report.elapsed() > progress_interval {
                info!("分散パイプライン実行進捗: {}/{} タスク完了 ({:.1}%)",
                      completed_count, total_tasks, (completed_count as f64 / total_tasks as f64) * 100.0);
                last_progress_report = Instant::now();
            }
            
            // 各タスクの状態を確認
            let tasks = self.tasks.read().await;
            
            let mut new_completed = 0;
            
            for task_id in &task_ids {
                if let Some(task) = tasks.get(task_id) {
                    match task.status {
                        TaskStatus::Completed => {
                            new_completed += 1;
                        },
                        TaskStatus::Failed | TaskStatus::TimedOut => {
                            failed_tasks.push(task.clone());
                            new_completed += 1;
                        },
                        _ => {
                            // まだ実行中
                        }
                    }
                }
            }
            
            if new_completed > completed_count {
                completed_count = new_completed;
                debug!("現在 {} タスクが完了", completed_count);
            }
            
            // 少し待機
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // 結果を集約
        let end_time = Instant::now();
        
        // エラーがあるかチェック
        if !failed_tasks.is_empty() {
            let error_message = failed_tasks.first()
                .and_then(|t| t.error.clone())
                .unwrap_or_else(|| "不明なエラー".to_string());
            
            return Ok(PipelineResult {
                pipeline_id: context.pipeline_id.clone(),
                status: PipelineStatus::Failed,
                start_time,
                end_time,
                error: Some(error_message),
                output: None,
                stage_results: HashMap::new(),
                metrics: crate::pipeline_manager::PipelineMetrics::default(),
            });
        }
        
        // 成功した場合は結果を集約
        // TODO: 各タスクからの出力を適切に集約
        
        Ok(PipelineResult {
            pipeline_id: context.pipeline_id.clone(),
            status: PipelineStatus::Completed,
            start_time,
            end_time,
            error: None,
            output: None, // TODO: 集約した出力
            stage_results: HashMap::new(), // TODO: 集約したステージ結果
            metrics: crate::pipeline_manager::PipelineMetrics::default(),
        })
    }
    
    /// シャットダウン
    pub async fn shutdown(&self) -> Result<()> {
        info!("分散パイプラインマネージャーをシャットダウンしています...");
        
        // ノードの状態をオフラインに変更
        self.update_local_node_info(NodeStatus::Offline, 0).await?;
        
        // 検出サービスからノードを削除
        self.discovery_service.unregister_node(&self.local_node_id).await?;
        
        // シャットダウン信号を送信
        if let Err(e) = self.shutdown_tx.send(()).await {
            warn!("シャットダウン信号の送信に失敗: {}", e);
        }
        
        info!("分散パイプラインマネージャーのシャットダウンが完了しました");
        
        Ok(())
    }
    
    // ノード障害を処理する機能を追加
    async fn handle_node_failure(&self, node_id: &NodeId) -> Result<()> {
        warn!("ノード {} の障害を検出しました - フェイルオーバー開始", node_id);
        
        // ノードの状態を更新
        {
            let mut nodes = self.nodes.write().await;
            if let Some(node) = nodes.get_mut(node_id) {
                node.status = NodeStatus::Offline;
            }
        }
        
        // 障害ノードに割り当てられたタスクを特定
        let affected_tasks = {
            let tasks = self.tasks.read().await;
            tasks.values()
                .filter(|task| task.assigned_node.as_ref() == Some(node_id) && 
                       (task.status == TaskStatus::Assigned || task.status == TaskStatus::Running))
                .cloned()
                .collect::<Vec<_>>()
        };
        
        if affected_tasks.is_empty() {
            info!("ノード {} にはアクティブタスクがありません", node_id);
            return Ok(());
        }
        
        info!("ノード {} の障害により {} 件のタスクが影響を受けました", node_id, affected_tasks.len());
        
        // 有効なノードのリストを取得
        let active_nodes = self.get_online_nodes().await;
        
        // フェイルオーバーを実行
        let reassignments = self.failover_manager.handle_node_failure(
            node_id,
            &active_nodes,
            &affected_tasks
        ).await?;
        
        // タスクを再割り当て
        for (task_id, new_node_id) in reassignments {
            // タスクの状態を更新
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.assigned_node = Some(new_node_id.clone());
                task.status = TaskStatus::Queued;
                task.retry_count += 1;
                
                // タスクをキューに戻す
                let mut queue = self.task_queue.lock().await;
                queue.push(task_id.clone());
                
                info!("タスク {} をノード {} に再割り当てしました", task_id, new_node_id);
            }
        }
        
        Ok(())
    }
    
    // ノードタイムアウト検出タスクを強化
    fn start_node_monitoring(&self) {
        let timeout_nodes = self.nodes.clone();
        let local_node_id = self.local_node_id.clone();
        let node_timeout = self.config.node_timeout;
        let manager = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let check_interval = node_timeout / 2;
            let mut interval = tokio::time::interval(check_interval);
            
            loop {
                interval.tick().await;
                
                let mut nodes_to_failover = Vec::new();
                
                {
                    let nodes_read = timeout_nodes.read().await;
                    let now = Instant::now();
                    
                    // タイムアウトしたノードを検出
                    for (node_id, node) in nodes_read.iter() {
                        if *node_id != local_node_id && node.status != NodeStatus::Offline {
                            let elapsed = now.duration_since(node.last_heartbeat);
                            if elapsed > node_timeout {
                                warn!("ノード {} がタイムアウトしました (最終ハートビートから {:.2}秒)",
                                      node_id, elapsed.as_secs_f64());
                                nodes_to_failover.push(node_id.clone());
                            }
                        }
                    }
                }
                
                // 検出したノードのフェイルオーバーを実行
                for node_id in nodes_to_failover {
                    if let Err(e) = manager.handle_node_failure(&node_id).await {
                        error!("ノード {} のフェイルオーバー中にエラーが発生しました: {}", node_id, e);
                    }
                }
            }
        });
    }
    
    // チェックポイントを作成
    pub async fn create_task_checkpoint(&self, task_id: &str, progress: f32, data: Option<DataType>) -> Result<()> {
        let tasks = self.tasks.read().await;
        
        if let Some(task) = tasks.get(task_id) {
            self.failover_manager.create_checkpoint(task, progress, data).await?;
            debug!("タスク {} のチェックポイントを作成しました (進捗: {:.1}%)", task_id, progress * 100.0);
            Ok(())
        } else {
            Err(anyhow!("チェックポイントを作成できません: タスク {} が見つかりません", task_id))
        }
    }
    
    // チェックポイントから復元
    pub async fn restore_from_checkpoint(&self, task_id: &str) -> Result<Option<DataType>> {
        let checkpoint = self.failover_manager.get_checkpoint(task_id).await;
        
        if let Some(cp) = checkpoint {
            debug!("タスク {} をチェックポイント（進捗: {:.1}%）から復元します", 
                   task_id, cp.progress * 100.0);
            Ok(cp.intermediate_results)
        } else {
            debug!("タスク {} のチェックポイントが見つかりませんでした", task_id);
            Ok(None)
        }
    }
    
    // フェイルオーバー履歴を取得
    pub async fn get_failover_history(&self) -> Vec<FailoverEvent> {
        self.failover_manager.get_failover_history().await
    }
}

/// クラスタ健全性状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterHealthStatus {
    /// 正常
    Healthy,
    /// 警告
    Warning,
    /// 危険
    Critical,
    /// 分断
    Partitioned,
    /// 回復中
    Recovering,
}

/// クラスタの役割
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterRole {
    /// マスターノード
    Master,
    /// ワーカーノード
    Worker,
    /// バックアップマスター
    BackupMaster,
    /// モニタリングノード
    Monitor,
    /// ゲートウェイノード
    Gateway,
}

/// クラスタマネージャー
pub struct ClusterManager {
    /// クラスタ設定
    config: ClusterConfig,
    /// ノードレジストリ
    node_registry: Arc<RwLock<HashMap<NodeId, ClusterNodeInfo>>>,
    /// マスターノード選出エンジン
    election_engine: Arc<MasterElectionEngine>,
    /// クラスタトポロジ
    topology: Arc<RwLock<ClusterTopology>>,
    /// ハートビート監視
    heartbeat_monitor: Arc<HeartbeatMonitor>,
    /// クラスタパーティション検出
    partition_detector: Arc<PartitionDetector>,
    /// ノード検出トリガー
    discovery_trigger_tx: mpsc::Sender<DiscoveryTrigger>,
}

/// クラスタ設定
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// クラスタID
    pub cluster_id: String,
    /// クラスタシークレット
    pub cluster_secret: String,
    /// メンバーシップ確認インターバル
    pub membership_interval: Duration,
    /// ノード失敗検出タイムアウト
    pub failure_detection_timeout: Duration,
    /// マスター選出タイムアウト
    pub master_election_timeout: Duration,
    /// 最小クォーラムサイズ
    pub min_quorum_size: usize,
    /// 自動マスター選出を有効化
    pub enable_auto_election: bool,
    /// ノード間通信暗号化を有効化
    pub enable_encryption: bool,
    /// クラスタメタデータ
    pub metadata: HashMap<String, String>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            cluster_id: format!("nexus-cluster-{}", Uuid::new_v4()),
            cluster_secret: format!("{}", Uuid::new_v4()),
            membership_interval: Duration::from_secs(10),
            failure_detection_timeout: Duration::from_secs(30),
            master_election_timeout: Duration::from_secs(15),
            min_quorum_size: 2,
            enable_auto_election: true,
            enable_encryption: true,
            metadata: HashMap::new(),
        }
    }
}

/// クラスタノード情報
#[derive(Debug, Clone)]
pub struct ClusterNodeInfo {
    /// 基本ノード情報
    pub base_info: NodeInfo,
    /// クラスタ内の役割
    pub role: ClusterRole,
    /// 参加時間
    pub join_time: chrono::DateTime<chrono::Utc>,
    /// マスター選出の優先度
    pub election_priority: u8,
    /// メタデータ
    pub metadata: HashMap<String, String>,
    /// セキュリティ関連情報
    pub security_info: Option<NodeSecurityInfo>,
    /// ノード間の通信統計
    pub peer_stats: HashMap<NodeId, PeerCommunicationStats>,
}

/// ノードセキュリティ情報
#[derive(Debug, Clone)]
pub struct NodeSecurityInfo {
    /// 公開鍵
    pub public_key: String,
    /// 証明書のフィンガープリント
    pub cert_fingerprint: String,
    /// アクセストークン
    pub access_token: String,
    /// 最終認証時間
    pub last_authenticated: chrono::DateTime<chrono::Utc>,
}

/// ピア通信統計
#[derive(Debug, Clone)]
pub struct PeerCommunicationStats {
    /// 平均応答時間
    pub avg_response_time: Duration,
    /// 成功率
    pub success_rate: f64,
    /// 最終通信時間
    pub last_communication: Instant,
    /// 送信バイト数
    pub bytes_sent: u64,
    /// 受信バイト数
    pub bytes_received: u64,
}

/// クラスタトポロジー
#[derive(Debug, Clone)]
pub struct ClusterTopology {
    /// ノード間の接続グラフ
    connections: HashMap<NodeId, HashSet<NodeId>>,
    /// ネットワークの分断状態
    partitions: Vec<HashSet<NodeId>>,
    /// トポロジー更新時間
    last_updated: Instant,
    /// 健全性状態
    health_status: ClusterHealthStatus,
}

/// ノード検出トリガー
#[derive(Debug, Clone)]
enum DiscoveryTrigger {
    /// 定期実行
    Scheduled,
    /// 手動トリガー
    Manual,
    /// ノード喪失による
    NodeLost(NodeId),
    /// クォーラム不足による
    BelowQuorum,
}

/// マスター選出エンジン
#[derive(Debug)]
struct MasterElectionEngine {
    /// 選出アルゴリズム
    algorithm: ElectionAlgorithm,
    /// 現在のマスターノード
    current_master: RwLock<Option<NodeId>>,
    /// バックアップマスターノード
    backup_masters: RwLock<Vec<NodeId>>,
    /// 選出中フラグ
    election_in_progress: AtomicBool,
    /// 最終選出時間
    last_election_time: RwLock<Instant>,
    /// 選出タイムアウト
    election_timeout: Duration,
}

/// 選出アルゴリズム
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElectionAlgorithm {
    /// Bully アルゴリズム
    Bully,
    /// Raft コンセンサスベース
    RaftBased,
    /// 優先度ベース
    PriorityBased,
    /// ラウンドロビン
    RoundRobin,
}

/// ハートビートモニター
#[derive(Debug)]
struct HeartbeatMonitor {
    /// ハートビート間隔
    interval: Duration,
    /// ノードタイムアウト
    node_timeout: Duration,
    /// 最終ハートビート
    last_heartbeats: RwLock<HashMap<NodeId, Instant>>,
    /// 監視ステータス
    monitoring_status: AtomicBool,
}

/// パーティション検出
#[derive(Debug)]
struct PartitionDetector {
    /// 検出アルゴリズム
    algorithm: PartitionDetectionAlgorithm,
    /// 検出間隔
    detection_interval: Duration,
    /// 最終検出時間
    last_detection: RwLock<Instant>,
    /// 検出閾値
    threshold: f64,
}

/// パーティション検出アルゴリズム
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartitionDetectionAlgorithm {
    /// SWIM プロトコル
    SWIM,
    /// ゴシップベース
    GossipBased,
    /// クォーラムベース
    QuorumBased,
    /// Phi アクレーション検出
    PhiAccretion,
}

impl ClusterManager {
    /// 新しいクラスタマネージャーを作成
    pub fn new(config: ClusterConfig, local_node: NodeInfo) -> Result<Self> {
        let (discovery_trigger_tx, discovery_trigger_rx) = mpsc::channel(10);
        
        let election_engine = Arc::new(MasterElectionEngine {
            algorithm: ElectionAlgorithm::PriorityBased,
            current_master: RwLock::new(None),
            backup_masters: RwLock::new(Vec::new()),
            election_in_progress: AtomicBool::new(false),
            last_election_time: RwLock::new(Instant::now()),
            election_timeout: config.master_election_timeout,
        });
        
        let heartbeat_monitor = Arc::new(HeartbeatMonitor {
            interval: Duration::from_secs(5),
            node_timeout: config.failure_detection_timeout,
            last_heartbeats: RwLock::new(HashMap::new()),
            monitoring_status: AtomicBool::new(false),
        });
        
        let partition_detector = Arc::new(PartitionDetector {
            algorithm: PartitionDetectionAlgorithm::GossipBased,
            detection_interval: Duration::from_secs(30),
            last_detection: RwLock::new(Instant::now()),
            threshold: 0.8,
        });
        
        let topology = Arc::new(RwLock::new(ClusterTopology {
            connections: HashMap::new(),
            partitions: Vec::new(),
            last_updated: Instant::now(),
            health_status: ClusterHealthStatus::Healthy,
        }));
        
        // ローカルノードをクラスタノードとして登録
        let local_cluster_node = ClusterNodeInfo {
            base_info: local_node,
            role: ClusterRole::Worker, // 初期状態はワーカー
            join_time: chrono::Utc::now(),
            election_priority: 5, // デフォルト優先度
            metadata: HashMap::new(),
            security_info: None,
            peer_stats: HashMap::new(),
        };
        
        let node_registry = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut registry = node_registry.write().unwrap();
            registry.insert(local_cluster_node.base_info.id.clone(), local_cluster_node);
        }
        
        let manager = Self {
            config,
            node_registry,
            election_engine,
            topology,
            heartbeat_monitor,
            partition_detector,
            discovery_trigger_tx,
        };
        
        // バックグラウンドサービスを開始
        manager.start_background_services(discovery_trigger_rx);
        
        Ok(manager)
    }
    
    /// バックグラウンドサービスを開始
    fn start_background_services(&self, mut discovery_trigger_rx: mpsc::Receiver<DiscoveryTrigger>) {
        let node_registry = self.node_registry.clone();
        let topology = self.topology.clone();
        let election_engine = self.election_engine.clone();
        let heartbeat_monitor = self.heartbeat_monitor.clone();
        let config = self.config.clone();
        
        // ノード検出サービス
        tokio::spawn(async move {
            let mut discovery_interval = tokio::time::interval(config.membership_interval);
            
            loop {
                tokio::select! {
                    _ = discovery_interval.tick() => {
                        // 定期的なノード検出
                        Self::discover_nodes(node_registry.clone(), topology.clone()).await;
                    },
                    Some(trigger) = discovery_trigger_rx.recv() => {
                        // トリガーによるノード検出
                        match trigger {
                            DiscoveryTrigger::Manual | DiscoveryTrigger::BelowQuorum => {
                                Self::discover_nodes(node_registry.clone(), topology.clone()).await;
                            },
                            DiscoveryTrigger::NodeLost(node_id) => {
                                // ノード喪失による再検出
                                debug!("ノード {} が喪失したため、ノード検出を開始", node_id);
                                Self::discover_nodes(node_registry.clone(), topology.clone()).await;
                                
                                // マスターノードが喪失した場合は選出を開始
                                let current_master = election_engine.current_master.read().unwrap();
                                if current_master.as_ref() == Some(&node_id) {
                                    info!("マスターノードが喪失しました。新しいマスター選出を開始します");
                                    Self::start_master_election(election_engine.clone(), node_registry.clone()).await;
                                }
                            },
                            _ => {}
                        }
                    }
                }
            }
        });
        
        // ハートビート監視サービス
        let registry_for_heartbeat = self.node_registry.clone();
        let detection_timeout = self.config.failure_detection_timeout;
        let trigger_tx = self.discovery_trigger_tx.clone();
        let local_node_id = {
            let registry = self.node_registry.read().unwrap();
            registry.keys().next().unwrap().clone()
        };
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let mut nodes_to_remove = Vec::new();
                
                // タイムアウトしたノードをチェック
                {
                    let heartbeats = heartbeat_monitor.last_heartbeats.read().unwrap();
                    let registry = registry_for_heartbeat.read().unwrap();
                    
                    for (node_id, last_heartbeat) in heartbeats.iter() {
                        // 自分自身は除外
                        if *node_id == local_node_id {
                            continue;
                        }
                        
                        // タイムアウトをチェック
                        if now.duration_since(*last_heartbeat) > detection_timeout {
                            if let Some(node) = registry.get(node_id) {
                                warn!("ノード {} ({}) がタイムアウトしました", node.base_info.name, node_id);
                                nodes_to_remove.push(node_id.clone());
                            }
                        }
                    }
                }
                
                // タイムアウトしたノードを処理
                for node_id in nodes_to_remove {
                    // ノード検出をトリガー
                    let _ = trigger_tx.send(DiscoveryTrigger::NodeLost(node_id.clone())).await;
                    
                    // ハートビートリストから削除
                    {
                        let mut heartbeats = heartbeat_monitor.last_heartbeats.write().unwrap();
                        heartbeats.remove(&node_id);
                    }
                }
            }
        });
        
        // マスター選出監視サービス
        let election_engine_clone = self.election_engine.clone();
        let registry_for_election = self.node_registry.clone();
        let election_timeout = self.config.master_election_timeout;
        let auto_election = self.config.enable_auto_election;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                if !auto_election {
                    continue;
                }
                
                // マスターノードがあるか確認
                let need_election = {
                    let current_master = election_engine_clone.current_master.read().unwrap();
                    let registry = registry_for_election.read().unwrap();
                    
                    match &*current_master {
                        Some(master_id) => {
                            // マスターノードが登録されているか確認
                            !registry.contains_key(master_id)
                        },
                        None => {
                            // マスターノードがない場合
                            registry.len() >= config.min_quorum_size
                        }
                    }
                };
                
                // 選出が必要かつ選出中でなければ選出を開始
                if need_election && !election_engine_clone.election_in_progress.load(std::sync::atomic::Ordering::Relaxed) {
                    Self::start_master_election(election_engine_clone.clone(), registry_for_election.clone()).await;
                }
            }
        });
    }
    
    /// ノードを検出
    async fn discover_nodes(node_registry: Arc<RwLock<HashMap<NodeId, ClusterNodeInfo>>>, topology: Arc<RwLock<ClusterTopology>>) {
        debug!("クラスタノード検出を実行中...");
        
        // TODO: 実際のノード検出ロジックを実装
        // 例: マルチキャスト、固定ピアリスト、サービスレジストリなど
        
        let nodes_discovered = 0; // 仮の値
        
        if nodes_discovered > 0 {
            // トポロジーの更新
            let mut topo = topology.write().unwrap();
            topo.last_updated = Instant::now();
            
            // TODO: 接続グラフの更新
        }
    }
    
    /// マスター選出を開始
    async fn start_master_election(
        election_engine: Arc<MasterElectionEngine>,
        node_registry: Arc<RwLock<HashMap<NodeId, ClusterNodeInfo>>>
    ) {
        // 選出中フラグを設定
        election_engine.election_in_progress.store(true, std::sync::atomic::Ordering::Relaxed);
        
        match election_engine.algorithm {
            ElectionAlgorithm::PriorityBased => {
                // 優先度ベースの選出（最も優先度の高いノードがマスターになる）
                let mut selected_master = None;
                let mut highest_priority = 0;
                
                {
                    let registry = node_registry.read().unwrap();
                    
                    for (node_id, node_info) in registry.iter() {
                        if node_info.election_priority > highest_priority {
                            highest_priority = node_info.election_priority;
                            selected_master = Some(node_id.clone());
                        }
                    }
                }
                
                if let Some(master_id) = selected_master {
                    // マスターノードを設定
                    {
                        let mut current_master = election_engine.current_master.write().unwrap();
                        *current_master = Some(master_id.clone());
                    }
                    
                    // ノードの役割を更新
                    {
                        let mut registry = node_registry.write().unwrap();
                        
                        for (node_id, node_info) in registry.iter_mut() {
                            if *node_id == master_id {
                                node_info.role = ClusterRole::Master;
                            } else {
                                // 現在のマスターはワーカーに降格
                                if node_info.role == ClusterRole::Master {
                                    node_info.role = ClusterRole::Worker;
                                }
                            }
                        }
                    }
                    
                    info!("新しいマスターノードを選出しました: {}", master_id);
                }
            },
            // 他のアルゴリズムの実装
            _ => {
                // デフォルトは優先度ベース
                warn!("未対応の選出アルゴリズム: {:?}、優先度ベースを使用します", election_engine.algorithm);
            }
        }
        
        // 選出終了
        *election_engine.last_election_time.write().unwrap() = Instant::now();
        election_engine.election_in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    
    /// ノードをクラスタに参加させる
    pub async fn join_node(&self, node: NodeInfo) -> Result<()> {
        info!("ノード {} ({}) をクラスタに参加させます", node.name, node.id);
        
        // クラスタノード情報を作成
        let cluster_node = ClusterNodeInfo {
            base_info: node.clone(),
            role: ClusterRole::Worker,
            join_time: chrono::Utc::now(),
            election_priority: 5, // デフォルト優先度
            metadata: HashMap::new(),
            security_info: None,
            peer_stats: HashMap::new(),
        };
        
        // ノードレジストリに追加
        {
            let mut registry = self.node_registry.write().unwrap();
            registry.insert(node.id.clone(), cluster_node);
        }
        
        // ハートビートを初期化
        {
            let mut heartbeats = self.heartbeat_monitor.last_heartbeats.write().unwrap();
            heartbeats.insert(node.id.clone(), Instant::now());
        }
        
        // トポロジーを更新
        {
            let mut topology = self.topology.write().await;
            // ノードの接続情報を初期化
            topology.connections.entry(node.id.clone()).or_insert_with(HashSet::new);
            topology.last_updated = Instant::now();
        }
        
        Ok(())
    }
    
    /// ノードをクラスタから削除
    pub async fn remove_node(&self, node_id: &NodeId) -> Result<()> {
        info!("ノード {} をクラスタから削除します", node_id);
        
        // ノードレジストリから削除
        {
            let mut registry = self.node_registry.write().unwrap();
            registry.remove(node_id);
        }
        
        // ハートビートリストから削除
        {
            let mut heartbeats = self.heartbeat_monitor.last_heartbeats.write().unwrap();
            heartbeats.remove(node_id);
        }
        
        // トポロジーを更新
        {
            let mut topology = self.topology.write().await;
            // ノードの接続情報を削除
            topology.connections.remove(node_id);
            
            // このノードへの接続も削除
            for connections in topology.connections.values_mut() {
                connections.remove(node_id);
            }
            
            topology.last_updated = Instant::now();
        }
        
        // マスターノードだった場合は選出をトリガー
        {
            let current_master = self.election_engine.current_master.read().unwrap();
            if current_master.as_ref() == Some(node_id) {
                // マスターノードを無効化
                {
                    let mut master = self.election_engine.current_master.write().unwrap();
                    *master = None;
                }
                
                // マスター選出をトリガー
                let _ = self.discovery_trigger_tx.send(DiscoveryTrigger::NodeLost(node_id.clone())).await;
            }
        }
        
        Ok(())
    }
    
    /// クラスタの健全性を確認
    pub async fn check_cluster_health(&self) -> ClusterHealthStatus {
        // ノード登録数をチェック
        let node_count = {
            let registry = self.node_registry.read().unwrap();
            registry.len()
        };
        
        // クォーラムをチェック
        if node_count < self.config.min_quorum_size {
            return ClusterHealthStatus::Critical;
        }
        
        // マスターノードをチェック
        let has_master = {
            let master = self.election_engine.current_master.read().unwrap();
            master.is_some()
        };
        
        if !has_master {
            return ClusterHealthStatus::Warning;
        }
        
        // パーティション状態をチェック
        let is_partitioned = {
            let topology = self.topology.read().await;
            !topology.partitions.is_empty()
        };
        
        if is_partitioned {
            return ClusterHealthStatus::Partitioned;
        }
        
        // すべて正常
        ClusterHealthStatus::Healthy
    }
    
    /// 現在のマスターノードを取得
    pub fn get_master_node(&self) -> Option<NodeId> {
        let master = self.election_engine.current_master.read().unwrap();
        master.clone()
    }
    
    /// 手動でマスターノードを設定
    pub async fn set_master_node(&self, node_id: NodeId) -> Result<()> {
        // ノードが存在するか確認
        {
            let registry = self.node_registry.read().unwrap();
            if !registry.contains_key(&node_id) {
                return Err(anyhow!("ノード {} が見つかりません", node_id));
            }
        }
        
        // マスターノードを設定
        {
            let mut master = self.election_engine.current_master.write().unwrap();
            *master = Some(node_id.clone());
        }
        
        // ノードの役割を更新
        {
            let mut registry = self.node_registry.write().unwrap();
            
            for (id, node_info) in registry.iter_mut() {
                if *id == node_id {
                    node_info.role = ClusterRole::Master;
                } else if node_info.role == ClusterRole::Master {
                    // 現在のマスターはワーカーに降格
                    node_info.role = ClusterRole::Worker;
                }
            }
        }
        
        info!("マスターノードを手動で {} に設定しました", node_id);
        
        Ok(())
    }
    
    /// すべてのクラスタノードの情報を取得
    pub fn get_all_nodes(&self) -> Vec<ClusterNodeInfo> {
        let registry = self.node_registry.read().unwrap();
        registry.values().cloned().collect()
    }
    
    /// ハートビートを送信
    pub async fn send_heartbeat(&self, from_node: &NodeId, to_node: &NodeId) -> Result<()> {
        // TODO: 実際のネットワーク通信を実装
        debug!("ハートビート送信: {} -> {}", from_node, to_node);
        
        // 受信側のハートビートを更新（シミュレーション用）
        {
            let mut heartbeats = self.heartbeat_monitor.last_heartbeats.write().unwrap();
            heartbeats.insert(to_node.clone(), Instant::now());
        }
        
        Ok(())
    }
}

// DistributedPipelineManagerとの統合
impl DistributedPipelineManager {
    // ... existing methods ...
    
    /// 新しい分散パイプラインマネージャーを作成（クラスタ対応版）
    pub fn new_with_cluster(
        config: DistributedConfig,
        local_node_info: NodeInfo,
        ha_config: Option<HighAvailabilityConfig>,
        cluster_config: Option<ClusterConfig>,
    ) -> Result<Self> {
        let mut manager = Self::new(config, local_node_info.clone(), ha_config)?;
        
        // クラスタ設定がある場合はクラスタマネージャーを初期化
        if let Some(cluster_cfg) = cluster_config {
            let cluster_manager = ClusterManager::new(cluster_cfg, local_node_info)?;
            manager.cluster_manager = Some(Arc::new(cluster_manager));
        }
        
        Ok(manager)
    }
    
    /// クラスタの状態を取得
    pub async fn get_cluster_status(&self) -> Result<Option<ClusterHealthStatus>> {
        if let Some(cluster_manager) = &self.cluster_manager {
            let status = cluster_manager.check_cluster_health().await;
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }
    
    /// 分散パイプラインをクラスタで実行
    pub async fn execute_cluster_pipeline(
        &self,
        pipeline: Arc<Pipeline>,
        context: PipelineContext,
    ) -> Result<PipelineResult> {
        // クラスタマネージャーが初期化されているか確認
        let cluster_manager = match &self.cluster_manager {
            Some(cm) => cm,
            None => return Err(anyhow!("クラスタマネージャーが初期化されていません")),
        };
        
        // クラスタの健全性をチェック
        let health = cluster_manager.check_cluster_health().await;
        if health == ClusterHealthStatus::Critical {
            return Err(anyhow!("クラスタの健全性が致命的な状態です: {:?}", health));
        }
        
        info!("クラスタ上で分散パイプライン {} を実行します", context.pipeline_id);
        
        // 利用可能なノードを取得
        let available_nodes = cluster_manager.get_all_nodes();
        if available_nodes.is_empty() {
            return Err(anyhow!("利用可能なクラスタノードがありません"));
        }
        
        // 実行するノード数を決定（最低2ノード、最大は利用可能なノード数）
        let node_count = available_nodes.len().min(10).max(2);
        
        // 標準の分散実行を使用
        self.execute_distributed_pipeline(pipeline, context, node_count as u32).await
    }
    
    /// クラスタにノードを参加させる
    pub async fn join_cluster(&self, node_info: NodeInfo) -> Result<()> {
        if let Some(cluster_manager) = &self.cluster_manager {
            cluster_manager.join_node(node_info).await?;
            Ok(())
        } else {
            Err(anyhow!("クラスタマネージャーが初期化されていません"))
        }
    }
    
    /// クラスタからノードを削除
    pub async fn leave_cluster(&self, node_id: &NodeId) -> Result<()> {
        if let Some(cluster_manager) = &self.cluster_manager {
            cluster_manager.remove_node(node_id).await?;
            Ok(())
        } else {
            Err(anyhow!("クラスタマネージャーが初期化されていません"))
        }
    }
    
    /// マスターノードを取得
    pub fn get_cluster_master(&self) -> Option<NodeId> {
        self.cluster_manager.as_ref().and_then(|cm| cm.get_master_node())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_node_id() {
        let id1 = NodeId::new();
        let id2 = NodeId::new();
        assert_ne!(id1, id2);
        
        let id_str = "test-node";
        let id3 = NodeId::from_string(id_str.to_string());
        assert_eq!(id3.as_str(), id_str);
    }
    
    // 他のテストも追加...
} 