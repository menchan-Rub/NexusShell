use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use tokio::sync::RwLock;
use log::trace;
use std::collections::HashMap;
use uuid::Uuid;

use super::{ExecutionDomain, TaskPriority};

/// ランタイムイベント
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEvent {
    /// 初期化
    Initialized,
    /// 初期化失敗
    InitializationFailed,
    /// タスク生成失敗
    SpawnFailed,
    /// ブロッキングタスク生成失敗
    SpawnBlockingFailed,
    /// シャットダウン
    Shutdown,
    /// 高負荷
    HighLoad,
    /// スレッド数増加
    ThreadsIncreased,
    /// スレッド数減少
    ThreadsDecreased,
    /// タイムアウト
    Timeout,
    /// パニック発生
    Panic,
    /// リソース不足
    ResourceExhaustion,
}

/// ランタイム統計情報
/// 非同期ランタイムの動作統計を収集・保持します
pub struct RuntimeStats {
    /// 作成されたタスク総数
    spawned_tasks: AtomicU64,
    /// 完了したタスク総数
    completed_tasks: AtomicU64,
    /// 失敗したタスク総数
    failed_tasks: AtomicU64,
    /// スケジュールされたタスク総数
    scheduled_tasks: AtomicU64,
    /// タイムアウトしたタスク総数
    timed_out_tasks: AtomicU64,
    /// キャンセルされたタスク総数
    cancelled_tasks: AtomicU64,
    /// 実行中のタスク数
    active_tasks: AtomicU64,
    /// スレッドプールのスレッド数
    thread_count: AtomicU64,
    /// 現在のスレッド負荷（0.0-1.0）
    thread_load: Arc<RwLock<f64>>,
    /// タスク実行時間の統計（タスクID -> 実行時間ミリ秒）
    task_timings: Arc<RwLock<HashMap<Uuid, u64>>>,
    /// ドメインごとのタスク数
    domain_tasks: Arc<RwLock<HashMap<ExecutionDomain, u64>>>,
    /// 優先度ごとのタスク数
    priority_tasks: Arc<RwLock<HashMap<TaskPriority, u64>>>,
    /// 統計収集開始時刻
    start_time: Instant,
}

impl RuntimeStats {
    /// 新しいランタイム統計を作成
    pub fn new() -> Self {
        Self {
            spawned_tasks: AtomicU64::new(0),
            completed_tasks: AtomicU64::new(0),
            failed_tasks: AtomicU64::new(0),
            scheduled_tasks: AtomicU64::new(0),
            timed_out_tasks: AtomicU64::new(0),
            cancelled_tasks: AtomicU64::new(0),
            active_tasks: AtomicU64::new(0),
            thread_count: AtomicU64::new(0),
            thread_load: Arc::new(RwLock::new(0.0)),
            task_timings: Arc::new(RwLock::new(HashMap::new())),
            domain_tasks: Arc::new(RwLock::new(HashMap::new())),
            priority_tasks: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        }
    }
    
    /// 作成されたタスク数をインクリメント
    pub fn increment_spawned_tasks(&self) {
        self.spawned_tasks.fetch_add(1, Ordering::SeqCst);
        self.active_tasks.fetch_add(1, Ordering::SeqCst);
    }
    
    /// 完了したタスク数をインクリメント
    pub fn increment_completed_tasks(&self) {
        self.completed_tasks.fetch_add(1, Ordering::SeqCst);
        self.active_tasks.fetch_sub(1, Ordering::SeqCst);
    }
    
    /// 失敗したタスク数をインクリメント
    pub fn increment_failed_tasks(&self) {
        self.failed_tasks.fetch_add(1, Ordering::SeqCst);
        self.active_tasks.fetch_sub(1, Ordering::SeqCst);
    }
    
    /// スケジュールされたタスク数をインクリメント
    pub fn increment_scheduled_tasks(&self) {
        self.scheduled_tasks.fetch_add(1, Ordering::SeqCst);
    }
    
    /// タイムアウトしたタスク数をインクリメント
    pub fn increment_timed_out_tasks(&self) {
        self.timed_out_tasks.fetch_add(1, Ordering::SeqCst);
        self.active_tasks.fetch_sub(1, Ordering::SeqCst);
    }
    
    /// キャンセルされたタスク数をインクリメント
    pub fn increment_cancelled_tasks(&self) {
        self.cancelled_tasks.fetch_add(1, Ordering::SeqCst);
        self.active_tasks.fetch_sub(1, Ordering::SeqCst);
    }
    
    /// スレッド数を設定
    pub fn set_thread_count(&self, count: u64) {
        self.thread_count.store(count, Ordering::SeqCst);
    }
    
    /// スレッド負荷を更新
    pub fn update_thread_load(&self, load: f64) {
        let mut thread_load = self.thread_load.blocking_write();
        *thread_load = load;
    }
    
    /// タスクの開始を記録
    pub fn start_task(&self, task_id: Uuid, domain: ExecutionDomain, priority: TaskPriority) {
        // ドメインごとのカウントを更新
        {
            let mut domain_tasks = self.domain_tasks.blocking_write();
            let count = domain_tasks.entry(domain).or_insert(0);
            *count += 1;
        }
        
        // 優先度ごとのカウントを更新
        {
            let mut priority_tasks = self.priority_tasks.blocking_write();
            let count = priority_tasks.entry(priority).or_insert(0);
            *count += 1;
        }
    }
    
