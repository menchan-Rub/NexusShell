use std::collections::HashSet;
use std::time::Instant;

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