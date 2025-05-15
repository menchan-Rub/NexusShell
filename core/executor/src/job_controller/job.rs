use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;
use std::fmt::{Debug, Display, Formatter};
use std::collections::HashMap;
use std::path::PathBuf;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, SystemExt, ProcessExt, PidExt};
use log::{debug, trace, warn, error};
use metrics::{counter, gauge, histogram};

/// ジョブID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobId(String);

impl JobId {
    /// 新しいジョブIDを生成します
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// 既存の文字列からジョブIDを作成します
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// 文字列スライスからジョブIDを作成します
    pub fn from_str(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for JobId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for JobId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// ジョブステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// 初期状態
    Pending,
    /// キュー待機中
    Queued,
    /// 開始処理中
    Starting,
    /// 実行中
    Running,
    /// 完了（成功）
    Completed,
    /// 完了（エラー）
    Failed,
    /// 停止（一時停止）
    Stopped,
    /// キャンセル
    Cancelled,
    /// タイムアウト
    TimedOut,
    /// リソース制限超過
    ResourceExceeded,
}

impl Display for JobStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "保留中"),
            JobStatus::Queued => write!(f, "キュー待機中"),
            JobStatus::Starting => write!(f, "開始処理中"),
            JobStatus::Running => write!(f, "実行中"),
            JobStatus::Completed => write!(f, "完了"),
            JobStatus::Failed => write!(f, "失敗"),
            JobStatus::Stopped => write!(f, "停止"),
            JobStatus::Cancelled => write!(f, "キャンセル"),
            JobStatus::TimedOut => write!(f, "タイムアウト"),
            JobStatus::ResourceExceeded => write!(f, "リソース制限超過"),
        }
    }
}

/// ジョブタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobType {
    /// フォアグラウンドジョブ（シェルがブロックされる）
    Foreground,
    /// バックグラウンドジョブ（シェルはブロックされない）
    Background,
    /// デーモンジョブ（バックグラウンドで長時間実行）
    Daemon,
    /// スケジュールジョブ（指定時刻に実行）
    Scheduled,
    /// バッチジョブ（一括処理）
    Batch,
}

impl Display for JobType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::Foreground => write!(f, "フォアグラウンド"),
            JobType::Background => write!(f, "バックグラウンド"),
            JobType::Daemon => write!(f, "デーモン"),
            JobType::Scheduled => write!(f, "スケジュール"),
            JobType::Batch => write!(f, "バッチ"),
        }
    }
}

/// ジョブの優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobPriority {
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
    /// リアルタイム優先度
    Realtime = 5,
}

impl Display for JobPriority {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JobPriority::Lowest => write!(f, "最低"),
            JobPriority::Low => write!(f, "低"),
            JobPriority::Normal => write!(f, "通常"),
            JobPriority::High => write!(f, "高"),
            JobPriority::Highest => write!(f, "最高"),
            JobPriority::Realtime => write!(f, "リアルタイム"),
        }
    }
}

impl Default for JobPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// ジョブのリソース制限
#[derive(Debug, Clone)]
pub struct JobResourceLimits {
    /// CPU時間の制限（秒）
    pub cpu_time_sec: Option<u64>,
    /// CPU使用率の制限（パーセント）
    pub max_cpu_percent: Option<u32>,
    /// メモリ使用量の制限（バイト）
    pub max_memory_bytes: Option<u64>,
    /// メモリ使用率の制限（パーセント）
    pub max_memory_percent: Option<u32>,
    /// ファイルサイズの制限（バイト）
    pub max_file_size_bytes: Option<u64>,
    /// オープンファイル数の制限
    pub max_open_files: Option<u32>,
    /// 子プロセス数の制限
    pub max_child_processes: Option<u32>,
    /// ディスク読み取りの制限（バイト/秒）
    pub max_disk_read_rate: Option<u64>,
    /// ディスク書き込みの制限（バイト/秒）
    pub max_disk_write_rate: Option<u64>,
    /// ネットワーク受信の制限（バイト/秒）
    pub max_network_rx_rate: Option<u64>,
    /// ネットワーク送信の制限（バイト/秒）
    pub max_network_tx_rate: Option<u64>,
    /// 実行時間の制限（秒）
    pub max_execution_time_sec: Option<u64>,
    /// 優先度のnice値（-20〜19）
    pub nice_value: Option<i32>,
    /// カスタム制限
    pub custom_limits: HashMap<String, f64>,
}

