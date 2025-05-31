/**
 * パイプラインマネージャーモジュール
 * 
 * パイプラインの作成、実行、管理を行います。
 * 複雑なパイプライン処理、並列実行、エラーハンドリングを実装します。
 */

mod pipeline;
mod pipe;
mod error;
mod stages;
mod command;
mod processor;
mod planner;
mod scheduler;
mod optimizer;
mod distributed {
    // サブモジュールは後で実装
}

// エラーモジュール
pub use error::PipelineError;

// パイプラインモジュール
pub use pipeline::{Pipeline, PipelineStage, PipelineKind};

// パイプモジュール
pub use pipe::{PipeStatus, PipeConfig, Pipe, StandardPipe, SharedPipe};

// ステージモジュール
pub use stages::{PipelineData, Stage, StageContext, StageMetrics, StageFactory};

// コマンドモジュール
pub use command::{Command, CommandArgs, CommandResult};

// プロセッサモジュール
pub use processor::{
    ProcessorKind, DataProcessor, FilterProcessor, MapperProcessor, 
    ReducerProcessor, ProcessorChain, ProcessorManager
};

// プランナーモジュール
pub use planner::{
    PipelinePlan, CommandParser, SimpleCommandParser, 
    StagePlan, StageType, RedirectType, ParsedCommand, CommandKind
};

// スケジューラーモジュール
pub use scheduler::{
    PipelineScheduler, SchedulingStrategy, ExecutionSchedule
};

// オプティマイザーモジュール
pub use optimizer::{
    PipelineOptimizer, OptimizationOptions, CostModel, OptimizationStats
};

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn, error, instrument};
use anyhow::{Result, anyhow, Context};
use uuid::Uuid;
use dashmap::DashMap;

use crate::job_controller::JobId;
use crate::async_runtime::{AsyncRuntime, ExecutionDomain};

/// パイプラインID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineId(String);

impl PipelineId {
    /// 新しいパイプラインIDを生成
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// 既存の文字列からパイプラインIDを作成
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for PipelineId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// パイプラインステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStatus {
    /// パイプライン作成中
    Creating,
    /// 準備完了
    Ready,
    /// 実行中
    Running,
    /// 一時停止中
    Paused,
    /// 完了
    Completed,
    /// 失敗
    Failed,
    /// キャンセル
    Canceled,
    /// タイムアウト
    TimedOut,
}

/// パイプライン実行結果
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// パイプラインID
    pub id: PipelineId,
    /// 成功したかどうか
    pub success: bool,
    /// 終了コード
    pub exit_code: Option<i32>,
    /// 出力
    pub output: Option<Vec<u8>>,
    /// エラー出力
    pub error: Option<Vec<u8>>,
    /// 各ステージの結果
    pub stage_results: Vec<StageResult>,
    /// 実行時間
    pub execution_time: Duration,
    /// 開始時間
    pub start_time: std::time::SystemTime,
    /// 終了時間
    pub end_time: std::time::SystemTime,
}

/// ステージ実行結果
#[derive(Debug, Clone)]
pub struct StageResult {
    /// ステージ名
    pub name: String,
    /// 成功したかどうか
    pub success: bool,
    /// 終了コード
    pub exit_code: Option<i32>,
    /// 出力
    pub output: Option<Vec<u8>>,
    /// エラー出力
    pub error: Option<Vec<u8>>,
    /// 実行時間
    pub execution_time: Duration,
}

/// パイプラインマネージャー
/// パイプラインの作成、実行、管理を行います
pub struct PipelineManager {
    /// 現在実行中のパイプライン
    active_pipelines: DashMap<PipelineId, Arc<RwLock<Pipeline>>>,
    /// パイプラインの結果
    pipeline_results: DashMap<PipelineId, PipelineResult>,
    /// パイプライン結果の購読者
    subscribers: RwLock<HashMap<PipelineId, watch::Sender<PipelineStatus>>>,
    /// 非同期ランタイム参照
    runtime: Option<Arc<AsyncRuntime>>,
}