    /// タスクの完了を記録
    pub fn complete_task(&self, task_id: Uuid, execution_time_ms: u64) {
        // タスク実行時間を記録
        {
            let mut task_timings = self.task_timings.blocking_write();
            task_timings.insert(task_id, execution_time_ms);
            
            // 必要に応じて古いデータをクリーンアップ
            if task_timings.len() > 1000 {
                // 最新の1000件だけを保持するように整理
                let mut entries: Vec<(Uuid, u64)> = task_timings.drain().collect();
                entries.sort_by(|a, b| b.1.cmp(&a.1)); // 実行時間の降順でソート
                entries.truncate(1000);
                
                for (id, time) in entries {
                    task_timings.insert(id, time);
                }
            }
        }
        
        self.increment_completed_tasks();
    }
    
    /// 作成されたタスク総数を取得
    pub fn spawned_tasks(&self) -> u64 {
        self.spawned_tasks.load(Ordering::SeqCst)
    }
    
    /// 完了したタスク総数を取得
    pub fn completed_tasks(&self) -> u64 {
        self.completed_tasks.load(Ordering::SeqCst)
    }
    
    /// 失敗したタスク総数を取得
    pub fn failed_tasks(&self) -> u64 {
        self.failed_tasks.load(Ordering::SeqCst)
    }
    
    /// スケジュールされたタスク総数を取得
    pub fn scheduled_tasks(&self) -> u64 {
        self.scheduled_tasks.load(Ordering::SeqCst)
    }
    
    /// タイムアウトしたタスク総数を取得
    pub fn timed_out_tasks(&self) -> u64 {
        self.timed_out_tasks.load(Ordering::SeqCst)
    }
    
    /// キャンセルされたタスク総数を取得
    pub fn cancelled_tasks(&self) -> u64 {
        self.cancelled_tasks.load(Ordering::SeqCst)
    }
    
    /// 実行中のタスク数を取得
    pub fn active_tasks(&self) -> u64 {
        self.active_tasks.load(Ordering::SeqCst)
    }
    
    /// スレッド数を取得
    pub fn thread_count(&self) -> u64 {
        self.thread_count.load(Ordering::SeqCst)
    }
    
    /// 現在のスレッド負荷を取得
    pub fn thread_load(&self) -> f64 {
        *self.thread_load.blocking_read()
    }
    
    /// 平均タスク実行時間を取得
    pub fn average_task_time(&self) -> f64 {
        let timings = self.task_timings.blocking_read();
        
        if timings.is_empty() {
            return 0.0;
        }
        
        let sum: u64 = timings.values().sum();
        sum as f64 / timings.len() as f64
    }
    
    /// 最大タスク実行時間を取得
    pub fn max_task_time(&self) -> u64 {
        let timings = self.task_timings.blocking_read();
        
        if timings.is_empty() {
            return 0;
        }
        
        *timings.values().max().unwrap_or(&0)
    }
    
    /// 統計収集からの経過時間を取得
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// ドメインごとのタスク数を取得
    pub fn tasks_by_domain(&self) -> HashMap<ExecutionDomain, u64> {
        self.domain_tasks.blocking_read().clone()
    }
    
    /// 優先度ごとのタスク数を取得
    pub fn tasks_by_priority(&self) -> HashMap<TaskPriority, u64> {
        self.priority_tasks.blocking_read().clone()
    }
    
    /// すべての統計情報をリセット
    pub fn reset(&self) {
        self.spawned_tasks.store(0, Ordering::SeqCst);
        self.completed_tasks.store(0, Ordering::SeqCst);
        self.failed_tasks.store(0, Ordering::SeqCst);
        self.scheduled_tasks.store(0, Ordering::SeqCst);
        self.timed_out_tasks.store(0, Ordering::SeqCst);
        self.cancelled_tasks.store(0, Ordering::SeqCst);
        self.active_tasks.store(0, Ordering::SeqCst);
        
        {
            let mut task_timings = self.task_timings.blocking_write();
            task_timings.clear();
        }
        
        {
            let mut domain_tasks = self.domain_tasks.blocking_write();
            domain_tasks.clear();
        }
        
        {
            let mut priority_tasks = self.priority_tasks.blocking_write();
            priority_tasks.clear();
        }
    }
}

impl Default for RuntimeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// タスク統計情報
#[derive(Debug, Clone)]
pub struct TaskStats {
    /// タスクID
    pub task_id: Uuid,
    /// タスク名
    pub task_name: Option<String>,
    /// 実行ドメイン
    pub domain: ExecutionDomain,
    /// 優先度
    pub priority: TaskPriority,
    /// 開始時刻
    pub start_time: Instant,
    /// 終了時刻
    pub end_time: Option<Instant>,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: Option<u64>,
    /// 成功フラグ
    pub success: Option<bool>,
}

impl TaskStats {
    /// 新しいタスク統計を作成
    pub fn new(task_id: Uuid, domain: ExecutionDomain, priority: TaskPriority) -> Self {
        Self {
            task_id,
            task_name: None,
            domain,
            priority,
            start_time: Instant::now(),
            end_time: None,
            execution_time_ms: None,
            success: None,
        }
    }
    
    /// タスク名を設定
    pub fn with_name(mut self, name: &str) -> Self {
        self.task_name = Some(name.to_string());
        self
    }
    
    /// タスクの完了を記録
    pub fn complete(&mut self, success: bool) {
        let now = Instant::now();
        self.end_time = Some(now);
        self.execution_time_ms = Some(now.duration_since(self.start_time).as_millis() as u64);
        self.success = Some(success);
    }
    
    /// タスクが完了しているか確認
    pub fn is_completed(&self) -> bool {
        self.end_time.is_some()
    }
    
    /// タスクの実行時間を取得
    pub fn execution_time(&self) -> Option<Duration> {
        self.end_time.map(|end| end.duration_since(self.start_time))
    }
} 