impl Default for JobResourceLimits {
    fn default() -> Self {
        Self {
            cpu_time_sec: None,
            max_cpu_percent: None,
            max_memory_bytes: None,
            max_memory_percent: None,
            max_file_size_bytes: None,
            max_open_files: None,
            max_child_processes: None,
            max_disk_read_rate: None,
            max_disk_write_rate: None,
            max_network_rx_rate: None,
            max_network_tx_rate: None,
            max_execution_time_sec: None,
            nice_value: None,
            custom_limits: HashMap::new(),
        }
    }
}

/// ジョブのリソース使用統計
#[derive(Debug, Clone)]
pub struct JobResourceStats {
    /// CPU使用率（％）
    pub cpu_usage: f32,
    /// メモリ使用量（バイト）
    pub memory_usage: u64,
    /// ディスク読み取り（バイト）
    pub disk_read: u64,
    /// ディスク書き込み（バイト）
    pub disk_write: u64,
    /// ネットワーク受信（バイト）
    pub net_rx: u64,
    /// ネットワーク送信（バイト）
    pub net_tx: u64,
    /// オープンファイル数
    pub open_files: u32,
    /// スレッド数
    pub thread_count: u32,
    /// 子プロセス数
    pub child_process_count: u32,
    /// ページフォールト数
    pub page_faults: u64,
    /// コンテキストスイッチ数
    pub context_switches: u64,
    /// CPU時間（マイクロ秒）
    pub cpu_time_us: u64,
    /// I/O待機時間（マイクロ秒）
    pub io_wait_time_us: u64,
    /// 最終更新時刻
    pub last_updated: Instant,
}

impl Default for JobResourceStats {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0,
            disk_read: 0,
            disk_write: 0,
            net_rx: 0,
            net_tx: 0,
            open_files: 0,
            thread_count: 0,
            child_process_count: 0,
            page_faults: 0,
            context_switches: 0,
            cpu_time_us: 0,
            io_wait_time_us: 0,
            last_updated: Instant::now(),
        }
    }
}

/// プロセスの出力ストリーム
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputStreamType {
    /// 標準出力
    Stdout,
    /// 標準エラー出力
    Stderr,
}

/// ジョブ情報を表す構造体
#[derive(Debug, Clone)]
pub struct JobInfo {
    /// ジョブID
    pub id: JobId,
    /// ジョブコマンド
    pub command: String,
    /// ジョブタイプ
    pub job_type: JobType,
    /// ジョブステータス
    pub status: JobStatus,
    /// 開始時刻
    pub started_at: Option<Instant>,
    /// 完了時刻
    pub finished_at: Option<Instant>,
    /// 実行時間
    pub runtime: Option<Duration>,
    /// プロセスID
    pub pid: Option<u32>,
    /// 終了コード
    pub exit_code: Option<i32>,
    /// エラー詳細
    pub error_details: Option<String>,
    /// リソース使用統計
    pub resource_stats: Option<JobResourceStats>,
}

