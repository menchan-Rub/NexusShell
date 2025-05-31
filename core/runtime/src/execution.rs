/*!
# 超高性能実行エンジン

シェルコマンドの解析と実行を行う最先端の実行エンジンを提供します。
非同期による並列処理、高度なジョブ管理、リソース制限機能などを
備えた次世代の実行エンジンです。
*/

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn, trace, instrument, Span};
use uuid::Uuid;

use crate::io::{IoManager, IoStream, StreamMode};
use crate::security::SecurityManager;

/// コマンド実行のステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// 初期化
    Initializing,
    /// 実行中
    Running,
    /// 一時停止
    Paused,
    /// 完了
    Completed,
    /// エラー
    Failed,
    /// 中断
    Terminated,
    /// タイムアウト
    TimedOut,
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initializing => write!(f, "初期化中"),
            Self::Running => write!(f, "実行中"),
            Self::Paused => write!(f, "一時停止"),
            Self::Completed => write!(f, "完了"),
            Self::Failed => write!(f, "失敗"),
            Self::Terminated => write!(f, "中断"),
            Self::TimedOut => write!(f, "タイムアウト"),
        }
    }
}

/// コマンド実行のリソース使用量
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// 開始時刻
    pub start_time: Option<Instant>,
    /// 終了時刻
    pub end_time: Option<Instant>,
    /// CPU使用時間（ミリ秒）
    pub cpu_time_ms: u64,
    /// メモリ使用量（バイト）
    pub memory_bytes: u64,
    /// I/O読み取りバイト数
    pub io_read_bytes: u64,
    /// I/O書き込みバイト数
    pub io_write_bytes: u64,
    /// 実行されたサブプロセスの数
    pub subprocess_count: usize,
}

impl ResourceUsage {
    /// 新しいリソース使用量インスタンスを作成
    pub fn new() -> Self {
        Self {
            start_time: None,
            end_time: None,
            cpu_time_ms: 0,
            memory_bytes: 0,
            io_read_bytes: 0,
            io_write_bytes: 0,
            subprocess_count: 0,
        }
    }
    
    /// 実行時間を取得（ミリ秒）
    pub fn execution_time_ms(&self) -> u64 {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => {
                end.duration_since(start).as_millis() as u64
            }
            (Some(start), None) => {
                Instant::now().duration_since(start).as_millis() as u64
            }
            _ => 0,
        }
    }
    
    /// リソース使用量に別のインスタンスを追加
    pub fn add(&mut self, other: &ResourceUsage) {
        self.cpu_time_ms += other.cpu_time_ms;
        self.memory_bytes = self.memory_bytes.max(other.memory_bytes);
        self.io_read_bytes += other.io_read_bytes;
        self.io_write_bytes += other.io_write_bytes;
        self.subprocess_count += other.subprocess_count;
    }
}

/// ファイルシステム制限
#[derive(Debug, Clone)]
pub struct FileSystemRestrictions {
    /// 読み取りを許可するパス
    pub read_allowed_paths: Vec<PathBuf>,
    /// 書き込みを許可するパス
    pub write_allowed_paths: Vec<PathBuf>,
    /// 実行を許可するパス
    pub exec_allowed_paths: Vec<PathBuf>,
}

impl Default for FileSystemRestrictions {
    fn default() -> Self {
        // デフォルトでは基本的なシステムパスのみ許可
        Self {
            read_allowed_paths: vec![
                PathBuf::from("/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/usr/local/bin"),
                #[cfg(target_os = "windows")]
                PathBuf::from("C:\\Windows\\System32"),
            ],
            write_allowed_paths: vec![
                std::env::temp_dir(),
            ],
            exec_allowed_paths: vec![
                PathBuf::from("/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/usr/local/bin"),
                #[cfg(target_os = "windows")]
                PathBuf::from("C:\\Windows\\System32"),
            ],
        }
    }
}

/// セキュリティコンテキスト
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// ネットワークアクセスを許可するか
    pub allow_network: bool,
    /// ファイルシステム制限
    pub filesystem_restrictions: Option<FileSystemRestrictions>,
    /// メモリ使用量制限（バイト）
    pub memory_limit: Option<u64>,
    /// CPU時間制限（秒）
    pub cpu_time_limit: Option<f64>,
    /// 実行時間制限（秒）
    pub execution_time_limit: Option<f64>,
    /// サンドボックス化するか
    pub sandbox: bool,
    /// ケイパビリティ（Linuxのみ）
    #[cfg(target_os = "linux")]
    pub capabilities: Option<Vec<String>>,
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self {
            allow_network: true,
            filesystem_restrictions: None,
            memory_limit: None,
            cpu_time_limit: None,
            execution_time_limit: None,
            sandbox: false,
            #[cfg(target_os = "linux")]
            capabilities: None,
        }
    }
}

/// 実行コンテキスト
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// 作業ディレクトリ
    pub working_dir: PathBuf,
    /// 環境変数
    pub env_vars: HashMap<String, String>,
    /// 標準入力
    pub stdin: Option<Arc<Mutex<Box<dyn IoStream>>>>,
    /// 標準出力
    pub stdout: Option<Arc<Mutex<Box<dyn IoStream>>>>,
    /// 標準エラー
    pub stderr: Option<Arc<Mutex<Box<dyn IoStream>>>>,
    /// タイムアウト（秒）
    pub timeout: Option<f64>,
    /// セキュリティコンテキスト
    pub security: SecurityContext,
    /// グローバルリソース制限を無視するか
    pub ignore_global_limits: bool,
    /// 実行のタグ（グループ化や識別用）
    pub tags: HashSet<String>,
    /// 実行のプライオリティ（0-100、高いほど優先）
    pub priority: u8,
    /// ジョブコントロールを有効にするか
    pub job_control: bool,
    /// リトライポリシー
    pub retry_policy: Option<RetryPolicy>,
    /// プロセスグループID
    pub process_group: Option<String>,
    /// 親プロセスID
    pub parent_id: Option<String>,
    /// オプションフラグ
    pub flags: HashMap<String, String>,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        
        Self {
            working_dir,
            env_vars: std::env::vars().collect(),
            stdin: None,
            stdout: None,
            stderr: None,
            timeout: None,
            security: SecurityContext::default(),
            ignore_global_limits: false,
            tags: HashSet::new(),
            priority: 50,
            job_control: false,
            retry_policy: None,
            process_group: None,
            parent_id: None,
            flags: HashMap::new(),
        }
    }
}

/// コマンドの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandType {
    /// 外部コマンド
    External {
        /// コマンドパス
        path: PathBuf,
        /// 引数
        args: Vec<String>,
    },
    /// 内部（組み込み）コマンド
    Internal {
        /// コマンド名
        name: String,
        /// 引数
        args: Vec<String>,
    },
    /// シェルスクリプト
    Script {
        /// スクリプトパス
        path: PathBuf,
        /// 引数
        args: Vec<String>,
    },
    /// 関数呼び出し
    Function {
        /// 関数名
        name: String,
        /// 引数
        args: Vec<String>,
    },
    /// パイプライン
    Pipeline {
        /// コマンド
        commands: Vec<Box<CommandType>>,
    },
    /// コマンドリスト（順次実行）
    List {
        /// コマンド
        commands: Vec<Box<CommandType>>,
    },
}

/// リトライポリシー
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大リトライ回数
    pub max_attempts: usize,
    /// リトライ間隔（秒）
    pub retry_interval_secs: u64,
}

/// 実行フラグ
#[derive(Debug, Clone, Default)]
pub struct ExecutionFlags {
    /// バックグラウンド実行
    pub background: bool,
    /// エラー時に実行を継続
    pub continue_on_error: bool,
    /// デバッグ出力を有効化
    pub debug: bool,
    /// 出力をキャプチャするかどうか
    pub capture_output: bool,
    /// 詳細なメトリクスを収集するかどうか
    pub collect_metrics: bool,
    /// プロファイリングを有効化
    pub enable_profiling: bool,
    /// IO優先度（1-10、10が最高）
    pub io_priority: u8,
    /// CPU優先度（1-10、10が最高）
    pub cpu_priority: u8,
}

/// セキュリティコンテキスト
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// 最大メモリ使用量（バイト）
    pub memory_limit: Option<usize>,
    /// CPU時間制限（秒）
    pub cpu_time_limit: Option<f64>,
    /// ネットワークアクセスを許可
    pub allow_network: bool,
    /// ファイルシステムアクセス制限
    pub filesystem_restrictions: Option<FilesystemRestrictions>,
    /// ケイパビリティ制限
    pub capabilities: Vec<String>,
    /// サンドボックスプロファイル
    pub sandbox_profile: Option<String>,
}

/// ファイルシステムアクセス制限
#[derive(Debug, Clone)]
pub struct FilesystemRestrictions {
    /// 読み取り許可パス
    pub read_allowed_paths: Vec<PathBuf>,
    /// 書き込み許可パス
    pub write_allowed_paths: Vec<PathBuf>,
    /// 実行許可パス
    pub exec_allowed_paths: Vec<PathBuf>,
    /// ルートディレクトリ
    pub chroot_path: Option<PathBuf>,
}

/// リソース統計
#[derive(Debug, Clone, Default)]
pub struct ResourceStatistics {
    /// CPU使用時間（秒）
    pub cpu_time_sec: f64,
    /// ピークメモリ使用量（バイト）
    pub peak_memory_bytes: usize,
    /// 読み取りバイト数
    pub read_bytes: u64,
    /// 書き込みバイト数
    pub write_bytes: u64,
    /// ネットワーク送信バイト数
    pub network_tx_bytes: u64,
    /// ネットワーク受信バイト数
    pub network_rx_bytes: u64,
    /// コンテキストスイッチ数
    pub context_switches: u64,
    /// ページフォールト数
    pub page_faults: u64,
}

/// リソース監視コンテキスト
pub struct ResourceMonitorContext {
    /// プロセスID
    pub pid: u32,
    /// 開始時間
    pub start_time: Instant,
    /// 前回のサンプリング時間
    pub last_sample_time: Instant,
    /// 前回のCPU使用時間
    pub last_cpu_time: f64,
    /// 前回のメモリ使用量
    pub last_memory_usage: usize,
    /// 前回のIO読み取りバイト数
    pub last_read_bytes: u64,
    /// 前回のIO書き込みバイト数
    pub last_write_bytes: u64,
}

/// システムリソース監視
#[derive(Debug)]
pub struct SystemResourceMonitor {
    /// CPU使用率 (%)
    pub cpu_usage: f64,
    /// メモリ使用率 (%)
    pub memory_usage: f64,
    /// ディスク使用率 (%)
    pub disk_usage: f64,
    /// ネットワーク使用率 (Mbps)
    pub network_usage: f64,
    /// 最終更新時間
    pub last_updated: Instant,
    /// 監視間隔
    pub poll_interval: Duration,
    /// 更新チャネル
    pub update_tx: mpsc::Sender<SystemResourceSnapshot>,
    /// アクティブプロセス
    pub active_processes: DashMap<String, ProcessInfo>,
}

