/**
 * 非同期ランタイムモジュール
 *
 * Tokioベースの非同期実行環境を提供し、効率的なコマンド実行とジョブ管理を実現します。
 * 主な機能:
 * - 非同期タスクのライフサイクル管理
 * - 実行ドメインによるタスク優先度制御
 * - パフォーマンスメトリクス収集
 * - スマートなスレッドプール管理
 * - タイムアウト処理と中断可能なタスク
 */

mod error;
mod runtime_config;
mod metrics;
mod stats;
mod thread_pool;

pub use error::AsyncRuntimeError;
pub use runtime_config::{RuntimeConfig, ExecutionDomain, TaskPriority};
pub use metrics::{MetricsReporter, MetricEvent, PerformanceMetrics};
pub use stats::{RuntimeStats, TaskStats};
pub use thread_pool::{ThreadPool, ThreadPoolStrategy};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::{RwLock, Mutex, mpsc, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use futures::Future;
use tracing::{debug, info, warn, error, instrument, trace};
use uuid::Uuid;

/// ランタイムのタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeType {
    /// マルチスレッドランタイム
    MultiThread,
    /// 単一スレッドランタイム
    CurrentThread,
    /// アダプティブスレッドプール
    Adaptive,
    /// 専用I/Oワーカー付きランタイム
    DedicatedIo,
}

/// タスク設定
#[derive(Debug, Clone)]
pub struct TaskConfig {
    /// タスクの優先度
    pub priority: TaskPriority,
    /// 実行ドメイン
    pub domain: ExecutionDomain,
    /// タイムアウト
    pub timeout: Option<Duration>,
    /// 名前タグ（デバッグ用）
    pub name: Option<String>,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            priority: TaskPriority::Normal,
            domain: ExecutionDomain::Compute,
            timeout: None,
            name: None,
        }
    }
}

/// 非同期ランタイム
/// Tokioベースの非同期タスク実行環境を提供します
pub struct AsyncRuntime {
    /// 内部のランタイム
    runtime: Option<Runtime>,
    /// 設定
    config: RuntimeConfig,
    /// ランタイムの統計情報
    stats: Arc<RuntimeStats>,
    /// メトリクスレポーター
    metrics_reporter: Arc<MetricsReporter>,
    /// ドメインごとのアクティブタスク数
    active_tasks: Arc<RwLock<HashMap<ExecutionDomain, usize>>>,
    /// ドメインごとの同時実行制限
    concurrency_limits: Arc<RwLock<HashMap<ExecutionDomain, Arc<Semaphore>>>>,
    /// ランタイムの起動時刻
    start_time: Instant,
    /// シャットダウン通知チャンネル
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// ランタイム名
    name: String,
    /// スレッドプール管理
    thread_pool: Arc<ThreadPool>,
    /// ワーカースレッドの最大負荷（0.0-1.0）
    max_thread_load: Arc<RwLock<f64>>,
}

impl AsyncRuntime {
    /// 新しい非同期ランタイムを作成します
    pub fn new() -> Self {
        Self::with_config(RuntimeConfig::default())
    }

