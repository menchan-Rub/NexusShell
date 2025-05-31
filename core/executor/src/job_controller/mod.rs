/*!
# ジョブコントローラーモジュール

シェルジョブの作成、実行、管理、監視を行うモジュールです。
前景/背景ジョブの制御、ジョブのライフサイクル管理、リソース制限などを提供します。
*/

mod error;
mod job;
mod job_group;
mod metrics;
mod resource_monitor;
mod scheduler;

pub use error::JobError;
pub use job::{Job, JobId, JobInfo, JobStatus, JobType, JobPriority};
pub use job_group::{JobGroup, JobGroupId};
pub use metrics::JobMetrics;
pub use resource_monitor::ResourceMonitor;
pub use scheduler::JobScheduler;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock, Mutex, Semaphore};
use tokio::time::{sleep, timeout};
use tracing::{debug, info, warn, error, instrument, trace};
use uuid::Uuid;
use async_trait::async_trait;
use dashmap::DashMap;
use anyhow::{Result, anyhow, Context};
use chrono;
use log::{error, warn};
use metrics::{counter, gauge};
use tokio::sync::OwnedSemaphorePermit;

use crate::async_runtime::AsyncRuntime;

/// ジョブオプション
#[derive(Debug, Clone)]
pub struct JobOptions {
    /// ジョブ名
    pub name: Option<String>,
    /// ジョブタイプ
    pub job_type: JobType,
    /// 優先度
    pub priority: JobPriority,
    /// 親ジョブID
    pub parent_job_id: Option<JobId>,
    /// フォアグラウンドフラグ
    pub is_foreground: bool,
    /// メタデータ
    pub metadata: HashMap<String, String>,
    /// タイムアウト
    pub timeout: Option<Duration>,
    /// 環境変数
    pub env_vars: HashMap<String, String>,
    /// リソース制限
    pub resource_limits: Option<ResourceLimits>,
    /// 実行ディレクトリ
    pub working_directory: Option<std::path::PathBuf>,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            name: None,
            job_type: JobType::Command,
            priority: JobPriority::Normal,
            parent_job_id: None,
            is_foreground: true,
            metadata: HashMap::new(),
            timeout: None,
            env_vars: HashMap::new(),
            resource_limits: None,
            working_directory: None,
        }
    }
}

/// リソース制限
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// CPU使用率制限 (0.0-1.0)
    pub cpu_limit: Option<f64>,
    /// メモリ制限 (バイト)
    pub memory_limit: Option<u64>,
    /// ファイルディスクリプタ制限
    pub fd_limit: Option<u64>,
    /// サブプロセス数制限
    pub process_limit: Option<u32>,
    /// ディスクI/O制限 (バイト/秒)
    pub io_limit: Option<u64>,
    /// ネットワーク帯域制限 (バイト/秒)
    pub network_limit: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_limit: None,
            memory_limit: None,
            fd_limit: None,
            process_limit: None,
            io_limit: None,
            network_limit: None,
        }
    }
}

/// ジョブコントローラーの設定
#[derive(Debug, Clone)]
pub struct JobControllerConfig {
    /// 最大同時実行ジョブ数
    pub max_concurrent_jobs: usize,
    /// ジョブ履歴の最大サイズ
    pub max_job_history: usize,
    /// デフォルトのジョブタイムアウト
    pub default_timeout: Duration,
    /// 自動クリーンアップ間隔
    pub cleanup_interval: Duration,
}

impl Default for JobControllerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: 32,
            max_job_history: 1000,
            default_timeout: Duration::from_secs(3600), // 1時間
            cleanup_interval: Duration::from_secs(300), // 5分
        }
    }
}

/// ジョブ結果
#[derive(Debug, Clone)]
pub struct JobResult {
    /// ジョブID
    pub job_id: JobId,
    /// 結果コード
    pub exit_code: i32,
    /// 出力テキスト
    pub output: Option<String>,
    /// エラーテキスト
    pub error: Option<String>,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
}

