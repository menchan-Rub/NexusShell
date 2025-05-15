use std::time::Duration;

use super::RuntimeType;

/// スレッドプールの戦略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadPoolStrategy {
    /// 最大スループット優先（並列化重視）
    Throughput,
    /// レイテンシ優先（応答性重視）
    Latency,
    /// リソース効率優先（メモリ使用量最小化）
    ResourceEfficient,
    /// バランス型（スループットとレイテンシのバランス）
    Balanced,
    /// アダプティブ（負荷に応じて自動調整）
    Adaptive,
}

impl Default for ThreadPoolStrategy {
    fn default() -> Self {
        Self::Balanced
    }
}

/// 実行ドメイン
/// 異なるタイプのタスクに対して異なる同時実行制限を適用するために使用
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionDomain {
    /// 計算ドメイン（CPU集中タスク）
    Compute,
    /// I/Oドメイン（ファイルI/Oなど）
    IO,
    /// ネットワークドメイン
    Network,
    /// バックグラウンドドメイン（低優先度タスク）
    Background,
    /// カスタムドメイン（カスタムID付き）
    Custom(u32),
}

impl std::fmt::Display for ExecutionDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compute => write!(f, "計算"),
            Self::IO => write!(f, "I/O"),
            Self::Network => write!(f, "ネットワーク"),
            Self::Background => write!(f, "バックグラウンド"),
            Self::Custom(id) => write!(f, "カスタム({})", id),
        }
    }
}

/// タスク優先度
/// タスクの実行優先度を制御するために使用
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// 最低優先度
    Lowest = 0,
    /// 低優先度
    Low = 1,
    /// 通常優先度
    Normal = 2,
    /// 高優先度
    High = 3,
    /// 最高優先度
    Highest = 4,
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// 非同期ランタイムの設定
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// ランタイムの種類
    runtime_type: RuntimeType,
    /// ワーカースレッド数
    worker_threads: usize,
    /// I/Oワーカースレッド数（DedicatedIoモード用）
    io_worker_threads: usize,
    /// IOドライバを有効にするかどうか
    enable_io_driver: bool,
    /// タイマーを有効にするかどうか
    enable_time: bool,
    /// スレッドスタックサイズ（バイト）
    thread_stack_size: usize,
    /// 最大ブロッキングスレッド数
    max_blocking_threads: Option<usize>,
    /// 全体タイムアウト時間
    global_timeout: Option<Duration>,
    /// タスクの最大キュー長
    max_queue_depth: Option<usize>,
    /// スレッドプール戦略
    thread_pool_strategy: ThreadPoolStrategy,
    /// スレッド名プレフィックス
    thread_name_prefix: String,
    /// スレッドプライオリティ（-20〜19）
    thread_priority: Option<i32>,
    /// メモリ制限（バイト）
    memory_limit: Option<usize>,
    /// パニック時にシャットダウンするか
    shutdown_on_panic: bool,
    /// シャットダウン猶予時間
    shutdown_grace_period: Duration,
    /// ブロッキングタスクの最大実行時間
    blocking_task_timeout: Option<Duration>,
    /// アフィニティマスク（CPUコアIDのセット）
    cpu_affinity: Option<Vec<usize>>,
    /// バックグラウンドタスクの最大同時実行数
    max_background_tasks: Option<usize>,
    /// ランタイム状態モニタリングの間隔
    monitoring_interval: Option<Duration>,
    /// スレッドプール自動調整を有効にするか
    auto_adjust_pool: bool,
    /// I/Oポーリング間隔
    io_poll_interval: Option<Duration>,
    /// 計算ドメインの同時実行制限
    compute_concurrency: usize,
    /// I/Oドメインの同時実行制限
    io_concurrency: usize,
    /// ネットワークドメインの同時実行制限
    network_concurrency: usize,
    /// バックグラウンドドメインの同時実行制限
    background_concurrency: usize,
    /// メトリクス収集間隔
    metrics_interval: Duration,
    /// 自動スケーリングを有効にするかどうか
    auto_scaling: bool,
}

