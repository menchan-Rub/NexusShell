use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::RwLock;
use metrics::{counter, gauge, histogram};
use log::trace;
use uuid::Uuid;

use super::ExecutionDomain;
use super::TaskPriority;

/// メトリクスイベントの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricEventType {
    /// タスク作成
    TaskCreated,
    /// タスク開始
    TaskStarted,
    /// タスク完了
    TaskCompleted,
    /// タスク失敗
    TaskFailed,
    /// タスクキャンセル
    TaskCancelled,
    /// タスクタイムアウト
    TaskTimedOut,
    /// スレッドプール調整
    ThreadPoolAdjusted,
    /// メモリ使用量変更
    MemoryUsageChanged,
    /// CPU使用率変更
    CpuUsageChanged,
    /// カスタムメトリクス
    Custom(String),
}

impl std::fmt::Display for MetricEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TaskCreated => write!(f, "タスク作成"),
            Self::TaskStarted => write!(f, "タスク開始"),
            Self::TaskCompleted => write!(f, "タスク完了"),
            Self::TaskFailed => write!(f, "タスク失敗"),
            Self::TaskCancelled => write!(f, "タスクキャンセル"),
            Self::TaskTimedOut => write!(f, "タスクタイムアウト"),
            Self::ThreadPoolAdjusted => write!(f, "スレッドプール調整"),
            Self::MemoryUsageChanged => write!(f, "メモリ使用量変更"),
            Self::CpuUsageChanged => write!(f, "CPU使用率変更"),
            Self::Custom(name) => write!(f, "カスタム({})", name),
        }
    }
}

/// メトリクスイベント
#[derive(Debug, Clone)]
pub struct MetricEvent {
    /// イベント種類
    pub event_type: MetricEventType,
    /// イベント発生時刻
    pub timestamp: Instant,
    /// 関連するドメイン
    pub domain: Option<ExecutionDomain>,
    /// 関連するタスクID
    pub task_id: Option<Uuid>,
    /// 数値メトリクス
    pub values: HashMap<String, f64>,
    /// ラベル
    pub labels: HashMap<String, String>,
}

impl MetricEvent {
    /// 新しいメトリクスイベントを作成
    pub fn new(event_type: MetricEventType) -> Self {
        Self {
            event_type,
            timestamp: Instant::now(),
            domain: None,
            task_id: None,
            values: HashMap::new(),
            labels: HashMap::new(),
        }
    }
    
    /// ドメインを設定
    pub fn with_domain(mut self, domain: ExecutionDomain) -> Self {
        self.domain = Some(domain);
        self
    }
    
    /// タスクIDを設定
    pub fn with_task_id(mut self, task_id: Uuid) -> Self {
        self.task_id = Some(task_id);
        self
    }
    
    /// 数値メトリクスを追加
    pub fn with_value(mut self, key: &str, value: f64) -> Self {
        self.values.insert(key.to_string(), value);
        self
    }
    
    /// ラベルを追加
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }
    
    /// イベント発生からの経過時間を取得
    pub fn elapsed(&self) -> Duration {
        self.timestamp.elapsed()
    }
}

/// パフォーマンスメトリクス
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// 作成されたタスク数
    pub tasks_created: u64,
    /// 完了したタスク数
    pub tasks_completed: u64,
    /// 失敗したタスク数
    pub tasks_failed: u64,
    /// キャンセルされたタスク数
    pub tasks_cancelled: u64,
    /// タイムアウトしたタスク数
    pub tasks_timed_out: u64,
    /// 現在実行中のタスク数
    pub tasks_running: u64,
    /// 平均タスク実行時間（ミリ秒）
    pub avg_task_execution_time_ms: f64,
    /// スレッドプール使用率（0-1）
    pub thread_pool_utilization: f64,
    /// メモリ使用量（バイト）
    pub memory_usage_bytes: u64,
    /// CPU使用率（0-1）
    pub cpu_utilization: f64,
    /// ドメインごとのタスク数
    pub tasks_by_domain: HashMap<ExecutionDomain, u64>,
    /// 優先度ごとのタスク数
    pub tasks_by_priority: HashMap<TaskPriority, u64>,
    /// カスタムメトリクス
    pub custom_metrics: HashMap<String, f64>,
}

/// メトリクスレポーター
/// 非同期ランタイムのメトリクスをPrometheusに報告する機能を提供します
pub struct MetricsReporter {
    /// 最新のパフォーマンスメトリクス
    current_metrics: RwLock<PerformanceMetrics>,
    /// 履歴イベント（最新のN個）
    event_history: RwLock<Vec<MetricEvent>>,
    /// 最大履歴サイズ
    max_history_size: usize,
    /// ランタイム名
    runtime_name: RwLock<String>,
    /// 開始時刻
    start_time: Instant,
}