/// ジョブを表すクラス
#[derive(Clone)]
pub struct Job {
    /// ジョブの一意な識別子
    id: JobId,
    /// ジョブのコマンド
    command: String,
    /// ジョブのタイプ
    job_type: JobType,
    /// ジョブの優先度
    priority: JobPriority,
    /// ジョブの状態
    status: Arc<RwLock<JobStatus>>,
    /// ジョブの作成時間
    created_at: Instant,
    /// ジョブの開始時間
    started_at: Arc<RwLock<Option<Instant>>>,
    /// ジョブの終了時間
    finished_at: Arc<RwLock<Option<Instant>>>,
    /// ジョブのプロセスID（実行中の場合）
    pid: Arc<RwLock<Option<u32>>>,
    /// ジョブのコマンドパス
    path: String,
    /// ジョブの引数
    args: Vec<String>,
    /// ジョブの環境変数
    env_vars: HashMap<String, String>,
    /// ジョブの作業ディレクトリ
    working_dir: String,
    /// ジョブの終了コード
    exit_code: Arc<RwLock<Option<i32>>>,
    /// ジョブの標準出力
    stdout: Arc<RwLock<Vec<u8>>>,
    /// ジョブの標準エラー出力
    stderr: Arc<RwLock<Vec<u8>>>,
    /// システム情報（リソース使用状況の監視に使用）
    system: Arc<RwLock<System>>,
    /// リソース使用統計
    resource_stats: Arc<RwLock<JobResourceStats>>,
    /// 親プロセスID
    parent_pid: Option<u32>,
    /// 子プロセスIDs
    child_pids: Arc<RwLock<Vec<u32>>>,
    /// 出力制限（バイト単位）
    output_limit: usize,
    /// 出力を保存するかどうか
    save_output: bool,
    /// スクリプト実行の場合、スクリプトファイルパス
    script_file: Option<PathBuf>,
    /// リソース制限
    resource_limits: Option<JobResourceLimits>,
    /// ジョブタイムアウト時間
    timeout: Option<Duration>,
    /// タイムアウト監視タスクID
    timeout_task_id: Arc<RwLock<Option<uuid::Uuid>>>,
    /// ジョブのラベル（キー・バリューペア）
    labels: HashMap<String, String>,
    /// ジョブのユーザーID
    user_id: Option<String>,
    /// ジョブの実行回数（再実行時にインクリメント）
    execution_count: Arc<RwLock<u32>>,
    /// アイドル状態のタイムアウト（無通信時間の制限）
    idle_timeout: Option<Duration>,
    /// ジョブエラーの詳細
    error_details: Arc<RwLock<Option<String>>>,
    /// ジョブ停止時のシグナル
    stop_signal: Option<i32>,
}

impl Job {
    /// 新しいジョブを作成します
    pub fn new(job_type: JobType, command: &str) -> Self {
        let id = JobId::new();
        let now = Instant::now();
        
        // コマンドを解析して実行パスと引数に分割
        let parts: Vec<&str> = command.split_whitespace().collect();
        let path = parts.first().unwrap_or(&"").to_string();
        let args = parts
            .iter()
            .skip(1)
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        // システム情報の初期化
        let system = System::new_with_specifics(
            RefreshKind::new()
                .with_processes(ProcessRefreshKind::everything())
                .with_cpu()
                .with_memory()
        );

        Self {
            id,
            command: command.to_string(),
            job_type,
            priority: JobPriority::Normal,
            status: Arc::new(RwLock::new(JobStatus::Pending)),
            created_at: now,
            started_at: Arc::new(RwLock::new(None)),
            finished_at: Arc::new(RwLock::new(None)),
            pid: Arc::new(RwLock::new(None)),
            path,
            args,
            env_vars: std::env::vars().collect(),
            working_dir: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            exit_code: Arc::new(RwLock::new(None)),
            stdout: Arc::new(RwLock::new(Vec::new())),
            stderr: Arc::new(RwLock::new(Vec::new())),
            system: Arc::new(RwLock::new(system)),
            resource_stats: Arc::new(RwLock::new(JobResourceStats::default())),
            parent_pid: std::process::id().try_into().ok(),
            child_pids: Arc::new(RwLock::new(Vec::new())),
            output_limit: 1024 * 1024, // 1MB
            save_output: true,
            script_file: None,
            resource_limits: None,
            timeout: None,
            timeout_task_id: Arc::new(RwLock::new(None)),
            labels: HashMap::new(),
            user_id: None,
            execution_count: Arc::new(RwLock::new(0)),
            idle_timeout: None,
            error_details: Arc::new(RwLock::new(None)),
            stop_signal: None,
        }
    }

