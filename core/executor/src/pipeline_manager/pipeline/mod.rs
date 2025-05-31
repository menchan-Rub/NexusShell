/*!
# パイプラインモジュール

高性能で柔軟なパイプライン実行エンジンを提供するコアモジュール。
複雑なデータ処理フローを効率的に実行できます。

## 主な機能

- 宣言的パイプライン構築
- 非同期・並列実行
- 詳細なメトリクス収集
- キャンセル可能な実行フロー
- リアルタイムの進捗監視
*/

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, Context};
use futures::{stream::FuturesUnordered, StreamExt};
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, trace, warn};
use uuid::Uuid;

use crate::pipeline_manager::error::{PipelineError, StageError};
use crate::pipeline_manager::stages::{
    DataType, DataTypeKind, Stage, StageConfig, StageDefinition, StageFactory, 
    StageId, StageKind, StageMetrics, StageRef, StageState
};
use crate::pipeline_manager::{PipelineContext, PipelineId, PipelineResult, StageResult};
use crate::sandbox::SandboxConfig;

/// パイプラインの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineStatus {
    /// 初期状態
    Initial,
    /// 準備中
    Preparing,
    /// 実行中
    Running,
    /// 一時停止中
    Paused,
    /// 完了
    Completed,
    /// 失敗
    Failed,
    /// キャンセル済み
    Cancelled,
}

impl fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineStatus::Initial => write!(f, "Initial"),
            PipelineStatus::Preparing => write!(f, "Preparing"),
            PipelineStatus::Running => write!(f, "Running"),
            PipelineStatus::Paused => write!(f, "Paused"),
            PipelineStatus::Completed => write!(f, "Completed"),
            PipelineStatus::Failed => write!(f, "Failed"),
            PipelineStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// パイプラインイベント
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// パイプライン開始
    Started {
        /// パイプラインID
        pipeline_id: PipelineId,
        /// ステージ数
        stage_count: usize,
    },
    /// ステージ開始
    StageStarted {
        /// パイプラインID
        pipeline_id: PipelineId,
        /// ステージID
        stage_id: StageId,
        /// ステージ名
        stage_name: String,
    },
    /// ステージ完了
    StageCompleted {
        /// パイプラインID
        pipeline_id: PipelineId,
        /// ステージID
        stage_id: StageId,
        /// ステージ名
        stage_name: String,
        /// 実行時間
        execution_time: Duration,
    },
    /// ステージ失敗
    StageFailed {
        /// パイプラインID
        pipeline_id: PipelineId,
        /// ステージID
        stage_id: StageId,
        /// ステージ名
        stage_name: String,
        /// エラーメッセージ
        error: String,
    },
    /// パイプライン完了
    Completed {
        /// パイプラインID
        pipeline_id: PipelineId,
        /// 実行時間
        execution_time: Duration,
    },
    /// パイプライン失敗
    Failed {
        /// パイプラインID
        pipeline_id: PipelineId,
        /// エラーメッセージ
        error: String,
    },
    /// パイプラインキャンセル
    Cancelled {
        /// パイプラインID
        pipeline_id: PipelineId,
    },
    /// パイプライン一時停止
    Paused {
        /// パイプラインID
        pipeline_id: PipelineId,
    },
    /// パイプライン再開
    Resumed {
        /// パイプラインID
        pipeline_id: PipelineId,
    },
}

/// パイプラインスナップショット
#[derive(Debug, Clone)]
pub struct PipelineSnapshot {
    /// パイプラインID
    pub pipeline_id: PipelineId,
    /// パイプラインの状態
    pub status: PipelineStatus,
    /// 各ステージの状態
    pub stage_states: HashMap<StageId, StageState>,
    /// 開始時間
    pub start_time: Instant,
    /// 終了時間
    pub end_time: Option<Instant>,
    /// エラー（存在する場合）
    pub error: Option<String>,
    /// 現在実行中のステージ
    pub current_stage: Option<StageId>,
}