impl MetricsReporter {
    /// 新しいメトリクスレポーターを作成
    pub fn new() -> Self {
        Self {
            current_metrics: RwLock::new(PerformanceMetrics::default()),
            event_history: RwLock::new(Vec::with_capacity(100)),
            max_history_size: 100,
            runtime_name: RwLock::new("unnamed-runtime".to_string()),
            start_time: Instant::now(),
        }
    }
    
    /// 最大履歴サイズを指定してメトリクスレポーターを作成
    pub fn with_history_size(max_history_size: usize) -> Self {
        Self {
            current_metrics: RwLock::new(PerformanceMetrics::default()),
            event_history: RwLock::new(Vec::with_capacity(max_history_size)),
            max_history_size,
            runtime_name: RwLock::new("unnamed-runtime".to_string()),
            start_time: Instant::now(),
        }
    }
    
    /// ランタイム名を設定
    pub fn set_runtime_name(&self, name: &str) {
        let mut runtime_name = self.runtime_name.blocking_write();
        *runtime_name = name.to_string();
    }
    
    /// メトリクスイベントを記録
    pub async fn record_event(&self, event: MetricEvent) {
        // イベントを履歴に追加
        {
            let mut history = self.event_history.write().await;
            history.push(event.clone());
            
            // 履歴サイズを制限
            if history.len() > self.max_history_size {
                history.remove(0);
            }
        }
        
        // メトリクスを更新
        let mut metrics = self.current_metrics.write().await;
        
        match event.event_type {
            MetricEventType::TaskCreated => {
                metrics.tasks_created += 1;
                metrics.tasks_running += 1;
                
                // ドメインごとのカウント更新
                if let Some(domain) = event.domain {
                    *metrics.tasks_by_domain.entry(domain).or_insert(0) += 1;
                }
                
                // 優先度ごとのカウント更新
                if let Some(priority) = event.labels.get("priority").and_then(|p| {
                    match p.as_str() {
                        "lowest" => Some(TaskPriority::Lowest),
                        "low" => Some(TaskPriority::Low),
                        "normal" => Some(TaskPriority::Normal),
                        "high" => Some(TaskPriority::High),
                        "highest" => Some(TaskPriority::Highest),
                        _ => None,
                    }
                }) {
                    *metrics.tasks_by_priority.entry(priority).or_insert(0) += 1;
                }
            },
            MetricEventType::TaskCompleted => {
                metrics.tasks_completed += 1;
                if metrics.tasks_running > 0 {
                    metrics.tasks_running -= 1;
                }
                
                // 実行時間の更新
                if let Some(execution_time) = event.values.get("execution_time_ms") {
                    // 指数移動平均で更新
                    if metrics.avg_task_execution_time_ms == 0.0 {
                        metrics.avg_task_execution_time_ms = *execution_time;
                    } else {
                        metrics.avg_task_execution_time_ms = 
                            0.9 * metrics.avg_task_execution_time_ms + 0.1 * execution_time;
                    }
                }
            },
            MetricEventType::TaskFailed => {
                metrics.tasks_failed += 1;
                if metrics.tasks_running > 0 {
                    metrics.tasks_running -= 1;
                }
            },
            MetricEventType::TaskCancelled => {
                metrics.tasks_cancelled += 1;
                if metrics.tasks_running > 0 {
                    metrics.tasks_running -= 1;
                }
            },
            MetricEventType::TaskTimedOut => {
                metrics.tasks_timed_out += 1;
                if metrics.tasks_running > 0 {
                    metrics.tasks_running -= 1;
                }
            },
            MetricEventType::ThreadPoolAdjusted => {
                // スレッドプール使用率の更新
                if let Some(utilization) = event.values.get("utilization") {
                    metrics.thread_pool_utilization = *utilization;
                }
            },
            MetricEventType::MemoryUsageChanged => {
                // メモリ使用量の更新
                if let Some(memory_bytes) = event.values.get("memory_bytes") {
                    metrics.memory_usage_bytes = *memory_bytes as u64;
                }
            },
            MetricEventType::CpuUsageChanged => {
                // CPU使用率の更新
                if let Some(cpu_usage) = event.values.get("cpu_usage") {
                    metrics.cpu_utilization = *cpu_usage;
                }
            },
            MetricEventType::Custom(ref name) => {
                // カスタムメトリクスの更新
                for (key, value) in &event.values {
                    metrics.custom_metrics.insert(format!("{}_{}", name, key), *value);
                }
            },
        }
    }
    
    /// 現在のメトリクスを取得
    pub async fn get_metrics(&self) -> PerformanceMetrics {
        self.current_metrics.read().await.clone()
    }
    
    /// イベント履歴を取得
    pub async fn get_event_history(&self) -> Vec<MetricEvent> {
        self.event_history.read().await.clone()
    }
    
    /// ランタイム名を取得
    pub async fn get_runtime_name(&self) -> String {
        self.runtime_name.read().await.clone()
    }
    
    /// ランタイム稼働時間を取得
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for MetricsReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MetricsReporter {
    fn clone(&self) -> Self {
        // ランタイム内で共有することを想定しているため、クローンは新しいインスタンスを返す
        Self::new()
    }
} 