    /// 設定を指定して新しい非同期ランタイムを作成します
    pub fn with_config(config: RuntimeConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        // ランタイムを構築せずに初期化
        let mut runtime = Self {
            runtime: None,
            config: config.clone(),
            stats: Arc::new(RuntimeStats::new()),
            metrics_reporter: Arc::new(MetricsReporter::new()),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            concurrency_limits: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            shutdown_tx: Some(shutdown_tx),
            name: format!("nexusshell-runtime-{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
            thread_pool: Arc::new(ThreadPool::new(config.worker_threads(), config.thread_pool_strategy())),
            max_thread_load: Arc::new(RwLock::new(0.0)),
        };
        
        // デフォルトのドメイン同時実行制限を設定
        runtime.init_concurrency_limits();
        
        // メトリクスの初期化
        runtime.init_metrics();
        
        // Tokioランタイムを初期化
        runtime.init_tokio_runtime();
        
        // 負荷監視を開始
        runtime.start_load_monitor();
        
        // シャットダウン監視を開始
        runtime.start_shutdown_monitor(shutdown_rx);
        
        runtime
    }
    
    /// ランタイムに名前を設定します
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// ランタイムを初期化します
    fn init_tokio_runtime(&mut self) {
        // Tokioランタイムの設定
        let builder = Builder::new_multi_thread()
            .worker_threads(self.config.worker_threads())
            .thread_name(&self.name)
            .thread_stack_size(self.config.thread_stack_size())
            .enable_all();
            
        // ランタイムを構築
        match builder.build() {
            Ok(rt) => {
                debug!("Tokioランタイムを初期化しました: {}", self.name);
                self.runtime = Some(rt);
            },
            Err(e) => {
                error!("Tokioランタイムの初期化に失敗しました: {}", e);
                // フォールバック: シングルスレッドランタイム
                match Builder::new_current_thread().enable_all().build() {
                    Ok(rt) => {
                        warn!("フォールバック: シングルスレッドランタイムを使用します");
                        self.runtime = Some(rt);
                    },
                    Err(e) => {
                        error!("バックアップランタイムの初期化にも失敗しました: {}", e);
                        // ランタイムなしで続行 - スポーン操作は失敗します
                    }
                }
            }
        }
    }
    
    /// メトリクスを初期化します
    fn init_metrics(&self) {
        // メトリクスレポーターの設定
        self.metrics_reporter.set_runtime_name(&self.name);
        
        // 初期メトリクスを記録
        debug!("メトリクスレポーターを初期化しました: {}", self.name);
    }
    
    /// 制限を初期化します
    fn init_concurrency_limits(&self) {
        let mut limits = self.concurrency_limits.blocking_write();
        
        // デフォルトの制限を設定
        limits.insert(
            ExecutionDomain::Compute, 
            Arc::new(Semaphore::new(self.config.compute_concurrency()))
        );
        
        limits.insert(
            ExecutionDomain::IO, 
            Arc::new(Semaphore::new(self.config.io_concurrency()))
        );
        
        limits.insert(
            ExecutionDomain::Network, 
            Arc::new(Semaphore::new(self.config.network_concurrency()))
        );
        
        limits.insert(
            ExecutionDomain::Background, 
            Arc::new(Semaphore::new(self.config.background_concurrency()))
        );
        
        debug!("ドメイン実行制限を初期化しました: {}", self.name);
    }
    
    /// 負荷監視を開始します
    fn start_load_monitor(&self) {
        let stats = self.stats.clone();
        let max_load = self.max_thread_load.clone();
        let thread_pool = self.thread_pool.clone();
        let config = self.config.clone();
        let interval = self.config.metrics_interval();
        
        // バックグラウンドタスクとして負荷監視を開始
        if let Some(rt) = &self.runtime {
            rt.spawn(async move {
                loop {
                    tokio::time::sleep(interval).await;
                    
                    // 現在の負荷を計算
                    let current_load = thread_pool.get_load();
                    
                    // 最大負荷を更新
                    {
                        let mut load = max_load.write().await;
                        *load = current_load;
                    }
                    
                    // 統計を更新
                    stats.update_thread_load(current_load);
                    
                    // 自動スケーリングが有効な場合は実行
                    if config.auto_scaling() {
                        if current_load > 0.8 && thread_pool.can_scale_up() {
                            debug!("スレッドプールをスケールアップします: 負荷 = {:.2}", current_load);
                            thread_pool.scale_up().await;
                        } else if current_load < 0.3 && thread_pool.can_scale_down() {
                            debug!("スレッドプールをスケールダウンします: 負荷 = {:.2}", current_load);
                            thread_pool.scale_down().await;
                        }
                    }
                }
            });
        }
    }
    
    /// シャットダウン監視を開始します
    fn start_shutdown_monitor(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        let name = self.name.clone();
        
        // シャットダウン要求を監視
        if let Some(rt) = &self.runtime {
            rt.spawn(async move {
                if shutdown_rx.recv().await.is_some() {
                    info!("ランタイムのシャットダウン要求を受信しました: {}", name);
                    // ここでクリーンアップ処理を行う
                    // 実際のシャットダウンはdropで行われる
                }
            });
        }
    }
    
    /// 非同期タスクを実行します
    pub fn spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, AsyncRuntimeError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        if let Some(rt) = &self.runtime {
            let handle = rt.spawn(future);
            
            // 統計情報を更新
            self.stats.increment_spawned_tasks();
            
            Ok(handle)
        } else {
            Err(AsyncRuntimeError::RuntimeNotInitialized)
        }
    }
    