    /// コマンドでフォアグラウンドジョブを作成するショートカット
    pub fn with_command(command: &str) -> Self {
        Self::new(JobType::Foreground, command)
    }
    
    /// 特定のタイプとコマンドでジョブを作成するショートカット
    pub fn with_type_and_command(job_type: JobType, command: &str) -> Self {
        Self::new(job_type, command)
    }

    /// ジョブIDを取得します
    pub fn id(&self) -> &JobId {
        &self.id
    }

    /// ジョブのコマンドを返します
    pub fn command(&self) -> &str {
        &self.command
    }

    /// ジョブのタイプを返します
    pub fn job_type(&self) -> JobType {
        self.job_type
    }
    
    /// ジョブのタイプを設定します
    pub fn set_job_type(&mut self, job_type: JobType) {
        self.job_type = job_type;
    }

    /// ジョブの優先度を返します
    pub fn priority(&self) -> JobPriority {
        self.priority
    }

    /// ジョブの優先度を設定します
    pub fn set_priority(&mut self, priority: JobPriority) {
        self.priority = priority;
    }

    /// ジョブの現在の状態を返します
    pub fn status(&self) -> JobStatus {
        *self.status.try_read().unwrap()
    }

    /// ジョブの状態を設定します
    pub async fn set_status(&self, status: JobStatus) {
        let previous_status = {
            let mut status_guard = self.status.write().await;
            let prev = *status_guard;
            *status_guard = status;
            prev
        };

        // 状態変化に応じた処理
        match status {
            JobStatus::Running => {
                if previous_status != JobStatus::Running {
                    let mut started = self.started_at.write().await;
                    if started.is_none() {
                        *started = Some(Instant::now());
                    }
                    
                    // 実行回数をインクリメント
                    let mut count = self.execution_count.write().await;
                    *count += 1;
                    
                    // メトリクスを更新
                    counter!("nexusshell_job_starts_total", "job_id" => self.id.0.clone()).increment(1);
                }
            }
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled | JobStatus::TimedOut | JobStatus::ResourceExceeded => {
                if previous_status == JobStatus::Running {
                    let mut finished = self.finished_at.write().await;
                    if finished.is_none() {
                        *finished = Some(Instant::now());
                    }
                    
                    // メトリクスを更新
                    match status {
                        JobStatus::Completed => {
                            counter!("nexusshell_job_completions_total", "job_id" => self.id.0.clone()).increment(1);
                        }
                        JobStatus::Failed => {
                            counter!("nexusshell_job_failures_total", "job_id" => self.id.0.clone()).increment(1);
                        }
                        JobStatus::Cancelled => {
                            counter!("nexusshell_job_cancellations_total", "job_id" => self.id.0.clone()).increment(1);
                        }
                        JobStatus::TimedOut => {
                            counter!("nexusshell_job_timeouts_total", "job_id" => self.id.0.clone()).increment(1);
                        }
                        JobStatus::ResourceExceeded => {
                            counter!("nexusshell_job_resource_exceeded_total", "job_id" => self.id.0.clone()).increment(1);
                        }
                        _ => {}
                    }
                    
                    // 実行時間を計算
                    if let Some(start_time) = *self.started_at.read().await {
                        let runtime = Instant::now().duration_since(start_time);
                        histogram!("nexusshell_job_runtime_seconds", "job_id" => self.id.0.clone())
                            .record(runtime.as_secs_f64());
                    }
                }
            }
            _ => {}
        }
    }

    /// ジョブの作成時刻を返します
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// ジョブの開始時刻を返します
    pub async fn started_at(&self) -> Option<Instant> {
        *self.started_at.read().await
    }

    /// ジョブの終了時刻を返します
    pub async fn finished_at(&self) -> Option<Instant> {
        *self.finished_at.read().await
    }

    /// ジョブの実行時間を返します
    pub async fn runtime(&self) -> Option<Duration> {
        let started = *self.started_at.read().await;
        
        if let Some(start_time) = started {
            let end_time = if let Some(finish_time) = *self.finished_at.read().await {
                finish_time
            } else {
                Instant::now()
            };
            
            Some(end_time.duration_since(start_time))
        } else {
            None
        }
    }

