/**
 * 分散フェイルオーバーモジュール
 * 
 * 分散パイプライン実行でのノード障害時のフェイルオーバーを担当するモジュール
 */

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use anyhow::{Result, Context};
use super::node::{Node, NodeState};
use super::task::{Task, TaskState};
use std::fmt;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use super::node::NodeId;
use super::task::{DistributedTask, TaskCheckpoint};

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

impl Default for HighAvailabilityConfig {
    fn default() -> Self {
        Self {
            failover_strategy: FailoverStrategy::ImmediateReassignment,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
            task_resumption: TaskResumptionStrategy::Restart,
            critical_task_criteria: None,
            node_quorum: 1,
        }
    }
}

/// フェイルオーバーイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    /// イベントID
    pub id: String,
    /// 失敗したノード
    pub failed_node: NodeId,
    /// 引き継いだノード
    pub takeover_node: Option<NodeId>,
    /// 影響を受けたタスク
    pub affected_tasks: Vec<String>,
    /// イベント発生時間
    pub timestamp: DateTime<Utc>,
    /// 復旧時間
    pub recovery_time: Option<Duration>,
    /// フェイルオーバー成功フラグ
    pub success: bool,
}

impl FailoverEvent {
    /// 新しいフェイルオーバーイベントを作成
    pub fn new(failed_node: NodeId) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            failed_node,
            takeover_node: None,
            affected_tasks: Vec::new(),
            timestamp: Utc::now(),
            recovery_time: None,
            success: false,
        }
    }
    
    /// 成功としてイベントを完了
    pub fn complete_success(&mut self, takeover_node: Option<NodeId>, affected_tasks: Vec<String>, recovery_time: Duration) {
        self.takeover_node = takeover_node;
        self.affected_tasks = affected_tasks;
        self.recovery_time = Some(recovery_time);
        self.success = true;
    }
    
    /// 失敗としてイベントを完了
    pub fn complete_failure(&mut self, affected_tasks: Vec<String>, recovery_time: Duration) {
        self.affected_tasks = affected_tasks;
        self.recovery_time = Some(recovery_time);
        self.success = false;
    }
}

/// ノード障害検出条件
#[derive(Debug, Clone)]
pub struct FailureDetectionCriteria {
    /// ハートビートタイムアウト
    pub heartbeat_timeout: Duration,
    /// 連続失敗回数
    pub consecutive_failures: u32,
    /// 失敗率しきい値 (0.0-1.0)
    pub failure_rate_threshold: f64,
    /// レスポンス時間しきい値
    pub response_time_threshold: Duration,
}

impl Default for FailureDetectionCriteria {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_secs(30),
            consecutive_failures: 3,
            failure_rate_threshold: 0.7,
            response_time_threshold: Duration::from_secs(5),
        }
    }
}

/// タスク引き継ぎ情報
#[derive(Debug, Clone)]
pub struct TaskTakeover {
    /// タスクID
    pub task_id: String,
    /// 元のノード
    pub original_node: NodeId,
    /// 新しいノード
    pub new_node: NodeId,
    /// 最後のチェックポイント
    pub last_checkpoint: Option<TaskCheckpoint>,
    /// 再試行回数
    pub retry_count: u32,
    /// タスク引き継ぎ状態
    pub status: TaskTakeoverStatus,
}

/// タスク引き継ぎ状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskTakeoverStatus {
    /// 待機中
    Pending,
    /// 進行中
    InProgress,
    /// 完了
    Completed,
    /// 失敗
    Failed(String),
}

/// フェイルオーバーマネージャー
pub struct FailoverManager {
    /// HA設定
    config: HighAvailabilityConfig,
    /// タスクチェックポイント
    checkpoints: Arc<RwLock<HashMap<String, TaskCheckpoint>>>,
    /// タスク再試行カウンター
    retry_counters: Arc<RwLock<HashMap<String, u32>>>,
    /// フェイルオーバーイベント履歴
    failover_history: Arc<Mutex<Vec<FailoverEvent>>>,
    /// 障害検出条件
    detection_criteria: FailureDetectionCriteria,
    /// タスク引き継ぎレジストリ
    takeovers: Arc<RwLock<HashMap<String, TaskTakeover>>>,
}