impl PipelineManager {
    /// 新しいパイプラインマネージャーを作成
    pub fn new() -> Self {
        Self {
            active_pipelines: DashMap::new(),
            pipeline_results: DashMap::new(),
            subscribers: RwLock::new(HashMap::new()),
            runtime: None,
        }
    }
    
    /// ランタイムを設定
    pub fn set_runtime(&mut self, runtime: Arc<AsyncRuntime>) {
        self.runtime = Some(runtime);
    }
    
    /// 新しいパイプラインを作成
    #[instrument(skip(self))]
    pub async fn create_pipeline(&self, command_line: &str) -> Result<PipelineId> {
        let pipeline_id = PipelineId::new();
        debug!("パイプライン作成開始: {}", pipeline_id);
        
        // パイプライン作成通知
        self.notify_status(&pipeline_id, PipelineStatus::Creating).await;
        
        // パイプラインの作成
        let pipeline = Pipeline::new();
                    
        // パイプラインをアクティブリストに追加
        self.active_pipelines.insert(pipeline_id.clone(), Arc::new(RwLock::new(pipeline)));
        
        // パイプライン準備完了通知
        self.notify_status(&pipeline_id, PipelineStatus::Ready).await;
        
        debug!("パイプライン作成完了: {}", pipeline_id);
        Ok(pipeline_id)
    }
    