impl RuntimeConfig {
    /// 新しいランタイム設定を作成します
    pub fn new() -> Self {
        let available_cpus = num_cpus::get();
        
        Self {
            runtime_type: RuntimeType::MultiThread,
            worker_threads: available_cpus,
            io_worker_threads: available_cpus / 4,
            enable_io_driver: true,
            enable_time: true,
            thread_stack_size: 0, // デフォルト値を使用
            max_blocking_threads: None, // デフォルト値を使用
            global_timeout: None,
            max_queue_depth: None,
            thread_pool_strategy: ThreadPoolStrategy::Balanced,
            thread_name_prefix: "nexusshell-worker".to_string(),
            thread_priority: None,
            memory_limit: None,
            shutdown_on_panic: false,
            shutdown_grace_period: Duration::from_secs(10),
            blocking_task_timeout: None,
            cpu_affinity: None,
            max_background_tasks: None,
            monitoring_interval: Some(Duration::from_secs(30)),
            auto_adjust_pool: true,
            io_poll_interval: None,
            compute_concurrency: available_cpus * 2,
            io_concurrency: available_cpus * 4,
            network_concurrency: available_cpus * 8,
            background_concurrency: available_cpus,
            metrics_interval: Duration::from_secs(5),
            auto_scaling: true,
        }
    }
    
    /// マルチスレッドランタイム設定を作成します
    pub fn multi_thread() -> Self {
        let available_cpus = num_cpus::get();
        
        Self {
            runtime_type: RuntimeType::MultiThread,
            worker_threads: available_cpus,
            ..Self::new()
        }
    }

    /// 単一スレッドランタイム設定を作成します
    pub fn current_thread() -> Self {
        Self {
            runtime_type: RuntimeType::CurrentThread,
            worker_threads: 1,
            ..Self::new()
        }
    }

    /// アダプティブランタイム構成を作成します
    pub fn adaptive() -> Self {
        let available_cpus = num_cpus::get();
        
        Self {
            runtime_type: RuntimeType::Adaptive,
            worker_threads: available_cpus,
            thread_pool_strategy: ThreadPoolStrategy::Adaptive,
            auto_adjust_pool: true,
            ..Self::new()
        }
    }

    /// I/O集約型ワークロード向けの構成を作成します
    pub fn io_optimized() -> Self {
        let available_cpus = num_cpus::get();
        let io_workers = std::cmp::max(available_cpus / 2, 2);
        
        Self {
            runtime_type: RuntimeType::DedicatedIo,
            worker_threads: available_cpus - io_workers,
            io_worker_threads: io_workers,
            thread_pool_strategy: ThreadPoolStrategy::Latency,
            ..Self::new()
        }
    }

    /// 高スループット向けの構成を作成します
    pub fn high_throughput() -> Self {
        let available_cpus = num_cpus::get();
        
        Self {
            runtime_type: RuntimeType::MultiThread,
            worker_threads: available_cpus,
            thread_pool_strategy: ThreadPoolStrategy::Throughput,
            max_blocking_threads: Some(available_cpus * 4),
            ..Self::new()
        }
    }

    /// 低レイテンシ向けの構成を作成します
    pub fn low_latency() -> Self {
        let available_cpus = num_cpus::get();
        
        Self {
            runtime_type: RuntimeType::MultiThread,
            worker_threads: available_cpus,
            thread_pool_strategy: ThreadPoolStrategy::Latency,
            io_poll_interval: Some(Duration::from_micros(10)),
            ..Self::new()
        }
    }

    /// リソース効率向けの構成を作成します
    pub fn resource_efficient() -> Self {
        let available_cpus = num_cpus::get();
        let worker_count = std::cmp::max(available_cpus / 2, 1);
        
        Self {
            runtime_type: RuntimeType::MultiThread,
            worker_threads: worker_count,
            thread_pool_strategy: ThreadPoolStrategy::ResourceEfficient,
            auto_adjust_pool: true,
            ..Self::new()
        }
    }