impl FailoverManager {
    /// 新しいフェイルオーバーマネージャーを作成
    pub fn new(config: HighAvailabilityConfig) -> Self {
        Self {
            config,
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            retry_counters: Arc::new(RwLock::new(HashMap::new())),
            failover_history: Arc::new(Mutex::new(Vec::new())),
            detection_criteria: FailureDetectionCriteria::default(),
            takeovers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 障害検出条件を設定
    pub fn set_detection_criteria(&mut self, criteria: FailureDetectionCriteria) {
        self.detection_criteria = criteria;
    }
    
    /// チェックポイントを作成
    pub async fn create_checkpoint(&self, checkpoint: TaskCheckpoint) -> Result<()> {
        let mut checkpoints = self.checkpoints.write().await;
        checkpoints.insert(checkpoint.task_id.clone(), checkpoint);
        debug!("タスク {} のチェックポイントを作成しました", checkpoint.task_id);
        Ok(())
    }
    
    /// チェックポイントを取得
    pub async fn get_checkpoint(&self, task_id: &str) -> Option<TaskCheckpoint> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints.get(task_id).cloned()
    }
    
    /// ノード障害を処理
    pub async fn handle_node_failure(
        &self,
        failed_node: &NodeId,
        active_nodes: &[NodeInfo],
        affected_tasks: &[DistributedTask],
    ) -> Result<HashMap<String, NodeId>> {
        let start_time = Instant::now();
        let node_id_str = failed_node.to_string();
        
        info!("ノード {} の障害を処理しています、影響を受けるタスク数: {}", 
              node_id_str, affected_tasks.len());
        
        // フェイルオーバーイベントを作成
        let mut event = FailoverEvent::new(failed_node.clone());
        
        // タスクの再割り当て結果
        let mut reassignments = HashMap::new();
        
        // 利用可能なノードをフィルタリング
        let available_nodes = self.filter_eligible_nodes(active_nodes, &self.config.failover_strategy);
        
        if available_nodes.is_empty() {
            error!("ノード {} の障害を処理できません：利用可能なノードがありません", node_id_str);
            
            let elapsed = start_time.elapsed();
            event.complete_failure(
                affected_tasks.iter().map(|t| t.id.clone()).collect(),
                elapsed
            );
            
            // イベント履歴に追加
            let mut history = self.failover_history.lock().await;
            history.push(event);
            
            return Err(anyhow!("ノード障害を処理できません：利用可能なノードがありません"));
        }
        
        let affected_task_ids: Vec<String> = affected_tasks.iter()
            .map(|task| task.id.clone())
            .collect();
        
        let mut successful_takeovers = Vec::new();
        
        // 各タスクを処理
        for task in affected_tasks {
            // タスクが再試行回数を超えていないか確認
            let retry_count = {
                let mut counters = self.retry_counters.write().await;
                let count = counters.entry(task.id.clone()).or_insert(0);
                *count += 1;
                *count
            };
            
            if retry_count > self.config.max_retries {
                warn!("タスク {} の最大再試行回数を超えました", task.id);
                continue;
            }
            
            // タスクの優先度に基づいて適切なノードを選択
            if let Some(replacement_node) = self.select_replacement_node(available_nodes.clone(), task) {
                // タスク引き継ぎを登録
                let task_id = task.id.clone();
                let original_node = failed_node.clone();
                let new_node = replacement_node.id.clone();
                
                // 最後のチェックポイントを取得
                let last_checkpoint = {
                    let checkpoints = self.checkpoints.read().await;
                    checkpoints.get(&task_id).cloned()
                };
                
                let takeover = TaskTakeover {
                    task_id: task_id.clone(),
                    original_node: original_node.clone(),
                    new_node: new_node.clone(),
                    last_checkpoint,
                    retry_count,
                    status: TaskTakeoverStatus::Pending,
                };
                
                // 引き継ぎを登録
                {
                    let mut takeovers = self.takeovers.write().await;
                    takeovers.insert(task_id.clone(), takeover);
                }
                
                debug!("タスク {} をノード {} に再割り当てします", task_id, new_node);
                reassignments.insert(task_id.clone(), new_node.clone());
                successful_takeovers.push(task_id);
            } else {
                warn!("タスク {} の再割り当て先ノードが見つかりません", task.id);
            }
        }
        
        let elapsed = start_time.elapsed();
        
        // フェイルオーバーイベントを完了
        if !successful_takeovers.is_empty() {
            event.complete_success(
                Some(reassignments.values().next().unwrap().clone()),
                successful_takeovers,
                elapsed
            );
        } else {
            event.complete_failure(affected_task_ids, elapsed);
        }
        
        // イベント履歴に追加
        {
            let mut history = self.failover_history.lock().await;
            history.push(event);
        }
        
        info!("ノード {} の障害処理が完了しました、{}タスクを再割り当て、所要時間: {:?}", 
              node_id_str, reassignments.len(), elapsed);
        
        Ok(reassignments)
    }
    
    /// 利用可能なノードをフィルタリング
    fn filter_eligible_nodes<'a>(
        &self,
        active_nodes: &'a [NodeInfo],
        strategy: &FailoverStrategy,
    ) -> Vec<&'a NodeInfo> {
        match strategy {
            FailoverStrategy::LimitedToNodes(allowed_nodes) => {
                active_nodes.iter()
                    .filter(|node| {
                        // ノードが利用可能かつ許可リストに含まれているか確認
                        node.is_available() && allowed_nodes.contains(&node.id)
                    })
                    .collect()
            },
            _ => {
                // デフォルトでは利用可能なすべてのノードを返す
                active_nodes.iter()
                    .filter(|node| node.is_available())
                    .collect()
            }
        }
    }
    
    /// タスクの代替ノードを選択
    fn select_replacement_node<'a>(
        &self,
        available_nodes: Vec<&'a NodeInfo>,
        task: &DistributedTask,
    ) -> Option<&'a NodeInfo> {
        // タスクの重要度に基づいてノードを選択
        let is_critical = match &self.config.critical_task_criteria {
            Some(criteria) => criteria(task),
            None => task.priority > 80, // デフォルトでは優先度が高いタスクを重要とみなす
        };
        
        // 重要なタスクの場合は、最も高性能なノードを選択
        if is_critical {
            available_nodes.iter()
                .max_by_key(|node| node.capabilities.available_cores)
                .copied()
        } else {
            // それ以外の場合は、最も負荷の低いノードを選択
            available_nodes.iter()
                .min_by_key(|node| node.active_pipelines)
                .copied()
        }
    }
    
    /// フェイルオーバーイベント履歴を取得
    pub async fn get_failover_history(&self) -> Vec<FailoverEvent> {
        let history = self.failover_history.lock().await;
        history.clone()
    }
    
    /// タスク引き継ぎ状態を更新
    pub async fn update_takeover_status(&self, task_id: &str, status: TaskTakeoverStatus) -> Result<()> {
        let mut takeovers = self.takeovers.write().await;
        
        if let Some(takeover) = takeovers.get_mut(task_id) {
            debug!("タスク {} の引き継ぎ状態を更新: {:?}", task_id, status);
            takeover.status = status;
            Ok(())
        } else {
            Err(anyhow!("タスク {} の引き継ぎ情報が見つかりません", task_id))
        }
    }
    
    /// 全てのタスク引き継ぎ情報を取得
    pub async fn get_all_takeovers(&self) -> Vec<TaskTakeover> {
        let takeovers = self.takeovers.read().await;
        takeovers.values().cloned().collect()
    }
    
    /// 特定のノードに関連するタスク引き継ぎ情報を取得
    pub async fn get_node_takeovers(&self, node_id: &NodeId) -> Vec<TaskTakeover> {
        let takeovers = self.takeovers.read().await;
        takeovers.values()
            .filter(|t| t.new_node == *node_id)
            .cloned()
            .collect()
    }
}