/// パイプライン設定
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// 最大ステージ数
    pub max_stages: usize,
    /// 実行タイムアウト
    pub timeout: Duration,
    /// メトリクス収集を有効にするかどうか
    pub enable_metrics: bool,
    /// サンドボックス設定
    pub sandbox_config: Option<SandboxConfig>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_stages: 64,
            timeout: Duration::from_secs(3600), // 1時間
            enable_metrics: true,
            sandbox_config: None,
        }
    }
}

/// パイプライン
pub struct Pipeline {
    /// パイプラインID
    id: PipelineId,
    /// パイプラインの状態
    status: Arc<RwLock<PipelineStatus>>,
    /// パイプラインステージ
    stages: HashMap<StageId, StageRef>,
    /// ステージ依存関係
    dependencies: HashMap<StageId, Vec<StageId>>,
    /// 実行順序
    execution_order: Vec<StageId>,
    /// キャンセルチャネル
    cancel_tx: mpsc::Sender<()>,
    cancel_rx: Arc<Mutex<mpsc::Receiver<()>>>,
    /// ステージ間データバッファ
    stage_outputs: Arc<RwLock<HashMap<StageId, DataType>>>,
    /// パイプライン設定
    config: PipelineConfig,
    /// メトリクス
    metrics_tx: Option<mpsc::Sender<PipelineEvent>>,
    /// パイプラインの開始時間
    start_time: Instant,
}

impl Pipeline {
    /// 新しいパイプラインを作成
    pub fn new(id: PipelineId, config: PipelineConfig) -> Self {
        // パイプラインの開始時間を記録
        let start_time = Instant::now();
        
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        
        let mut pipeline = Self {
            id,
            status: Arc::new(RwLock::new(PipelineStatus::Initial)),
            stages: HashMap::new(),
            dependencies: HashMap::new(),
            execution_order: Vec::new(),
            cancel_tx,
            cancel_rx: Arc::new(Mutex::new(cancel_rx)),
            stage_outputs: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics_tx: None,
            start_time, // 開始時間を保持
        };
        
        // メトリクス収集を設定
        if pipeline.config.enable_metrics {
            let (tx, rx) = mpsc::channel(100);
            pipeline.metrics_tx = Some(tx);
            pipeline.start_metrics_collection(rx);
        }
        
        pipeline
    }
    
    /// ステージを追加
    pub fn add_stage(&mut self, stage: StageRef, dependencies: Vec<StageId>) -> Result<()> {
        let stage_id = stage.id().clone();
        
        // 既に存在するステージIDをチェック
        if self.stages.contains_key(&stage_id) {
            return Err(anyhow!("ステージID '{}'は既に存在します", stage_id));
        }
        
        // 最大ステージ数をチェック
        if self.stages.len() >= self.config.max_stages {
            return Err(anyhow!("パイプラインの最大ステージ数 ({}) に達しました", self.config.max_stages));
        }
        
        // 依存関係をチェック
        for dep_id in &dependencies {
            if !self.stages.contains_key(dep_id) {
                return Err(anyhow!("依存ステージ '{}' が見つかりません", dep_id));
            }
        }
        
        // ステージと依存関係を追加
        self.stages.insert(stage_id.clone(), stage);
        self.dependencies.insert(stage_id.clone(), dependencies);
        
        // 実行順序を更新
        self.update_execution_order()?;
        
        Ok(())
    }
    