    /// 優先度とドメインを指定して非同期タスクを実行します
    pub async fn spawn_with_config<F>(&self, future: F, config: TaskConfig) -> Result<JoinHandle<F::Output>, AsyncRuntimeError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        // ドメイン制限を取得
        let semaphore = {
            let limits = self.concurrency_limits.read().await;
            match limits.get(&config.domain) {
                Some(sem) => sem.clone(),
                None => return Err(AsyncRuntimeError::DomainNotFound),
            }
        };
        
        // アクティブタスク数を更新
        {
            let mut active = self.active_tasks.write().await;
            let count = active.entry(config.domain).or_insert(0);
            *count += 1;
        }
        
        // ドメインの実行許可を取得
        let permit = match semaphore.acquire().await {
            Ok(permit) => permit,
            Err(_) => return Err(AsyncRuntimeError::SemaphoreAcquisitionFailed),
        };
        
        // 開始時間を記録
        let start_time = Instant::now();
        let task_id = Uuid::new_v4();
        let domain = config.domain;
        let priority = config.priority;
        let stats = self.stats.clone();
        
        // メトリクスを記録
        stats.start_task(task_id, domain, priority);
        
        // タスクを実行
        let task_future = async move {
            // スコープを抜けるときに許可を解放
            let _permit = permit;
            
            // タスクの結果を返す
            let result = future.await;
            
            // 完了統計を記録
            let elapsed = start_time.elapsed();
            stats.complete_task(task_id, elapsed.as_millis() as u64);
            
            result
        };
        
        // タイムアウトラッパー
        let wrapped_future = if let Some(timeout_duration) = config.timeout {
            let timeout_future = timeout(timeout_duration, task_future);
            Box::pin(async move {
                match timeout_future.await {
                    Ok(result) => result,
                    Err(_) => {
                        // タイムアウトの統計を記録
                        stats.increment_timed_out_tasks();
                        panic!("Task timed out after {:?}", timeout_duration);
                    }
                }
            }) as std::pin::Pin<Box<dyn Future<Output = F::Output> + Send>>
        } else {
            Box::pin(task_future) as std::pin::Pin<Box<dyn Future<Output = F::Output> + Send>>
        };
        
        // 最終的なタスクをスポーン
        if let Some(rt) = &self.runtime {
            let handle = rt.spawn(wrapped_future);
            Ok(handle)
        } else {
            Err(AsyncRuntimeError::RuntimeNotInitialized)
        }
    }
    
    /// 制御ブロックを実行します
    pub fn block_on<F>(&self, future: F) -> Result<F::Output, AsyncRuntimeError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        if let Some(rt) = &self.runtime {
            Ok(rt.block_on(future))
        } else {
            Err(AsyncRuntimeError::RuntimeNotInitialized)
        }
    }
    
    /// 一定時間後に非同期タスクをスケジュールします
    pub fn schedule<F>(&self, future: F, delay: Duration) -> Result<JoinHandle<F::Output>, AsyncRuntimeError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let stats = self.stats.clone();
        
        // タスクを遅延実行するラッパーを作成
        let delayed_future = async move {
            // 指定時間待機
            tokio::time::sleep(delay).await;
            
            // 統計情報を更新
            stats.increment_scheduled_tasks();
            
            // 元のタスクを実行
            future.await
        };
        
        // 通常のスポーン処理を使用
        self.spawn(delayed_future)
    }
    
    /// ドメイン実行制限を設定します
    pub fn set_domain_concurrency_limit(&self, domain: ExecutionDomain, limit: usize) {
        let mut limits = self.concurrency_limits.blocking_write();
        limits.insert(domain, Arc::new(Semaphore::new(limit)));
        
        // メトリクスを更新
        debug!("ドメイン {:?} の同時実行制限を {} に設定しました", domain, limit);
    }
    
    /// ランタイムの統計情報を取得します
    pub fn get_stats(&self) -> Arc<RuntimeStats> {
        self.stats.clone()
    }
    
    /// ランタイムのメトリクスレポーターを取得します
    pub fn get_metrics_reporter(&self) -> Arc<MetricsReporter> {
        self.metrics_reporter.clone()
    }
    
    /// ランタイム名を取得します
    pub fn get_name(&self) -> &str {
        &self.name
    }
    
    /// スレッドプールの現在の負荷を取得します
    pub async fn get_current_load(&self) -> f64 {
        *self.max_thread_load.read().await
    }
    
    /// ランタイムをシャットダウンします
    pub fn shutdown(&mut self) {
        if let Some(sender) = self.shutdown_tx.take() {
            // シャットダウン通知を送信
            let _ = sender.blocking_send(());
            
            // Tokioランタイムをシャットダウン
            if let Some(rt) = self.runtime.take() {
                // 強制的にシャットダウン
                drop(rt);
            }
            
            info!("ランタイム {} をシャットダウンしました", self.name);
        }
    }
}

