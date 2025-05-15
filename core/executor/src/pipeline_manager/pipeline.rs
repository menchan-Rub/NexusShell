/**
 * Pipeline - パイプライン実装
 * 
 * パイプラインの作成、管理、実行を行う中核コンポーネント
 */

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock, broadcast};
use uuid::Uuid;
use log::{debug, error, info, warn, trace};
use std::time::{Instant, Duration};
use std::collections::HashMap;
use metrics::{counter, gauge, histogram};
use futures::future::{self, FutureExt};
use tokio::time::timeout;
use anyhow::{Result, Context, anyhow};
use tokio::sync::Semaphore;

use super::error::PipelineError;
use super::stages::{PipelineStage, PipelineData, StageContext, StageMetrics};
use crate::async_runtime::AsyncRuntime;
use crate::job_controller::JobId;

type StageResult = Result<(), PipelineError>;

/// パイプライン種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineKind {
    /// 標準パイプライン（通常のコマンド実行）
    Standard,
    /// 並列パイプライン（複数コマンドを並列実行）
    Parallel,
    /// 条件付きパイプライン（前のコマンドの成功/失敗に基づいて実行）
    Conditional,
    /// ストリーミングパイプライン（データをストリーミング処理）
    Streaming,
    /// バックグラウンドパイプライン（バックグラウンドで実行）
    Background,
}

/// パイプラインを表すクラス
/// 一連のステージを連結し、各ステージの出力を次のステージの入力として順次処理します。
pub struct Pipeline {
    /// パイプラインの一意な識別子
    id: String,
    /// パイプラインの名前（オプション）
    name: Option<String>,
    /// パイプラインのステージ
    stages: Arc<RwLock<Vec<PipelineStage>>>,
    /// キャンセルチャンネル
    cancel_tx: broadcast::Sender<()>,
    /// パイプラインの状態
    state: Arc<RwLock<PipelineState>>,
    /// パイプラインのメトリクス
    metrics: Arc<RwLock<PipelineMetrics>>,
    /// パイプラインのプロパティ
    properties: Arc<RwLock<HashMap<String, String>>>,
    /// 実行オプション
    options: Arc<RwLock<PipelineOptions>>,
    /// ジョブID (オプション)
    job_id: Option<JobId>,
    /// 非同期ランタイム
    runtime: Option<Arc<AsyncRuntime>>,
}

/// パイプラインの実行オプション
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    /// ステージ間のチャンネルバッファサイズ
    channel_buffer_size: usize,
    /// 全体のタイムアウト（秒）
    timeout_sec: Option<u64>,
    /// 失敗したステージを再試行するかどうか
    retry_failed_stages: bool,
    /// 最大再試行回数
    max_retries: u32,
    /// 実行モード
    execution_mode: ExecutionMode,
}

/// パイプライン実行モード
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionMode {
    /// 順次実行（各ステージが前のステージの完了を待つ）
    Sequential,
    /// パイプライン実行（各ステージが並行して実行され、データフローで連携）
    Pipelined,
    /// ステージごとに並列実行（各ステージが独立して実行され、実行順序の依存関係のみ維持）
    Parallel,
}

/// パイプラインの状態
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineState {
    /// 初期状態
    Initial,
    /// 実行中
    Running(f32), // 進捗率（0.0〜1.0）
    /// 完了
    Completed,
    /// エラー
    Failed(String),
    /// キャンセル
    Cancelled,
    /// 一時停止
    Paused,
}

/// パイプラインのメトリクス
#[derive(Debug, Clone, Default)]
pub struct PipelineMetrics {
    /// 開始時刻
    pub start_time: Option<Instant>,
    /// 終了時刻
    pub end_time: Option<Instant>,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
    /// 処理したデータ量（バイト）
    pub processed_data_bytes: u64,
    /// 最大メモリ使用量（バイト）
    pub peak_memory_bytes: u64,
    /// 各ステージのメトリクス
    pub stage_metrics: Vec<StageMetrics>,
}

impl Pipeline {
    /// 新しいパイプラインを作成します
    pub fn new() -> Self {
        let (cancel_tx, _) = broadcast::channel(16);
        
        let default_options = PipelineOptions {
            channel_buffer_size: 100,
            timeout_sec: None,
            retry_failed_stages: false,
            max_retries: 3,
            execution_mode: ExecutionMode::Pipelined,
        };

        Self {
            id: Uuid::new_v4().to_string(),
            name: None,
            stages: Arc::new(RwLock::new(Vec::new())),
            cancel_tx,
            state: Arc::new(RwLock::new(PipelineState::Initial)),
            metrics: Arc::new(RwLock::new(PipelineMetrics::default())),
            properties: Arc::new(RwLock::new(HashMap::new())),
            options: Arc::new(RwLock::new(default_options)),
            job_id: None,
            runtime: None,
        }
    }

