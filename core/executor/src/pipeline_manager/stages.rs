use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock, broadcast};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use metrics::{counter, gauge, histogram};
use log::{debug, error, info, warn, trace};

use super::error::PipelineError;

/// パイプラインステージの実行コンテキスト
#[derive(Clone)]
pub struct StageContext {
    /// 環境変数
    pub env_vars: HashMap<String, String>,
    /// 共有データ
    pub shared_data: HashMap<String, Box<dyn Any + Send + Sync>>,
    /// キャンセルチャンネル
    pub cancel_rx: Option<broadcast::Receiver<()>>,
    /// コンテキスト固有の変数
    pub variables: HashMap<String, String>,
    /// ロガー設定
    pub logging_enabled: bool,
    /// タイムアウト設定
    pub timeout: Option<Duration>,
}

impl StageContext {
    /// 新しいステージコンテキストを作成します
    pub fn new() -> Self {
        Self {
            env_vars: std::env::vars().collect(),
            shared_data: HashMap::new(),
            cancel_rx: None,
            variables: HashMap::new(),
            logging_enabled: true,
            timeout: None,
        }
    }

    /// キャンセルチャンネルを設定します
    pub fn with_cancel_channel(mut self, rx: broadcast::Receiver<()>) -> Self {
        self.cancel_rx = Some(rx);
        self
    }

    /// 環境変数を追加します
    pub fn add_env_var(&mut self, key: &str, value: &str) {
        self.env_vars.insert(key.to_string(), value.to_string());
    }

    /// 共有データを設定します
    pub fn set_shared_data<T: 'static + Send + Sync>(&mut self, key: &str, value: T) {
        self.shared_data.insert(key.to_string(), Box::new(value));
    }

    /// 共有データを取得します
    pub fn get_shared_data<T: 'static + Send + Sync>(&self, key: &str) -> Option<&T> {
        self.shared_data.get(key).and_then(|data| data.downcast_ref::<T>())
    }

    /// 可変の共有データを取得します
    pub fn get_shared_data_mut<T: 'static + Send + Sync>(&mut self, key: &str) -> Option<&mut T> {
        self.shared_data.get_mut(key).and_then(|data| data.downcast_mut::<T>())
    }

    /// 変数を設定します
    pub fn set_variable(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
    }

    /// 変数を取得します
    pub fn get_variable(&self, key: &str) -> Option<&str> {
        self.variables.get(key).map(|s| s.as_str())
    }

    /// タイムアウトを設定します
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    /// キャンセルされたかどうかを確認します
    pub async fn is_cancelled(&mut self) -> bool {
        if let Some(ref mut rx) = self.cancel_rx {
            match rx.try_recv() {
                Ok(_) | Err(broadcast::error::TryRecvError::Closed) => true,
                Err(broadcast::error::TryRecvError::Empty) => false,
                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                    // 受信が追いつかない場合も続行
                    false
                }
            }
        } else {
            false
        }
    }
}

impl Debug for StageContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageContext")
            .field("env_vars_count", &self.env_vars.len())
            .field("shared_data_count", &self.shared_data.len())
            .field("has_cancel_rx", &self.cancel_rx.is_some())
            .field("variables_count", &self.variables.len())
            .field("logging_enabled", &self.logging_enabled)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl Default for StageContext {
    fn default() -> Self {
        Self::new()
    }
}

/// パイプラインデータの型
#[derive(Debug, Clone)]
pub enum PipelineData {
    /// テキストデータ
    Text(String),
    /// バイナリデータ
    Binary(Vec<u8>),
    /// 構造化データ（JSON）
    Json(serde_json::Value),
    /// 複数データのバッチ
    Batch(Vec<Box<PipelineData>>),
    /// キー・バリューマップ
    Map(HashMap<String, Box<PipelineData>>),
    /// 空データ（開始または終了マーカー）
    Empty,
}

impl PipelineData {
    /// データサイズを計算します（バイト単位）
    pub fn size(&self) -> usize {
        match self {
            PipelineData::Text(text) => text.len(),
            PipelineData::Binary(data) => data.len(),
            PipelineData::Json(json) => json.to_string().len(),
            PipelineData::Batch(items) => items.iter().map(|item| item.size()).sum(),
            PipelineData::Map(map) => map.iter().map(|(k, v)| k.len() + v.size()).sum(),
            PipelineData::Empty => 0,
        }
    }