impl Drop for AsyncRuntime {
    fn drop(&mut self) {
        // 明示的にシャットダウンを呼び出していない場合
        if self.shutdown_tx.is_some() {
            self.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_runtime_creation() {
        let runtime = AsyncRuntime::new();
        assert!(runtime.runtime.is_some());
    }
    
    #[test]
    fn test_task_execution() {
        let runtime = AsyncRuntime::new();
        
        let result = runtime.block_on(async {
            let handle = runtime.spawn(async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                42
            }).unwrap();
            
            handle.await.unwrap()
        });
        
        assert_eq!(result.unwrap(), 42);
    }
    
    #[test]
    fn test_scheduled_task() {
        let runtime = AsyncRuntime::new();
        
        let result = runtime.block_on(async {
            let start = Instant::now();
            let handle = runtime.schedule(async { 42 }, Duration::from_millis(50)).unwrap();
            let result = handle.await.unwrap();
            let elapsed = start.elapsed();
            
            (result, elapsed)
        });
        
        if let Ok((result, elapsed)) = result {
            assert_eq!(result, 42);
            assert!(elapsed.as_millis() >= 50);
        } else {
            panic!("Scheduled task failed");
        }
    }

    #[test]
    fn test_task_with_config() {
        let runtime = AsyncRuntime::new();
        
        let result = runtime.block_on(async {
            let config = TaskConfig {
                priority: TaskPriority::High,
                domain: ExecutionDomain::Compute,
                timeout: Some(Duration::from_millis(100)),
                name: Some("test_task".to_string()),
            };
            
            let handle = runtime.spawn_with_config(async { 42 }, config).await.unwrap();
            handle.await.unwrap()
        });
        
        assert_eq!(result.unwrap(), 42);
    }
    
    #[test]
    fn test_multiple_tasks() {
        let runtime = AsyncRuntime::new();
        
        let result = runtime.block_on(async {
            let mut handles = Vec::new();
            
            for i in 0..10 {
                let handle = runtime.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    i
                }).unwrap();
                
                handles.push(handle);
            }
            
            let mut results = Vec::new();
            for handle in handles {
                results.push(handle.await.unwrap());
            }
            
            results
        });
        
        if let Ok(results) = result {
            assert_eq!(results.len(), 10);
            assert_eq!(results, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        } else {
            panic!("Multiple tasks failed");
        }
    }
    
    #[test]
    fn test_domain_concurrency() {
        let runtime = AsyncRuntime::new();
        
        // テスト用に並行実行数を制限
        runtime.set_domain_concurrency_limit(ExecutionDomain::Compute, 2);
        
        let result = runtime.block_on(async {
            use std::sync::atomic::{AtomicUsize, Ordering};
            
            let active_count = Arc::new(AtomicUsize::new(0));
            let max_concurrent = Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::new();
            
            for _ in 0..5 {
                let active = active_count.clone();
                let max = max_concurrent.clone();
                
                let config = TaskConfig {
                    domain: ExecutionDomain::Compute,
                    ..Default::default()
                };
                
                let handle = runtime.spawn_with_config(async move {
                    // アクティブカウントを増加
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    
                    // 最大同時実行数を更新
                    let mut max_seen = max.load(Ordering::SeqCst);
                    while current > max_seen {
                        match max.compare_exchange(max_seen, current, Ordering::SeqCst, Ordering::SeqCst) {
                            Ok(_) => break,
                            Err(actual) => max_seen = actual,
                        }
                    }
                    
                    // 少し待機して同時実行をシミュレート
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    
                    // アクティブカウントを減少
                    active.fetch_sub(1, Ordering::SeqCst);
                    
                    true
                }, config).await.unwrap();
                
                handles.push(handle);
            }
            
            // すべてのタスクが完了するのを待つ
            for handle in handles {
                handle.await.unwrap();
            }
            
            max_concurrent.load(Ordering::SeqCst)
        });
        
        if let Ok(max_concurrent) = result {
            // 同時実行制限（2）を超えないことを確認
            assert!(max_concurrent <= 2);
        } else {
            panic!("Concurrency test failed");
        }
    }
} 