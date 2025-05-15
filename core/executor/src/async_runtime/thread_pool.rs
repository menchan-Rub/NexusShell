use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use log::{debug, info, trace, warn};
use metrics::{counter, gauge, histogram};
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use tokio::sync::{Mutex, Semaphore};

use super::runtime_config::ThreadPoolStrategy;

/// スレッドプールのメトリクス情報
#[derive(Debug, Clone)]
pub struct ThreadPoolMetrics {
    /// アクティブなワーカー数
    pub active_workers: usize,
    /// 実行中のタスク数
    pub running_tasks: usize,
    /// キュー内のタスク数
    pub queued_tasks: usize,
    /// スレッドあたりの平均タスク数
    pub tasks_per_thread: f64,
    /// CPU使用率（0.0-1.0）
    pub cpu_usage: f64,
    /// 平均スレッド負荷（0.0-1.0）
    pub average_load: f64,
    /// 最大スレッド負荷（0.0-1.0）
    pub max_load: f64,
    /// 平均タスク実行時間（ミリ秒）
    pub avg_task_time_ms: f64,
    /// タスク実行時間の分散
    pub task_time_variance: f64,
    /// キュー遅延（ミリ秒）
    pub queue_delay_ms: f64,
    /// タイムスタンプ
    pub timestamp: u64,
    /// 最終更新からの経過時間（ミリ秒）
    pub age_ms: u64,
    /// スレッドごとの負荷
    pub thread_loads: HashMap<usize, f64>,
    /// スケジューリング戦略
    pub strategy: ThreadPoolStrategy,
}

impl Default for ThreadPoolMetrics {
    fn default() -> Self {
        Self {
            active_workers: 0,
            running_tasks: 0,
            queued_tasks: 0,
            tasks_per_thread: 0.0,
            cpu_usage: 0.0,
            average_load: 0.0,
            max_load: 0.0,
            avg_task_time_ms: 0.0,
            task_time_variance: 0.0,
            queue_delay_ms: 0.0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_millis() as u64,
            age_ms: 0,
            thread_loads: HashMap::new(),
            strategy: ThreadPoolStrategy::default(),
        }
    }
}

/// スレッドプール戦略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadPoolStrategy {
    /// 固定サイズ（変更なし）
    Fixed,
    /// 自動拡張（必要に応じて増加のみ）
    AutoExpand,
    /// 自動縮小（必要に応じて減少のみ）
    AutoShrink,
    /// 適応型（負荷に応じて増減）
    Adaptive,
}

impl Default for ThreadPoolStrategy {
    fn default() -> Self {
        Self::Adaptive
    }
}

/// スレッドプール設定
#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    /// 初期スレッド数
    pub initial_threads: usize,
    /// 最小スレッド数
    pub min_threads: usize,
    /// 最大スレッド数
    pub max_threads: usize,
    /// スケーリング戦略
    pub strategy: ThreadPoolStrategy,
    /// スケール間隔（ミリ秒）
    pub scale_interval_ms: u64,
    /// スレッドスタックサイズ（バイト）
    pub thread_stack_size: usize,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        let cpu_count = num_cpus::get();
        
        Self {
            initial_threads: cpu_count,
            min_threads: cpu_count / 2,
            max_threads: cpu_count * 4,
            strategy: ThreadPoolStrategy::Adaptive,
            scale_interval_ms: 10000, // 10秒
            thread_stack_size: 2 * 1024 * 1024, // 2MB
        }
    }
}

/// スレッドプール
pub struct ThreadPool {
    /// 現在のスレッド数
    current_threads: AtomicUsize,
    /// アクティブスレッド数
    active_threads: AtomicUsize,
    /// プール設定
    config: ThreadPoolConfig,
    /// 最後のスケール操作時刻
    last_scale: RwLock<Instant>,
    /// アクティブなワーカーリクエスト
    active_workers: Arc<Semaphore>,
    /// スレッド使用統計
    thread_stats: Arc<Mutex<ThreadStats>>,
    /// スケールロック
    scale_lock: Mutex<()>,
}

impl ThreadPool {
    /// 新しいスレッドプールを作成
    pub fn new(threads: usize, strategy: ThreadPoolStrategy) -> Self {
        let config = ThreadPoolConfig {
            initial_threads: threads,
            min_threads: threads.saturating_sub(threads / 4),
            max_threads: threads.saturating_add(threads),
            strategy,
            ..Default::default()
        };
        
        Self::with_config(config)
    }
    
    /// 設定を指定して新しいスレッドプールを作成
    pub fn with_config(config: ThreadPoolConfig) -> Self {
        let pool = Self {
            current_threads: AtomicUsize::new(config.initial_threads),
            active_threads: AtomicUsize::new(0),
            config: config.clone(),
            last_scale: RwLock::new(Instant::now()),
            active_workers: Arc::new(Semaphore::new(config.initial_threads)),
            thread_stats: Arc::new(Mutex::new(ThreadStats::new())),
            scale_lock: Mutex::new(()),
        };
        
        info!("スレッドプールを初期化しました (スレッド数: {}, 戦略: {:?})", 
             config.initial_threads, config.strategy);
        
        pool
    }
    