    /// ステージ数を取得
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }
    
    /// パイプラインの状態を取得
    pub async fn status(&self) -> PipelineStatus {
        let status = self.status.read().await;
        *status
    }
    
    /// 実行順序を更新（トポロジカルソート）
    fn update_execution_order(&mut self) -> Result<()> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();
        
        // 依存関係に基づいて実行順序を計算
        for stage_id in self.stages.keys() {
            if !visited.contains(stage_id) {
                self.visit_stage(stage_id, &mut visited, &mut temp_visited, &mut order)?;
            }
        }
        
        // 逆順にして正しい実行順序に
        order.reverse();
        self.execution_order = order;
        
        Ok(())
    }
    
    /// ステージを再帰的に訪問（トポロジカルソート用）
    fn visit_stage(
        &self,
        stage_id: &StageId,
        visited: &mut HashSet<StageId>,
        temp_visited: &mut HashSet<StageId>,
        order: &mut Vec<StageId>,
    ) -> Result<()> {
        // 循環依存関係のチェック
        if temp_visited.contains(stage_id) {
            return Err(anyhow!("循環依存関係が検出されました"));
        }
        
        // 既に訪問済みならスキップ
        if visited.contains(stage_id) {
            return Ok(());
        }
        
        temp_visited.insert(stage_id.clone());
        
        // 依存関係を再帰的に訪問
        if let Some(deps) = self.dependencies.get(stage_id) {
            for dep_id in deps {
                self.visit_stage(dep_id, visited, temp_visited, order)?;
            }
        }
        
        temp_visited.remove(stage_id);
        visited.insert(stage_id.clone());
        order.push(stage_id.clone());
        
        Ok(())
    }
    
    /// パイプラインのスナップショットを取得
    pub async fn snapshot(&self) -> PipelineSnapshot {
        let status = self.status.read().await;
        
        // 各ステージの状態を収集
        let mut stage_states = HashMap::new();
        for (id, stage) in &self.stages {
            stage_states.insert(id.clone(), stage.state().await);
        }
        
        // 現在実行中のステージを特定
        let current_stage = if *status == PipelineStatus::Running {
            // 実行中のステージを検索
            let mut result = None;
            for (id, stage) in &self.stages {
                let state = stage.state().await;
                if state == StageState::Running {
                        return PipelineSnapshot {
                            pipeline_id: self.id.clone(),
                            status: *status,
                            stage_states,
                        start_time: self.start_time, // 保存された開始時間を使用
                            end_time: None,
                            error: None,
                            current_stage: Some(id.clone()),
                        };
                }
            }
            None
        } else {
            None
        };
        
        PipelineSnapshot {
            pipeline_id: self.id.clone(),
            status: *status,
            stage_states,
            start_time: self.start_time, // 保存された開始時間を使用
            end_time: None,
            error: None,
            current_stage,
        }
    }
    
    /// パイプラインを実行
    #[instrument(skip(self, context), fields(pipeline_id = %context.pipeline_id))]
    pub async fn execute(&self, context: &PipelineContext) -> Result<PipelineResult> {
        let start_time = Instant::now();
        
        // 状態を更新
        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Preparing;
        }
        
        // ステージを初期化
        debug!("パイプライン {} を準備中（ステージ数: {}）", self.id, self.stages.len());
        let mut initialization_errors = Vec::new();
        for (id, stage) in &self.stages {
            debug!("ステージ {} ({:?}) を初期化中", id, stage.kind());
            match stage.initialize().await {
                Ok(_) => {
                    trace!("ステージ {} の初期化完了", id);
                },
                Err(e) => {
                    error!("ステージ {} の初期化に失敗: {}", id, e);
                    initialization_errors.push((id.clone(), e));
                }
            }
        }
        
        // 初期化エラーをチェック
        if !initialization_errors.is_empty() {
            let (id, err) = &initialization_errors[0];
            let mut status = self.status.write().await;
            *status = PipelineStatus::Failed;
            
            return Err(PipelineError::stage_error(
                id.clone(),
                self.stages.get(id).unwrap().kind(),
                format!("初期化エラー: {}", err),
                Some(err.clone()),
            ).into());
        }
        
        // 状態を実行中に更新
        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Running;
        }
        
        debug!("パイプライン {} の実行を開始", self.id);
        
        // ステージ実行結果を保持
        let mut stage_results = HashMap::new();
        
        // 実行順序に従ってステージを実行
        for stage_id in &self.execution_order {
            // キャンセルチェック
            if self.is_cancelled().await {
                info!("パイプライン {} がキャンセルされました", self.id);
                
                let mut status = self.status.write().await;
                *status = PipelineStatus::Cancelled;
                
                return Ok(PipelineResult {
                    pipeline_id: self.id.clone(),
                    status: PipelineStatus::Cancelled,
                    start_time,
                    end_time: Instant::now(),
                    error: Some("パイプラインがキャンセルされました".to_string()),
                    output: None,
                    stage_results,
                    metrics: crate::pipeline_manager::PipelineMetrics::default(),
                });
            }
            
            let stage = &self.stages[stage_id];
            info!("ステージ {} ({:?}) を実行中", stage_id, stage.kind());
            
            // イベント発行
            self.emit_event(PipelineEvent::StageStarted {
                pipeline_id: self.id.clone(),
                stage_id: stage_id.clone(),
                stage_name: stage.name().to_string(),
            }).await;
            
            // 入力データを収集
            let input = self.collect_stage_input(stage_id).await;
            
            // ステージ実行開始時間
            let stage_start_time = Instant::now();
            
            // ステージを実行
            let stage_result = match stage.execute(input).await {
                Ok(output) => {
                    let execution_time = stage_start_time.elapsed();
                    info!("ステージ {} が完了（実行時間: {:?}）", stage_id, execution_time);
                    
                    // イベント発行
                    self.emit_event(PipelineEvent::StageCompleted {
                        pipeline_id: self.id.clone(),
                        stage_id: stage_id.clone(),
                        stage_name: stage.name().to_string(),
                        execution_time,
                    }).await;
                    
                    // 出力を保存
                    let mut outputs = self.stage_outputs.write().await;
                    outputs.insert(stage_id.clone(), output.clone());
                    
                    // メトリクスを取得
                    let metrics = stage.metrics().await;
                    
                    // ステージ結果を作成
                    StageResult {
                        stage_id: stage_id.clone(),
                        stage_name: stage.name().to_string(),
                        status: StageState::Completed,
                        start_time: stage_start_time,
                        end_time: Instant::now(),
                        output: Some(output),
                        error: None,
                        metrics,
                    }
                },
                Err(e) => {
                    error!("ステージ {} の実行に失敗: {}", stage_id, e);
                    
                    // イベント発行
                    self.emit_event(PipelineEvent::StageFailed {
                        pipeline_id: self.id.clone(),
                        stage_id: stage_id.clone(),
                        stage_name: stage.name().to_string(),
                        error: e.to_string(),
                    }).await;
                    
                    // ステージ結果を作成
                    StageResult {
                        stage_id: stage_id.clone(),
                        stage_name: stage.name().to_string(),
                        status: StageState::Failed,
                        start_time: stage_start_time,
                        end_time: Instant::now(),
                        output: None,
                        error: Some(e.to_string()),
                        metrics: stage.metrics().await,
                    }
                }
            };
            
            // 結果を保存
            stage_results.insert(stage_id.clone(), stage_result.clone());
            
            // エラーチェック
            if stage_result.status == StageState::Failed {
                // パイプライン全体を失敗として扱う
                let mut status = self.status.write().await;
                *status = PipelineStatus::Failed;
                
                // イベント発行
                self.emit_event(PipelineEvent::Failed {
                    pipeline_id: self.id.clone(),
                    error: stage_result.error.clone().unwrap_or_else(|| "不明なエラー".to_string()),
                }).await;
                
                return Ok(PipelineResult {
                    pipeline_id: self.id.clone(),
                    status: PipelineStatus::Failed,
                    start_time,
                    end_time: Instant::now(),
                    error: stage_result.error,
                    output: None,
                    stage_results,
                    metrics: crate::pipeline_manager::PipelineMetrics::default(),
                });
            }
        }
        
        // すべてのステージが成功した場合
        let mut status = self.status.write().await;
        *status = PipelineStatus::Completed;
        
        // 最終出力を取得（最後のステージの出力）
        let final_output = if let Some(last_stage_id) = self.execution_order.last() {
            let outputs = self.stage_outputs.read().await;
            outputs.get(last_stage_id).cloned()
        } else {
            None
        };
        
        let execution_time = start_time.elapsed();
        info!("パイプライン {} の実行が完了（実行時間: {:?}）", self.id, execution_time);
        
        // イベント発行
        self.emit_event(PipelineEvent::Completed {
            pipeline_id: self.id.clone(),
            execution_time,
        }).await;
        
        // パイプライン結果を返す
        Ok(PipelineResult {
            pipeline_id: self.id.clone(),
            status: PipelineStatus::Completed,
            start_time,
            end_time: Instant::now(),
            error: None,
            output: final_output,
            stage_results,
            metrics: crate::pipeline_manager::PipelineMetrics::default(),
        })
    }
    
    /// ステージの入力データを収集
    async fn collect_stage_input(&self, stage_id: &StageId) -> Option<DataType> {
        // 依存関係を確認
        if let Some(deps) = self.dependencies.get(stage_id) {
            if deps.is_empty() {
                // 入力ステージは外部入力がない
                None
            } else if deps.len() == 1 {
                // 単一の依存関係
                let dep_id = &deps[0];
                let outputs = self.stage_outputs.read().await;
                outputs.get(dep_id).cloned()
            } else {
                // 複数の依存関係（データをマージ）
                let outputs = self.stage_outputs.read().await;
                let mut combined = HashMap::new();
                
                for dep_id in deps {
                    if let Some(output) = outputs.get(dep_id) {
                        match output {
                            DataType::KeyValue(kv) => {
                                for (k, v) in kv {
                                    combined.insert(k.clone(), v.clone());
                                }
                            },
                            _ => {
                                // 非KeyValue型は依存関係の名前をキーにして格納
                                combined.insert(dep_id.to_string(), output.clone());
                            }
                        }
                    }
                }
                
                if combined.is_empty() {
                    None
                } else {
                    Some(DataType::KeyValue(combined))
                }
            }
        } else {
            None
        }
    }
    
    /// パイプラインをキャンセル
    pub async fn cancel(&self) -> Result<()> {
        debug!("パイプライン {} をキャンセル中", self.id);
        
        // キャンセル通知を送信
        if let Err(e) = self.cancel_tx.send(()).await {
            warn!("パイプライン {} のキャンセル通知送信に失敗: {}", self.id, e);
        }
        
        // すべてのステージをキャンセル
        for (id, stage) in &self.stages {
            if let Err(e) = stage.cancel().await {
                warn!("ステージ {} のキャンセルに失敗: {}", id, e);
            }
        }
        
        // 状態を更新
        let mut status = self.status.write().await;
        *status = PipelineStatus::Cancelled;
        
        info!("パイプライン {} がキャンセルされました", self.id);
        
        Ok(())
    }
    
    /// キャンセルされたかどうかを確認
    async fn is_cancelled(&self) -> bool {
        let mut rx = self.cancel_rx.lock().await;
        match rx.try_recv() {
            Ok(_) | Err(mpsc::error::TryRecvError::Closed) => true,
            Err(mpsc::error::TryRecvError::Empty) => false,
        }
    }
    
    /// イベントを発行
    async fn emit_event(&self, event: PipelineEvent) {
        if let Some(tx) = &self.metrics_tx {
            if let Err(e) = tx.send(event).await {
                warn!("イベント送信に失敗: {}", e);
            }
        }
    }
    
    /// メトリクス収集タスクを開始
    fn start_metrics_collection(&self, mut rx: mpsc::Receiver<PipelineEvent>) {
        // メトリクス収集タスクをバックグラウンドで実行
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                trace!("パイプラインイベント受信: {:?}", event);
                // ここでメトリクスを集計・保存する処理を実装
            }
        });
    }
}

