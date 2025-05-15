use std::collections::HashSet;
use std::time::Instant;
use super::node::NodeCapability;

/// タスク優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// 低優先度
    Low = 0,
    /// 通常優先度
    Normal = 1,
    /// 高優先度
    High = 2,
    /// 最高優先度（割り込み）
    Critical = 3,
}

/// タスク状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 待機中
    Pending,
    /// 割り当て済み
    Assigned,
    /// 実行中
    Running,
    /// 完了
    Completed,
    /// 失敗
    Failed,
    /// キャンセル
    Canceled,
}

/// 分散実行タスク
#[derive(Debug, Clone)]
pub struct Task {
    /// タスクID
    pub id: String,
    /// タスク優先度
    pub priority: TaskPriority,
    /// 必要なノード能力
    pub required_capabilities: HashSet<NodeCapability>,
    /// タスク状態
    pub state: TaskState,
    /// 作成時刻
    pub created_at: Instant,
}

impl Task {
    /// 新しいタスクを作成
    pub fn new(id: String, priority: TaskPriority, required_capabilities: HashSet<NodeCapability>) -> Self {
        Self {
            id,
            priority,
            required_capabilities,
            state: TaskState::Pending,
            created_at: Instant::now(),
        }
    }
    
    /// タスク状態を更新
    pub fn set_state(&mut self, state: TaskState) {
        self.state = state;
    }
    
    /// タスクが期限切れかチェック
    pub fn is_expired(&self, timeout: std::time::Duration) -> bool {
        self.state == TaskState::Pending && 
        self.created_at.elapsed() > timeout
    }
    
    /// タスクが長時間実行中かチェック
    pub fn is_long_running(&self, threshold: std::time::Duration) -> bool {
        self.state == TaskState::Running && 
        self.created_at.elapsed() > threshold
    }
} 