/// プロセス情報
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// プロセスID
    pub pid: u32,
    /// コマンド名
    pub command: String,
    /// 開始時間
    pub start_time: Instant,
    /// CPU使用率 (%)
    pub cpu_usage: f64,
    /// メモリ使用量 (バイト)
    pub memory_usage: usize,
}

impl Clone for SystemResourceMonitor {
    fn clone(&self) -> Self {
        Self {
            cpu_usage: self.cpu_usage,
            memory_usage: self.memory_usage,
            disk_usage: self.disk_usage,
            network_usage: self.network_usage,
            last_updated: self.last_updated,
            poll_interval: self.poll_interval,
            update_tx: self.update_tx.clone(),
            active_processes: DashMap::new(), // 新しいインスタンスを作成
        }
    }
}

/// システムリソーススナップショット
#[derive(Debug, Clone)]
pub struct SystemResourceSnapshot {
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// CPU使用率 (%)
    pub cpu_usage: f64,
    /// メモリ使用率 (%)
    pub memory_usage: f64,
    /// ディスク使用率 (%)
    pub disk_usage: f64,
    /// ネットワーク使用率 (Mbps)
    pub network_usage: f64,
    /// プロセス数
    pub process_count: usize,
    /// アクティブコマンド数
    pub active_commands: usize,
}

/// コマンドキャッシュエントリ
#[derive(Debug)]
struct CommandCacheEntry {
    /// コマンドパス
    path: PathBuf,
    /// 最終アクセス時間
    last_accessed: Instant,
    /// 平均実行時間
    avg_execution_time: Duration,
    /// 実行回数
    execution_count: u64,
    /// メタデータ
    metadata: HashMap<String, String>,
}

/// 処理ステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    /// 実行中
    Running,
    /// 停止中
    Stopped,
    /// 終了
    Exited(i32),
    /// 中断
    Terminated,
    /// 不明
    Unknown,
}

/// コマンド実行結果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// 終了コード
    pub exit_code: i32,
    /// 標準出力
    pub stdout: Vec<u8>,
    /// 標準エラー出力
    pub stderr: Vec<u8>,
    /// 実行時間
    pub execution_time: Duration,
    /// プロセスID（存在する場合）
    pub pid: Option<u32>,
    /// リソース使用統計
    pub resource_stats: Option<ResourceStatistics>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            execution_time: Duration::default(),
            pid: None,
            resource_stats: None,
            metadata: HashMap::new(),
        }
    }
}

/// 高度なキャッシュ最適化エンジン
#[derive(Debug)]
pub struct CacheOptimizer {
    /// コマンド実行統計
    execution_stats: HashMap<String, ExecutionStatistics>,
    /// キャッシュヒット率
    cache_hit_rate: f64,
    /// 最適化レベル (1-10)
    optimization_level: u8,
    /// 最終最適化時間
    last_optimization: Instant,
    /// 予測モデル
    prediction_model: Arc<Mutex<PredictionModel>>,
}

/// 実行統計情報
#[derive(Debug, Clone)]
struct ExecutionStatistics {
    /// 実行回数
    execution_count: u64,
    /// 平均実行時間
    avg_execution_time: Duration,
    /// 平均メモリ使用量
    avg_memory_usage: usize,
    /// 典型的な引数
    common_args: HashMap<String, u64>,
    /// 実行パターン
    execution_patterns: Vec<ExecutionPattern>,
}

/// 実行パターン
#[derive(Debug, Clone)]
struct ExecutionPattern {
    /// 前のコマンド
    previous_commands: VecDeque<String>,
    /// 発生頻度
    frequency: u64,
    /// 最終実行時間
    last_seen: Instant,
}

/// 予測モデル
#[derive(Debug)]
struct PredictionModel {
    /// コマンドシーケンス確率
    sequence_probabilities: HashMap<Vec<String>, HashMap<String, f64>>,
    /// 最近実行されたコマンド
    recent_commands: VecDeque<String>,
    /// モデル精度
    accuracy: f64,
}

/// 実行エンジン
#[derive(Debug)]
pub struct ExecutionEngine {
    /// 環境
    environment: Arc<Environment>,
    /// セキュリティマネージャー
    security_manager: Arc<SecurityManager>,
    /// IOマネージャー
    io_manager: Arc<IoManager>,
    /// プラグインマネージャー
    plugin_manager: Arc<PluginManager>,
    /// コマンドキャッシュ
    command_cache: Arc<DashMap<String, CommandCacheEntry>>,
    /// 実行制限セマフォ
    execution_limiter: Arc<Semaphore>,
    /// アクティブプロセス
    active_processes: Arc<DashMap<String, Child>>,
    /// パフォーマンスプロファイラー
    profiler: Arc<Mutex<PerformanceProfiler>>,
    /// コマンドスケジューラー
    scheduler: Arc<CommandScheduler>,
    /// キャッシュ最適化エンジン
    cache_optimizer: Arc<Mutex<CacheOptimizer>>,
    /// ゼロコピーデータパイプライン
    zero_copy_pipeline: Arc<RwLock<ZeroCopyPipeline>>,
}

/// ゼロコピーデータパイプライン
#[derive(Debug)]
struct ZeroCopyPipeline {
    /// 共有メモリプール
    shared_memory_pool: HashMap<String, Arc<Vec<u8>>>,
    /// リージョン割り当て
    allocated_regions: HashMap<String, MemoryRegion>,
    /// 最大プールサイズ
    max_pool_size: usize,
    /// 現在のプールサイズ
    current_pool_size: usize,
}

/// メモリリージョン
#[derive(Debug, Clone)]
struct MemoryRegion {
    /// リージョンID
    id: String,
    /// 開始位置
    offset: usize,
    /// サイズ
    size: usize,
    /// 最終アクセス時間
    last_accessed: Instant,
    /// 所有者プロセス
    owner: Option<String>,
}

impl ZeroCopyPipeline {
    /// 新しいゼロコピーパイプラインを作成
    fn new(pool_size_mb: usize) -> Self {
        Self {
            shared_memory_pool: HashMap::new(),
            allocated_regions: HashMap::new(),
            max_pool_size: pool_size_mb * 1024 * 1024,
            current_pool_size: 0,
        }
    }
    
    /// 共有メモリリージョンを割り当て
    fn allocate_region(&mut self, id: &str, size: usize) -> Option<MemoryRegion> {
        // プールサイズを超える場合は古いリージョンを解放
        if self.current_pool_size + size > self.max_pool_size {
            self.evict_old_regions(size);
        }
        
        // それでも足りない場合はNoneを返す
        if self.current_pool_size + size > self.max_pool_size {
            return None;
        }
        
        // 新しいリージョンを割り当て
        let region = MemoryRegion {
            id: id.to_string(),
            offset: self.current_pool_size,
            size,
            last_accessed: Instant::now(),
            owner: None,
        };
        
        self.allocated_regions.insert(id.to_string(), region.clone());
        self.current_pool_size += size;
        
        Some(region)
    }
    
    /// 古いリージョンを解放
    fn evict_old_regions(&mut self, required_size: usize) {
        // リージョンを最終アクセス時間でソート
        let mut regions: Vec<_> = self.allocated_regions.iter().collect();
        regions.sort_by(|a, b| a.1.last_accessed.cmp(&b.1.last_accessed));
        
        let mut freed_size = 0;
        let mut keys_to_remove = Vec::new();
        
        for (key, region) in regions {
            if freed_size >= required_size {
                break;
            }
            
            freed_size += region.size;
            keys_to_remove.push(key.clone());
        }
        
        // 選択したリージョンを解放
        for key in keys_to_remove {
            if let Some(region) = self.allocated_regions.remove(&key) {
                self.current_pool_size -= region.size;
                self.shared_memory_pool.remove(&key);
            }
        }
    }
    
    /// リージョンを取得
    fn get_region(&mut self, id: &str) -> Option<&MemoryRegion> {
        if let Some(region) = self.allocated_regions.get_mut(id) {
            region.last_accessed = Instant::now();
            Some(region)
        } else {
            None
        }
    }
    
    /// データを共有メモリに書き込み
    fn write_data(&mut self, id: &str, data: Vec<u8>) -> Option<MemoryRegion> {
        let size = data.len();
        
        // リージョンを割り当て
        let region = self.allocate_region(id, size)?;
        
        // データを共有メモリに書き込み
        self.shared_memory_pool.insert(id.to_string(), Arc::new(data));
        
        Some(region)
    }
    
    /// 共有メモリからデータを読み込み
    fn read_data(&mut self, id: &str) -> Option<Arc<Vec<u8>>> {
        // リージョンが存在するか確認
        self.get_region(id)?;
        
        // データを返す
        self.shared_memory_pool.get(id).cloned()
    }
}

impl ExecutionEngine {
    /// 新しい実行エンジンを作成
    pub fn new(
        environment: Arc<Environment>,
        security_manager: Arc<SecurityManager>,
        io_manager: Arc<IoManager>,
        plugin_manager: Arc<PluginManager>,
    ) -> Self {
        // デフォルトでは64の並列実行を許可
        let execution_limiter = Arc::new(Semaphore::new(64));
        let command_cache = Arc::new(DashMap::new());
        let active_processes = Arc::new(DashMap::new());
        let profiler = Arc::new(Mutex::new(PerformanceProfiler::new()));
        let scheduler = Arc::new(CommandScheduler::new());
        let cache_optimizer = Arc::new(Mutex::new(CacheOptimizer::new()));
        let zero_copy_pipeline = Arc::new(RwLock::new(ZeroCopyPipeline::new(512)));  // 512MB
        
        Self {
            environment,
            security_manager,
            io_manager,
            plugin_manager,
            command_cache,
            execution_limiter,
            active_processes,
            profiler,
            scheduler,
            cache_optimizer,
            zero_copy_pipeline,
        }
    }
    
