/**
 * 分散タスクモジュール
 * 
 * 分散パイプライン実行におけるタスク管理を担当するモジュール
 */

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use crate::pipeline_manager::PipelineId;
use crate::pipeline_manager::stages::{StageId, DataType};
use super::node::NodeId;
use super::DataPartition;

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

impl TaskStatus {
    /// タスクが終了状態かどうか
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Completed | Self::Failed | Self::TimedOut | Self::Cancelled => true,
            _ => false,
        }
    }
    
    /// タスクが実行中かどうか
    pub fn is_active(&self) -> bool {
        match self {
            Self::Running | Self::Assigned => true,
            _ => false,
        }
    }
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
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl DistributedTask {
    /// 新しい分散タスクを作成
    pub fn new(pipeline_id: PipelineId, stage_id: StageId) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            pipeline_id,
            stage_id,
            assigned_node: None,
            input_partition: None,
            status: TaskStatus::Created,
            start_time: None,
            end_time: None,
            error: None,
            retry_count: 0,
            priority: 50,
            metadata: HashMap::new(),
        }
    }
    
    /// タスクをキューに入れる
    pub fn queue(&mut self) {
        self.status = TaskStatus::Queued;
    }
    
    /// タスクをノードに割り当てる
    pub fn assign(&mut self, node_id: NodeId) {
        self.assigned_node = Some(node_id);
        self.status = TaskStatus::Assigned;
    }
    
    /// タスクの実行を開始
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
        self.start_time = Some(Instant::now());
    }
    
    /// タスクを完了
    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
        self.end_time = Some(Instant::now());
    }
    
    /// タスクを失敗
    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.error = Some(error);
        self.end_time = Some(Instant::now());
    }
    
    /// タスクのタイムアウト
    pub fn timeout(&mut self) {
        self.status = TaskStatus::TimedOut;
        self.error = Some("タスクがタイムアウトしました".to_string());
        self.end_time = Some(Instant::now());
    }
    
    /// タスクをキャンセル
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.end_time = Some(Instant::now());
    }
    
    /// タスクを再試行
    pub fn retry(&mut self) {
        self.status = TaskStatus::Queued;
        self.start_time = None;
        self.end_time = None;
        self.error = None;
        self.retry_count += 1;
    }
    
    /// 実行時間を取得
    pub fn execution_time(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            (Some(start), None) => Some(Instant::now().duration_since(start)),
            _ => None,
        }
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

/// タスク実行結果
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

impl TaskResult {
    /// 成功の結果を作成
    pub fn success(task_id: String, output: DataType, execution_time: Duration) -> Self {
        Self {
            task_id,
            output: Some(output),
            execution_time,
            error: None,
            metrics: HashMap::new(),
        }
    }
    
    /// 失敗の結果を作成
    pub fn failure(task_id: String, error: String, execution_time: Duration) -> Self {
        Self {
            task_id,
            output: None,
            execution_time,
            error: Some(error),
            metrics: HashMap::new(),
        }
    }
    
    /// メトリクスを追加
    pub fn add_metric(&mut self, key: &str, value: f64) {
        self.metrics.insert(key.to_string(), value);
    }
    