/// ジョブイベント
#[derive(Debug, Clone)]
pub enum JobEvent {
    /// ジョブ作成
    Created(JobInfo),
    /// ジョブ開始
    Started(JobId),
    /// ジョブ一時停止
    Paused(JobId),
    /// ジョブ再開
    Resumed(JobId),
    /// ジョブ完了
    Completed(JobId, JobResult),
    /// ジョブ失敗
    Failed(JobId, String),
    /// ジョブキャンセル
    Cancelled(JobId),
    /// ジョブ状態変更
    StatusChanged(JobId, JobStatus),
}

/// ジョブイベントハンドラ
#[async_trait]
pub trait JobEventHandler: Send + Sync {
    /// ジョブイベントを処理
    async fn handle_event(&self, event: JobEvent) -> Result<()>;
}

/// ジョブコントローラ
pub struct JobController {
    /// 設定
    config: JobControllerConfig,
    /// アクティブなジョブ
    active_jobs: Arc<RwLock<HashMap<JobId, Arc<RwLock<JobInfo>>>>>,
    /// 完了したジョブの履歴
    job_history: Arc<RwLock<VecDeque<JobInfo>>>,
    /// ジョブ結果
    job_results: Arc<RwLock<HashMap<JobId, JobResult>>>,
    /// 実行中ジョブを制限するセマフォ
    concurrency_limiter: Arc<Semaphore>,
    /// ジョブごとのセマフォ許可を保持するマップ
    job_permits: Arc<RwLock<HashMap<JobId, OwnedSemaphorePermit>>>,
    /// イベントハンドラ
    event_handlers: Arc<RwLock<Vec<Box<dyn JobEventHandler>>>>,
    /// フォアグラウンドジョブID
    foreground_job: Arc<RwLock<Option<JobId>>>,
    /// 非同期ランタイム
    runtime: Arc<AsyncRuntime>,
}

impl JobController {
    /// 新しいジョブコントローラを作成
    pub fn new() -> Self {
        let config = JobControllerConfig::default();
        let runtime = Arc::new(AsyncRuntime::new());
        
        Self {
            config: config.clone(),
            active_jobs: Arc::new(RwLock::new(HashMap::new())),
            job_history: Arc::new(RwLock::new(VecDeque::with_capacity(config.max_job_history))),
            job_results: Arc::new(RwLock::new(HashMap::new())),
            concurrency_limiter: Arc::new(Semaphore::new(config.max_concurrent_jobs)),
            job_permits: Arc::new(RwLock::new(HashMap::new())),
            event_handlers: Arc::new(RwLock::new(Vec::new())),
            foreground_job: Arc::new(RwLock::new(None)),
            runtime,
        }
    }
    
    /// 設定を指定してジョブコントローラを作成
    pub fn with_config(config: JobControllerConfig, runtime: Arc<AsyncRuntime>) -> Self {
        Self {
            config: config.clone(),
            active_jobs: Arc::new(RwLock::new(HashMap::new())),
            job_history: Arc::new(RwLock::new(VecDeque::with_capacity(config.max_job_history))),
            job_results: Arc::new(RwLock::new(HashMap::new())),
            concurrency_limiter: Arc::new(Semaphore::new(config.max_concurrent_jobs)),
            job_permits: Arc::new(RwLock::new(HashMap::new())),
            event_handlers: Arc::new(RwLock::new(Vec::new())),
            foreground_job: Arc::new(RwLock::new(None)),
            runtime,
        }
    }
    
    /// ジョブを作成
    pub async fn create_job(&self, options: JobOptions) -> Result<JobId> {
        let job_name = options.name.unwrap_or_else(|| format!("job-{}", Uuid::new_v4()));
        
        let job_info = JobInfo {
            id: JobId::new(),
            name: job_name,
            job_type: options.job_type,
            status: JobStatus::Pending,
            priority: options.priority,
            start_time: None,
            end_time: None,
            pid: None,
            parent_job_id: options.parent_job_id,
            is_foreground: options.is_foreground,
            metadata: options.metadata,
        };
        
        let job_id = job_info.id.clone();
        
        // アクティブジョブに追加
        {
            let mut active_jobs = self.active_jobs.write().await;
            active_jobs.insert(job_id.clone(), Arc::new(RwLock::new(job_info.clone())));
        }
        
        // ジョブ作成イベントを発行
        self.emit_event(JobEvent::Created(job_info)).await?;
        
        // フォアグラウンドジョブの場合はフォアグラウンドジョブIDを更新
        if options.is_foreground {
            let mut fg_job = self.foreground_job.write().await;
            *fg_job = Some(job_id.clone());
        }
        
        Ok(job_id)
    }
    