    /// テキストデータとして取得します
    pub fn as_text(&self) -> Option<&str> {
        match self {
            PipelineData::Text(text) => Some(text),
            _ => None,
        }
    }

    /// バイナリデータとして取得します
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            PipelineData::Binary(data) => Some(data),
            _ => None,
        }
    }

    /// JSONデータとして取得します
    pub fn as_json(&self) -> Option<&serde_json::Value> {
        match self {
            PipelineData::Json(json) => Some(json),
            _ => None,
        }
    }
}

/// ステージメトリクス
#[derive(Debug, Clone, Default)]
pub struct StageMetrics {
    /// 開始時刻
    pub start_time: Option<Instant>,
    /// 終了時刻
    pub end_time: Option<Instant>,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
    /// 処理した入力データ量（バイト）
    pub input_data_bytes: u64,
    /// 生成した出力データ量（バイト）
    pub output_data_bytes: u64,
    /// CPU使用率（0.0-1.0）
    pub cpu_usage: f64,
    /// メモリ使用量（バイト）
    pub memory_usage_bytes: u64,
    /// 処理したアイテム数
    pub items_processed: u64,
    /// 処理成功フラグ
    pub success: bool,
}

/// パイプラインステージのトレイト
#[async_trait]
pub trait Stage: Send + Sync {
    /// ステージの名前を返します
    fn name(&self) -> &str;
    
    /// ステージの説明を返します
    fn description(&self) -> &str;
    
    /// ステージを実行します
    async fn execute(
        &self,
        context: &mut StageContext,
        input: PipelineData,
    ) -> Result<PipelineData, PipelineError>;
    
    /// ステージに依存する他のステージの名前を返します（並列実行時に使用）
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }
    
    /// ステージがクローン可能かどうかを返します
    fn is_cloneable(&self) -> bool {
        false
    }
    
    /// ステージをクローンします（is_cloneableがtrueの場合のみ呼び出されます）
    fn clone_stage(&self) -> Option<Box<dyn Stage>> {
        None
    }
}

/// パイプラインステージ
#[derive(Clone)]
pub struct PipelineStage {
    /// ステージの実装
    stage: Arc<dyn Stage>,
    /// 入力チャンネル
    input_tx: Option<mpsc::Sender<PipelineData>>,
    /// 出力チャンネル
    output_rx: Option<mpsc::Receiver<PipelineData>>,
    /// ステージの実行コンテキスト
    context: Arc<RwLock<StageContext>>,
    /// ステージのメトリクス
    metrics: Arc<RwLock<StageMetrics>>,
    /// ステージのタイムアウト
    timeout: Option<Duration>,
    /// ステージの依存関係
    dependencies: Vec<String>,
    /// ステージのプロパティ
    properties: HashMap<String, String>,
}

impl PipelineStage {
    /// 新しいパイプラインステージを作成します
    pub fn new(stage: impl Stage + 'static) -> Self {
        Self {
            stage: Arc::new(stage),
            input_tx: None,
            output_rx: None,
            context: Arc::new(RwLock::new(StageContext::new())),
            metrics: Arc::new(RwLock::new(StageMetrics::default())),
            timeout: None,
            dependencies: Vec::new(),
            properties: HashMap::new(),
        }
    }

    /// ステージの名前を返します
    pub fn name(&self) -> &str {
        self.stage.name()
    }

    /// ステージの説明を返します
    pub fn description(&self) -> &str {
        self.stage.description()
    }

    /// ステージの依存関係を返します
    pub fn dependencies(&self) -> &Vec<String> {
        &self.dependencies
    }

    /// ステージの依存関係を設定します
    pub fn set_dependencies(&mut self, dependencies: Vec<String>) {
        self.dependencies = dependencies;
    }

    /// 入力チャンネルを設定します
    pub fn set_input_channel(&mut self, tx: mpsc::Sender<PipelineData>) {
        self.input_tx = Some(tx);
    }