/// パイプラインビルダー
pub struct PipelineBuilder {
    /// パイプラインID
    id: PipelineId,
    /// パイプライン設定
    config: PipelineConfig,
    /// ステージ定義
    stage_definitions: Vec<StageDefinition>,
    /// ステージファクトリ
    stage_factories: HashMap<StageId, Arc<dyn StageFactory>>,
}

impl PipelineBuilder {
    /// 新しいパイプラインビルダーを作成
    pub fn new(id: PipelineId, config: PipelineConfig) -> Self {
        Self {
            id,
            config,
            stage_definitions: Vec::new(),
            stage_factories: HashMap::new(),
        }
    }
    
    /// ステージ定義を追加
    pub fn add_stage_definition(&mut self, definition: StageDefinition) -> &mut Self {
        self.stage_definitions.push(definition);
        self
    }
    
    /// ステージファクトリを追加
    pub fn add_stage_factory(&mut self, stage_id: StageId, factory: Arc<dyn StageFactory>) -> &mut Self {
        self.stage_factories.insert(stage_id, factory);
        self
    }
    
    /// ステージ定義一覧を取得
    pub fn stage_definitions(&self) -> &[StageDefinition] {
        &self.stage_definitions
    }
    
    /// パイプラインを構築
    pub fn build(&self) -> Result<Pipeline> {
        let mut pipeline = Pipeline::new(self.id.clone(), self.config.clone());
        
        // 各ステージを作成して追加
        for definition in &self.stage_definitions {
            let stage_id = definition.id.clone();
            
            // ステージファクトリを取得
            let factory = self.stage_factories.get(&stage_id).ok_or_else(|| {
                anyhow!("ステージID '{}' のファクトリが見つかりません", stage_id)
            })?;
            
            // ステージ設定を作成
            let config = StageConfig {
                id: stage_id.clone(),
                name: definition.name.clone(),
                kind: definition.kind.clone(),
                timeout: match self.config.timeout {
                    // 設定から取得
                    Some(timeout) => Some(timeout),
                    None => None,
                },
                retry: Some(RetryConfig {
                    max_attempts: 3, // デフォルト値
                    backoff_strategy: RetryBackoffStrategy::Exponential,
                    backoff_base_ms: 1000,
                }),
                memory_limit: Some(MemoryLimit {
                    max_bytes: 1024 * 1024 * 512, // 512MB
                    enforce: true,
                }),
                cpu_limit: Some(CpuLimit {
                    max_percent: 80.0, // 80% CPU使用率制限
                    enforce: true,
                }),
                properties: definition.properties.clone(),
                sandbox_config: self.config.sandbox_config.clone(),
                data_transformation: Some(crate::pipeline_manager::stages::DataTransformation {
                    input_type: definition.input_type,
                    output_type: definition.output_type,
                    schema: None,
                }),
            };
            
            // ステージを作成
            let stage = factory.create_stage(config).await?;
            
            // パイプラインに追加
            pipeline.add_stage(stage, definition.dependencies.clone())?;
        }
        
        Ok(pipeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    // テスト用のモックステージ
    struct MockStage {
        id: StageId,
        name: String,
        kind: StageKind,
        state: Arc<RwLock<StageState>>,
        metrics: StageMetrics,
    }
    
    impl MockStage {
        fn new(id: StageId, name: &str, kind: StageKind) -> Self {
            Self {
                id,
                name: name.to_string(),
                kind,
                state: Arc::new(RwLock::new(StageState::Initial)),
                metrics: StageMetrics::default(),
            }
        }
    }
    
    #[async_trait]
    impl Stage for MockStage {
        fn id(&self) -> &StageId {
            &self.id
        }
        
        fn name(&self) -> &str {
            &self.name
        }
        
        fn kind(&self) -> StageKind {
            self.kind.clone()
        }
        
        async fn state(&self) -> StageState {
            let state = self.state.read().await;
            *state
        }
        
        async fn metrics(&self) -> StageMetrics {
            self.metrics.clone()
        }
        
        async fn initialize(&self) -> Result<(), StageError> {
            let mut state = self.state.write().await;
            *state = StageState::Preparing;
            Ok(())
        }
        
        async fn execute(&self, _input: Option<DataType>) -> Result<DataType, StageError> {
            {
                let mut state = self.state.write().await;
                *state = StageState::Running;
            }
            
            // 簡単な処理をシミュレート
            tokio::time::sleep(Duration::from_millis(10)).await;
            
            {
                let mut state = self.state.write().await;
                *state = StageState::Completed;
            }
            
            Ok(DataType::Text(format!("Output from {}", self.name)))
        }
        
        async fn cleanup(&self) -> Result<(), StageError> {
            Ok(())
        }
        
        async fn cancel(&self) -> Result<(), StageError> {
            let mut state = self.state.write().await;
            *state = StageState::Cancelled;
            Ok(())
        }
        
        async fn pause(&self) -> Result<(), StageError> {
            let mut state = self.state.write().await;
            *state = StageState::Paused;
            Ok(())
        }
        
        async fn resume(&self) -> Result<(), StageError> {
            let mut state = self.state.write().await;
            *state = StageState::Running;
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_pipeline_execution() {
        // パイプラインを構築
        let mut pipeline = Pipeline::new(
            PipelineId::new(),
            PipelineConfig::default(),
        );
        
        // ステージを作成
        let stage1 = Arc::new(MockStage::new(
            StageId::from_string("stage1".to_string()),
            "Stage 1",
            StageKind::Filter,
        ));
        
        let stage2 = Arc::new(MockStage::new(
            StageId::from_string("stage2".to_string()),
            "Stage 2",
            StageKind::Transform,
        ));
        
        let stage3 = Arc::new(MockStage::new(
            StageId::from_string("stage3".to_string()),
            "Stage 3",
            StageKind::Aggregate,
        ));
        
        // パイプラインにステージを追加
        pipeline.add_stage(stage1, vec![]).unwrap();
        pipeline.add_stage(stage2, vec![StageId::from_string("stage1".to_string())]).unwrap();
        pipeline.add_stage(stage3, vec![StageId::from_string("stage2".to_string())]).unwrap();
        
        // 実行順序を確認
        assert_eq!(pipeline.execution_order.len(), 3);
        assert_eq!(pipeline.execution_order[0], StageId::from_string("stage1".to_string()));
        assert_eq!(pipeline.execution_order[1], StageId::from_string("stage2".to_string()));
        assert_eq!(pipeline.execution_order[2], StageId::from_string("stage3".to_string()));
        
        // パイプラインを実行
        let context = PipelineContext::new(pipeline.id.clone());
        let result = pipeline.execute(&context).await.unwrap();
        
        // 結果を確認
        assert_eq!(result.status, PipelineStatus::Completed);
        assert!(result.error.is_none());
        assert_eq!(result.stage_results.len(), 3);
        
        // すべてのステージが成功したことを確認
        for (_, stage_result) in &result.stage_results {
            assert_eq!(stage_result.status, StageState::Completed);
        }
    }
    
    #[tokio::test]
    async fn test_pipeline_cancellation() {
        // パイプラインを構築
        let mut pipeline = Pipeline::new(
            PipelineId::new(),
            PipelineConfig::default(),
        );
        
        // ステージを作成
        let stage1 = Arc::new(MockStage::new(
            StageId::from_string("stage1".to_string()),
            "Stage 1",
            StageKind::Filter,
        ));
        
        let stage2 = Arc::new(MockStage::new(
            StageId::from_string("stage2".to_string()),
            "Stage 2",
            StageKind::Transform,
        ));
        
        // パイプラインにステージを追加
        pipeline.add_stage(stage1, vec![]).unwrap();
        pipeline.add_stage(stage2, vec![StageId::from_string("stage1".to_string())]).unwrap();
        
        // バックグラウンドでパイプラインを実行
        let pipeline_arc = Arc::new(pipeline);
        let pipeline_clone = pipeline_arc.clone();
        let context = PipelineContext::new(pipeline_arc.id.clone());
        
        let execution_handle = tokio::spawn(async move {
            pipeline_clone.execute(&context).await
        });
        
        // 少し待ってからキャンセル
        tokio::time::sleep(Duration::from_millis(5)).await;
        pipeline_arc.cancel().await.unwrap();
        
        // 実行結果を取得
        let result = execution_handle.await.unwrap().unwrap();
        
        // キャンセルされたことを確認
        assert_eq!(result.status, PipelineStatus::Cancelled);
    }
} 