/*!
# パイプラインステージモジュール

パイプラインの各処理ステージを定義・管理する高性能モジュール。
様々な処理ステージの抽象化、データフロー制御、状態管理を提供します。

## 主な機能

- 多様なステージタイプのサポート
- ステージ間データ変換
- 状態管理とメトリクス収集
- 非同期ステージ処理
- 動的ステージ構成
*/

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::pipeline_manager::error::StageError;
use crate::sandbox::SandboxConfig;

/// ステージの識別子
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StageId(String);

impl StageId {
    /// 新しいステージIDを生成
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// 特定の文字列からステージIDを作成
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// ステージIDを文字列として取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StageId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Default for StageId {
    fn default() -> Self {
        Self::new()
    }
}

/// ステージの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StageKind {
    /// コマンド実行ステージ
    Command,
    /// フィルタステージ
    Filter,
    /// マップステージ
    Map,
    /// リデューサステージ
    Reduce,
    /// 集約ステージ
    Aggregate,
    /// 分割ステージ
    Split,
    /// 結合ステージ
    Join,
    /// ソートステージ
    Sort,
    /// グループ化ステージ
    Group,
    /// 変換ステージ
    Transform,
    /// 検証ステージ
    Validate,
    /// ロードステージ
    Load,
    /// ストアステージ
    Store,
    /// エクスポートステージ
    Export,
    /// インポートステージ
    Import,
    /// スクリプトステージ
    Script,
    /// カスタムステージ
    Custom(String),
}

impl fmt::Display for StageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StageKind::Command => write!(f, "Command"),
            StageKind::Filter => write!(f, "Filter"),
            StageKind::Map => write!(f, "Map"),
            StageKind::Reduce => write!(f, "Reduce"),
            StageKind::Aggregate => write!(f, "Aggregate"),
            StageKind::Split => write!(f, "Split"),
            StageKind::Join => write!(f, "Join"),
            StageKind::Sort => write!(f, "Sort"),
            StageKind::Group => write!(f, "Group"),
            StageKind::Transform => write!(f, "Transform"),
            StageKind::Validate => write!(f, "Validate"),
            StageKind::Load => write!(f, "Load"),
            StageKind::Store => write!(f, "Store"),
            StageKind::Export => write!(f, "Export"),
            StageKind::Import => write!(f, "Import"),
            StageKind::Script => write!(f, "Script"),
            StageKind::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// パイプラインデータ型
#[derive(Debug, Clone)]
pub enum DataType {
    /// バイナリデータ
    Binary(Vec<u8>),
    /// テキストデータ
    Text(String),
    /// JSONデータ
    Json(serde_json::Value),
    /// キーバリューペア
    KeyValue(HashMap<String, String>),
    /// レコードセット
    Records(Vec<HashMap<String, String>>),
    /// 複数タイプのデータセット
    Multi(HashMap<String, Box<DataType>>),
    /// 空データ
    Empty,
}

impl DataType {
    /// データサイズを取得
    pub fn size(&self) -> usize {
        match self {
            DataType::Binary(data) => data.len(),
            DataType::Text(text) => text.len(),
            DataType::Json(json) => json.to_string().len(),
            DataType::KeyValue(kv) => {
                kv.iter().map(|(k, v)| k.len() + v.len()).sum()
            },
            DataType::Records(records) => {
                records.iter().map(|record| {
                    record.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
                }).sum()
            },
            DataType::Multi(map) => {
                map.iter().map(|(k, v)| k.len() + v.size()).sum()
            },
            DataType::Empty => 0,
        }
    }
    
    /// テキスト表現に変換
    pub fn to_string(&self) -> Result<String> {
        match self {
            DataType::Binary(data) => {
                // バイナリデータを16進数表記に変換
                let hex_string = data.iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join("");
                Ok(hex_string)
            },
            DataType::Text(text) => Ok(text.clone()),
            DataType::Json(json) => Ok(json.to_string()),
            DataType::KeyValue(kv) => {
                let json = serde_json::to_string(kv)?;
                Ok(json)
            },
            DataType::Records(records) => {
                let json = serde_json::to_string(records)?;
                Ok(json)
            },
            DataType::Multi(_) => Err(anyhow!("Multiデータ型は直接文字列に変換できません")),
            DataType::Empty => Ok(String::new()),
        }
    }
    