    /// 名前を指定して新しいパイプラインを作成します
    pub fn with_name(name: &str) -> Self {
        let mut pipeline = Self::new();
        pipeline.name = Some(name.to_string());
        pipeline
    }

    /// パイプラインのIDを返します
    pub fn id(&self) -> &str {
        &self.id
    }

    /// パイプラインの名前を返します
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// ステージをパイプラインに追加します
    pub fn add_stage(&self, stage: PipelineStage) {
        let mut stages = self.stages.try_lock().expect("ステージリストのロックに失敗しました");
        stages.push(stage);
    }

    /// パイプラインを実行します
    pub async fn execute(&self) -> Result<(), PipelineError> {
        let stages_count = {
            let stages = self.stages.read().await;
            stages.len()
        };
        
        if stages_count == 0 {
            return Err(PipelineError::PipelineConstructionFailed(
                "パイプラインにステージが追加されていません".to_string()
            ));
        }
        
        // 状態を更新
        self.update_state(PipelineState::Running(0.0)).await;
        
        // メトリクスの初期化
        {
            let mut metrics = self.metrics.write().await;
            metrics.start_time = Some(Instant::now());
            metrics.stage_metrics = vec![StageMetrics::default(); stages_count];
        }
        
        // Prometheusメトリクスを更新
        counter!("nexusshell_pipeline_executions_total", "pipeline_id" => self.id.clone()).increment(1);
        gauge!("nexusshell_pipeline_running", "pipeline_id" => self.id.clone()).set(1.0);
        
        debug!("パイプライン {} の実行を開始します（ステージ数: {}）", self.id, stages_count);
        
        // 実行オプションを取得
        let options = self.options.read().await.clone();
        
        // 実行モードに基づいて処理
        let result = match options.execution_mode {
            ExecutionMode::Sequential => self.execute_sequential().await,
            ExecutionMode::Pipelined => self.execute_pipelined().await,
            ExecutionMode::Parallel => self.execute_parallel().await,
        };
        
        // メトリクスを更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.end_time = Some(Instant::now());
            if let Some(start_time) = metrics.start_time {
                metrics.execution_time_ms = start_time.elapsed().as_millis() as u64;
                
                // Prometheusメトリクス
                histogram!("nexusshell_pipeline_execution_time_ms", "pipeline_id" => self.id.clone())
                    .record(metrics.execution_time_ms as f64);
            }
        }
        
        // 状態を更新
        match &result {
            Ok(_) => {
                self.update_state(PipelineState::Completed).await;
                gauge!("nexusshell_pipeline_running", "pipeline_id" => self.id.clone()).set(0.0);
                counter!("nexusshell_pipeline_completed_total", "pipeline_id" => self.id.clone()).increment(1);
                info!("パイプライン {} の実行が完了しました", self.id);
            }
            Err(e) => {
                self.update_state(PipelineState::Failed(e.to_string())).await;
                gauge!("nexusshell_pipeline_running", "pipeline_id" => self.id.clone()).set(0.0);
                counter!("nexusshell_pipeline_failed_total", "pipeline_id" => self.id.clone()).increment(1);
                error!("パイプライン {} の実行に失敗しました: {}", self.id, e);
            }
        }
        