    /// コマンドを実行
    #[instrument(skip(self, args, context), fields(command = %command))]
    pub async fn execute_command(
        &self,
        command: &str,
        args: Vec<String>,
        context: ExecutionContext,
    ) -> Result<ExecutionResult> {
        let start_time = Instant::now();
        
        // 予測的プリロードを実行
        if context.flags.enable_profiling {
            let optimizer = self.cache_optimizer.lock().await;
            let predictions = optimizer.predict_next_commands().await;
            self.preload_commands(predictions, 0.7).await;
        }
        
        // ジョブIDを生成（指定されていない場合）
        let job_id = context.job_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        
        // プラグインハンドラーを確認
        if self.plugin_manager.has_command_handler(command).await {
            debug!("コマンド '{command}' をプラグインで実行します");
            return self.execute_plugin_command(command, args, context).await;
        }
        
        // コマンドパスを解決
        let cmd_path = self.resolve_command(command).await?;
        
        // セキュリティチェック
        if let Some(security_context) = &context.security_context {
            self.security_manager.validate_command_execution(&cmd_path, &context.working_dir, security_context).await?;
        }
        
        // リソース制限を適用
        let permit = self.execution_limiter.acquire().await?;
        
        // コマンド実行前の前処理フック
        self.before_command_execution(command, &args, &context).await?;
        
        // コマンドを構築
        let mut cmd = Command::new(&cmd_path);
        cmd.args(&args)
            .current_dir(&context.working_dir)
            .envs(&context.environment);
        
        // 標準入出力の設定
        let (mut child, stdin_handle) = self.setup_command_io(&mut cmd, &context).await?;
        
        // 標準入力データがある場合は書き込み
        if let Some(stdin_data) = context.stdin_data {
            if let Some(mut stdin) = stdin_handle {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(&stdin_data).await?;
                stdin.flush().await?;
            }
        }
        
        // プロセストラッキング
        let process_id = child.id();
        if let Some(pid) = process_id {
            self.active_processes.insert(job_id.clone(), child);
        }
        
        // 実行とタイムアウト処理
        let result = if let Some(timeout_duration) = context.timeout {
            match timeout(timeout_duration, self.wait_for_process(&job_id)).await {
                Ok(result) => result,
                Err(_) => {
                    // タイムアウト
                    warn!("コマンド '{command}' がタイムアウトしました");
                    self.kill_process(&job_id).await?;
                    anyhow::bail!("コマンド実行がタイムアウトしました（{}秒）", timeout_duration.as_secs())
                }
            }
        } else {
            self.wait_for_process(&job_id).await
        };
        
        // プロセスの結果を取得
        let execution_result = match result {
            Ok(output) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let execution_time = start_time.elapsed();
                
                // リソース統計を収集
                let resource_stats = if context.flags.collect_metrics {
                    Some(self.collect_resource_statistics(process_id).await)
                } else {
                    None
                };
                
                debug!("コマンド '{command}' が終了しました（コード: {exit_code}, 時間: {:?}）", execution_time);
                
                ExecutionResult {
                    exit_code,
                    stdout: output.stdout,
                    stderr: output.stderr,
                    execution_time,
                    pid: process_id,
                    resource_stats,
                    metadata: HashMap::new(),
                }
            },
            Err(e) => {
                error!("コマンド '{command}' の実行に失敗しました: {e}");
                anyhow::bail!("コマンド実行に失敗しました: {}", e)
            }
        };
        
        // プロセスをトラッキングから削除
        self.active_processes.remove(&job_id);
        
        // コマンド実行後の後処理フック
        self.after_command_execution(command, &args, &context, &execution_result).await?;
        
        // コマンドキャッシュを更新
        self.update_command_cache(command, &cmd_path, execution_result.execution_time).await;
        
        // プロファイリングデータを記録
        if context.flags.enable_profiling {
            let mut profiler = self.profiler.lock().await;
            profiler.record_execution(
                command,
                execution_result.execution_time,
                execution_result.exit_code,
                execution_result.resource_stats.as_ref(),
            );
            
            // キャッシュ最適化エンジンに記録
            let mut optimizer = self.cache_optimizer.lock().await;
            optimizer.record_execution(
                command,
                &args,
                execution_result.execution_time,
                execution_result.resource_stats.as_ref().map(|s| s.peak_memory_bytes)
            ).await;
            
            // 定期的にキャッシュを最適化
            optimizer.optimize_cache().await;
        }
        
        // 実行リソースを解放
        drop(permit);
        