    /// バイナリ表現に変換
    pub fn to_binary(&self) -> Result<Vec<u8>> {
        match self {
            DataType::Binary(data) => Ok(data.clone()),
            DataType::Text(text) => Ok(text.as_bytes().to_vec()),
            DataType::Json(json) => {
                let json_string = json.to_string();
                Ok(json_string.as_bytes().to_vec())
            },
            DataType::KeyValue(kv) => {
                let json = serde_json::to_vec(kv)?;
                Ok(json)
            },
            DataType::Records(records) => {
                let json = serde_json::to_vec(records)?;
                Ok(json)
            },
            DataType::Multi(_) => Err(anyhow!("Multiデータ型は直接バイナリに変換できません")),
            DataType::Empty => Ok(Vec::new()),
        }
    }
    
    /// 別のデータ型に変換
    pub fn convert_to(&self, target_type: DataTypeKind) -> Result<DataType> {
        match (self, target_type) {
            // 同じ型への変換
            (DataType::Binary(_), DataTypeKind::Binary) |
            (DataType::Text(_), DataTypeKind::Text) |
            (DataType::Json(_), DataTypeKind::Json) |
            (DataType::KeyValue(_), DataTypeKind::KeyValue) |
            (DataType::Records(_), DataTypeKind::Records) |
            (DataType::Multi(_), DataTypeKind::Multi) |
            (DataType::Empty, DataTypeKind::Empty) => Ok(self.clone()),
            
            // バイナリからの変換
            (DataType::Binary(data), DataTypeKind::Text) => {
                String::from_utf8(data.clone())
                    .map(DataType::Text)
                    .map_err(|e| anyhow!("バイナリデータをテキストに変換できません: {}", e))
            },
            (DataType::Binary(data), DataTypeKind::Json) => {
                serde_json::from_slice(data)
                    .map(DataType::Json)
                    .map_err(|e| anyhow!("バイナリデータをJSONに変換できません: {}", e))
            },
            
            // テキストからの変換
            (DataType::Text(text), DataTypeKind::Binary) => {
                Ok(DataType::Binary(text.as_bytes().to_vec()))
            },
            (DataType::Text(text), DataTypeKind::Json) => {
                serde_json::from_str(text)
                    .map(DataType::Json)
                    .map_err(|e| anyhow!("テキストをJSONに変換できません: {}", e))
            },
            
            // JSONからの変換
            (DataType::Json(json), DataTypeKind::Binary) => {
                Ok(DataType::Binary(json.to_string().into_bytes()))
            },
            (DataType::Json(json), DataTypeKind::Text) => {
                Ok(DataType::Text(json.to_string()))
            },
            (DataType::Json(json), DataTypeKind::KeyValue) => {
                match json {
                    serde_json::Value::Object(obj) => {
                        let mut kv = HashMap::new();
                        for (k, v) in obj {
                            if let Some(v_str) = v.as_str() {
                                kv.insert(k.clone(), v_str.to_string());
                            } else {
                                kv.insert(k.clone(), v.to_string());
                            }
                        }
                        Ok(DataType::KeyValue(kv))
                    },
                    _ => Err(anyhow!("JSONオブジェクトのみキーバリューに変換できます"))
                }
            },
            (DataType::Json(json), DataTypeKind::Records) => {
                match json {
                    serde_json::Value::Array(arr) => {
                        let mut records = Vec::new();
                        for item in arr {
                            if let serde_json::Value::Object(obj) = item {
                                let mut record = HashMap::new();
                                for (k, v) in obj {
                                    if let Some(v_str) = v.as_str() {
                                        record.insert(k.clone(), v_str.to_string());
                                    } else {
                                        record.insert(k.clone(), v.to_string());
                                    }
                                }
                                records.push(record);
                            } else {
                                return Err(anyhow!("JSONレコードの配列のみレコードセットに変換できます"));
                            }
                        }
                        Ok(DataType::Records(records))
                    },
                    _ => Err(anyhow!("JSON配列のみレコードセットに変換できます"))
                }
            },
            
            // キーバリューからの変換
            (DataType::KeyValue(kv), DataTypeKind::Json) => {
                Ok(DataType::Json(serde_json::to_value(kv)?))
            },
            (DataType::KeyValue(kv), DataTypeKind::Text) => {
                Ok(DataType::Text(serde_json::to_string(kv)?))
            },
            (DataType::KeyValue(kv), DataTypeKind::Binary) => {
                Ok(DataType::Binary(serde_json::to_vec(kv)?))
            },
            
            // レコードセットからの変換
            (DataType::Records(records), DataTypeKind::Json) => {
                Ok(DataType::Json(serde_json::to_value(records)?))
            },
            (DataType::Records(records), DataTypeKind::Text) => {
                Ok(DataType::Text(serde_json::to_string(records)?))
            },
            (DataType::Records(records), DataTypeKind::Binary) => {
                Ok(DataType::Binary(serde_json::to_vec(records)?))
            },
            
            // その他の変換は未サポート
            _ => Err(anyhow!("{:?}から{:?}への変換はサポートされていません", self, target_type)),
        }
    }
}

