use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use anyhow::{Result, Context};
use super::node::{Node, NodeState};
use super::task::{Task, TaskState};

/// フェイルオーバー戦略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverStrategy {
    /// 即時再実行
    Immediate,
    /// 遅延再実行（バックオフ付き）
    Delayed,
    /// 最適ノード選択
    Optimal,
    /// 手動介入
    Manual,
}

/// フェイルオーバー管理
pub struct FailoverManager {
    /// フェイルオーバー戦略
    strategy: FailoverStrategy,
    /// 最大再試行回数
    max_retries: usize,
    /// 再試行待機時間 (ミリ秒)
    retry_delay_ms: u64,
    /// 再試行履歴
    retry_history: HashMap<String, usize>,
}

impl FailoverManager {
    /// 新しいフェイルオーバーマネージャーを作成
    pub fn new() -> Self {
        Self {
            strategy: FailoverStrategy::Immediate,
            max_retries: 3,
            retry_delay_ms: 1000,
            retry_history: HashMap::new(),
        }
    }
    
    /// フェイルオーバー戦略を設定
    pub fn set_strategy(&mut self, strategy: FailoverStrategy) {
        self.strategy = strategy;
    }
    
    /// 最大再試行回数を設定
    pub fn set_max_retries(&mut self, max_retries: usize) {
        self.max_retries = max_retries;
    }
    
    /// 再試行待機時間を設定
    pub fn set_retry_delay(&mut self, delay_ms: u64) {
        self.retry_delay_ms = delay_ms;
    }
    
    /// ノード障害の処理
    pub async fn handle_node_failure(
        &mut self,
        node_id: &str,
        nodes: &Arc<RwLock<HashMap<String, Node>>>,
        running_tasks: &Arc<RwLock<HashMap<String, (String, Task)>>>,
        pending_tasks: &Arc<RwLock<Vec<Task>>>,
    ) -> Result<()> {
        // ノードの状態を更新
        {
            let mut nodes_guard = nodes.write().unwrap();
            if let Some(node) = nodes_guard.get_mut(node_id) {
                node.set_state(NodeState::Failed);
            }
        }
        
        // 失敗したノードで実行中だったタスクを再スケジュール
        let failed_tasks = {
            let running_guard = running_tasks.read().unwrap();
            running_guard.iter()
                .filter(|(_, (nid, _))| nid == node_id)
                .map(|(tid, (_, task))| (tid.clone(), task.clone()))
                .collect::<Vec<_>>()
        };
        
        let mut rescheduled = 0;
        
        for (task_id, mut task) in failed_tasks {
            // 再試行回数をチェック
            let retry_count = self.retry_history.entry(task_id.clone()).or_insert(0);
            *retry_count += 1;
            
            if *retry_count > self.max_retries {
                println!("Task {} exceeded maximum retry count", task_id);
                continue;
            }
            
            // 戦略に応じた処理
            match self.strategy {
                FailoverStrategy::Immediate => {
                    task.set_state(TaskState::Pending);
                    pending_tasks.write().unwrap().push(task);
                    rescheduled += 1;
                },
                FailoverStrategy::Delayed => {
                    // 再試行回数に応じて遅延を増加 (指数バックオフ)
                    let delay = self.retry_delay_ms * (1 << (*retry_count - 1));
                    
                    // 非同期で遅延後にタスクを再キュー
                    let task_clone = task.clone();
                    let pending_clone = Arc::clone(pending_tasks);
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        task_clone.set_state(TaskState::Pending);
                        pending_clone.write().unwrap().push(task_clone);
                    });
                    
                    rescheduled += 1;
                },
                FailoverStrategy::Optimal => {
                    // 最適なノードを見つける
                    if let Some(new_node_id) = self.find_optimal_node(&task, nodes).await {
                        // 直接新しいノードに割り当て
                        task.set_state(TaskState::Assigned);
                        
                        // 実行中タスクの更新
                        let mut running = running_tasks.write().unwrap();
                        running.remove(&task_id);
                        running.insert(task_id.clone(), (new_node_id, task));
                        
                        rescheduled += 1;
                    } else {
                        // 適切なノードがなければ保留キューに戻す
                        task.set_state(TaskState::Pending);
                        pending_tasks.write().unwrap().push(task);
                        rescheduled += 1;
                    }
                },
                FailoverStrategy::Manual => {
                    // 手動介入が必要なタスクを特別なキューに入れる処理
                    println!("Task {} requires manual intervention", task_id);
                    // 実際の実装ではログやアラートシステムに通知
                }
            }
        }
        
        println!("Rescheduled {} tasks from failed node {}", rescheduled, node_id);
        Ok(())
    }
    
    /// タスクに最適なノードを見つける
    async fn find_optimal_node(
        &self,
        task: &Task,
        nodes: &Arc<RwLock<HashMap<String, Node>>>,
    ) -> Option<String> {
        let nodes_guard = nodes.read().unwrap();
        
        nodes_guard.iter()
            .filter(|(_, node)| {
                node.state == NodeState::Available &&
                node.capabilities.is_superset(&task.required_capabilities)
            })
            .max_by(|(_, a), (_, b)| {
                a.health_score().partial_cmp(&b.health_score()).unwrap()
            })
            .map(|(id, _)| id.clone())
    }
    
    /// 再試行履歴をクリア
    pub fn clear_retry_history(&mut self, task_id: &str) {
        self.retry_history.remove(task_id);
    }
} 