        result
    }

    /// 順次実行モードでパイプラインを実行します
    async fn execute_sequential(&self) -> Result<(), PipelineError> {
        let mut stages = self.stages.write().await;
        let stages_count = stages.len();
        
        let options = self.options.read().await.clone();
        
        debug!("パイプライン {} を順次実行モードで実行します", self.id);
        
        let mut context = StageContext::new();
        let mut current_data = PipelineData::Empty;
        
        // 各ステージを順次実行
        for (i, stage) in stages.iter_mut().enumerate() {
            // キャンセルチェック
            if self.is_cancelled().await {
                return Err(PipelineError::CancellationFailed("パイプラインがキャンセルされました".to_string()));
            }
            
            debug!("ステージ {} ({}) を実行します", i + 1, stage.name());
            
            // ステージメトリクスを初期化
            {
                let mut metrics = self.metrics.write().await;
                metrics.stage_metrics[i].start_time = Some(Instant::now());
            }
            
            // キャンセルレシーバーをコンテキストに追加
            context.cancel_rx = Some(self.cancel_tx.subscribe());
            
            // ステージを実行
            let stage_result = match options.retry_failed_stages {
                true => {
                    let mut attempts = 0;
                    let mut last_error = None;
                    
                    loop {
                        attempts += 1;
                        
                        match stage.execute_with_data(&mut context, current_data.clone()).await {
                            Ok(output) => {
                                current_data = output;
                                break Ok(());
                            }
                            Err(e) => {
                                if attempts >= options.max_retries {
                                    last_error = Some(e);
                                    break Err(last_error.unwrap());
                                }
                                
                                warn!("ステージ {} の実行に失敗しました（試行 {}/{}）: {}", 
                                      stage.name(), attempts, options.max_retries, e);
                                
                                // 少し待機してから再試行
                                tokio::time::sleep(Duration::from_millis(500)).await;
                            }
                        }
                    }
                }
                false => {
                    match stage.execute_with_data(&mut context, current_data.clone()).await {
                        Ok(output) => {
                            current_data = output;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            
            // ステージのメトリクスを更新
            {
                let mut metrics = self.metrics.write().await;
                metrics.stage_metrics[i].end_time = Some(Instant::now());
                if let Some(start_time) = metrics.stage_metrics[i].start_time {
                    metrics.stage_metrics[i].execution_time_ms = start_time.elapsed().as_millis() as u64;
                }
                metrics.stage_metrics[i].success = stage_result.is_ok();
            }
            
            // エラー処理
            if let Err(e) = stage_result {
                error!("ステージ {} の実行に失敗しました: {}", stage.name(), e);
                
                // 進捗状態を更新
                self.update_state(PipelineState::Running(i as f32 / stages_count as f32)).await;
                
                return Err(PipelineError::StageExecutionFailed(
                    format!("ステージ {} の実行に失敗しました: {}", stage.name(), e)
                ));
            }
            
            // 進捗状態を更新
            self.update_state(PipelineState::Running((i + 1) as f32 / stages_count as f32)).await;
        }
        
        debug!("パイプライン {} の順次実行が完了しました", self.id);
        
        Ok(())
    }

    /// パイプライン実行モードでパイプラインを実行します
    async fn execute_pipelined(&self) -> Result<(), PipelineError> {
        let mut stages = self.stages.write().await;
        let stages_count = stages.len();
        
        let options = self.options.read().await.clone();
        
        debug!("パイプライン {} をパイプライン実行モードで実行します", self.id);
        
        // ステージ間のチャンネルを設定
        let channel_buffer_size = options.channel_buffer_size;
        let mut channels = Vec::with_capacity(stages_count + 1);
        
        // チャンネルを作成
        for _ in 0..=stages_count {
            let (tx, rx) = mpsc::channel::<PipelineData>(channel_buffer_size);
            channels.push((tx, rx));
        }
        
        // ステージにチャンネルを設定
        let mut stage_handles = Vec::with_capacity(stages_count);
        
        for (i, stage) in stages.iter_mut().enumerate() {
            let (_, input_rx) = &channels[i];
            let (output_tx, _) = &channels[i + 1];
            
            // 入出力チャンネルを設定
            stage.set_input_channel(output_tx.clone());
            stage.set_output_channel(input_rx.clone());
            
            // キャンセルチャンネルを含むコンテキストを設定
            let mut context = StageContext::new();
            context.cancel_rx = Some(self.cancel_tx.subscribe());
            stage.set_context(context);
            
            // ステージメトリクスを初期化
            {
                let mut metrics = self.metrics.write().await;
                metrics.stage_metrics[i].start_time = Some(Instant::now());
            }
            
            // ステージを実行
            let name = stage.name().to_string();
            let stage_metrics = Arc::clone(&self.metrics);
            let stage_index = i;
            
            let handle = tokio::spawn(async move {
                debug!("ステージ {} ({}) を実行します", stage_index + 1, name);
                
                let result = stage.execute().await;
                
                // ステージのメトリクスを更新
                {
                    let mut metrics = stage_metrics.write().await;
                    metrics.stage_metrics[stage_index].end_time = Some(Instant::now());
                    if let Some(start_time) = metrics.stage_metrics[stage_index].start_time {
                        metrics.stage_metrics[stage_index].execution_time_ms = start_time.elapsed().as_millis() as u64;
                    }
                    metrics.stage_metrics[stage_index].success = result.is_ok();
                }
                
                if let Err(e) = &result {
                    error!("ステージ {} の実行に失敗しました: {}", name, e);
                }
                
                result
            });
            
            stage_handles.push(handle);
        }
        
        // 初期入力を送信
        let (first_tx, _) = &channels[0];
        if let Err(_) = first_tx.send(PipelineData::Empty).await {
            return Err(PipelineError::DataTransferFailed("初期入力の送信に失敗しました".to_string()));
        }
        
        // タイムアウトの設定
        let pipeline_future = async {
            // すべてのステージが完了するのを待つ
            for (i, handle) in stage_handles.into_iter().enumerate() {
                match handle.await {
                    Ok(result) => {
                        if let Err(e) = result {
                            return Err(PipelineError::StageExecutionFailed(
                                format!("ステージ {} の実行に失敗しました: {}", i + 1, e)
                            ));
                        }
                        
                        // 進捗状態を更新
                        self.update_state(PipelineState::Running((i + 1) as f32 / stages_count as f32)).await;
                    }
                    Err(e) => {
                        return Err(PipelineError::StageExecutionFailed(
                            format!("ステージ {} の実行タスクがパニックしました: {}", i + 1, e)
                        ));
                    }
                }
            }
            
            debug!("パイプライン {} のパイプライン実行が完了しました", self.id);
            Ok(())
        };
        
        // オプションでタイムアウトを適用
        if let Some(timeout_sec) = options.timeout_sec {
            match timeout(Duration::from_secs(timeout_sec), pipeline_future).await {
                Ok(result) => result,
                Err(_) => {
                    // タイムアウトが発生したらキャンセル
                    let _ = self.cancel().await;
                    Err(PipelineError::Timeout)
                }
            }
        } else {
            pipeline_future.await
        }
    }

    /// 並列実行モードでパイプラインを実行します
    async fn execute_parallel(&self) -> Result<(), PipelineError> {
        let stages = self.stages.read().await;
        let stages_count = stages.len();
        
        debug!("パイプライン {} を並列実行モードで実行します", self.id);
        
        // 各ステージを並列に実行するための準備
        let mut stage_contexts = Vec::with_capacity(stages_count);
        let mut stage_handles = Vec::with_capacity(stages_count);
        
        // PipelineDataをステージ間で共有するためのストレージ
        let data_storage = Arc::new(RwLock::new(HashMap::<String, PipelineData>::new()));
        
        // 各ステージのコンテキストを設定
        for _ in 0..stages_count {
            let mut context = StageContext::new();
            context.cancel_rx = Some(self.cancel_tx.subscribe());
            context.shared_data.insert("data_storage".to_string(), Box::new(Arc::clone(&data_storage)));
            stage_contexts.push(context);
        }
        
        // 各ステージを実行
        for (i, stage) in stages.iter().enumerate() {
            // ステージメトリクスを初期化
            {
                let mut metrics = self.metrics.write().await;
                metrics.stage_metrics[i].start_time = Some(Instant::now());
            }
            
            let name = stage.name().to_string();
            let stage_clone = stage.clone();
            let mut context = stage_contexts[i].clone();
            let stage_metrics = Arc::clone(&self.metrics);
            let stage_index = i;
            let data_storage_clone = Arc::clone(&data_storage);
            
            // 依存関係のあるステージの出力を待つための条件
            let deps_condition = stage.dependencies().clone();
            
            let handle = tokio::spawn(async move {
                debug!("ステージ {} ({}) を実行準備します", stage_index + 1, name);
                
                // 依存関係のあるステージの出力を待つ
                if !deps_condition.is_empty() {
                    for dep in &deps_condition {
                        loop {
                            let storage = data_storage_clone.read().await;
                            if storage.contains_key(dep) {
                                break;
                            }
                            drop(storage);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                
                debug!("ステージ {} ({}) を実行します", stage_index + 1, name);
                
                // 入力データを取得（依存ステージがある場合）
                let input_data = if !deps_condition.is_empty() && !deps_condition[0].is_empty() {
                    let storage = data_storage_clone.read().await;
                    storage.get(&deps_condition[0]).cloned().unwrap_or(PipelineData::Empty)
                } else {
                    PipelineData::Empty
                };
                
                // ステージを実行
                let result = match stage_clone.execute_with_data(&mut context, input_data).await {
                    Ok(output) => {
                        // 出力を保存
                        let mut storage = data_storage_clone.write().await;
                        storage.insert(name.clone(), output);
                        Ok(())
                    }
                    Err(e) => Err(e),
                };
                
                // ステージのメトリクスを更新
                {
                    let mut metrics = stage_metrics.write().await;
                    metrics.stage_metrics[stage_index].end_time = Some(Instant::now());
                    if let Some(start_time) = metrics.stage_metrics[stage_index].start_time {
                        metrics.stage_metrics[stage_index].execution_time_ms = start_time.elapsed().as_millis() as u64;
                    }
                    metrics.stage_metrics[stage_index].success = result.is_ok();
                }
                
                if let Err(e) = &result {
                    error!("ステージ {} の実行に失敗しました: {}", name, e);
                }
                
                result
            });
            
            stage_handles.push(handle);
        }
        
        // すべてのステージが完了するのを待つ
        let results = future::join_all(stage_handles).await;
        
        // 結果を処理
        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(stage_result) => {
                    if let Err(e) = stage_result {
                        return Err(PipelineError::StageExecutionFailed(
                            format!("ステージ {} の実行に失敗しました: {}", i + 1, e)
                        ));
                    }
                }
                Err(e) => {
                    return Err(PipelineError::StageExecutionFailed(
                        format!("ステージ {} の実行タスクがパニックしました: {}", i + 1, e)
                    ));
                }
            }
            
            // 進捗状態を更新
            self.update_state(PipelineState::Running((i + 1) as f32 / stages_count as f32)).await;
        }
        
        debug!("パイプライン {} の並列実行が完了しました", self.id);
        
        Ok(())
    }

    /// パイプラインをキャンセルします
    pub async fn cancel(&self) -> Result<(), PipelineError> {
        debug!("パイプライン {} をキャンセルします", self.id);
        
        // 状態がRunningでなければエラー
        {
            let state = self.state.read().await;
            if !matches!(*state, PipelineState::Running(_)) {
                return Err(PipelineError::CancellationFailed(
                    format!("パイプラインを現在の状態 ({:?}) からキャンセルできません", state)
                ));
            }
        }
        
        // キャンセル通知を送信
        let _ = self.cancel_tx.send(());
        
        // 状態を更新
        self.update_state(PipelineState::Cancelled).await;
        
        // Prometheusメトリクスを更新
        gauge!("nexusshell_pipeline_running", "pipeline_id" => self.id.clone()).set(0.0);
        counter!("nexusshell_pipeline_cancelled_total", "pipeline_id" => self.id.clone()).increment(1);
        
        debug!("パイプライン {} をキャンセルしました", self.id);
        
        Ok(())
    }

    /// パイプラインの状態を取得します
    pub async fn state(&self) -> PipelineState {
        self.state.read().await.clone()
    }

    /// パイプラインの状態を更新します
    async fn update_state(&self, state: PipelineState) {
        let mut current_state = self.state.write().await;
        *current_state = state.clone();
        
        // 進捗状態なら進捗メトリクスを更新
        if let PipelineState::Running(progress) = state {
            gauge!("nexusshell_pipeline_progress", "pipeline_id" => self.id.clone()).set(progress as f64 * 100.0);
        }
    }

    /// パイプラインがキャンセルされたかどうかを確認します
    async fn is_cancelled(&self) -> bool {
        matches!(*self.state.read().await, PipelineState::Cancelled)
    }

    /// パイプラインのメトリクスを取得します
    pub async fn metrics(&self) -> PipelineMetrics {
        self.metrics.read().await.clone()
    }

    /// パイプラインのプロパティを設定します
    pub async fn set_property(&self, key: &str, value: &str) {
        let mut properties = self.properties.write().await;
        properties.insert(key.to_string(), value.to_string());
    }

    /// パイプラインのプロパティを取得します
    pub async fn get_property(&self, key: &str) -> Option<String> {
        let properties = self.properties.read().await;
        properties.get(key).cloned()
    }

    /// パイプラインの実行オプションを設定します
    pub async fn set_options(&self, options: PipelineOptions) {
        let mut current_options = self.options.write().await;
        *current_options = options;
    }

    /// パイプラインをリセットします
    pub async fn reset(&self) -> Result<(), PipelineError> {
        debug!("パイプライン {} をリセットします", self.id);
        
        // 実行中なら中止
        if matches!(*self.state.read().await, PipelineState::Running(_)) {
            let _ = self.cancel().await;
        }
        
        // 状態を初期状態に戻す
        self.update_state(PipelineState::Initial).await;
        
        // メトリクスをリセット
        {
            let mut metrics = self.metrics.write().await;
            *metrics = PipelineMetrics::default();
        }
        
        debug!("パイプライン {} をリセットしました", self.id);
        
        Ok(())
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            channel_buffer_size: 100,
            timeout_sec: None,
            retry_failed_stages: false,
            max_retries: 3,
            execution_mode: ExecutionMode::Pipelined,
        }
    }
} 