/// データ型の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataTypeKind {
    Binary,
    Text,
    Json,
    KeyValue,
    Records,
    Multi,
    Empty,
}

/// ステージ状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StageState {
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
    /// スキップ
    Skipped,
}

impl fmt::Display for StageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StageState::Initial => write!(f, "Initial"),
            StageState::Preparing => write!(f, "Preparing"),
            StageState::Running => write!(f, "Running"),
            StageState::Paused => write!(f, "Paused"),
            StageState::Completed => write!(f, "Completed"),
            StageState::Failed => write!(f, "Failed"),
            StageState::Cancelled => write!(f, "Cancelled"),
            StageState::Skipped => write!(f, "Skipped"),
        }
    }
}

/// ステージ設定
#[derive(Debug, Clone)]
pub struct StageConfig {
    /// ステージID
    pub id: StageId,
    /// ステージ名
    pub name: String,
    /// ステージの種類
    pub kind: StageKind,
    /// タイムアウト
    pub timeout: Option<Duration>,
    /// リトライ設定
    pub retry: Option<RetryConfig>,
    /// メモリ制限（バイト）
    pub memory_limit: Option<usize>,
    /// CPUコア制限
    pub cpu_limit: Option<f64>,
    /// 設定プロパティ
    pub properties: HashMap<String, String>,
    /// サンドボックス設定
    pub sandbox_config: Option<SandboxConfig>,
    /// データ変換設定
    pub data_transformation: Option<DataTransformation>,
}

/// リトライ設定
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大リトライ回数
    pub max_attempts: u32,
    /// リトライ間隔
    pub retry_interval: Duration,
    /// 指数バックオフを使用するかどうか
    pub exponential_backoff: bool,
}

/// データ変換設定
#[derive(Debug, Clone)]
pub struct DataTransformation {
    /// 入力データ型
    pub input_type: DataTypeKind,
    /// 出力データ型
    pub output_type: DataTypeKind,
    /// 変換スキーマ（オプション）
    pub schema: Option<String>,
}

/// ステージメトリクス
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageMetrics {
    /// 処理開始時間
    pub start_time: Option<Instant>,
    /// 処理終了時間
    pub end_time: Option<Instant>,
    /// 処理されたレコード数
    pub records_processed: u64,
    /// 処理されたバイト数
    pub bytes_processed: u64,
    /// エラー数
    pub error_count: u32,
    /// リトライ回数
    pub retry_count: u32,
    /// CPU使用時間（ミリ秒）
    pub cpu_time_ms: u64,
    /// メモリ使用量（バイト）
    pub memory_usage_bytes: u64,
    /// カスタムメトリクス
    pub custom: HashMap<String, f64>,
}

impl StageMetrics {
    /// 新しいメトリクスを作成
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 処理開始を記録
    pub fn record_start(&mut self) {
        self.start_time = Some(Instant::now());
    }
    
    /// 処理終了を記録
    pub fn record_end(&mut self) {
        self.end_time = Some(Instant::now());
    }
    