    /// ワーカースレッドを獲得
    pub async fn acquire_worker(&self) -> Option<WorkerGuard> {
        match self.active_workers.acquire().await {
            Ok(permit) => {
                self.active_threads.fetch_add(1, Ordering::SeqCst);
                let stats = self.thread_stats.clone();
                
                // 統計情報を更新
                tokio::spawn(async move {
                    let mut stats_guard = stats.lock().await;
                    stats_guard.increment_active_requests();
                });
                
                Some(WorkerGuard {
                    permit: Some(permit),
                    pool: self,
                })
            },
            Err(_) => None,
        }
    }
    
    /// 現在のスレッド数を取得
    pub fn thread_count(&self) -> usize {
        self.current_threads.load(Ordering::SeqCst)
    }
    
    /// 現在のアクティブスレッド数を取得
    pub fn active_thread_count(&self) -> usize {
        self.active_threads.load(Ordering::SeqCst)
    }
    
    /// 現在の負荷を取得（0.0-1.0）
    pub fn get_load(&self) -> f64 {
        let active = self.active_threads.load(Ordering::SeqCst) as f64;
        let total = self.current_threads.load(Ordering::SeqCst) as f64;
        
        if total == 0.0 {
            0.0
        } else {
            (active / total).min(1.0)
        }
    }
    
    /// スレッドプールをスケールアップ
    pub async fn scale_up(&self) -> bool {
        // スケーリング戦略がFixed以外かを確認
        if self.config.strategy == ThreadPoolStrategy::Fixed || 
           self.config.strategy == ThreadPoolStrategy::AutoShrink {
            return false;
        }
        
        // スケーリング操作をロック
        let _guard = self.scale_lock.lock().await;
        
        // 前回のスケーリングからの経過時間を確認
        let last_scale = {
            let guard = self.last_scale.read().await;
            *guard
        };
        
        let elapsed = last_scale.elapsed().as_millis() as u64;
        if elapsed < self.config.scale_interval_ms {
            // スケーリング間隔が経過していない
            return false;
        }
        
        // 現在のスレッド数と最大スレッド数を確認
        let current = self.current_threads.load(Ordering::SeqCst);
        if current >= self.config.max_threads {
            // 既に最大スレッド数に達している
            return false;
        }
        
        // 新しいスレッド数を計算
        let new_count = (current * 3 / 2).min(self.config.max_threads);
        let diff = new_count - current;
        
        if diff > 0 {
            // スレッド数を増加
            self.current_threads.store(new_count, Ordering::SeqCst);
            self.active_workers.add_permits(diff);
            
            // 最終スケール時刻を更新
            {
                let mut guard = self.last_scale.write().await;
                *guard = Instant::now();
            }
            
            info!("スレッドプールをスケールアップ: {} -> {}", current, new_count);
            true
        } else {
            false
        }
    }
    
    /// スレッドプールをスケールダウン
    pub async fn scale_down(&self) -> bool {
        // スケーリング戦略がFixed以外かを確認
        if self.config.strategy == ThreadPoolStrategy::Fixed || 
           self.config.strategy == ThreadPoolStrategy::AutoExpand {
            return false;
        }
        
        // スケーリング操作をロック
        let _guard = self.scale_lock.lock().await;
        
        // 前回のスケーリングからの経過時間を確認
        let last_scale = {
            let guard = self.last_scale.read().await;
            *guard
        };
        
        let elapsed = last_scale.elapsed().as_millis() as u64;
        if elapsed < self.config.scale_interval_ms {
            // スケーリング間隔が経過していない
            return false;
        }
        
        // 現在のスレッド数と最小スレッド数を確認
        let current = self.current_threads.load(Ordering::SeqCst);
        if current <= self.config.min_threads {
            // 既に最小スレッド数に達している
            return false;
        }
        
        // 新しいスレッド数を計算
        let new_count = (current * 2 / 3).max(self.config.min_threads);
        let diff = current - new_count;
        
        if diff > 0 {
            // スレッド数を減少
            // 注意: セマフォのパーミット数を減らす直接的な方法はない
            // 代わりに現在のカウントを更新し、超過分は使用されなくなるようにする
            self.current_threads.store(new_count, Ordering::SeqCst);
            
            // 最終スケール時刻を更新
            {
                let mut guard = self.last_scale.write().await;
                *guard = Instant::now();
            }
            
            info!("スレッドプールをスケールダウン: {} -> {}", current, new_count);
            true
        } else {
            false
        }
    }
    