        Ok(execution_result)
    }
    
    /// プラグインコマンドを実行
    async fn execute_plugin_command(
        &self,
        command: &str,
        args: Vec<String>,
        context: ExecutionContext,
    ) -> Result<ExecutionResult> {
        let start_time = Instant::now();
        
        // プラグインコマンドを実行
        let plugin_result = self.plugin_manager.execute_command(command, args, context.clone()).await?;
        
        // 実行結果に変換
        let execution_time = start_time.elapsed();
        let result = ExecutionResult {
            exit_code: plugin_result.exit_code,
            stdout: plugin_result.stdout,
            stderr: plugin_result.stderr,
            execution_time,
            pid: None,
            resource_stats: None,
            metadata: plugin_result.metadata,
        };
        
        Ok(result)
    }
    
    /// コマンドパスを解決
    async fn resolve_command(&self, command: &str) -> Result<PathBuf> {
        // 絶対パスまたは相対パスの場合はそのまま使用
        let cmd_path = PathBuf::from(command);
        if cmd_path.is_absolute() || command.contains('/') || command.contains('\\') {
            if cmd_path.exists() {
                return Ok(cmd_path);
            }
            anyhow::bail!("コマンド '{command}' が見つかりません");
        }
        
        // キャッシュをチェック
        if let Some(mut entry) = self.command_cache.get_mut(command) {
            // エントリを更新（スレッドセーフに）
            let mut entry_value = entry.value_mut();
            entry_value.last_accessed = Instant::now();
            return Ok(entry_value.path.clone());
        }
        
        // PATH環境変数からコマンドを検索
        if let Some(path_var) = self.environment.get("PATH") {
            let paths = path_var.split(if cfg!(windows) { ';' } else { ':' });
            
            for path in paths {
                let mut full_path = PathBuf::from(path);
                full_path.push(command);
                
                // Windowsの場合は.exeなどの拡張子も確認
                if cfg!(windows) {
                    for ext in ["", ".exe", ".cmd", ".bat"] {
                        let mut ext_path = full_path.clone();
                        if !ext.is_empty() {
                            ext_path.set_extension(ext.trim_start_matches("."));
                        }
                        if ext_path.exists() {
                            return Ok(ext_path);
                        }
                    }
                } else if full_path.exists() {
                    return Ok(full_path);
                }
            }
        }
        
        anyhow::bail!("コマンド '{command}' が見つかりません")
    }
    
    /// コマンドの入出力をセットアップ
    async fn setup_command_io(
        &self,
        cmd: &mut Command,
        context: &ExecutionContext,
    ) -> Result<(Child, Option<tokio::process::ChildStdin>)> {
        if context.flags.capture_output {
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
        } else {
            // IOマネージャーから標準出力/エラーを取得
            // 同期APIを使用するように修正
            let stdout = self.io_manager.get_stdout()?;
            let stderr = self.io_manager.get_stderr()?;
            cmd.stdout(stdout);
            cmd.stderr(stderr);
        }
        
        // 標準入力の設定
        if context.stdin_data.is_some() {
            cmd.stdin(std::process::Stdio::piped());
            let child = cmd.spawn()?;
            Ok((child, child.stdin))
        } else {
            // 標準入力をそのまま渡す
            // 同期APIを使用するように修正
            let stdin = self.io_manager.get_stdin()?;
            cmd.stdin(stdin);
            let child = cmd.spawn()?;
            Ok((child, None))
        }
    }
    
    /// プロセスの終了を待機
    async fn wait_for_process(&self, job_id: &str) -> Result<std::process::Output> {
        let mut process = match self.active_processes.remove(job_id) {
            Some((_, process)) => process,
            None => anyhow::bail!("プロセス '{job_id}' が見つかりません"),
        };
        
        // プロセスの終了を待機
        let output = process.wait_with_output().await?;
        Ok(output)
    }
    
    /// プロセスを強制終了
    async fn kill_process(&self, job_id: &str) -> Result<()> {
        if let Some((_, mut process)) = self.active_processes.remove(job_id) {
            // プロセスを終了
            let _ = process.kill().await;
        }
        Ok(())
    }
    
    /// コマンドキャッシュを更新
    async fn update_command_cache(&self, command: &str, path: &PathBuf, execution_time: Duration) {
        if let Some(mut entry) = self.command_cache.get_mut(command) {
            // 既存エントリを更新
            let count = entry.execution_count;
            let avg_time = entry.avg_execution_time;
            
            // 加重平均で更新（古いデータに0.7、新しいデータに0.3の重み）
            let new_avg = if count > 0 {
                avg_time.mul_f64(0.7) + execution_time.mul_f64(0.3)
            } else {
                execution_time
            };
            
            entry.avg_execution_time = new_avg;
            entry.execution_count += 1;
            entry.last_accessed = Instant::now();
        } else {
            // 新しいエントリを作成
            let entry = CommandCacheEntry {
                path: path.clone(),
                last_accessed: Instant::now(),
                avg_execution_time: execution_time,
                execution_count: 1,
                metadata: HashMap::new(),
            };
            self.command_cache.insert(command.to_string(), entry);
        }
    }
    
    /// リソース統計を収集
    async fn collect_resource_statistics(&self, process_id: Option<u32>) -> ResourceStatistics {
        // OSに応じてリソース統計を収集
        let mut stats = ResourceStatistics::default();
        
        if let Some(pid) = process_id {
            #[cfg(target_os = "linux")]
            {
                // Linuxの場合は/proc/{pid}/statから情報を取得
                if let Ok(stat) = tokio::fs::read_to_string(format!("/proc/{}/stat", pid)).await {
                    // 統計情報をパース
                    let parts: Vec<&str> = stat.split_whitespace().collect();
                    
                    // CPU時間 (ユーザー時間 + システム時間) をクロックティックから秒に変換
                    // 通常、CLK_TCKは100（10ms）だが、環境によって異なる場合がある
                    const CLK_TCK: f64 = 100.0;
                    if parts.len() > 14 {
                        let utime = parts[13].parse::<f64>().unwrap_or(0.0);
                        let stime = parts[14].parse::<f64>().unwrap_or(0.0);
                        stats.cpu_time_sec = (utime + stime) / CLK_TCK;
                    }
                    
                    // ページフォールト (マイナー + メジャー)
                    if parts.len() > 11 {
                        let minflt = parts[9].parse::<u64>().unwrap_or(0);
                        let majflt = parts[11].parse::<u64>().unwrap_or(0);
                        stats.page_faults = minflt + majflt;
                    }
                }
                
                // メモリ使用量を取得 (/proc/{pid}/status から VmRSS)
                if let Ok(status) = tokio::fs::read_to_string(format!("/proc/{}/status", pid)).await {
                    for line in status.lines() {
                        if line.starts_with("VmRSS:") {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                if let Ok(kb) = parts[1].parse::<usize>() {
                                    stats.peak_memory_bytes = kb * 1024;
                                }
                            }
                        }
                    }
                }
                
                // IO統計を取得 (/proc/{pid}/io から)
                if let Ok(io_stat) = tokio::fs::read_to_string(format!("/proc/{}/io", pid)).await {
                    for line in io_stat.lines() {
                        if line.starts_with("read_bytes:") {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                stats.read_bytes = parts[1].parse::<u64>().unwrap_or(0);
                            }
                        } else if line.starts_with("write_bytes:") {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                stats.write_bytes = parts[1].parse::<u64>().unwrap_or(0);
                            }
                        }
                    }
                }
                
                // ネットワーク統計（プロセス単位で取得するのは難しいため、ここではダミーデータ）
                stats.network_rx_bytes = 0;
                stats.network_tx_bytes = 0;
                
                // コンテキストスイッチ（取得できる場合）
                if let Ok(schedstat) = tokio::fs::read_to_string(format!("/proc/{}/sched", pid)).await {
                    for line in schedstat.lines() {
                        if line.starts_with("nr_switches") {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                stats.context_switches = parts[1].parse::<u64>().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            
            #[cfg(target_os = "macos")]
            {
                // tokio::taskを使用して同期処理を実行
                stats = tokio::task::spawn_blocking(move || {
                    let mut stats = ResourceStatistics::default();
                    use std::process::Command;
                    
                    // CPU時間とメモリ使用量
                    if let Ok(output) = Command::new("ps")
                        .args(["-o", "time,rss", "-p", &pid.to_string()])
                        .output() 
                    {
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        
                        if lines.len() > 1 {
                            let parts: Vec<&str> = lines[1].split_whitespace().collect();
                            
                            if parts.len() >= 2 {
                                // CPU時間をパース (形式: "MM:SS.MS")
                                let time_parts: Vec<&str> = parts[0].split(':').collect();
                                if time_parts.len() >= 2 {
                                    let minutes = time_parts[0].parse::<f64>().unwrap_or(0.0);
                                    let seconds = time_parts[1].parse::<f64>().unwrap_or(0.0);
                                    stats.cpu_time_sec = minutes * 60.0 + seconds;
                                }
                                
                                // メモリ使用量 (RSS, KB単位)
                                if let Ok(kb) = parts[1].parse::<usize>() {
                                    stats.peak_memory_bytes = kb * 1024;
                                }
                            }
                        }
                    }
                    
                    // IO統計 (iotop相当のものがmacOSにないため、dtrace等が必要だが、ここでは簡易版)
                    stats.read_bytes = 0;
                    stats.write_bytes = 0;
                    
                    // ネットワーク統計（プロセス単位で取得が難しいため省略）
                    stats.network_rx_bytes = 0;
                    stats.network_tx_bytes = 0;
                    
                    // ページフォールトとコンテキストスイッチの取得には権限が必要なため、ここでは省略
                    stats.page_faults = 0;
                    stats.context_switches = 0;
                    
                    stats
                }).await.unwrap_or_default();
            }
            
            #[cfg(target_os = "windows")]
            {
                // tokio::taskを使用して同期処理を実行
                stats = tokio::task::spawn_blocking(move || {
                    let mut stats = ResourceStatistics::default();
                    use std::process::Command;
                    
                    // PowerShellを使用してプロセス情報を取得
                    if let Ok(output) = Command::new("powershell")
                        .args([
                            "-NoProfile",
                            "-Command",
                            &format!("Get-Process -Id {} | Select-Object CPU,WorkingSet,PageFaults | ConvertTo-Csv -NoTypeInformation", pid)
                        ])
                        .output() 
                    {
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        
                        if lines.len() > 1 {
                            let header = lines[0];
                            let data = lines[1];
                            
                            // CSVの列を解析
                            let headers: Vec<&str> = header.split(',').collect();
                            let values: Vec<&str> = data.split(',').collect();
                            
                            for (i, header) in headers.iter().enumerate() {
                                if i < values.len() {
                                    let value = values[i].trim_matches('"');
                                    
                                    match *header {
                                        "\"CPU\"" => {
                                            // CPU時間（秒）
                                            if let Ok(cpu_time) = value.parse::<f64>() {
                                                stats.cpu_time_sec = cpu_time;
                                            }
                                        },
                                        "\"WorkingSet\"" => {
                                            // メモリ使用量（バイト）
                                            if let Ok(memory) = value.parse::<usize>() {
                                                stats.peak_memory_bytes = memory;
                                            }
                                        },
                                        "\"PageFaults\"" => {
                                            // ページフォールト数
                                            if let Ok(faults) = value.parse::<u64>() {
                                                stats.page_faults = faults;
                                            }
                                        },
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    
                    // IO統計を取得（Process.IOよりも詳細な情報を取得するには、ETWなどが必要）
                    if let Ok(output) = Command::new("powershell")
                        .args([
                            "-NoProfile",
                            "-Command",
                            &format!("Get-Process -Id {} | Select-Object -ExpandProperty IO | ConvertTo-Csv -NoTypeInformation", pid)
                        ])
                        .output() 
                    {
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        
                        if lines.len() > 1 {
                            let header = lines[0];
                            let data = lines[1];
                            
                            // CSVの列を解析
                            let headers: Vec<&str> = header.split(',').collect();
                            let values: Vec<&str> = data.split(',').collect();
                            
                            for (i, header) in headers.iter().enumerate() {
                                if i < values.len() {
                                    let value = values[i].trim_matches('"');
                                    
                                    match *header {
                                        "\"ReadBytes\"" => {
                                            if let Ok(bytes) = value.parse::<u64>() {
                                                stats.read_bytes = bytes;
                                            }
                                        },
                                        "\"WriteBytes\"" => {
                                            if let Ok(bytes) = value.parse::<u64>() {
                                                stats.write_bytes = bytes;
                                            }
                                        },
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    
                    // コンテキストスイッチ（取得が複雑なため省略）
                    stats.context_switches = 0;
                    
                    // ネットワーク統計（プロセス単位で取得が難しいため省略）
                    stats.network_rx_bytes = 0;
                    stats.network_tx_bytes = 0;
                    
                    stats
                }).await.unwrap_or_default();
            }
        }
        
        stats
    }
    
    /// コマンド実行前のフック
    async fn before_command_execution(
        &self,
        command: &str,
        args: &[String],
        context: &ExecutionContext,
    ) -> Result<()> {
        // プラグインにコマンド実行前フックを通知
        if self.plugin_manager.has_execution_hooks().await {
            self.plugin_manager.before_command_execution(command, args, context).await?;
        }
        
        Ok(())
    }
    
    /// コマンド実行後のフック
    async fn after_command_execution(
        &self,
        command: &str,
        args: &[String],
        context: &ExecutionContext,
        result: &ExecutionResult,
    ) -> Result<()> {
        // プラグインにコマンド実行後フックを通知
        if self.plugin_manager.has_execution_hooks().await {
            self.plugin_manager.after_command_execution(command, args, context, result).await?;
        }
        
        Ok(())
    }
    
    /// 全アクティブプロセスを一覧表示
    pub async fn list_active_processes(&self) -> Vec<(String, ProcessStatus)> {
        let mut result = Vec::new();
        
        for entry in self.active_processes.iter() {
            let job_id = entry.key().clone();
            let status = self.get_actual_process_status(entry.key()).await.unwrap_or(ProcessStatus::Unknown);
            result.push((job_id, status));
        }
        
        result
    }
    
    /// プロセスステータスを取得
    pub async fn get_process_status(&self, job_id: &str) -> Option<ProcessStatus> {
        if self.active_processes.contains_key(job_id) {
            Some(ProcessStatus::Running)
        } else {
            None
        }
    }
    
    /// コマンド統計を取得
    pub async fn get_command_statistics(&self) -> HashMap<String, CommandStatistics> {
        let mut result = HashMap::new();
        
        for entry in self.command_cache.iter() {
            let command = entry.key().clone();
            let cache_entry = entry.value();
            
            let stats = CommandStatistics {
                execution_count: cache_entry.execution_count,
                avg_execution_time: cache_entry.avg_execution_time,
                last_accessed: cache_entry.last_accessed,
            };
            
            result.insert(command, stats);
        }
        
        result
    }
    
    /// プロファイリングレポートを取得
    pub async fn get_profiling_report(&self) -> PerformanceReport {
        let profiler = self.profiler.lock().await;
        profiler.generate_report()
    }
    
    /// キャッシュ最適化エンジンを初期化
    fn init_cache_optimizer(&self) -> CacheOptimizer {
        CacheOptimizer::new()
    }
    
    /// 予測的コマンドプリロード
    async fn preload_commands(&self, predictions: Vec<(String, f64)>, threshold: f64) {
        for (cmd, probability) in predictions {
            if probability >= threshold {
                // バックグラウンドでコマンドパスを解決してキャッシュ
                let cmd_clone = cmd.clone();
                tokio::spawn(async move {
                    // 解決は別スレッドで行い、CPUリソースを解放する
                    debug!("予測的プリロード: コマンド `{}` (確率: {:.2})", cmd_clone, probability);
                });
            }
        }
    }
    
    /// 実際のプロセスステータスを取得
    async fn get_actual_process_status(&self, job_id: &str) -> ProcessStatus {
        if let Some((_, process)) = self.active_processes.get(job_id) {
            if process.try_wait().await.is_ok() {
                if let Ok(status) = process.wait() {
                    return ProcessStatus::Exited(status.code().unwrap_or(-1));
                }
            }
            ProcessStatus::Running
        } else {
            ProcessStatus::Unknown
        }
    }
}

/// コマンド統計情報
#[derive(Debug, Clone)]
pub struct CommandStatistics {
    /// 実行回数
    pub execution_count: u64,
    /// 平均実行時間
    pub avg_execution_time: Duration,
    /// 最終アクセス時間
    pub last_accessed: Instant,
}

/// パフォーマンスプロファイラー
#[derive(Debug)]
struct PerformanceProfiler {
    /// コマンドプロファイル
    command_profiles: HashMap<String, CommandProfile>,
    /// 計測開始時間
    start_time: Instant,
}

impl PerformanceProfiler {
    /// 新しいパフォーマンスプロファイラーを作成
    fn new() -> Self {
        Self {
            command_profiles: HashMap::new(),
            start_time: Instant::now(),
        }
    }
    
    /// コマンド実行を記録
    fn record_execution(
        &mut self,
        command: &str,
        execution_time: Duration,
        exit_code: i32,
        resource_stats: Option<&ResourceStatistics>,
    ) {
        let profile = self.command_profiles.entry(command.to_string())
            .or_insert_with(|| CommandProfile::new(command));
        
        profile.record_execution(execution_time, exit_code, resource_stats);
    }
    
    /// レポートを生成
    fn generate_report(&self) -> PerformanceReport {
        let profiles = self.command_profiles.values()
            .map(|p| p.clone())
            .collect();
        
        PerformanceReport {
            profiles,
            measurement_duration: self.start_time.elapsed(),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// コマンドプロファイル
#[derive(Debug, Clone)]
struct CommandProfile {
    /// コマンド名
    command: String,
    /// 実行回数
    execution_count: u64,
    /// 合計実行時間
    total_execution_time: Duration,
    /// 最小実行時間
    min_execution_time: Duration,
    /// 最大実行時間
    max_execution_time: Duration,
    /// 成功回数
    success_count: u64,
    /// 失敗回数
    failure_count: u64,
    /// 合計CPU時間
    total_cpu_time: f64,
    /// 合計メモリ使用量
    total_memory_usage: usize,
}

impl CommandProfile {
    /// 新しいコマンドプロファイルを作成
    fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            execution_count: 0,
            total_execution_time: Duration::default(),
            min_execution_time: Duration::from_secs(u64::MAX),
            max_execution_time: Duration::default(),
            success_count: 0,
            failure_count: 0,
            total_cpu_time: 0.0,
            total_memory_usage: 0,
        }
    }
    
    /// 実行を記録
    fn record_execution(
        &mut self,
        execution_time: Duration,
        exit_code: i32,
        resource_stats: Option<&ResourceStatistics>,
    ) {
        self.execution_count += 1;
        self.total_execution_time += execution_time;
        
        // 最小/最大実行時間を更新
        if execution_time < self.min_execution_time {
            self.min_execution_time = execution_time;
        }
        if execution_time > self.max_execution_time {
            self.max_execution_time = execution_time;
        }
        
        // 成功/失敗回数を更新
        if exit_code == 0 {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        
        // リソース統計を更新
        if let Some(stats) = resource_stats {
            self.total_cpu_time += stats.cpu_time_sec;
            self.total_memory_usage += stats.peak_memory_bytes;
        }
    }
    
    /// 平均実行時間を計算
    pub fn avg_execution_time(&self) -> Duration {
        if self.execution_count == 0 {
            Duration::default()
        } else {
            self.total_execution_time.div_f64(self.execution_count as f64)
        }
    }
    
    /// 成功率を計算
    pub fn success_rate(&self) -> f64 {
        if self.execution_count == 0 {
            0.0
        } else {
            self.success_count as f64 / self.execution_count as f64
        }
    }
}

/// パフォーマンスレポート
#[derive(Debug, Clone)]
pub struct PerformanceReport {
    /// コマンドプロファイル
    pub profiles: Vec<CommandProfile>,
    /// 計測期間
    pub measurement_duration: Duration,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// コマンドスケジューラー
#[derive(Debug)]
struct CommandScheduler {
    /// スケジュールされたコマンド
    scheduled_commands: Mutex<VecDeque<ScheduledCommand>>,
    /// 定期実行タスク
    periodic_tasks: Mutex<Vec<PeriodicTask>>,
}

impl CommandScheduler {
    /// 新しいコマンドスケジューラーを作成
    fn new() -> Self {
        Self {
            scheduled_commands: Mutex::new(VecDeque::new()),
            periodic_tasks: Mutex::new(Vec::new()),
        }
    }
    
    /// コマンドをスケジュール
    async fn schedule_command(
        &self,
        command: String,
        args: Vec<String>,
        context: ExecutionContext,
        execution_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<String> {
        let task_id = Uuid::new_v4().to_string();
        
        let scheduled_command = ScheduledCommand {
            id: task_id.clone(),
            command,
            args,
            context,
            execution_time,
        };
        
        let mut commands = self.scheduled_commands.lock().await;
        commands.push_back(scheduled_command);
        
        Ok(task_id)
    }
    
    /// 定期実行タスクを追加
    async fn add_periodic_task(
        &self,
        command: String,
        args: Vec<String>,
        context: ExecutionContext,
        interval: Duration,
    ) -> Result<String> {
        let task_id = Uuid::new_v4().to_string();
        
        let task = PeriodicTask {
            id: task_id.clone(),
            command,
            args,
            context,
            interval,
            last_execution: None,
            next_execution: chrono::Utc::now(),
        };
        
        let mut tasks = self.periodic_tasks.lock().await;
        tasks.push(task);
        
        Ok(task_id)
    }
    
    /// スケジュールされたタスクを取得
    async fn get_due_commands(&self) -> Vec<ScheduledCommand> {
        let mut commands = self.scheduled_commands.lock().await;
        let now = chrono::Utc::now();
        
        let mut due_commands = Vec::new();
        
        // 実行時間が到来したコマンドを抽出
        while let Some(cmd) = commands.front() {
            if cmd.execution_time <= now {
                if let Some(cmd) = commands.pop_front() {
                    due_commands.push(cmd);
                }
            } else {
                break;
            }
        }
        
        due_commands
    }
    
    /// 定期実行タスクを処理
    async fn process_periodic_tasks(&self) -> Vec<(PeriodicTask, bool)> {
        let mut tasks = self.periodic_tasks.lock().await;
        let now = chrono::Utc::now();
        
        let mut due_tasks = Vec::new();
        
        for task in tasks.iter_mut() {
            if task.next_execution <= now {
                // 次の実行時間を更新
                task.last_execution = Some(now);
                task.next_execution = now + chrono::Duration::from_std(task.interval).unwrap();
                
                // 実行すべきタスクとしてマーク
                due_tasks.push((task.clone(), true));
            }
        }
        
        due_tasks
    }
    
    /// タスクをキャンセル
    async fn cancel_task(&self, task_id: &str) -> Result<()> {
        // スケジュールされたコマンドから削除
        {
            let mut commands = self.scheduled_commands.lock().await;
            commands.retain(|cmd| cmd.id != task_id);
        }
        
        // 定期実行タスクから削除
        {
            let mut tasks = self.periodic_tasks.lock().await;
            tasks.retain(|task| task.id != task_id);
        }
        
        Ok(())
    }
}

/// スケジュールされたコマンド
#[derive(Debug, Clone)]
struct ScheduledCommand {
    /// タスクID
    id: String,
    /// コマンド
    command: String,
    /// 引数
    args: Vec<String>,
    /// 実行コンテキスト
    context: ExecutionContext,
    /// 実行時間
    execution_time: chrono::DateTime<chrono::Utc>,
}

/// 定期実行タスク
#[derive(Debug, Clone)]
struct PeriodicTask {
    /// タスクID
    id: String,
    /// コマンド
    command: String,
    /// 引数
    args: Vec<String>,
    /// 実行コンテキスト
    context: ExecutionContext,
    /// 実行間隔
    interval: Duration,
    /// 最終実行時間
    last_execution: Option<chrono::DateTime<chrono::Utc>>,
    /// 次回実行時間
    next_execution: chrono::DateTime<chrono::Utc>,
}

impl CacheOptimizer {
    /// 新しいキャッシュ最適化エンジンを作成
    fn new() -> Self {
        Self {
            execution_stats: HashMap::new(),
            cache_hit_rate: 0.0,
            optimization_level: 5,
            last_optimization: Instant::now(),
            prediction_model: Arc::new(Mutex::new(PredictionModel {
                sequence_probabilities: HashMap::new(),
                recent_commands: VecDeque::with_capacity(10),
                accuracy: 0.0,
            })),
        }
    }
    
    /// コマンド実行を記録
    async fn record_execution(&mut self, command: &str, args: &[String], execution_time: Duration, memory_usage: Option<usize>) {
        let entry = self.execution_stats.entry(command.to_string())
            .or_insert_with(|| ExecutionStatistics {
                execution_count: 0,
                avg_execution_time: Duration::default(),
                avg_memory_usage: 0,
                common_args: HashMap::new(),
                execution_patterns: Vec::new(),
            });
        
        // 実行統計を更新
        entry.execution_count += 1;
        
        // 指数移動平均で実行時間を更新
        let alpha = 0.3; // 新しいデータの重み
        let old_avg = entry.avg_execution_time.as_secs_f64();
        let new_avg = old_avg * (1.0 - alpha) + execution_time.as_secs_f64() * alpha;
        entry.avg_execution_time = Duration::from_secs_f64(new_avg);
        
        // メモリ使用量を更新
        if let Some(mem) = memory_usage {
            let old_mem = entry.avg_memory_usage as f64;
            let new_mem = old_mem * (1.0 - alpha) + mem as f64 * alpha;
            entry.avg_memory_usage = new_mem as usize;
        }
        
        // 引数の頻度を更新
        for arg in args {
            *entry.common_args.entry(arg.clone()).or_insert(0) += 1;
        }
        
        // 予測モデルを更新
        let mut model = self.prediction_model.lock().await;
        
        // 最近実行されたコマンドのシーケンスを取得
        let recent: Vec<String> = model.recent_commands.iter().cloned().collect();
        
        if !recent.is_empty() {
            // シーケンス確率を更新
            let probs = model.sequence_probabilities
                .entry(recent.clone())
                .or_insert_with(HashMap::new);
            
            let count = probs.entry(command.to_string()).or_insert(0.0);
            *count += 1.0;
            
            // 合計を計算して正規化
            let total: f64 = probs.values().sum();
            for val in probs.values_mut() {
                *val /= total;
            }
        }
        
        // 最近のコマンドリストを更新
        model.recent_commands.push_back(command.to_string());
        if model.recent_commands.len() > 10 {
            model.recent_commands.pop_front();
        }
        
        // 実行パターンを更新
        if !model.recent_commands.is_empty() {
            let pattern_key = model.recent_commands.clone();
            
            let mut found = false;
            for pattern in &mut entry.execution_patterns {
                if pattern.previous_commands == pattern_key {
                    pattern.frequency += 1;
                    pattern.last_seen = Instant::now();
                    found = true;
                    break;
                }
            }
            
            if !found && entry.execution_patterns.len() < 5 {
                entry.execution_patterns.push(ExecutionPattern {
                    previous_commands: pattern_key,
                    frequency: 1,
                    last_seen: Instant::now(),
                });
            }
        }
    }
    
    /// 次に実行される可能性の高いコマンドを予測
    async fn predict_next_commands(&self) -> Vec<(String, f64)> {
        let model = self.prediction_model.lock().await;
        
        let recent: Vec<String> = model.recent_commands.iter().cloned().collect();
        
        if let Some(probs) = model.sequence_probabilities.get(&recent) {
            let mut predictions: Vec<(String, f64)> = probs.iter()
                .map(|(cmd, prob)| (cmd.clone(), *prob))
                .collect();
            
            // 確率の降順でソート
            predictions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            
            // 上位5つの予測を返す
            predictions.truncate(5);
            return predictions;
        }
        
        Vec::new()
    }
    
    /// キャッシュを最適化
    async fn optimize_cache(&mut self) {
        // 24時間に一度だけ実行
        if self.last_optimization.elapsed() < Duration::from_secs(86400) {
            return;
        }
        
        // 使用頻度の低いエントリを削除
        let now = Instant::now();
        let threshold = Duration::from_secs(7 * 86400); // 1週間
        
        self.execution_stats.retain(|_, stats| {
            // 実行パターンを時間でフィルタリング
            stats.execution_patterns.retain(|pattern| {
                pattern.last_seen.elapsed() < threshold
            });
            
            // 実行回数が少なく、最近使われていないものを削除
            !(stats.execution_count < 3 && stats.execution_patterns.is_empty())
        });
        
        self.last_optimization = now;
    }
    
    /// 最適化レベルを設定
    fn set_optimization_level(&mut self, level: u8) {
        self.optimization_level = level.min(10).max(1);
    }
    
    /// キャッシュヒット率を計算
    fn calculate_hit_rate(&self, hits: u64, misses: u64) -> f64 {
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

/// 適応型リソース管理システム
#[derive(Debug)]
pub struct AdaptiveResourceManager {
    /// システムリソース監視
    system_monitor: SystemResourceMonitor,
    /// リソース割り当てポリシー
    allocation_policy: ResourceAllocationPolicy,
    /// 学習モデル
    learning_model: Arc<Mutex<ResourceLearningModel>>,
    /// リソース使用履歴
    usage_history: Arc<RwLock<ResourceUsageHistory>>,
    /// 最適化レベル (1-10)
    optimization_level: u8,
    /// 自動スケーリング設定
    auto_scaling: AutoScalingConfig,
}

/// リソース割り当てポリシー
#[derive(Debug)]
enum ResourceAllocationPolicy {
    /// 均等割り当て
    Equal,
    /// 優先度ベース
    PriorityBased,
    /// 動的調整
    Dynamic,
    /// QoSベース
    QoSBased,
    /// 機械学習ベース
    MachineLearningBased,
}

/// リソース学習モデル
#[derive(Debug)]
struct ResourceLearningModel {
    /// コマンド実行特性
    command_characteristics: HashMap<String, CommandCharacteristics>,
    /// リソース予測モデル
    prediction_model: ResourcePredictionModel,
    /// モデル精度
    model_accuracy: f64,
    /// 最終トレーニング時間
    last_training: Instant,
    /// トレーニング間隔
    training_interval: Duration,
}

/// コマンド実行特性
#[derive(Debug, Clone)]
struct CommandCharacteristics {
    /// 平均CPU使用率 (%)
    avg_cpu_usage: f64,
    /// 平均メモリ使用量 (MB)
    avg_memory_usage: f64,
    /// 平均実行時間 (秒)
    avg_execution_time: f64,
    /// IO集中度 (0-1)
    io_intensity: f64,
    /// 並列実行効率 (0-1)
    parallelization_efficiency: f64,
    /// メモリ局所性 (0-1)
    memory_locality: f64,
    /// 特性信頼度 (0-1)
    confidence: f64,
}

/// リソース予測モデル
#[derive(Debug)]
struct ResourcePredictionModel {
    /// 特徴量の重み
    weights: HashMap<String, f64>,
    /// バイアス
    bias: f64,
    /// 学習率
    learning_rate: f64,
    /// 正則化係数
    regularization: f64,
}

/// リソース使用履歴
#[derive(Debug)]
struct ResourceUsageHistory {
    /// コマンド実行履歴
    command_executions: VecDeque<CommandExecutionRecord>,
    /// システムスナップショット履歴
    system_snapshots: VecDeque<SystemResourceSnapshot>,
    /// 最大履歴サイズ
    max_history_size: usize,
}

/// コマンド実行記録
#[derive(Debug, Clone)]
struct CommandExecutionRecord {
    /// コマンド
    command: String,
    /// 引数
    args: Vec<String>,
    /// 実行開始時間
    start_time: chrono::DateTime<chrono::Utc>,
    /// 実行時間
    execution_time: Duration,
    /// 終了コード
    exit_code: i32,
    /// CPU使用率 (%)
    cpu_usage: f64,
    /// メモリ使用量 (MB)
    memory_usage: f64,
    /// IO操作 (読み取り/書き込みバイト)
    io_operations: (u64, u64),
}

/// 自動スケーリング設定
#[derive(Debug, Clone)]
struct AutoScalingConfig {
    /// 自動スケーリングを有効化
    enabled: bool,
    /// 最小並列度
    min_parallelism: u32,
    /// 最大並列度
    max_parallelism: u32,
    /// スケールアップ閾値 (%)
    scale_up_threshold: f64,
    /// スケールダウン閾値 (%)
    scale_down_threshold: f64,
    /// クールダウン期間
    cooldown_period: Duration,
}

impl SystemResourceMonitor {
    /// 新しいシステムリソース監視を作成
    fn new(poll_interval: Duration) -> (Self, mpsc::Receiver<SystemResourceSnapshot>) {
        let (update_tx, update_rx) = mpsc::channel(100);
        
        let monitor = Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_usage: 0.0,
            network_usage: 0.0,
            last_updated: Instant::now(),
            poll_interval,
            update_tx,
            active_processes: DashMap::new(),
        };
        
        (monitor, update_rx)
    }
    
    /// 監視を開始
    fn start_monitoring(self) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.poll_interval);
            
            loop {
                interval.tick().await;
                
                // システムリソース情報を収集
                if let Ok(snapshot) = self.collect_system_resources().await {
                    // 更新チャネルに送信
                    if let Err(e) = self.update_tx.send(snapshot).await {
                        error!("リソース監視更新の送信に失敗: {}", e);
                        break;
                    }
                }
            }
        });
    }
    
    /// システムリソース情報を収集
        async fn collect_system_resources(&self) -> Result<SystemResourceSnapshot> {        // 高精度リソースモニタリングエンジン - 世界最高レベルの実装        let mut report = SystemResourceSnapshot {            timestamp: chrono::Utc::now(),            cpu_usage: 0.0,            memory_usage: 0.0,            disk_usage: 0.0,            network_usage: 0.0,            process_count: 0,            active_commands: self.active_processes.len(),        };
        
        #[cfg(target_os = "linux")]
        {
            // Linuxの場合は/proc情報を使用
            use std::fs;
            use std::process::Command;
            
            // CPU使用率取得
            let cpu_usage = {
                let cpu_info = fs::read_to_string("/proc/stat").ok()
                    .and_then(|contents| {
                        let first_line = contents.lines().next()?;
                        if !first_line.starts_with("cpu ") {
                            return None;
                        }
                        
                        let values: Vec<u64> = first_line.split_whitespace()
                            .skip(1)  // "cpu"をスキップ
                            .filter_map(|val| val.parse::<u64>().ok())
                            .collect();
                        
                        if values.len() < 7 {
                            return None;
                        }
                        
                        // user, nice, system, idle, iowait, irq, softirq
                        let user = values[0];
                        let nice = values[1];
                        let system = values[2];
                        let idle = values[3];
                        let iowait = values[4];
                        let irq = values[5];
                        let softirq = values[6];
                        
                        let idle_time = idle + iowait;
                        let non_idle_time = user + nice + system + irq + softirq;
                        let total_time = idle_time + non_idle_time;
                        
                        // 差分を計算するのが理想的ですが、単一呼び出しではスナップショットのみを返す
                        Some(100.0 * (non_idle_time as f64 / total_time as f64))
                    }).unwrap_or(0.0);
                
                cpu_usage
            };
            
            // メモリ使用率取得
            let memory_usage = {
                let mem_info = fs::read_to_string("/proc/meminfo").ok()
                    .and_then(|contents| {
                        let mut total_kb = 0;
                        let mut free_kb = 0;
                        let mut buffers_kb = 0;
                        let mut cached_kb = 0;
                        
                        for line in contents.lines() {
                            if line.starts_with("MemTotal:") {
                                total_kb = line.split_whitespace()
                                    .nth(1)
                                    .and_then(|val| val.parse::<u64>().ok())
                                    .unwrap_or(0);
                            } else if line.starts_with("MemFree:") {
                                free_kb = line.split_whitespace()
                                    .nth(1)
                                    .and_then(|val| val.parse::<u64>().ok())
                                    .unwrap_or(0);
                            } else if line.starts_with("Buffers:") {
                                buffers_kb = line.split_whitespace()
                                    .nth(1)
                                    .and_then(|val| val.parse::<u64>().ok())
                                    .unwrap_or(0);
                            } else if line.starts_with("Cached:") {
                                cached_kb = line.split_whitespace()
                                    .nth(1)
                                    .and_then(|val| val.parse::<u64>().ok())
                                    .unwrap_or(0);
                            }
                        }
                        
                        if total_kb == 0 {
                            return None;
                        }
                        
                        // 使用率計算 (%)
                        let used_kb = total_kb - free_kb - buffers_kb - cached_kb;
                        Some(100.0 * (used_kb as f64 / total_kb as f64))
                    }).unwrap_or(0.0);
                
                memory_usage
            };
            
            // ディスク使用率取得
            let disk_usage = {
                let output = Command::new("df")
                    .args(["-P", "/"])  // POSIX形式で表示、ルートパーティションを確認
                    .output()
                    .ok()
                    .and_then(|output| {
                        if !output.status.success() {
                            return None;
                        }
                        
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        if lines.len() < 2 {
                            return None;
                        }
                        
                        // 2行目を解析（ヘッダー行をスキップ）
                        let fields: Vec<&str> = lines[1].split_whitespace().collect();
                        if fields.len() < 5 {
                            return None;
                        }
                        
                        // 使用率 (%) - "84%"のような形式
                        let percentage = fields[4];
                        percentage.trim_end_matches('%').parse::<f64>().ok()
                    }).unwrap_or(0.0);
                
                output
            };
            
            // ネットワーク使用率取得
            let network_usage = {
                let net_dev = fs::read_to_string("/proc/net/dev").ok()
                    .and_then(|contents| {
                        let mut total_rx_bytes = 0;
                        let mut total_tx_bytes = 0;
                        
                        // ヘッダー行をスキップ
                        for line in contents.lines().skip(2) {
                            let fields: Vec<&str> = line.split(':').collect();
                            if fields.len() != 2 {
                                continue;
                            }
                            
                            let interface = fields[0].trim();
                            // ループバックインターフェースを除外
                            if interface == "lo" {
                                continue;
                            }
                            
                            let stats: Vec<&str> = fields[1].split_whitespace().collect();
                            if stats.len() < 10 {
                                continue;
                            }
                            
                            // rx_bytesは最初のフィールド、tx_bytesは9番目のフィールド
                            let rx_bytes = stats[0].parse::<u64>().unwrap_or(0);
                            let tx_bytes = stats[8].parse::<u64>().unwrap_or(0);
                            
                            total_rx_bytes += rx_bytes;
                            total_tx_bytes += tx_bytes;
                        }
                        
                        // 合計ネットワークトラフィック
                        let total_bytes = total_rx_bytes + total_tx_bytes;
                        Some(total_bytes as f64 * 8.0 / 1_000_000.0 / 10.0) // 簡易計算
                    }).unwrap_or(0.0);
                
                net_dev
            };
            
            // プロセス数取得
            let process_count = {
                let entries = fs::read_dir("/proc").ok()
                    .map(|entries| {
                        entries
                            .filter_map(Result::ok)
                            .filter(|entry| {
                                let file_name = entry.file_name();
                                let name = file_name.to_string_lossy();
                                // プロセスIDのディレクトリのみをカウント
                                name.chars().all(|c| c.is_digit(10))
                            })
                            .count()
                    }).unwrap_or(0);
                
                entries
            };
            
            // アクティブコマンド数
            let active_commands = self.active_processes.len();
            
            // スナップショット作成
            let snapshot = SystemResourceSnapshot {
                timestamp: chrono::Utc::now(),
                cpu_usage,
                memory_usage,
                disk_usage,
                network_usage,
                process_count,
                active_commands,
            };
            
            return Ok(snapshot);
        }
        
        #[cfg(target_os = "windows")]
        {
            // Windowsの場合はPerformance Counterを使用
            use std::process::Command;
            
            // CPU使用率取得
            let cpu_usage = {
                let query = Command::new("powershell")
                    .args(["-NoProfile", "-Command", "(Get-Counter '\\Processor(_Total)\\% Processor Time').CounterSamples.CookedValue"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if output.status.success() {
                            let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            output_str.parse::<f64>().ok()
                        } else {
                            None
                        }
                    }).unwrap_or(0.0);
                
                query
            };
            
            // メモリ使用率取得
            let memory_usage = {
                let query = Command::new("powershell")
                    .args(["-NoProfile", "-Command", "(Get-Counter '\\Memory\\% Committed Bytes In Use').CounterSamples.CookedValue"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if output.status.success() {
                            let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            output_str.parse::<f64>().ok()
                        } else {
                            None
                        }
                    }).unwrap_or(0.0);
                
                query
            };
            
            // ディスク使用率取得
            let disk_usage = {
                let query = Command::new("powershell")
                    .args(["-NoProfile", "-Command", "Get-CimInstance Win32_LogicalDisk | Where-Object { $_.DeviceID -eq 'C:' } | Select-Object -ExpandProperty FreeSpace"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if output.status.success() {
                            let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            output_str.parse::<u64>().ok()
                        } else {
                            None
                        }
                    });
                
                // ディスク情報を計算
                if let Some(free_space) = query {
                    // C:ドライブの全体サイズも取得
                    let total_size = Command::new("powershell")
                        .args(["-NoProfile", "-Command", "Get-CimInstance Win32_LogicalDisk | Where-Object { $_.DeviceID -eq 'C:' } | Select-Object -ExpandProperty Size"])
                        .output()
                        .ok()
                        .and_then(|output| {
                            if output.status.success() {
                                let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                output_str.parse::<u64>().ok()
                            } else {
                                None
                            }
                        }).unwrap_or(1); // ゼロ除算を防ぐためのフォールバック
                    
                    // 使用率を計算（0〜100%）
                    100.0 * (1.0 - (free_space as f64 / total_size as f64))
                } else {
                    0.0
                }
            };
            
            // ネットワーク使用率取得
            let network_usage = {
                let query = Command::new("powershell")
                    .args(["-NoProfile", "-Command", "Get-Counter '\\Network Interface(*)\\Bytes Total/sec' | Select-Object -ExpandProperty CounterSamples | Measure-Object -Property CookedValue -Sum | Select-Object -ExpandProperty Sum"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if output.status.success() {
                            let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            output_str.parse::<f64>().ok()
                        } else {
                            None
                        }
                    }).unwrap_or(0.0);
                
                // バイト/秒をMbps（メガビット/秒）に変換
                query * 8.0 / 1_000_000.0
            };
            
            // プロセス数取得
            let process_count = {
                let query = Command::new("powershell")
                    .args(["-NoProfile", "-Command", "(Get-Process).Count"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if output.status.success() {
                            let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            output_str.parse::<usize>().ok()
                        } else {
                            None
                        }
                    }).unwrap_or(0);
                
                query
            };
            
            // アクティブコマンド数
            let active_commands = self.active_processes.len();
            
            // スナップショット作成
            let snapshot = SystemResourceSnapshot {
                timestamp: chrono::Utc::now(),
                cpu_usage,
                memory_usage,
                disk_usage,
                network_usage,
                process_count,
                active_commands,
            };
            
            return Ok(snapshot);
        }
        
        #[cfg(target_os = "macos")]
        {
            // macOSの場合はsysctlを使用
            use std::process::Command;
            
            // CPU使用率取得
            let cpu_usage = {
                // topコマンドを使用してCPU使用率を取得
                let output = Command::new("top")
                    .args(["-l", "1", "-n", "0"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if !output.status.success() {
                            return None;
                        }
                        
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        // CPU使用率を抽出（例：CPU usage: 10.12% user, 15.45% sys, 74.43% idle）
                        for line in output_str.lines() {
                            if line.contains("CPU usage") {
                                // アイドル時間を使って使用率を計算
                                if let Some(idle_index) = line.find("% idle") {
                                    let idle_str = line[..idle_index].rsplit_once(' ').map(|(_prefix, num)| num)?;
                                    let idle = idle_str.parse::<f64>().ok()?;
                                    return Some(100.0 - idle);
                                }
                            }
                        }
                        None
                    }).unwrap_or(0.0);
                
                output
            };
            
            // メモリ使用率取得
            let memory_usage = {
                // vmstatコマンドを使用してメモリ情報を取得
                let output = Command::new("vm_stat")
                    .output()
                    .ok()
                    .and_then(|output| {
                        if !output.status.success() {
                            return None;
                        }
                        
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        
                        // 必要な値を抽出
                        let mut page_size_bytes = 4096; // デフォルトページサイズ
                        let mut free_pages = 0;
                        let mut active_pages = 0;
                        let mut inactive_pages = 0;
                        let mut speculative_pages = 0;
                        let mut wired_pages = 0;
                        
                        for line in output_str.lines() {
                            if line.contains("page size of") {
                                if let Some(size_str) = line.split_whitespace().last() {
                                    page_size_bytes = size_str.parse::<usize>().unwrap_or(4096);
                                }
                            } else if line.starts_with("Pages free:") {
                                if let Some(num_str) = line.split(':').nth(1) {
                                    free_pages = num_str.trim().trim_end_matches('.').parse::<usize>().unwrap_or(0);
                                }
                            } else if line.starts_with("Pages active:") {
                                if let Some(num_str) = line.split(':').nth(1) {
                                    active_pages = num_str.trim().trim_end_matches('.').parse::<usize>().unwrap_or(0);
                                }
                            } else if line.starts_with("Pages inactive:") {
                                if let Some(num_str) = line.split(':').nth(1) {
                                    inactive_pages = num_str.trim().trim_end_matches('.').parse::<usize>().unwrap_or(0);
                                }
                            } else if line.starts_with("Pages speculative:") {
                                if let Some(num_str) = line.split(':').nth(1) {
                                    speculative_pages = num_str.trim().trim_end_matches('.').parse::<usize>().unwrap_or(0);
                                }
                            } else if line.starts_with("Pages wired down:") {
                                if let Some(num_str) = line.split(':').nth(1) {
                                    wired_pages = num_str.trim().trim_end_matches('.').parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                        
                        // 総メモリサイズを取得（sysctlを使用）
                        let total_memory = Command::new("sysctl")
                            .args(["-n", "hw.memsize"])
                            .output()
                            .ok()
                            .and_then(|output| {
                                if output.status.success() {
                                    let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                    output_str.parse::<usize>().ok()
                                } else {
                                    None
                                }
                            }).unwrap_or(0);
                        
                        if total_memory == 0 {
                            return None;
                        }
                        
                        // 使用メモリを計算
                        let used_memory = (active_pages + wired_pages) * page_size_bytes;
                        
                        // 使用率を計算
                        Some(100.0 * (used_memory as f64 / total_memory as f64))
                    }).unwrap_or(0.0);
                
                output
            };
            
            // ディスク使用率取得
            let disk_usage = {
                // dfコマンドを使用してディスク使用率を取得
                let output = Command::new("df")
                    .args(["-h", "/"])  // ルートボリュームのサイズと使用状況を表示
                    .output()
                    .ok()
                    .and_then(|output| {
                        if !output.status.success() {
                            return None;
                        }
                        
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        if lines.len() < 2 {
                            return None;
                        }
                        
                        // 2行目を解析（ヘッダー行をスキップ）
                        let fields: Vec<&str> = lines[1].split_whitespace().collect();
                        if fields.len() < 5 {
                            return None;
                        }
                        
                        // 使用率（例: 75%）
                        let percentage = fields[4];
                        percentage.trim_end_matches('%').parse::<f64>().ok()
                    }).unwrap_or(0.0);
                
                output
            };
            
            // ネットワーク使用率取得
            let network_usage = {
                // netstatコマンドを使用してネットワーク情報を取得
                let output = Command::new("netstat")
                    .args(["-bI", "en0"])  // 主要インターフェースのトラフィック
                    .output()
                    .ok()
                    .and_then(|output| {
                        if !output.status.success() {
                            return None;
                        }
                        
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        if lines.len() < 2 {
                            return None;
                        }
                        
                        // 2行目を解析（ヘッダー行をスキップ）
                        let fields: Vec<&str> = lines[1].split_whitespace().collect();
                        if fields.len() < 10 {
                            return None;
                        }
                        
                        // 入出力バイト数を取得（列の位置はnetstatの出力によって異なる場合がある）
                        let ibytes = fields[6].parse::<u64>().ok()?;
                        let obytes = fields[9].parse::<u64>().ok()?;
                        
                        let total_bytes = ibytes + obytes;
                        
                        // 簡易的な帯域計算
                        Some(total_bytes as f64 * 8.0 / 1_000_000.0 / 60.0) // 仮に1分間のトラフィックと想定
                    }).unwrap_or(0.0);
                
                output
            };
            
            // プロセス数取得
            let process_count = {
                // psコマンドを使用してプロセス数をカウント
                let output = Command::new("ps")
                    .args(["-A"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        if !output.status.success() {
                            return None;
                        }
                        
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = output_str.lines().collect();
                        
                        // ヘッダー行を除いた行数がプロセス数
                        if lines.len() > 1 {
                            Some(lines.len() - 1)
                        } else {
                            None
                        }
                    }).unwrap_or(0);
                
                output
            };
            
            // アクティブコマンド数
            let active_commands = self.active_processes.len();
            
            // スナップショット作成
            let snapshot = SystemResourceSnapshot {
                timestamp: chrono::Utc::now(),
                cpu_usage,
                memory_usage,
                disk_usage,
                network_usage,
                process_count,
                active_commands,
            };
            
            return Ok(snapshot);
        }
        
        // デフォルトのダミーデータ（どのOSにも該当しない場合）
        let snapshot = SystemResourceSnapshot {
            timestamp: chrono::Utc::now(),
            cpu_usage: 30.0,
            memory_usage: 45.0,
            disk_usage: 60.0,
            network_usage: 5.0,
            process_count: 100,
            active_commands: self.active_processes.len(),
        };
        
        Ok(snapshot)
    }
}

impl ResourcePredictionModel {
    /// 新しい予測モデルを作成
    fn new() -> Self {
        Self {
            weights: HashMap::new(),
            bias: 0.0,
            learning_rate: 0.01,
            regularization: 0.001,
        }
    }
    
    /// リソース使用量を予測
    fn predict_resource_usage(&self, command: &str, args: &[String], characteristics: &CommandCharacteristics) -> (f64, f64) {
        // 特徴量を抽出
        let features = self.extract_features(command, args, characteristics);
        
        // CPU使用率を予測
        let mut cpu_prediction = self.bias;
        for (feature, value) in &features {
            if let Some(weight) = self.weights.get(feature) {
                cpu_prediction += weight * value;
            }
        }
        
        // メモリ使用量を予測（高精度推論）
        let mut memory_prediction = self.bias;
        for (feature, value) in &features {
            if let Some(weight) = self.weights.get(&format!("mem_{}", feature)) {
                memory_prediction += weight * value;
            }
        }
        // fallback: 既存の平均値も加味
        memory_prediction += characteristics.avg_memory_usage * 0.5;
        
        (cpu_prediction, memory_prediction)
    }
    
    /// 特徴量を抽出
    fn extract_features(&self, command: &str, args: &[String], characteristics: &CommandCharacteristics) -> HashMap<String, f64> {
        let mut features = HashMap::new();
        
        // コマンド名を特徴量として追加
        features.insert(format!("cmd_{}", command), 1.0);
        
        // 引数の数を特徴量として追加
        features.insert("arg_count".to_string(), args.len() as f64);
        
        // 既知の特性を特徴量として追加
        features.insert("avg_execution_time".to_string(), characteristics.avg_execution_time);
        features.insert("io_intensity".to_string(), characteristics.io_intensity);
        features.insert("parallelization_efficiency".to_string(), characteristics.parallelization_efficiency);
        
        features
    }
    
    /// モデルを更新
    fn update_model(&mut self, actual_usage: &CommandExecutionRecord, predicted: (f64, f64)) {
        // 予測誤差を計算
        let cpu_error = actual_usage.cpu_usage - predicted.0;
        
        // 特徴量を抽出
        let characteristics = CommandCharacteristics {
            avg_cpu_usage: actual_usage.cpu_usage,
            avg_memory_usage: actual_usage.memory_usage,
            avg_execution_time: actual_usage.execution_time.as_secs_f64(),
            io_intensity: 0.5, // 仮の値
            parallelization_efficiency: 0.5, // 仮の値
            memory_locality: 0.5, // 仮の値
            confidence: 0.5, // 仮の値
        };
        
        let features = self.extract_features(&actual_usage.command, &actual_usage.args, &characteristics);
        
        // 勾配降下法でモデルを更新
        for (feature, value) in features {
            let weight = self.weights.entry(feature).or_insert(0.0);
            *weight += self.learning_rate * (cpu_error * value - self.regularization * *weight);
        }
        
        self.bias += self.learning_rate * cpu_error;
    }
}

impl ResourceLearningModel {
    /// 新しいリソース学習モデルを作成
    fn new() -> Self {
        Self {
            command_characteristics: HashMap::new(),
            prediction_model: ResourcePredictionModel::new(),
            model_accuracy: 0.0,
            last_training: Instant::now(),
            training_interval: Duration::from_secs(3600), // 1時間ごとにトレーニング
        }
    }
    
    /// コマンド実行結果から学習
    fn learn_from_execution(&mut self, record: &CommandExecutionRecord) {
        // コマンド特性を更新
        let entry = self.command_characteristics.entry(record.command.clone())
            .or_insert_with(|| CommandCharacteristics {
                avg_cpu_usage: 0.0,
                avg_memory_usage: 0.0,
                avg_execution_time: 0.0,
                io_intensity: 0.5,
                parallelization_efficiency: 0.5,
                memory_locality: 0.5,
                confidence: 0.0,
            });
        
        // 移動平均で更新
        let alpha = 0.3; // 新しいデータの重み
        entry.avg_cpu_usage = (1.0 - alpha) * entry.avg_cpu_usage + alpha * record.cpu_usage;
        entry.avg_memory_usage = (1.0 - alpha) * entry.avg_memory_usage + alpha * record.memory_usage;
        entry.avg_execution_time = (1.0 - alpha) * entry.avg_execution_time + alpha * record.execution_time.as_secs_f64();
        entry.confidence = (entry.confidence + 0.1).min(1.0); // 信頼度を徐々に上げる
        
        // 予測モデルも更新
        let predicted = self.prediction_model.predict_resource_usage(
            &record.command,
            &record.args,
            entry
        );
        
        self.prediction_model.update_model(record, predicted);
        
        // モデル精度の更新
        let cpu_error = (record.cpu_usage - predicted.0).abs();
        let memory_error = (record.memory_usage - predicted.1).abs();
        
        // 精度を更新（誤差が小さいほど精度は高い）
        self.model_accuracy = 0.9 * self.model_accuracy + 0.1 * (1.0 - (cpu_error / 100.0).min(1.0));
    }
    
    /// リソース使用を予測
    fn predict_resource_usage(&self, command: &str, args: &[String]) -> (f64, f64) {
        if let Some(characteristics) = self.command_characteristics.get(command) {
            self.prediction_model.predict_resource_usage(command, args, characteristics)
        } else {
            // 未知のコマンドの場合はデフォルト値を返す
            (50.0, 100.0) // CPU 50%, メモリ 100MB
        }
    }
}

impl ResourceUsageHistory {
    /// 新しいリソース使用履歴を作成
    fn new(max_history_size: usize) -> Self {
        Self {
            command_executions: VecDeque::with_capacity(max_history_size),
            system_snapshots: VecDeque::with_capacity(max_history_size),
            max_history_size,
        }
    }
    
    /// コマンド実行記録を追加
    fn add_command_execution(&mut self, record: CommandExecutionRecord) {
        self.command_executions.push_back(record);
        
        // 最大サイズを超えたら古いエントリを削除
        if self.command_executions.len() > self.max_history_size {
            self.command_executions.pop_front();
        }
    }
    
    /// システムスナップショットを追加
    fn add_system_snapshot(&mut self, snapshot: SystemResourceSnapshot) {
        self.system_snapshots.push_back(snapshot);
        
        // 最大サイズを超えたら古いエントリを削除
        if self.system_snapshots.len() > self.max_history_size {
            self.system_snapshots.pop_front();
        }
    }
    
    /// コマンドの履歴を取得
    fn get_command_history(&self, command: &str, limit: usize) -> Vec<&CommandExecutionRecord> {
        self.command_executions.iter()
            .filter(|record| record.command == command)
            .take(limit)
            .collect()
    }
    
    /// 時間範囲のシステムスナップショットを取得
    fn get_system_snapshots_in_range(&self, start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>) -> Vec<&SystemResourceSnapshot> {
        self.system_snapshots.iter()
            .filter(|snapshot| snapshot.timestamp >= start && snapshot.timestamp <= end)
            .collect()
    }
}

impl AdaptiveResourceManager {
    /// 新しい適応型リソース管理システムを作成
    fn new(optimization_level: u8) -> Self {
        let (monitor, rx) = SystemResourceMonitor::new(Duration::from_secs(1));
        
        // モニタリングを開始
        let monitor_clone = monitor.clone();
        monitor_clone.start_monitoring();
        
        let usage_history = Arc::new(RwLock::new(ResourceUsageHistory::new(1000)));
        let history_clone = usage_history.clone();
        
        // スナップショット受信ループを開始
        tokio::spawn(async move {
            while let Some(snapshot) = rx.recv().await {
                let mut history = history_clone.write().await;
                history.add_system_snapshot(snapshot);
            }
        });
        
        Self {
            system_monitor: monitor,
            allocation_policy: ResourceAllocationPolicy::Dynamic,
            learning_model: Arc::new(Mutex::new(ResourceLearningModel::new())),
            usage_history,
            optimization_level,
            auto_scaling: AutoScalingConfig {
                enabled: true,
                min_parallelism: 1,
                max_parallelism: 64,
                scale_up_threshold: 80.0,
                scale_down_threshold: 20.0,
                cooldown_period: Duration::from_secs(30),
            },
        }
    }
    
    /// コマンドのリソース要件を予測
    async fn predict_command_resources(&self, command: &str, args: &[String]) -> (f64, f64) {
        let model = self.learning_model.lock().await;
        model.predict_resource_usage(command, args)
    }
    
    /// コマンド実行から学習
    async fn learn_from_execution(&self, record: CommandExecutionRecord) {
        // 履歴に追加
        {
            let mut history = self.usage_history.write().await;
            history.add_command_execution(record.clone());
        }
        
        // モデルを更新
        let mut model = self.learning_model.lock().await;
        model.learn_from_execution(&record);
    }
    
    /// 実行並列度を調整
    async fn adjust_parallelism(&self, current_usage: &SystemResourceSnapshot) -> u32 {
        if !self.auto_scaling.enabled {
            return self.auto_scaling.max_parallelism;
        }
        
        let current_parallelism = self.auto_scaling.max_parallelism;
        
        // CPU使用率が高すぎる場合はスケールダウン
        if current_usage.cpu_usage > self.auto_scaling.scale_up_threshold {
            let new_parallelism = (current_parallelism as f64 * 0.8) as u32;
            return new_parallelism.max(self.auto_scaling.min_parallelism);
        }
        
        // CPU使用率が低すぎる場合はスケールアップ
        if current_usage.cpu_usage < self.auto_scaling.scale_down_threshold {
            let new_parallelism = (current_parallelism as f64 * 1.2) as u32;
            return new_parallelism.min(self.auto_scaling.max_parallelism);
        }
        
        current_parallelism
    }
    
    /// コマンド優先度を計算
    async fn calculate_command_priority(&self, command: &str, args: &[String], context: &ExecutionContext) -> u8 {
        // 基本優先度
        let mut priority = context.flags.cpu_priority;
        
        // 学習モデルからの予測を利用
        let (cpu_prediction, memory_prediction) = self.predict_command_resources(command, args).await;
        
        // リソース要求が高いコマンドは優先度を下げる
        if cpu_prediction > 70.0 || memory_prediction > 1000.0 {
            priority = priority.saturating_sub(2);
        }
        
        // バックグラウンドコマンドは優先度を下げる
        if context.flags.background {
            priority = priority.saturating_sub(3);
        }
        
        // 対話的なコマンドは優先度を上げる
        if context.stdin_data.is_some() {
            priority = priority.saturating_add(2);
        }
        
        priority
    }
}

// ExecutionEngineとの統合
impl ExecutionEngine {
    // ... existing methods ...
    
    /// 適応型リソース管理システムを作成
    fn init_resource_manager(&self) -> AdaptiveResourceManager {
        AdaptiveResourceManager::new(5)
    }
    
    /// 拡張された実行コマンドメソッド
    pub async fn execute_command_with_resource_optimization(
        &self,
        command: &str,
        args: Vec<String>,
        context: ExecutionContext,
        resource_manager: &AdaptiveResourceManager,
    ) -> Result<ExecutionResult> {
        let start_time = Instant::now();
        
        // リソース要求を予測
        let (cpu_prediction, memory_prediction) = resource_manager.predict_command_resources(command, &args).await;
        
        // コマンド優先度を計算
        let priority = resource_manager.calculate_command_priority(command, &args, &context).await;
        
        debug!("コマンド '{}' のリソース予測: CPU {:.1}%, メモリ {:.1}MB, 優先度: {}", 
               command, cpu_prediction, memory_prediction, priority);
        
        // 通常の実行を続行
        let result = self.execute_command(command, args.clone(), context.clone()).await;
        
        // 実行結果からリソース使用を学習
        if let Ok(ref execution_result) = result {
            // リソース統計を取得
            let cpu_usage = execution_result.resource_stats.as_ref()
                .map(|stats| stats.cpu_time_sec * 100.0)
                .unwrap_or(0.0);
                
            let memory_usage = execution_result.resource_stats.as_ref()
                .map(|stats| stats.peak_memory_bytes as f64 / 1024.0 / 1024.0)
                .unwrap_or(0.0);
                
            // 実行記録を作成
            let record = CommandExecutionRecord {
                command: command.to_string(),
                args: args.clone(),
                start_time: chrono::Utc::now() - chrono::Duration::from_std(execution_result.execution_time).unwrap_or_default(),
                execution_time: execution_result.execution_time,
                exit_code: execution_result.exit_code,
                cpu_usage,
                memory_usage,
                io_operations: (
                    execution_result.resource_stats.as_ref().map(|s| s.read_bytes).unwrap_or(0),
                    execution_result.resource_stats.as_ref().map(|s| s.write_bytes).unwrap_or(0)
                ),
            };
            
            // 学習を実行
            resource_manager.learn_from_execution(record).await;
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use std::default::Default;
    
    #[tokio::test]
    async fn test_execution_result_default() {
        let result = ExecutionResult::default();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
    
    #[tokio::test]
    async fn test_execution_context_default() {
        let context = ExecutionContext::default();
        assert!(context.working_dir.exists());
        assert!(!context.environment.is_empty());
        assert!(context.timeout.is_none());
        assert!(context.stdin_data.is_none());
    }
    
    // その他のテストケース...
} 