    /// 出力チャンネルを設定します
    pub fn set_output_channel(&mut self, rx: mpsc::Receiver<PipelineData>) {
        self.output_rx = Some(rx);
    }

    /// コンテキストを設定します
    pub fn set_context(&mut self, context: StageContext) {
        let mut current_context = self.context.try_lock().unwrap();
        *current_context = context;
    }

    /// タイムアウトを設定します
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    /// プロパティを設定します
    pub fn set_property(&mut self, key: &str, value: &str) {
        self.properties.insert(key.to_string(), value.to_string());
    }

    /// プロパティを取得します
    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(|s| s.as_str())
    }

    /// メトリクスを取得します
    pub async fn metrics(&self) -> StageMetrics {
        self.metrics.read().await.clone()
    }

    /// ステージを実行します
    pub async fn execute(&mut self) -> Result<(), PipelineError> {
        let stage = self.stage.clone();
        let context = self.context.clone();
        let metrics = self.metrics.clone();
        
        let mut input_rx = match self.output_rx.take() {
            Some(rx) => rx,
            None => return Err(PipelineError::PipelineConstructionFailed(
                "出力チャンネルが設定されていません".to_string()
            )),
        };
        
        let output_tx = match self.input_tx.clone() {
            Some(tx) => tx,
            None => return Err(PipelineError::PipelineConstructionFailed(
                "入力チャンネルが設定されていません".to_string()
            )),
        };

        // ステージ名を取得
        let stage_name = stage.name().to_string();
        
        // メトリクスを初期化
        {
            let mut m = metrics.write().await;
            m.start_time = Some(Instant::now());
        }
        
        // Prometheusメトリクスを更新
        counter!("nexusshell_stage_executions_total", "stage" => stage_name.clone()).increment(1);

        let handle = tokio::spawn(async move {
            let mut items_processed: u64 = 0;
            let mut input_data_bytes: u64 = 0;
            let mut output_data_bytes: u64 = 0;
            
            while let Some(input) = input_rx.recv().await {
                let mut ctx = context.write().await;
                
                // キャンセルチェック
                if ctx.is_cancelled().await {
                    debug!("ステージ {} がキャンセルされました", stage_name);
                    break;
                }
                
                // 入力データサイズを記録
                let input_size = input.size() as u64;
                input_data_bytes += input_size;
                
                // タイムアウト設定
                let timeout_duration = ctx.timeout;
                
                // 実行開始を記録
                let start_processing = Instant::now();
                
                // ステージの実行（タイムアウト付き）
                let execution_result = match timeout_duration {
                    Some(duration) => {
                        tokio::time::timeout(
                            duration,
                            stage.execute(&mut ctx, input)
                        ).await
                        .unwrap_or_else(|_| {
                            Err(PipelineError::Timeout)
                        })
                    },
                    None => stage.execute(&mut ctx, input).await,
                };
                
                // 実行時間を記録
                let processing_time = start_processing.elapsed();
                
                // Prometheusメトリクス更新
                histogram!("nexusshell_stage_processing_time_ms", "stage" => stage_name.clone())
                    .record(processing_time.as_millis() as f64);
                
                match execution_result {
                    Ok(output) => {
                        // 出力データサイズを記録
                        let output_size = output.size() as u64;
                        output_data_bytes += output_size;
                        
                        // 処理アイテム数を更新
                        items_processed += 1;
                        
                        // 出力を次のステージに送信
                        if let Err(e) = output_tx.send(output).await {
                            error!("ステージ {} からの出力送信に失敗しました: {}", stage_name, e);
                            break;
                        }
                        
                        // メトリクス更新
                        if items_processed % 100 == 0 {
                            let mut m = metrics.write().await;
                            m.items_processed = items_processed;
                            m.input_data_bytes = input_data_bytes;
                            m.output_data_bytes = output_data_bytes;
                            
                            // Prometheusメトリクス更新
                            gauge!("nexusshell_stage_items_processed", "stage" => stage_name.clone())
                                .set(items_processed as f64);
                        }
                    }
                    Err(e) => {
                        error!("ステージ {} の実行に失敗しました: {}", stage_name, e);
                        
                        // エラーメトリクスを更新
                        counter!("nexusshell_stage_errors_total", "stage" => stage_name.clone(), "error" => e.to_string()).increment(1);
                        
                        // エラー出力を送信
                        let _ = output_tx.send(PipelineData::Empty).await;
                        break;
                    }
                }
            }
            
            // 終了メトリクスを更新
            {
                let mut m = metrics.write().await;
                m.end_time = Some(Instant::now());
                if let Some(start_time) = m.start_time {
                    m.execution_time_ms = start_time.elapsed().as_millis() as u64;
                }
                m.items_processed = items_processed;
                m.input_data_bytes = input_data_bytes;
                m.output_data_bytes = output_data_bytes;
                m.success = true;
                
                // Prometheusメトリクス最終更新
                gauge!("nexusshell_stage_items_processed", "stage" => stage_name.clone())
                    .set(items_processed as f64);
                gauge!("nexusshell_stage_data_processed_bytes", "stage" => stage_name.clone())
                    .set(input_data_bytes as f64);
                gauge!("nexusshell_stage_data_produced_bytes", "stage" => stage_name.clone())
                    .set(output_data_bytes as f64);
                histogram!("nexusshell_stage_execution_time_ms", "stage" => stage_name.clone())
                    .record(m.execution_time_ms as f64);
            }
            
            // 終了マーカーを送信
            let _ = output_tx.send(PipelineData::Empty).await;
            
            info!("ステージ {} の実行が完了しました (処理アイテム数: {}, 入力: {}B, 出力: {}B)",
                  stage_name, items_processed, input_data_bytes, output_data_bytes);
        });

        Ok(())
    }
    
    /// 直接データを指定してステージを実行します（順次実行モードで使用）
    pub async fn execute_with_data(
        &self,
        context: &mut StageContext,
        input: PipelineData,
    ) -> Result<PipelineData, PipelineError> {
        let stage = self.stage.clone();
        let metrics = self.metrics.clone();
        
        // メトリクスを更新
        {
            let mut m = metrics.write().await;
            if m.start_time.is_none() {
                m.start_time = Some(Instant::now());
            }
            m.input_data_bytes += input.size() as u64;
        }
        
        // ステージを実行
        let result = match self.timeout {
            Some(duration) => {
                tokio::time::timeout(
                    duration,
                    stage.execute(context, input)
                ).await
                .unwrap_or_else(|_| {
                    Err(PipelineError::Timeout)
                })
            },
            None => stage.execute(context, input).await,
        };
        
        // メトリクスを更新
        match &result {
            Ok(output) => {
                let mut m = metrics.write().await;
                m.output_data_bytes += output.size() as u64;
                m.items_processed += 1;
                m.success = true;
            },
            Err(_) => {
                let mut m = metrics.write().await;
                m.success = false;
            }
        };
        
        result
    }
}