    /// ジョブを開始
    pub async fn start_job(&self, job_id: &JobId) -> Result<()> {
        // ジョブの存在確認
        let job_info_arc = {
            let active_jobs = self.active_jobs.read().await;
            match active_jobs.get(job_id) {
                Some(job) => job.clone(),
                None => return Err(anyhow!("ジョブが見つかりません: {}", job_id)),
            }
        };
        
        // ジョブ状態の確認と更新
        {
            let mut job_info = job_info_arc.write().await;
            if job_info.status != JobStatus::Pending {
                return Err(anyhow!(
                    "ジョブを開始できません: {} (現在の状態: {:?})",
                    job_id, job_info.status
                ));
            }
            
            // 状態を更新
            job_info.status = JobStatus::Running;
            job_info.start_time = Some(chrono::Utc::now());
        }
        
        // 同時実行数制限のセマフォを取得
        let permit = match self.concurrency_limiter.try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                // ジョブ状態を待機中に戻す
                let mut job_info = job_info_arc.write().await;
                job_info.status = JobStatus::Pending;
                job_info.start_time = None;
                
                return Err(anyhow!("同時実行ジョブ数の上限に達しました"));
            }
        };
        
        // ジョブ開始イベントを発行
        self.emit_event(JobEvent::Started(job_id.clone())).await?;
        
        // セマフォをグローバルに保持してジョブと紐付ける
        {
            let mut job_permits = self.job_permits.write().await;
            job_permits.insert(job_id.clone(), permit);
        }
        
        Ok(())
    }
    
    /// ジョブを完了
    pub async fn complete_job(&self, job_id: &JobId, result: JobResult) -> Result<()> {
        // ジョブの存在確認
        let job_info_arc = {
            let active_jobs = self.active_jobs.read().await;
            match active_jobs.get(job_id) {
            Some(job) => job.clone(),
                None => return Err(anyhow!("ジョブが見つかりません: {}", job_id)),
            }
        };
        
        // ジョブ状態の更新
        {
            let mut job_info = job_info_arc.write().await;
            job_info.status = JobStatus::Completed;
            job_info.end_time = Some(chrono::Utc::now());
        }
        
        // ジョブ結果を保存
        {
            let mut results = self.job_results.write().await;
            results.insert(job_id.clone(), result.clone());
        }
        
        // ジョブ完了イベントを発行
        self.emit_event(JobEvent::Completed(job_id.clone(), result)).await?;
        
        // フォアグラウンドジョブだった場合はフォアグラウンドジョブIDをクリア
        {
            let mut fg_job = self.foreground_job.write().await;
            if let Some(current_fg_job) = &*fg_job {
                if current_fg_job == job_id {
                    *fg_job = None;
                }
            }
        }
        
        // ジョブに紐づいたセマフォ許可を解放
        {
            let mut job_permits = self.job_permits.write().await;
            job_permits.remove(job_id);
        }
        
        // 履歴に移動
        self.move_to_history(job_id).await;
        
        Ok(())
    }

    /// ジョブを一時停止
    pub async fn pause_job(&self, job_id: &JobId) -> Result<()> {
        // ジョブの存在確認
        let job_info_arc = {
            let active_jobs = self.active_jobs.read().await;
            match active_jobs.get(job_id) {
                Some(job) => job.clone(),
                None => return Err(anyhow!("ジョブが見つかりません: {}", job_id)),
            }
        };
        
        // ジョブ状態の確認と更新
        {
            let mut job_info = job_info_arc.write().await;
            if job_info.status != JobStatus::Running {
                return Err(anyhow!(
                    "ジョブを一時停止できません: {} (現在の状態: {:?})",
                    job_id, job_info.status
                ));
            }
            
            // 状態を更新
            job_info.status = JobStatus::Paused;
        }
        
        // ジョブ一時停止イベントを発行
        self.emit_event(JobEvent::Paused(job_id.clone())).await?;
        
        // 一時停止処理（OSごとに本物の実装）
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(self.pid), Signal::SIGSTOP)?;
        }
        #[cfg(windows)]
        {
            use winapi::um::processthreadsapi::{OpenProcess, SuspendThread};
            use winapi::um::winnt::PROCESS_SUSPEND_RESUME;
            let handle = unsafe { OpenProcess(PROCESS_SUSPEND_RESUME, 0, self.pid as u32) };
            if handle.is_null() { return Err(anyhow::anyhow!("プロセスハンドル取得失敗")); }
            unsafe { SuspendThread(handle); }
        }
        
        Ok(())
    }

    /// ジョブを再開
    pub async fn resume_job(&self, job_id: &JobId) -> Result<()> {
        // ジョブの存在確認
        let job_info_arc = {
            let active_jobs = self.active_jobs.read().await;
            match active_jobs.get(job_id) {
                Some(job) => job.clone(),
                None => return Err(anyhow!("ジョブが見つかりません: {}", job_id)),
            }
        };
        
        // ジョブ状態の確認と更新
        {
            let mut job_info = job_info_arc.write().await;
            if job_info.status != JobStatus::Paused {
                return Err(anyhow!(
                    "ジョブを再開できません: {} (現在の状態: {:?})",
                    job_id, job_info.status
                ));
            }
            
            // 状態を更新
            job_info.status = JobStatus::Running;
        }
        
        // ジョブ再開イベントを発行
        self.emit_event(JobEvent::Resumed(job_id.clone())).await?;
        
        // 再開処理（OSごとに本物の実装）
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(self.pid), Signal::SIGCONT)?;
        }
        #[cfg(windows)]
        {
            use winapi::um::processthreadsapi::{OpenProcess, ResumeThread};
            use winapi::um::winnt::PROCESS_SUSPEND_RESUME;
            let handle = unsafe { OpenProcess(PROCESS_SUSPEND_RESUME, 0, self.pid as u32) };
            if handle.is_null() { return Err(anyhow::anyhow!("プロセスハンドル取得失敗")); }
            unsafe { ResumeThread(handle); }
        }
        
        Ok(())
    }
    
    /// ジョブをキャンセル
    pub async fn cancel_job(&self, job_id: &JobId) -> Result<()> {
        // ジョブの存在確認
        let job_info_arc = {
            let active_jobs = self.active_jobs.read().await;
            match active_jobs.get(job_id) {
            Some(job) => job.clone(),
                None => return Err(anyhow!("ジョブが見つかりません: {}", job_id)),
            }
        };
        
        // ジョブ状態の確認と更新
        {
            let mut job_info = job_info_arc.write().await;
            if job_info.status != JobStatus::Running && job_info.status != JobStatus::Paused {
                return Err(anyhow!(
                    "ジョブをキャンセルできません: {} (現在の状態: {:?})",
                    job_id, job_info.status
                ));
            }
            
            // 状態を更新
            job_info.status = JobStatus::Cancelled;
            job_info.end_time = Some(chrono::Utc::now());
        }
        
        // ジョブキャンセルイベントを発行
        self.emit_event(JobEvent::Cancelled(job_id.clone())).await?;
        
        // フォアグラウンドジョブだった場合はフォアグラウンドジョブIDをクリア
        {
            let mut fg_job = self.foreground_job.write().await;
            if let Some(current_fg_job) = &*fg_job {
                if current_fg_job == job_id {
                    *fg_job = None;
                }
            }
        }
        
        // ジョブに紐づいたセマフォ許可を解放
        {
            let mut job_permits = self.job_permits.write().await;
            job_permits.remove(job_id);
        }
        
        // 履歴に移動
        self.move_to_history(job_id).await;
        
        Ok(())
    }

    /// ジョブイベントを発行
    async fn emit_event(&self, event: JobEvent) -> Result<()> {
        let handlers = self.event_handlers.read().await;
        for handler in handlers.iter() {
            if let Err(e) = handler.handle_event(event.clone()).await {
                error!("イベントハンドラでエラーが発生しました: {}", e);
            }
        }
        Ok(())
    }

    /// ジョブを履歴に移動
    async fn move_to_history(&self, job_id: &JobId) {
        let job_info_opt = {
            let mut active_jobs = self.active_jobs.write().await;
            active_jobs.remove(job_id)
        };
        
        if let Some(job_info_arc) = job_info_opt {
            let job_info = job_info_arc.read().await.clone();
            
            let mut history = self.job_history.write().await;
            
            // 履歴が最大数に達している場合は古いものを削除
            if history.len() >= self.config.max_job_history {
                history.pop_front();
            }
            
            // 履歴に追加
            history.push_back(job_info);
            
            debug!("ジョブを履歴に移動しました: {}", job_id);
        }
    }
    
    /// ジョブイベントハンドラを登録
    pub async fn register_event_handler(&self, handler: Box<dyn JobEventHandler>) {
        let mut handlers = self.event_handlers.write().await;
        handlers.push(handler);
    }
    
    /// アクティブなジョブ一覧を取得
    pub async fn get_active_jobs(&self) -> Vec<JobInfo> {
        let active_jobs = self.active_jobs.read().await;
        let mut result = Vec::with_capacity(active_jobs.len());
        
        for job_arc in active_jobs.values() {
            let job_info = job_arc.read().await.clone();
            result.push(job_info);
        }
        
        result
    }
    
    /// ジョブ履歴を取得
    pub async fn get_job_history(&self) -> Vec<JobInfo> {
        let history = self.job_history.read().await;
        history.iter().cloned().collect()
    }
    
    /// ジョブ情報を取得
    pub async fn get_job_info(&self, job_id: &JobId) -> Option<JobInfo> {
        let active_jobs = self.active_jobs.read().await;
        if let Some(job_arc) = active_jobs.get(job_id) {
            return Some(job_arc.read().await.clone());
        }
        
        // アクティブジョブにない場合は履歴を確認
        let history = self.job_history.read().await;
        for job in history.iter() {
            if &job.id == job_id {
                return Some(job.clone());
            }
        }
        
        None
    }
    
    /// ジョブ結果を取得
    pub async fn get_job_result(&self, job_id: &JobId) -> Option<JobResult> {
        let results = self.job_results.read().await;
        results.get(job_id).cloned()
    }
    
    /// フォアグラウンドジョブIDを取得
    pub async fn get_foreground_job_id(&self) -> Option<JobId> {
        let fg_job = self.foreground_job.read().await;
        fg_job.clone()
    }
    
    /// フォアグラウンドジョブ情報を取得
    pub async fn get_foreground_job(&self) -> Option<JobInfo> {
        let fg_job_id = self.get_foreground_job_id().await;
        if let Some(job_id) = fg_job_id {
            return self.get_job_info(&job_id).await;
        }
        None
    }
    
    /// 古いジョブをクリーンアップ
    pub async fn cleanup_old_jobs(&self, max_age: Duration) {
        let now = chrono::Utc::now();
        let threshold = chrono::Duration::from_std(max_age).unwrap_or_default();
        
        let mut history = self.job_history.write().await;
        
        // 古いジョブを絞り込む
        history.retain(|job| {
            if let Some(end_time) = job.end_time {
                now.signed_duration_since(end_time) < threshold
            } else {
                true // 終了時間がないジョブは保持
            }
        });
        
        debug!("古いジョブをクリーンアップしました（残り: {}）", history.len());
    }
    
    /// 長期実行中のジョブを監視する自動クリーンアップタスクを開始
    pub fn start_auto_cleanup(&self) {
        let interval = self.config.cleanup_interval;
        let max_age = Duration::from_secs(86400); // 1日
        let controller = self.clone();
        
        tokio::spawn(async move {
            loop {
                sleep(interval).await;
                controller.cleanup_old_jobs(max_age).await;
            }
        });
        
        debug!("自動クリーンアップタスクを開始しました (間隔: {:?})", interval);
    }
}

impl Clone for JobController {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            active_jobs: self.active_jobs.clone(),
            job_history: self.job_history.clone(),
            job_results: self.job_results.clone(),
            concurrency_limiter: self.concurrency_limiter.clone(),
            job_permits: self.job_permits.clone(),
            event_handlers: self.event_handlers.clone(),
            foreground_job: self.foreground_job.clone(),
            runtime: self.runtime.clone(),
        }
    }
} 