    /// ジョブのPIDを設定します
    pub async fn set_pid(&self, pid: u32) {
        let mut pid_guard = self.pid.write().await;
        *pid_guard = Some(pid);
    }

    /// ジョブのPIDを返します
    pub async fn pid(&self) -> Option<u32> {
        *self.pid.read().await
    }

    /// 子プロセスIDを追加します
    pub async fn add_child_pid(&self, pid: u32) {
        let mut pids = self.child_pids.write().await;
        pids.push(pid);
    }

    /// 子プロセスIDのリストを返します
    pub async fn child_pids(&self) -> Vec<u32> {
        self.child_pids.read().await.clone()
    }

    /// ジョブのパスを返します
    pub fn path(&self) -> &str {
        &self.path
    }

    /// ジョブの引数を返します
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// ジョブの環境変数を返します
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    /// 環境変数を追加します
    pub fn add_env_var(&mut self, key: &str, value: &str) {
        self.env_vars.insert(key.to_string(), value.to_string());
    }

    /// ジョブの作業ディレクトリを返します
    pub fn working_dir(&self) -> &str {
        &self.working_dir
    }

    /// ジョブの作業ディレクトリを設定します
    pub fn set_working_dir(&mut self, dir: &str) {
        self.working_dir = dir.to_string();
    }

    /// ジョブの終了コードを設定します
    pub async fn set_exit_code(&self, code: i32) {
        let mut exit_code = self.exit_code.write().await;
        *exit_code = Some(code);
    }

    /// ジョブの終了コードを返します
    pub async fn exit_code(&self) -> Option<i32> {
        *self.exit_code.read().await
    }

    /// 標準出力にデータを追加します
    pub async fn append_stdout(&self, data: &[u8]) {
        if !self.save_output {
            return;
        }
        
        let mut stdout = self.stdout.write().await;
        
        // 出力制限を超えないように追加
        if stdout.len() + data.len() <= self.output_limit {
            stdout.extend_from_slice(data);
        } else if stdout.len() < self.output_limit {
            // 制限に達するまでのデータだけを追加
            let remaining = self.output_limit - stdout.len();
            stdout.extend_from_slice(&data[..remaining.min(data.len())]);
            
            if stdout.len() >= self.output_limit {
                warn!("ジョブ {} の標準出力が制限 ({} バイト) に達しました", self.id.0, self.output_limit);
            }
        }
    }

    /// 標準エラー出力にデータを追加します
    pub async fn append_stderr(&self, data: &[u8]) {
        if !self.save_output {
            return;
        }
        
        let mut stderr = self.stderr.write().await;
        
        // 出力制限を超えないように追加
        if stderr.len() + data.len() <= self.output_limit {
            stderr.extend_from_slice(data);
        } else if stderr.len() < self.output_limit {
            // 制限に達するまでのデータだけを追加
            let remaining = self.output_limit - stderr.len();
            stderr.extend_from_slice(&data[..remaining.min(data.len())]);
            
            if stderr.len() >= self.output_limit {
                warn!("ジョブ {} の標準エラー出力が制限 ({} バイト) に達しました", self.id.0, self.output_limit);
            }
        }
    }

    /// 標準出力を返します
    pub async fn stdout(&self) -> Vec<u8> {
        self.stdout.read().await.clone()
    }

    /// 標準エラー出力を返します
    pub async fn stderr(&self) -> Vec<u8> {
        self.stderr.read().await.clone()
    }

