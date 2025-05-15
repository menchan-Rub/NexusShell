use std::fmt;
use std::io;

/// パイプラインエラー
#[derive(Debug)]
pub enum PipelineError {
    /// パイプラインの初期化に失敗しました
    InitializationFailed(String),
    /// パイプラインの実行に失敗しました
    ExecutionFailed(String),
    /// パイプライン構築エラー
    BuildError(String),
    /// 入出力エラー
    IoError(io::Error),
    /// タイムアウトエラー
    Timeout(String),
    /// 構文エラー
    SyntaxError(String),
    /// ランタイムエラー
    RuntimeError(String),
    /// コンポーネントエラー
    ComponentError {
        /// コンポーネント種類
        component_type: String,
        /// コンポーネント名
        component_name: String,
        /// エラーメッセージ
        message: String,
    },
    /// データ処理エラー
    DataProcessingError(String),
    /// 無効な設定
    InvalidConfiguration(String),
    /// シリアライズエラー
    SerializationError(String),
    /// デシリアライズエラー
    DeserializationError(String),
    /// システムエラー
    SystemError(String),
    /// 未知のエラー
    Unknown(String),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializationFailed(msg) => write!(f, "パイプラインの初期化に失敗しました: {}", msg),
            Self::ExecutionFailed(msg) => write!(f, "パイプラインの実行に失敗しました: {}", msg),
            Self::BuildError(msg) => write!(f, "パイプライン構築エラー: {}", msg),
            Self::IoError(e) => write!(f, "入出力エラー: {}", e),
            Self::Timeout(msg) => write!(f, "タイムアウトエラー: {}", msg),
            Self::SyntaxError(msg) => write!(f, "構文エラー: {}", msg),
            Self::RuntimeError(msg) => write!(f, "ランタイムエラー: {}", msg),
            Self::ComponentError { component_type, component_name, message } => write!(
                f, "{} '{}' でエラーが発生しました: {}", 
                component_type, component_name, message
            ),
            Self::DataProcessingError(msg) => write!(f, "データ処理エラー: {}", msg),
            Self::InvalidConfiguration(msg) => write!(f, "無効な設定: {}", msg),
            Self::SerializationError(msg) => write!(f, "シリアライズエラー: {}", msg),
            Self::DeserializationError(msg) => write!(f, "デシリアライズエラー: {}", msg),
            Self::SystemError(msg) => write!(f, "システムエラー: {}", msg),
            Self::Unknown(msg) => write!(f, "未知のエラー: {}", msg),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<io::Error> for PipelineError {
    fn from(error: io::Error) -> Self {
        Self::IoError(error)
    }
}

impl From<anyhow::Error> for PipelineError {
    fn from(error: anyhow::Error) -> Self {
        Self::Unknown(error.to_string())
    }
}

impl From<serde_json::Error> for PipelineError {
    fn from(error: serde_json::Error) -> Self {
        if error.is_syntax() {
            Self::SyntaxError(error.to_string())
        } else if error.is_data() {
            Self::DataProcessingError(error.to_string())
        } else {
            Self::DeserializationError(error.to_string())
        }
    }
}

impl From<std::string::FromUtf8Error> for PipelineError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        Self::DataProcessingError(format!("UTF-8 デコードエラー: {}", error))
    }
}

impl From<tokio::sync::broadcast::error::SendError<crate::pipeline_manager::PipelineData>> 
    for PipelineError 
{
    fn from(error: tokio::sync::broadcast::error::SendError<crate::pipeline_manager::PipelineData>) -> Self {
        Self::DataProcessingError(format!("ブロードキャスト送信エラー: {}", error))
    }
}

impl From<tokio::sync::mpsc::error::SendError<crate::pipeline_manager::PipelineData>> 
    for PipelineError 
{
    fn from(error: tokio::sync::mpsc::error::SendError<crate::pipeline_manager::PipelineData>) -> Self {
        Self::DataProcessingError(format!("チャネル送信エラー: {}", error))
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for PipelineError 
where T: std::fmt::Debug
{
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::DataProcessingError(format!("チャネル送信エラー: {}", error))
    }
}

/// パイプライン結果型
pub type PipelineResult<T> = Result<T, PipelineError>; 