    /// 成功したかどうか
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// タスクチェックポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    /// タスクID
    pub task_id: String,
    /// チェックポイント時間
    pub timestamp: DateTime<Utc>,
    /// 進捗率 (0.0-1.0)
    pub progress: f32,
    /// 中間結果
    pub intermediate_results: Option<DataType>,
    /// 実行状態
    pub execution_state: Vec<u8>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl TaskCheckpoint {
    /// 新しいチェックポイントを作成
    pub fn new(task_id: String, progress: f32, intermediate_results: Option<DataType>) -> Self {
        Self {
            task_id,
            timestamp: Utc::now(),
            progress,
            intermediate_results,
            execution_state: Vec::new(),
            metadata: HashMap::new(),
        }
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

/// タスクマネージャー
pub struct TaskManager {
    /// タスクレジストリ
    tasks: Arc<RwLock<HashMap<String, DistributedTask>>>,
    /// タスク結果レジストリ
    results: Arc<RwLock<HashMap<String, TaskResult>>>,
    /// チェックポイントレジストリ
    checkpoints: Arc<RwLock<HashMap<String, TaskCheckpoint>>>,
    /// タスクキュー
    task_queue: Arc<Mutex<Vec<String>>>,
}

impl TaskManager {
    /// 新しいタスクマネージャーを作成
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// タスクを登録
    pub async fn register_task(&self, task: DistributedTask) -> Result<()> {
        let task_id = task.id.clone();
        
        // タスクを登録
        {
            let mut tasks = self.tasks.write().await;
            if tasks.contains_key(&task_id) {
                return Err(anyhow!("タスクID {} は既に存在します", task_id));
            }
            
            tasks.insert(task_id.clone(), task);
        }
        
        // キューにタスクを追加
        {
            let mut queue = self.task_queue.lock().await;
            queue.push(task_id.clone());
        }
        
        debug!("タスク {} を登録しました", task_id);
        Ok(())
    }
    
    /// 次のタスクを取得
    pub async fn get_next_task(&self) -> Option<DistributedTask> {
        let mut queue = self.task_queue.lock().await;
        
        if queue.is_empty() {
            return None;
        }
        
        let task_id = queue.remove(0);
        let tasks = self.tasks.read().await;
        tasks.get(&task_id).cloned()
    }
    
    /// タスクを取得
    pub async fn get_task(&self, task_id: &str) -> Option<DistributedTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }
    
    /// タスクの状態を更新
    pub async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        
        if let Some(task) = tasks.get_mut(task_id) {
            match status {
                TaskStatus::Queued => task.queue(),
                TaskStatus::Assigned => {
                    return Err(anyhow!("ノードIDが必要です"));
                },
                TaskStatus::Running => task.start(),
                TaskStatus::Completed => task.complete(),
                TaskStatus::Failed => {
                    return Err(anyhow!("エラーメッセージが必要です"));
                },
                TaskStatus::TimedOut => task.timeout(),
                TaskStatus::Cancelled => task.cancel(),
                _ => {
                    task.status = status;
                }
            }
            
            debug!("タスク {} の状態を更新: {:?}", task_id, status);
            Ok(())
        } else {
            Err(anyhow!("タスクID {} は存在しません", task_id))
        }
    }
    
    /// タスクをノードに割り当て
    pub async fn assign_task(&self, task_id: &str, node_id: NodeId) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.assign(node_id);
            debug!("タスク {} をノード {} に割り当てました", task_id, node_id);
            Ok(())
        } else {
            Err(anyhow!("タスクID {} は存在しません", task_id))
        }
    }
    
    /// タスクを失敗状態に設定
    pub async fn fail_task(&self, task_id: &str, error: String) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.fail(error);
            debug!("タスク {} が失敗しました", task_id);
            Ok(())
        } else {
            Err(anyhow!("タスクID {} は存在しません", task_id))
        }
    }
    
    /// タスク結果を保存
    pub async fn store_task_result(&self, result: TaskResult) -> Result<()> {
        let task_id = result.task_id.clone();
        
        // 結果を保存
        {
            let mut results = self.results.write().await;
            results.insert(task_id.clone(), result);
        }
        
        // タスクを完了状態に更新
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.complete();
            }
        }
        
        debug!("タスク {} の結果を保存しました", task_id);
        Ok(())
    }
    
    /// タスク結果を取得
    pub async fn get_task_result(&self, task_id: &str) -> Option<TaskResult> {
        let results = self.results.read().await;
        results.get(task_id).cloned()
    }
    
    /// チェックポイントを作成
    pub async fn create_checkpoint(&self, checkpoint: TaskCheckpoint) -> Result<()> {
        let task_id = checkpoint.task_id.clone();
        
        // タスクの存在確認
        {
            let tasks = self.tasks.read().await;
            if !tasks.contains_key(&task_id) {
                return Err(anyhow!("タスクID {} は存在しません", task_id));
            }
        }
        
        // チェックポイントを保存
        {
            let mut checkpoints = self.checkpoints.write().await;
            checkpoints.insert(task_id.clone(), checkpoint);
        }
        
        debug!("タスク {} のチェックポイントを作成しました", task_id);
        Ok(())
    }
    
    /// チェックポイントを取得
    pub async fn get_checkpoint(&self, task_id: &str) -> Option<TaskCheckpoint> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints.get(task_id).cloned()
    }
    
    /// タスクを再試行
    pub async fn retry_task(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        
        if let Some(task) = tasks.get_mut(task_id) {
            // 最大再試行回数を超えていないか確認
            if task.retry_count >= 3 {
                return Err(anyhow!("タスク {} の最大再試行回数を超えています", task_id));
            }
            
            task.retry();
            
            // キューに再追加
            let mut queue = self.task_queue.lock().await;
            queue.push(task_id.to_string());
            
            debug!("タスク {} を再試行します (試行回数: {})", task_id, task.retry_count);
            Ok(())
        } else {
            Err(anyhow!("タスクID {} は存在しません", task_id))
        }
    }
    
    /// パイプラインのタスクを取得
    pub async fn get_pipeline_tasks(&self, pipeline_id: &PipelineId) -> Vec<DistributedTask> {
        let tasks = self.tasks.read().await;
        tasks.values()
            .filter(|task| task.pipeline_id == *pipeline_id)
            .cloned()
            .collect()
    }
    
    /// タスクキュー内のタスク数を取得
    pub async fn queue_size(&self) -> usize {
        let queue = self.task_queue.lock().await;
        queue.len()
    }
    
    /// すべてのタスクを取得
    pub async fn get_all_tasks(&self) -> Vec<DistributedTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_task_status() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        
        assert!(TaskStatus::Running.is_active());
        assert!(!TaskStatus::Created.is_active());
        assert!(!TaskStatus::Completed.is_active());
    }
    
    #[test]
    fn test_distributed_task() {
        let pipeline_id = PipelineId::new();
        let stage_id = "stage-1".into();
        
        let mut task = DistributedTask::new(pipeline_id, stage_id);
        
        assert_eq!(task.status, TaskStatus::Created);
        
        task.queue();
        assert_eq!(task.status, TaskStatus::Queued);
        
        let node_id = NodeId::new();
        task.assign(node_id.clone());
        assert_eq!(task.status, TaskStatus::Assigned);
        assert_eq!(task.assigned_node, Some(node_id));
        
        task.start();
        assert_eq!(task.status, TaskStatus::Running);
        assert!(task.start_time.is_some());
        
        task.complete();
        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.end_time.is_some());
    }
} 