    /// スケールアップ可能かどうかを確認
    pub fn can_scale_up(&self) -> bool {
        if self.config.strategy == ThreadPoolStrategy::Fixed || 
           self.config.strategy == ThreadPoolStrategy::AutoShrink {
            return false;
        }
        
        let current = self.current_threads.load(Ordering::SeqCst);
        current < self.config.max_threads
    }
    
    /// スケールダウン可能かどうかを確認
    pub fn can_scale_down(&self) -> bool {
        if self.config.strategy == ThreadPoolStrategy::Fixed || 
           self.config.strategy == ThreadPoolStrategy::AutoExpand {
            return false;
        }
        
        let current = self.current_threads.load(Ordering::SeqCst);
        current > self.config.min_threads
    }
    
    /// スレッド統計情報を取得
    pub async fn get_stats(&self) -> ThreadStats {
        let stats = self.thread_stats.lock().await;
        stats.clone()
    }
    
    /// アクティブタスク数を減少
    fn decrement_active(&self) {
        self.active_threads.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Clone for ThreadPool {
    fn clone(&self) -> Self {
        Self {
            current_threads: AtomicUsize::new(self.current_threads.load(Ordering::SeqCst)),
            active_threads: AtomicUsize::new(self.active_threads.load(Ordering::SeqCst)),
            config: self.config.clone(),
            last_scale: RwLock::new(*self.last_scale.blocking_read()),
            active_workers: self.active_workers.clone(),
            thread_stats: self.thread_stats.clone(),
            scale_lock: Mutex::new(()),
        }
    }
}

/// ワーカーガード
/// ワーカースレッドの使用をトラッキングするためのRAIIオブジェクト
pub struct WorkerGuard<'a> {
    /// セマフォパーミット
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    /// スレッドプールの参照
    pool: &'a ThreadPool,
}

impl<'a> Drop for WorkerGuard<'a> {
    fn drop(&mut self) {
        if self.permit.take().is_some() {
            // アクティブスレッド数を減少
            self.pool.decrement_active();
            
            // 統計情報を更新
            let stats = self.pool.thread_stats.clone();
            tokio::spawn(async move {
                let mut stats_guard = stats.lock().await;
                stats_guard.decrement_active_requests();
                stats_guard.increment_completed_requests();
            });
        }
    }
}

/// スレッド統計情報
#[derive(Debug, Clone)]
pub struct ThreadStats {
    /// 開始時刻
    start_time: Instant,
    /// 合計リクエスト数
    total_requests: u64,
    /// 完了したリクエスト数
    completed_requests: u64,
    /// アクティブリクエスト数
    active_requests: u64,
    /// ピーク時のリクエスト数
    peak_requests: u64,
    /// 直近のリクエスト時刻
    last_request_time: Option<Instant>,
    /// 平均処理時間（ミリ秒）
    avg_processing_time_ms: f64,
}

impl ThreadStats {
    /// 新しいスレッド統計情報を作成
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            total_requests: 0,
            completed_requests: 0,
            active_requests: 0,
            peak_requests: 0,
            last_request_time: None,
            avg_processing_time_ms: 0.0,
        }
    }
    
    /// 合計リクエスト数を取得
    pub fn total_requests(&self) -> u64 {
        self.total_requests
    }
    
    /// 完了したリクエスト数を取得
    pub fn completed_requests(&self) -> u64 {
        self.completed_requests
    }
    
    /// アクティブリクエスト数を取得
    pub fn active_requests(&self) -> u64 {
        self.active_requests
    }
    
    /// ピーク時のリクエスト数を取得
    pub fn peak_requests(&self) -> u64 {
        self.peak_requests
    }
    
    /// 平均処理時間を取得
    pub fn avg_processing_time_ms(&self) -> f64 {
        self.avg_processing_time_ms
    }
    
    /// 稼働時間を取得
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// アクティブリクエスト数をインクリメント
    pub fn increment_active_requests(&mut self) {
        self.active_requests += 1;
        self.total_requests += 1;
        
        // ピーク値を更新
        if self.active_requests > self.peak_requests {
            self.peak_requests = self.active_requests;
        }
        
        self.last_request_time = Some(Instant::now());
    }
    
    /// アクティブリクエスト数をデクリメント
    pub fn decrement_active_requests(&mut self) {
        if self.active_requests > 0 {
            self.active_requests -= 1;
        }
    }
    
    /// 完了したリクエスト数をインクリメント
    pub fn increment_completed_requests(&mut self) {
        self.completed_requests += 1;
        
        // 処理時間を更新
        if let Some(last_time) = self.last_request_time {
            let elapsed = last_time.elapsed().as_millis() as f64;
            
            // 指数移動平均で更新
            if self.avg_processing_time_ms == 0.0 {
                self.avg_processing_time_ms = elapsed;
            } else {
                self.avg_processing_time_ms = 
                    0.9 * self.avg_processing_time_ms + 0.1 * elapsed;
            }
        }
    }
} 