    /// ランタイムの種類を取得します
    pub fn runtime_type(&self) -> RuntimeType {
        self.runtime_type
    }

    /// ランタイムの種類を設定します
    pub fn set_runtime_type(&mut self, runtime_type: RuntimeType) {
        self.runtime_type = runtime_type;
    }

    /// ワーカースレッド数を取得します
    pub fn worker_threads(&self) -> usize {
        self.worker_threads
    }

    /// ワーカースレッド数を設定します
    pub fn set_worker_threads(&mut self, worker_threads: usize) {
        self.worker_threads = worker_threads;
    }

    /// I/Oワーカースレッド数を取得します
    pub fn io_worker_threads(&self) -> usize {
        self.io_worker_threads
    }

    /// I/Oワーカースレッド数を設定します
    pub fn set_io_worker_threads(&mut self, io_worker_threads: usize) {
        self.io_worker_threads = io_worker_threads;
    }

    /// IOドライバが有効かどうかを取得します
    pub fn enable_io_driver(&self) -> bool {
        self.enable_io_driver
    }

    /// IOドライバの有効/無効を設定します
    pub fn set_enable_io_driver(&mut self, enable: bool) {
        self.enable_io_driver = enable;
    }

    /// タイマーが有効かどうかを取得します
    pub fn enable_time(&self) -> bool {
        self.enable_time
    }

    /// タイマーの有効/無効を設定します
    pub fn set_enable_time(&mut self, enable: bool) {
        self.enable_time = enable;
    }

    /// スレッドスタックサイズを取得します
    pub fn thread_stack_size(&self) -> usize {
        self.thread_stack_size
    }

    /// スレッドスタックサイズを設定します
    pub fn set_thread_stack_size(&mut self, size: usize) {
        self.thread_stack_size = size;
    }

    /// 最大ブロッキングスレッド数を取得します
    pub fn max_blocking_threads(&self) -> Option<usize> {
        self.max_blocking_threads
    }

    /// 最大ブロッキングスレッド数を設定します
    pub fn set_max_blocking_threads(&mut self, max: Option<usize>) {
        self.max_blocking_threads = max;
    }

    /// 全体タイムアウト時間を取得します
    pub fn global_timeout(&self) -> Option<Duration> {
        self.global_timeout
    }

    /// 全体タイムアウト時間を設定します
    pub fn set_global_timeout(&mut self, timeout: Option<Duration>) {
        self.global_timeout = timeout;
    }

    /// タスクの最大キュー長を取得します
    pub fn max_queue_depth(&self) -> Option<usize> {
        self.max_queue_depth
    }

    /// タスクの最大キュー長を設定します
    pub fn set_max_queue_depth(&mut self, depth: Option<usize>) {
        self.max_queue_depth = depth;
    }

    /// スレッドプール戦略を取得します
    pub fn thread_pool_strategy(&self) -> ThreadPoolStrategy {
        self.thread_pool_strategy
    }

    /// スレッドプール戦略を設定します
    pub fn set_thread_pool_strategy(&mut self, strategy: ThreadPoolStrategy) {
        self.thread_pool_strategy = strategy;
    }

    /// スレッド名プレフィックスを取得します
    pub fn thread_name_prefix(&self) -> &str {
        &self.thread_name_prefix
    }

    /// スレッド名プレフィックスを設定します
    pub fn set_thread_name_prefix(&mut self, prefix: &str) {
        self.thread_name_prefix = prefix.to_string();
    }

    /// スレッドプライオリティを取得します
    pub fn thread_priority(&self) -> Option<i32> {
        self.thread_priority
    }

    /// スレッドプライオリティを設定します
    pub fn set_thread_priority(&mut self, priority: Option<i32>) {
        self.thread_priority = priority;
    }