    /// 処理時間を計算（ミリ秒）
    pub fn processing_time_ms(&self) -> Option<u64> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => {
                Some(end.duration_since(start).as_millis() as u64)
            },
            _ => None,
        }
    }
    
    /// レコード処理速度を計算（レコード/秒）
    pub fn records_per_second(&self) -> Option<f64> {
        if self.records_processed == 0 {
            return Some(0.0);
        }
        
        self.processing_time_ms().map(|time_ms| {
            if time_ms == 0 {
                return 0.0;
            }
            (self.records_processed as f64) / (time_ms as f64 / 1000.0)
        })
    }
    
    /// データスループットを計算（バイト/秒）
    pub fn throughput_bytes_per_second(&self) -> Option<f64> {
        if self.bytes_processed == 0 {
            return Some(0.0);
        }
        
        self.processing_time_ms().map(|time_ms| {
            if time_ms == 0 {
                return 0.0;
            }
            (self.bytes_processed as f64) / (time_ms as f64 / 1000.0)
        })
    }
    
    /// カスタムメトリクスを追加
    pub fn add_custom_metric(&mut self, key: &str, value: f64) {
        self.custom.insert(key.to_string(), value);
    }
    
    /// 別のメトリクスをマージ
    pub fn merge(&mut self, other: &StageMetrics) {
        // 開始時間は最小値を使用
        if let Some(other_start) = other.start_time {
            match self.start_time {
                Some(self_start) if other_start < self_start => {
                    self.start_time = Some(other_start);
                },
                None => {
                    self.start_time = Some(other_start);
                },
                _ => {}
            }
        }
        
        // 終了時間は最大値を使用
        if let Some(other_end) = other.end_time {
            match self.end_time {
                Some(self_end) if other_end > self_end => {
                    self.end_time = Some(other_end);
                },
                None => {
                    self.end_time = Some(other_end);
                },
                _ => {}
            }
        }
        
        // 他のメトリクスは加算
        self.records_processed += other.records_processed;
        self.bytes_processed += other.bytes_processed;
        self.error_count += other.error_count;
        self.retry_count += other.retry_count;
        self.cpu_time_ms += other.cpu_time_ms;
        self.memory_usage_bytes = self.memory_usage_bytes.max(other.memory_usage_bytes);
        
        // カスタムメトリクスもマージ
        for (key, value) in &other.custom {
            self.custom.insert(key.clone(), *value);
        }
    }
}

/// ステージ定義
#[derive(Debug, Clone)]
pub struct StageDefinition {
    /// ステージID
    pub id: StageId,
    /// ステージ名
    pub name: String,
    /// ステージの種類
    pub kind: StageKind,
    /// 入力データ型
    pub input_type: DataTypeKind,
    /// 出力データ型
    pub output_type: DataTypeKind,
    /// 設定プロパティ
    pub properties: HashMap<String, String>,
    /// 依存ステージ
    pub dependencies: Vec<StageId>,
}

impl StageDefinition {
    /// 新しいステージ定義を作成
    pub fn new(id: StageId, name: String, kind: StageKind) -> Self {
        Self {
            id,
            name,
            kind,
            input_type: DataTypeKind::Empty,
            output_type: DataTypeKind::Empty,
            properties: HashMap::new(),
            dependencies: Vec::new(),
        }
    }
    
    /// 入力データ型を設定
    pub fn with_input_type(mut self, input_type: DataTypeKind) -> Self {
        self.input_type = input_type;
        self
    }
    
    /// 出力データ型を設定
    pub fn with_output_type(mut self, output_type: DataTypeKind) -> Self {
        self.output_type = output_type;
        self
    }
    
    /// プロパティを設定
    pub fn with_property(mut self, key: &str, value: &str) -> Self {
        self.properties.insert(key.to_string(), value.to_string());
        self
    }
    
    /// 依存ステージを追加
    pub fn with_dependency(mut self, dependency: StageId) -> Self {
        self.dependencies.push(dependency);
        self
    }
}

/// ステージファクトリ
#[async_trait]
pub trait StageFactory: Send + Sync {
    /// ステージの種類を取得
    fn kind(&self) -> StageKind;
    
    /// ステージを作成
    async fn create_stage(&self, config: StageConfig) -> Result<StageRef>;
}

/// ステージ参照
pub type StageRef = Arc<dyn Stage>;

/// ステージインターフェース
#[async_trait]
pub trait Stage: Send + Sync {
    /// ステージIDを取得
    fn id(&self) -> &StageId;
    
    /// ステージ名を取得
    fn name(&self) -> &str;
    
    /// ステージの種類を取得
    fn kind(&self) -> StageKind;
    