impl Debug for PipelineStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineStage")
            .field("name", &self.name())
            .field("description", &self.description())
            .field("has_input_tx", &self.input_tx.is_some())
            .field("has_output_rx", &self.output_rx.is_some())
            .field("timeout", &self.timeout)
            .field("dependencies", &self.dependencies)
            .field("properties", &self.properties)
            .finish()
    }
}

/// ステージファクトリー
/// 再利用可能なステージを作成するためのファクトリーパターンの実装
pub struct StageFactory {
    /// 登録されたステージテンプレート
    templates: HashMap<String, Box<dyn Stage>>,
}

impl StageFactory {
    /// 新しいステージファクトリーを作成します
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }
    
    /// ステージテンプレートを登録します
    pub fn register(&mut self, template: Box<dyn Stage>) {
        self.templates.insert(template.name().to_string(), template);
    }
    
    /// 登録されたテンプレートからステージを作成します
    pub fn create_stage(&self, name: &str) -> Option<PipelineStage> {
        self.templates.get(name).and_then(|template| {
            if template.is_cloneable() {
                template.clone_stage().map(|stage| PipelineStage::new(*stage))
            } else {
                None
            }
        })
    }
    
    /// 利用可能なステージテンプレートの名前一覧を取得します
    pub fn available_templates(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }
}

impl Default for StageFactory {
    fn default() -> Self {
        Self::new()
    }
} 