    /// リソース使用状況を更新します
    pub async fn update_resource_stats(&self) {
        let pid_option = *self.pid.read().await;
        
        if let Some(pid) = pid_option {
            let mut system = self.system.write().await;
            
            // システム情報を更新
            system.refresh_processes();
            
            // プロセス情報を取得
            if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
                let mut stats = self.resource_stats.write().await;
                
                // 統計情報を更新
                stats.cpu_usage = process.cpu_usage();
                stats.memory_usage = process.memory();
                stats.thread_count = process.thread_count();
                
                if let Some(disk_usage) = process.disk_usage() {
                    stats.disk_read = disk_usage.read_bytes;
                    stats.disk_write = disk_usage.written_bytes;
                }
                
                // 子プロセスのリソース使用状況も集計
                let child_pids = self.child_pids.read().await.clone();
                let mut child_cpu_usage = 0.0;
                let mut child_memory_usage = 0;
                let mut child_count = 0;
                
                for child_pid in child_pids {
                    if let Some(child_process) = system.process(sysinfo::Pid::from_u32(child_pid)) {
                        child_cpu_usage += child_process.cpu_usage();
                        child_memory_usage += child_process.memory();
                        child_count += 1;
                    }
                }
                
                stats.child_process_count = child_count;
                stats.cpu_usage += child_cpu_usage;
                stats.memory_usage += child_memory_usage;
                
                stats.last_updated = Instant::now();
                
                // リソース制限をチェック
                if let Some(limits) = &self.resource_limits {
                    self.check_resource_limits(&stats, limits).await;
                }
                
                // メトリクスの更新
                gauge!("nexusshell_job_cpu_usage", "job_id" => self.id.0.clone()).set(stats.cpu_usage as f64);
                gauge!("nexusshell_job_memory_usage", "job_id" => self.id.0.clone()).set(stats.memory_usage as f64);
                gauge!("nexusshell_job_thread_count", "job_id" => self.id.0.clone()).set(stats.thread_count as f64);
                gauge!("nexusshell_job_child_processes", "job_id" => self.id.0.clone()).set(stats.child_process_count as f64);
            }
        }
    }
    
    /// リソース制限を確認し、超過している場合は対応します
    async fn check_resource_limits(&self, stats: &JobResourceStats, limits: &JobResourceLimits) {
        let current_status = self.status().await;
        
        // 実行中の場合のみチェック
        if current_status == JobStatus::Running {
            // CPU使用率の制限
            if let Some(max_cpu) = limits.max_cpu_percent {
                if stats.cpu_usage > max_cpu as f32 {
                    warn!("ジョブ {} がCPU使用率制限を超過しました: {:.1}% > {}%", 
                          self.id.0, stats.cpu_usage, max_cpu);
                    self.set_status(JobStatus::ResourceExceeded).await;
                    self.set_error_details(format!("CPU制限超過: {:.1}% > {}%", stats.cpu_usage, max_cpu)).await;
                    return;
                }
            }
            
            // メモリ使用量の制限
            if let Some(max_memory) = limits.max_memory_bytes {
                if stats.memory_usage > max_memory {
                    warn!("ジョブ {} がメモリ使用量制限を超過しました: {}B > {}B", 
                          self.id.0, stats.memory_usage, max_memory);
                    self.set_status(JobStatus::ResourceExceeded).await;
                    self.set_error_details(format!("メモリ制限超過: {}B > {}B", stats.memory_usage, max_memory)).await;
                    return;
                }
            }
            
            // 子プロセス数の制限
            if let Some(max_child) = limits.max_child_processes {
                if stats.child_process_count > max_child {
                    warn!("ジョブ {} が子プロセス数制限を超過しました: {} > {}", 
                          self.id.0, stats.child_process_count, max_child);
                    self.set_status(JobStatus::ResourceExceeded).await;
                    self.set_error_details(format!("子プロセス数制限超過: {} > {}", stats.child_process_count, max_child)).await;
                    return;
                }
            }
            
            // 実行時間の制限
            if let Some(max_time) = limits.max_execution_time_sec {
                if let Some(runtime) = self.runtime().await {
                    if runtime.as_secs() > max_time {
                        warn!("ジョブ {} が実行時間制限を超過しました: {}秒 > {}秒", 
                              self.id.0, runtime.as_secs(), max_time);
                        self.set_status(JobStatus::TimedOut).await;
                        self.set_error_details(format!("実行時間制限超過: {}秒 > {}秒", runtime.as_secs(), max_time)).await;
                        return;
                    }
                }
            }
        }
    }

    /// リソース使用状況を返します
    pub async fn resource_stats(&self) -> JobResourceStats {
        self.resource_stats.read().await.clone()
    }

    /// 出力制限を設定します
    pub fn set_output_limit(&mut self, limit: usize) {
        self.output_limit = limit;
    }

    /// 出力を保存するかどうかを設定します
    pub fn set_save_output(&mut self, save: bool) {
        self.save_output = save;
    }

    /// スクリプトファイルを設定します
    pub fn set_script_file(&mut self, path: PathBuf) {
        self.script_file = Some(path);
    }
    
    /// リソース制限を設定します
    pub fn set_resource_limits(&mut self, limits: JobResourceLimits) {
        self.resource_limits = Some(limits);
    }
    
    /// リソース制限を取得します
    pub fn resource_limits(&self) -> Option<&JobResourceLimits> {
        self.resource_limits.as_ref()
    }
    
    /// タイムアウトを設定します
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }
    
    /// タイムアウトを取得します
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }
    
    /// アイドルタイムアウトを設定します
    pub fn set_idle_timeout(&mut self, timeout: Duration) {
        self.idle_timeout = Some(timeout);
    }
    
    /// アイドルタイムアウトを取得します
    pub fn idle_timeout(&self) -> Option<Duration> {
        self.idle_timeout
    }
    
    /// ラベルを設定します
    pub fn set_label(&mut self, key: &str, value: &str) {
        self.labels.insert(key.to_string(), value.to_string());
    }
    
    /// ラベルを取得します
    pub fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }
    
    /// ユーザーIDを設定します
    pub fn set_user_id(&mut self, user_id: &str) {
        self.user_id = Some(user_id.to_string());
    }
    
    /// ユーザーIDを取得します
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }
    
    /// エラー詳細を設定します
    pub async fn set_error_details(&self, error: String) {
        let mut details = self.error_details.write().await;
        *details = Some(error);
    }
    
    /// エラー詳細を取得します
    pub async fn error_details(&self) -> Option<String> {
        self.error_details.read().await.clone()
    }
    
    /// 停止シグナルを設定します
    pub fn set_stop_signal(&mut self, signal: i32) {
        self.stop_signal = Some(signal);
    }
    
    /// 停止シグナルを取得します
    pub fn stop_signal(&self) -> Option<i32> {
        self.stop_signal
    }
    
    /// 実行回数を取得します
    pub async fn execution_count(&self) -> u32 {
        *self.execution_count.read().await
    }

    /// ジョブの概要情報を返します
    pub fn summary(&self) -> String {
        format!(
            "Job {{ id: {}, command: {}, type: {:?}, status: {:?} }}",
            self.id.0, self.command, self.job_type, self.status.try_read().unwrap()
        )
    }

    /// ジョブの詳細情報を返します
    pub async fn details(&self) -> String {
        let status = *self.status.read().await;
        let pid = *self.pid.read().await;
        let exit_code = *self.exit_code.read().await;
        let runtime = self.runtime().await.map(|d| format!("{}秒", d.as_secs())).unwrap_or_else(|| "N/A".to_string());
        
        format!(
            "Job {{ id: {}, command: {}, type: {:?}, status: {:?}, pid: {:?}, exit_code: {:?}, runtime: {} }}",
            self.id.0, self.command, self.job_type, status, pid, exit_code, runtime
        )
    }

    /// ジョブ情報を取得します
    pub async fn info(&self) -> JobInfo {
        JobInfo {
            id: self.id.clone(),
            command: self.command.clone(),
            job_type: self.job_type,
            status: *self.status.read().await,
            started_at: *self.started_at.read().await,
            finished_at: *self.finished_at.read().await,
            runtime: self.runtime().await,
            pid: self.pid().await,
            exit_code: self.exit_code().await,
            error_details: self.error_details().await,
            resource_stats: Some(self.resource_stats().await),
        }
    }
}

impl Debug for Job {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.summary())
    }
} 