    /// ステージの状態を取得
    async fn state(&self) -> StageState;
    
    /// ステージのメトリクスを取得
    async fn metrics(&self) -> StageMetrics;
    
    /// ステージを初期化
    async fn initialize(&self) -> Result<(), StageError>;
    
    /// ステージを実行
    async fn execute(&self, input: Option<DataType>) -> Result<DataType, StageError>;
    
    /// ステージをクリーンアップ
    async fn cleanup(&self) -> Result<(), StageError>;
    
    /// ステージをキャンセル
    async fn cancel(&self) -> Result<(), StageError>;
    
    /// ステージを一時停止
    async fn pause(&self) -> Result<(), StageError>;
    
    /// ステージを再開
    async fn resume(&self) -> Result<(), StageError>;
}

// 基本的なステージ実装のためのユーティリティ

/// 基本的なステージ実装
pub struct BaseStage {
    /// ステージID
    id: StageId,
    /// ステージ名
    name: String,
    /// ステージの種類
    kind: StageKind,
    /// ステージの状態
    state: Arc<RwLock<StageState>>,
    /// ステージのメトリクス
    metrics: Arc<RwLock<StageMetrics>>,
    /// ステージの設定
    config: StageConfig,
    /// キャンセルチャネル
    cancel_tx: mpsc::Sender<()>,
    cancel_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl BaseStage {
    /// 新しい基本ステージを作成
    pub fn new(config: StageConfig) -> Self {
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        
        Self {
            id: config.id.clone(),
            name: config.name.clone(),
            kind: config.kind.clone(),
            state: Arc::new(RwLock::new(StageState::Initial)),
            metrics: Arc::new(RwLock::new(StageMetrics::new())),
            config,
            cancel_tx,
            cancel_rx: Arc::new(Mutex::new(cancel_rx)),
        }
    }
    
    /// 状態を設定
    pub async fn set_state(&self, state: StageState) {
        let mut current = self.state.write().await;
        *current = state;
    }
    
    /// メトリクスを更新
    pub async fn update_metrics<F>(&self, updater: F) 
    where 
        F: FnOnce(&mut StageMetrics)
    {
        let mut metrics = self.metrics.write().await;
        updater(&mut metrics);
    }
    
    /// キャンセルされたかどうかを確認
    pub async fn is_cancelled(&self) -> bool {
        let mut rx = self.cancel_rx.lock().await;
        match rx.try_recv() {
            Ok(_) | Err(mpsc::error::TryRecvError::Closed) => true,
            Err(mpsc::error::TryRecvError::Empty) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_stage_id() {
        let id1 = StageId::new();
        let id2 = StageId::new();
        assert_ne!(id1, id2);
        
        let id_str = "test-stage";
        let id3 = StageId::from_string(id_str.to_string());
        assert_eq!(id3.as_str(), id_str);
    }
    
    #[tokio::test]
    async fn test_data_type_conversion() {
        // テキスト→バイナリ変換
        let text = DataType::Text("Hello, world!".to_string());
        let binary = text.convert_to(DataTypeKind::Binary).unwrap();
        if let DataType::Binary(data) = binary {
            assert_eq!(data, b"Hello, world!");
        } else {
            panic!("変換に失敗しました");
        }
        
        // JSON変換
        let json_str = r#"{"name":"test","value":123}"#;
        let text = DataType::Text(json_str.to_string());
        let json = text.convert_to(DataTypeKind::Json).unwrap();
        if let DataType::Json(value) = json {
            assert_eq!(value["name"], "test");
            assert_eq!(value["value"], 123);
        } else {
            panic!("変換に失敗しました");
        }
    }
    
    #[tokio::test]
    async fn test_stage_metrics() {
        let mut metrics = StageMetrics::new();
        metrics.record_start();
        
        // 処理をシミュレート
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        metrics.records_processed = 100;
        metrics.bytes_processed = 1024;
        metrics.record_end();
        
        // 処理時間を検証
        let time_ms = metrics.processing_time_ms().unwrap();
        assert!(time_ms >= 10);
        
        // スループットを検証
        let rps = metrics.records_per_second().unwrap();
        assert!(rps > 0.0);
        let bps = metrics.throughput_bytes_per_second().unwrap();
        assert!(bps > 0.0);
    }
} 