/// NodeInfoインポート用
use super::node::NodeInfo;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_failure_detection_criteria_default() {
        let criteria = FailureDetectionCriteria::default();
        assert_eq!(criteria.heartbeat_timeout, Duration::from_secs(30));
        assert_eq!(criteria.consecutive_failures, 3);
        assert_eq!(criteria.failure_rate_threshold, 0.7);
        assert_eq!(criteria.response_time_threshold, Duration::from_secs(5));
    }
    
    #[test]
    fn test_ha_config_default() {
        let config = HighAvailabilityConfig::default();
        assert_eq!(config.failover_strategy, FailoverStrategy::ImmediateReassignment);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay, Duration::from_secs(5));
        assert_eq!(config.task_resumption, TaskResumptionStrategy::Restart);
        assert_eq!(config.node_quorum, 1);
    }
    
    #[test]
    fn test_failover_event() {
        let node_id = NodeId::from_string("test-node".to_string());
        let mut event = FailoverEvent::new(node_id.clone());
        
        assert_eq!(event.failed_node, node_id);
        assert!(event.takeover_node.is_none());
        assert!(event.affected_tasks.is_empty());
        assert!(!event.success);
        
        let takeover_node = NodeId::from_string("takeover-node".to_string());
        let affected_tasks = vec!["task1".to_string(), "task2".to_string()];
        let recovery_time = Duration::from_secs(10);
        
        event.complete_success(Some(takeover_node.clone()), affected_tasks.clone(), recovery_time);
        
        assert_eq!(event.takeover_node, Some(takeover_node));
        assert_eq!(event.affected_tasks, affected_tasks);
        assert_eq!(event.recovery_time, Some(recovery_time));
        assert!(event.success);
    }
} 