    /// メモリ制限を取得します
    pub fn memory_limit(&self) -> Option<usize> {
        self.memory_limit
    }

    /// メモリ制限を設定します
    pub fn set_memory_limit(&mut self, limit: Option<usize>) {
        self.memory_limit = limit;
    }

    /// パニック時にシャットダウンするかを取得します
    pub fn shutdown_on_panic(&self) -> bool {
        self.shutdown_on_panic
    }

    /// パニック時にシャットダウンするかを設定します
    pub fn set_shutdown_on_panic(&mut self, shutdown: bool) {
        self.shutdown_on_panic = shutdown;
    }

    /// シャットダウン猶予時間を取得します
    pub fn shutdown_grace_period(&self) -> Duration {
        self.shutdown_grace_period
    }

    /// シャットダウン猶予時間を設定します
    pub fn set_shutdown_grace_period(&mut self, period: Duration) {
        self.shutdown_grace_period = period;
    }

    /// ブロッキングタスクの最大実行時間を取得します
    pub fn blocking_task_timeout(&self) -> Option<Duration> {
        self.blocking_task_timeout
    }

    /// ブロッキングタスクの最大実行時間を設定します
    pub fn set_blocking_task_timeout(&mut self, timeout: Option<Duration>) {
        self.blocking_task_timeout = timeout;
    }

    /// CPUアフィニティを取得します
    pub fn cpu_affinity(&self) -> Option<&Vec<usize>> {
        self.cpu_affinity.as_ref()
    }

    /// CPUアフィニティを設定します
    pub fn set_cpu_affinity(&mut self, affinity: Option<Vec<usize>>) {
        self.cpu_affinity = affinity;
    }

    /// バックグラウンドタスクの最大同時実行数を取得します
    pub fn max_background_tasks(&self) -> Option<usize> {
        self.max_background_tasks
    }

    /// バックグラウンドタスクの最大同時実行数を設定します
    pub fn set_max_background_tasks(&mut self, max: Option<usize>) {
        self.max_background_tasks = max;
    }

    /// ランタイム状態モニタリングの間隔を取得します
    pub fn monitoring_interval(&self) -> Option<Duration> {
        self.monitoring_interval
    }

    /// ランタイム状態モニタリングの間隔を設定します
    pub fn set_monitoring_interval(&mut self, interval: Option<Duration>) {
        self.monitoring_interval = interval;
    }

    /// スレッドプール自動調整が有効かを取得します
    pub fn auto_adjust_pool(&self) -> bool {
        self.auto_adjust_pool
    }

    /// スレッドプール自動調整の有効/無効を設定します
    pub fn set_auto_adjust_pool(&mut self, enable: bool) {
        self.auto_adjust_pool = enable;
    }

    /// I/Oポーリング間隔を取得します
    pub fn io_poll_interval(&self) -> Option<Duration> {
        self.io_poll_interval
    }

    /// I/Oポーリング間隔を設定します
    pub fn set_io_poll_interval(&mut self, interval: Option<Duration>) {
        self.io_poll_interval = interval;
    }

    /// 計算ドメインの同時実行制限を取得
    pub fn compute_concurrency(&self) -> usize {
        self.compute_concurrency
    }

    /// I/Oドメインの同時実行制限を取得
    pub fn io_concurrency(&self) -> usize {
        self.io_concurrency
    }

    /// ネットワークドメインの同時実行制限を取得
    pub fn network_concurrency(&self) -> usize {
        self.network_concurrency
    }

    /// バックグラウンドドメインの同時実行制限を取得
    pub fn background_concurrency(&self) -> usize {
        self.background_concurrency
    }

    /// メトリクス収集間隔を取得
    pub fn metrics_interval(&self) -> Duration {
        self.metrics_interval
    }

    /// 自動スケーリングが有効かどうかを取得
    pub fn auto_scaling(&self) -> bool {
        self.auto_scaling
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new()
    }
} 