    /// パイプラインを実行
    #[instrument(skip(self))]
    pub async fn execute_pipeline(&self, pipeline_id: &PipelineId) -> Result<PipelineResult> {
        debug!("パイプライン実行開始: {}", pipeline_id);
        
        // パイプラインの取得
        let pipeline_arc = self.active_pipelines.get(pipeline_id)
            .ok_or_else(|| anyhow!("パイプラインが見つかりません: {}", pipeline_id))?
            .clone();
            
        // 実行中状態に更新
        self.notify_status(pipeline_id, PipelineStatus::Running).await;
        
        // パイプライン実行開始時間を記録
        let start_time = std::time::SystemTime::now();
        
        // パイプラインを実行
        let execution_result = match &self.runtime {
            Some(runtime) => {
                // 非同期ランタイムを使用して実行
                let mut pipeline = pipeline_arc.write().await;
                // ランタイムを設定するためのメソッド（または同等の方法）を使用
                if let Some(runtime) = &self.runtime {
                    // pipelineオブジェクトにランタイムを設定
                    // ここではset_optionsを使ってオプションでランタイムを渡す例
                    let mut options = PipelineOptions::default();
                    pipeline.set_property("runtime", &runtime.to_string()).await;
                }
                
                // パイプラインを実行
                pipeline.execute().await
            },
            None => {
                // ランタイムがない場合は直接実行
                let mut pipeline = pipeline_arc.write().await;
                pipeline.execute().await
            }
        };
        
        // 実行時間を計算
        let execution_time = start_time.elapsed().unwrap_or_default();
        let end_time = std::time::SystemTime::now();
        
        // 結果に基づいてステータスを更新
        let status = match &execution_result {
            Ok(_) => PipelineStatus::Completed,
            Err(_) => PipelineStatus::Failed,
        };
        self.notify_status(pipeline_id, status).await;
        
        // 結果オブジェクトを作成
        let result = match execution_result {
            Ok(stage_results) => {
                // すべてのステージが成功したかどうかを確認
                let all_success = stage_results.iter().all(|r| r.success);
                
                // 最終ステージの終了コードを取得
                let exit_code = stage_results.last().and_then(|r| r.exit_code);
                
                // 最終ステージの出力を取得
                let output = stage_results.last().and_then(|r| r.output.clone());
                
                // エラー出力を集約
                let error = if stage_results.iter().any(|r| r.error.is_some()) {
                    // 各ステージのエラー出力をバイト配列として連結
                    let mut combined_error = Vec::new();
                    for result in &stage_results {
                        if let Some(err) = &result.error {
                            combined_error.extend_from_slice(err);
                        }
                    }
                    if !combined_error.is_empty() {
                        Some(combined_error)
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                PipelineResult {
                    id: pipeline_id.clone(),
                    success: all_success,
                    exit_code,
                    output,
                    error,
                    stage_results,
                    execution_time,
                    start_time,
                    end_time,
                }
            },
            Err(e) => {
                error!("パイプライン実行エラー: {}: {}", pipeline_id, e);
                PipelineResult {
                    id: pipeline_id.clone(),
                    success: false,
                    exit_code: Some(1),
                    output: None,
                    error: Some(format!("パイプライン実行エラー: {}", e).into_bytes()),
                    stage_results: vec![],
                    execution_time,
                    start_time,
                    end_time,
                }
            }
        };
        
        debug!("パイプライン実行完了: {}, 成功: {}, 時間: {:?}", 
               pipeline_id, result.success, execution_time);
        
        // 結果を保存
        self.pipeline_results.insert(pipeline_id.clone(), result.clone());
        
        // 結果を返す
        Ok(result)
    }
    
    /// パイプライン実行をキャンセル
    #[instrument(skip(self))]
    pub async fn cancel_pipeline(&self, pipeline_id: &PipelineId) -> Result<()> {
        debug!("パイプライン実行キャンセル: {}", pipeline_id);
        
        if let Some(pipeline_arc) = self.active_pipelines.get(pipeline_id) {
            let mut pipeline = pipeline_arc.write().await;
            pipeline.cancel().await?;
            self.notify_status(pipeline_id, PipelineStatus::Canceled).await;
            Ok(())
        } else {
            Err(anyhow!("パイプラインが見つかりません: {}", pipeline_id))
        }
    }
    
    /// パイプラインステータスを通知
    async fn notify_status(&self, pipeline_id: &PipelineId, status: PipelineStatus) {
        let subscribers = self.subscribers.read().await;
        if let Some(sender) = subscribers.get(pipeline_id) {
            if sender.send(status).is_err() {
                warn!("パイプラインステータス通知に失敗: {}", pipeline_id);
            }
        }
    }
    
    /// パイプラインステータスを購読
    pub async fn subscribe_status(&self, pipeline_id: &PipelineId) -> watch::Receiver<PipelineStatus> {
        let mut subscribers = self.subscribers.write().await;
        
        // 既存の購読者がいればそのレシーバーを返す
        if let Some(sender) = subscribers.get(pipeline_id) {
            return sender.subscribe();
        }
        
        // 新しい購読者を作成
        let (tx, rx) = watch::channel(PipelineStatus::Creating);
        subscribers.insert(pipeline_id.clone(), tx);
        rx
    }
    
    /// ジョブIDからパイプラインIDを取得
    pub async fn get_pipeline_for_job(&self, job_id: &JobId) -> Option<PipelineId> {
        for entry in self.active_pipelines.iter() {
            let pipeline = entry.value().read().await;
            // JobIDを比較するためのアクセサメソッドまたは同等の方法
            let pipeline_job_id = pipeline.get_property("job_id").await;
            if pipeline_job_id.is_some() && format!("{:?}", job_id) == pipeline_job_id.unwrap() {
                return Some(entry.key().clone());
            }
        }
        None
    }
    
    /// パイプラインの状態を取得
    pub async fn get_pipeline_status(&self, pipeline_id: &PipelineId) -> Option<PipelineStatus> {
        let subscribers = self.subscribers.read().await;
        subscribers.get(pipeline_id).map(|sender| *sender.borrow())
    }
    
    /// アクティブなパイプライン数を取得
    pub fn active_pipeline_count(&self) -> usize {
        self.active_pipelines.len()
    }
    
    /// すべてのパイプラインをキャンセル
    pub async fn cancel_all_pipelines(&self) -> Result<()> {
        debug!("すべてのパイプラインをキャンセル中...");
        
        let keys: Vec<PipelineId> = self.active_pipelines.iter()
            .map(|entry| entry.key().clone())
            .collect();
            
        for pipeline_id in keys {
            if let Err(e) = self.cancel_pipeline(&pipeline_id).await {
                warn!("パイプラインキャンセル失敗: {}: {}", pipeline_id, e);
            }
        }
        
        Ok(())
    }
    
    /// パイプラインの結果を待機
    pub async fn wait_for_pipeline(&self, pipeline_id: &PipelineId, timeout_dur: Option<Duration>) -> Result<PipelineStatus> {
        let rx = self.subscribe_status(pipeline_id).await;
        
        // タイムアウト処理
        match timeout_dur {
            Some(dur) => {
                match timeout(dur, wait_for_completion(rx)).await {
                    Ok(status) => Ok(status),
                    Err(_) => {
                        // タイムアウト時はパイプラインをキャンセル
                        warn!("パイプライン待機タイムアウト: {}", pipeline_id);
                        self.cancel_pipeline(pipeline_id).await?;
                        self.notify_status(pipeline_id, PipelineStatus::TimedOut).await;
                        Ok(PipelineStatus::TimedOut)
                    }
                }
            },
            None => {
                // タイムアウトなしで完了まで待機
                Ok(wait_for_completion(rx).await)
            }
        }
    }
}

/// パイプラインの完了を待機する非同期関数
async fn wait_for_completion(mut rx: watch::Receiver<PipelineStatus>) -> PipelineStatus {
    loop {
        let status = *rx.borrow();
        match status {
            PipelineStatus::Completed |
            PipelineStatus::Failed |
            PipelineStatus::Canceled |
            PipelineStatus::TimedOut => return status,
            _ => {
                // 状態が変わるまで待機
                if rx.changed().await.is_err() {
                    // チャネルがクローズされた場合は失敗扱い
                    return PipelineStatus::Failed;
                }
            }
        }
    }
}

impl Default for PipelineManager {
    fn default() -> Self {
        Self::new()
    }
}

/// PipelineOptionsの構造体を定義
#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    /// 実行タイムアウト（秒）
    pub timeout_sec: Option<u64>,
    /// 同時実行可能なステージ数
    pub max_parallel_stages: usize,
    /// ステージ間のバッファサイズ
    pub stage_buffer_size: usize,
    /// 失敗時の再試行回数
    pub retry_count: u32,
    /// 再試行間隔（ミリ秒）
    pub retry_interval_ms: u64,
    /// エラー時に中断するかどうか
    pub abort_on_error: bool,
    /// ログレベル
    pub log_level: LogLevel,
    /// 実行モード
    pub execution_mode: ExecutionMode,
    /// カスタム環境変数
    pub env_vars: std::collections::HashMap<String, String>,
    /// 実行ランタイム設定
    pub runtime_config: RuntimeConfig,
}

/// パイプラインログレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// トレースレベル（最も詳細）
    Trace,
    /// デバッグレベル
    Debug,
    /// 情報レベル
    Info,
    /// 警告レベル
    Warn,
    /// エラーレベル
    Error,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

/// 実行モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// 順次実行
    Sequential,
    /// パイプライン実行
    Pipelined,
    /// 並列実行
    Parallel,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Pipelined
    }
}

/// ランタイム設定
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    /// スレッドプール設定
    pub thread_pool_size: Option<usize>,
    /// I/Oワーカー数
    pub io_worker_count: Option<usize>,
    /// メモリ使用量制限（バイト）
    pub memory_limit: Option<u64>,
    /// CPUアフィニティ設定
    pub cpu_affinity: Option<Vec<usize>>,
} 