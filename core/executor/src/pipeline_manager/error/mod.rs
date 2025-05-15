/*!
# パイプラインエラーモジュール

パイプライン実行中に発生するエラーの種類、処理、伝播を管理する高度なモジュール。
詳細なエラーコンテキスト、リカバリメカニズム、エラー分析のためのツールを提供します。

## 主な機能

- 階層化されたエラータイプシステム
- 詳細なエラーコンテキスト追跡
- ステージ固有のエラー処理
- エラー回復メカニズム
- エラー統計と分析
- パイプライン再試行ポリシー
*/

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;
use serde::{Serialize, Deserialize};

use crate::pipeline_manager::stages::{StageId, StageKind};
use crate::pipeline_manager::PipelineId;

/// パイプラインエラー
#[derive(Error, Debug)]
pub enum PipelineError {
    /// パイプライン構築エラー
    #[error("パイプラインの構築に失敗しました: {0}")]
    BuildError(String),
    
    /// パイプライン実行エラー
    #[error("パイプラインの実行に失敗しました: {0}")]
    ExecutionError(String),
    
    /// パイプラインキャンセルエラー
    #[error("パイプラインのキャンセルに失敗しました: {0}")]
    CancellationError(String),
    
    /// ステージエラー
    #[error("ステージ '{stage_id}' ({stage_kind}) でエラーが発生しました: {message}")]
    StageError {
        /// ステージID
        stage_id: StageId,
        /// ステージ種類
        stage_kind: StageKind,
        /// エラーメッセージ
        message: String,
        /// 元のエラー
        source: Option<Arc<StageError>>,
    },
    
    /// タイムアウトエラー
    #[error("パイプラインがタイムアウトしました: {0}")]
    TimeoutError(String),
    
    /// リソース制約エラー
    #[error("リソース制約違反: {0}")]
    ResourceConstraintError(String),
    
    /// 入出力エラー
    #[error("入出力エラー: {0}")]
    IoError(#[from] std::io::Error),
    
    /// 設定エラー
    #[error("設定エラー: {0}")]
    ConfigurationError(String),
    
    /// 依存関係エラー
    #[error("依存関係エラー: {0}")]
    DependencyError(String),
    
    /// データ変換エラー
    #[error("データ変換エラー: {0}")]
    DataTransformationError(String),
    
    /// サンドボックスエラー
    #[error("サンドボックスエラー: {0}")]
    SandboxError(String),
    
    /// 権限エラー
    #[error("権限エラー: {0}")]
    PermissionError(String),
    
    /// 内部エラー
    #[error("内部エラー: {0}")]
    InternalError(String),
    
    /// その他のエラー
    #[error("その他のエラー: {0}")]
    Other(#[from] anyhow::Error),
}

impl PipelineError {
    /// エラーコードを取得
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::BuildError(_) => "PIPELINE_BUILD_ERROR",
            Self::ExecutionError(_) => "PIPELINE_EXECUTION_ERROR",
            Self::CancellationError(_) => "PIPELINE_CANCELLATION_ERROR",
            Self::StageError { .. } => "STAGE_ERROR",
            Self::TimeoutError(_) => "TIMEOUT_ERROR",
            Self::ResourceConstraintError(_) => "RESOURCE_CONSTRAINT_ERROR",
            Self::IoError(_) => "IO_ERROR",
            Self::ConfigurationError(_) => "CONFIGURATION_ERROR",
            Self::DependencyError(_) => "DEPENDENCY_ERROR",
            Self::DataTransformationError(_) => "DATA_TRANSFORMATION_ERROR",
            Self::SandboxError(_) => "SANDBOX_ERROR",
            Self::PermissionError(_) => "PERMISSION_ERROR",
            Self::InternalError(_) => "INTERNAL_ERROR",
            Self::Other(_) => "OTHER_ERROR",
        }
    }
    
    /// ステージエラーを作成
    pub fn stage_error(stage_id: StageId, stage_kind: StageKind, message: impl Into<String>, source: Option<StageError>) -> Self {
        Self::StageError {
            stage_id,
            stage_kind,
            message: message.into(),
            source: source.map(Arc::new),
        }
    }
    
    /// ビルドエラーを作成
    pub fn build_error(message: impl Into<String>) -> Self {
        Self::BuildError(message.into())
    }
    
    /// 実行エラーを作成
    pub fn execution_error(message: impl Into<String>) -> Self {
        Self::ExecutionError(message.into())
    }
    
    /// 内部エラーを作成
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::InternalError(message.into())
    }
    
    /// リカバリー可能かどうか
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::TimeoutError(_) | 
            Self::IoError(_) |
            Self::ResourceConstraintError(_) => true,
            Self::StageError { source, .. } => source.as_ref().map_or(false, |s| s.is_recoverable()),
            _ => false,
        }
    }
    
    /// ユーザーエラーかどうか（システムエラーでない）
    pub fn is_user_error(&self) -> bool {
        match self {
            Self::ConfigurationError(_) |
            Self::DataTransformationError(_) => true,
            Self::StageError { source, .. } => source.as_ref().map_or(false, |s| s.is_user_error()),
            _ => false,
        }
    }
    
    /// カテゴリを取得
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::BuildError(_) => ErrorCategory::Configuration,
            Self::ExecutionError(_) => ErrorCategory::Runtime,
            Self::CancellationError(_) => ErrorCategory::Runtime,
            Self::StageError { source, .. } => {
                source.as_ref().map_or(ErrorCategory::Runtime, |s| s.category())
            },
            Self::TimeoutError(_) => ErrorCategory::Runtime,
            Self::ResourceConstraintError(_) => ErrorCategory::Resource,
            Self::IoError(_) => ErrorCategory::IO,
            Self::ConfigurationError(_) => ErrorCategory::Configuration,
            Self::DependencyError(_) => ErrorCategory::Dependency,
            Self::DataTransformationError(_) => ErrorCategory::Data,
            Self::SandboxError(_) => ErrorCategory::Security,
            Self::PermissionError(_) => ErrorCategory::Security,
            Self::InternalError(_) => ErrorCategory::Internal,
            Self::Other(_) => ErrorCategory::Unknown,
        }
    }
    
    /// 詳細情報を取得
    pub fn details(&self) -> ErrorDetails {
        ErrorDetails {
            error_code: self.error_code().to_string(),
            message: self.to_string(),
            category: self.category(),
            recoverable: self.is_recoverable(),
            user_error: self.is_user_error(),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// ステージエラー
#[derive(Error, Debug)]
pub enum StageError {
    /// 初期化エラー
    #[error("ステージの初期化に失敗しました: {0}")]
    InitializationError(String),
    
    /// 入力エラー
    #[error("入力エラー: {0}")]
    InputError(String),
    
    /// 出力エラー
    #[error("出力エラー: {0}")]
    OutputError(String),
    
    /// 実行エラー
    #[error("実行エラー: {0}")]
    ExecutionError(String),
    
    /// 検証エラー
    #[error("検証エラー: {0}")]
    ValidationError(String),
    
    /// タイムアウトエラー
    #[error("ステージがタイムアウトしました: {0}")]
    TimeoutError(String),
    
    /// リソースエラー
    #[error("リソースエラー: {0}")]
    ResourceError(String),
    
    /// 依存関係エラー
    #[error("依存関係エラー: {0}")]
    DependencyError(String),
    
    /// データ処理エラー
    #[error("データ処理エラー: {0}")]
    DataProcessingError(String),
    
    /// キャンセルエラー
    #[error("ステージがキャンセルされました: {0}")]
    CancellationError(String),
    
    /// パーミッションエラー
    #[error("パーミッションエラー: {0}")]
    PermissionError(String),
    
    /// 内部エラー
    #[error("内部エラー: {0}")]
    InternalError(String),
    
    /// 不明なエラー
    #[error("不明なエラー: {0}")]
    UnknownError(String),
}

impl StageError {
    /// エラーコードを取得
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::InitializationError(_) => "STAGE_INIT_ERROR",
            Self::InputError(_) => "STAGE_INPUT_ERROR",
            Self::OutputError(_) => "STAGE_OUTPUT_ERROR",
            Self::ExecutionError(_) => "STAGE_EXECUTION_ERROR",
            Self::ValidationError(_) => "STAGE_VALIDATION_ERROR",
            Self::TimeoutError(_) => "STAGE_TIMEOUT_ERROR",
            Self::ResourceError(_) => "STAGE_RESOURCE_ERROR",
            Self::DependencyError(_) => "STAGE_DEPENDENCY_ERROR",
            Self::DataProcessingError(_) => "STAGE_DATA_PROCESSING_ERROR",
            Self::CancellationError(_) => "STAGE_CANCELLATION_ERROR",
            Self::PermissionError(_) => "STAGE_PERMISSION_ERROR",
            Self::InternalError(_) => "STAGE_INTERNAL_ERROR",
            Self::UnknownError(_) => "STAGE_UNKNOWN_ERROR",
        }
    }
    
    /// リカバリー可能かどうか
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::TimeoutError(_) |
            Self::ResourceError(_) |
            Self::CancellationError(_) => true,
            _ => false,
        }
    }
    
    /// ユーザーエラーかどうか（システムエラーでない）
    pub fn is_user_error(&self) -> bool {
        match self {
            Self::InputError(_) |
            Self::ValidationError(_) |
            Self::DataProcessingError(_) => true,
            _ => false,
        }
    }
    
    /// カテゴリを取得
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::InitializationError(_) => ErrorCategory::Configuration,
            Self::InputError(_) => ErrorCategory::Data,
            Self::OutputError(_) => ErrorCategory::Data,
            Self::ExecutionError(_) => ErrorCategory::Runtime,
            Self::ValidationError(_) => ErrorCategory::Validation,
            Self::TimeoutError(_) => ErrorCategory::Runtime,
            Self::ResourceError(_) => ErrorCategory::Resource,
            Self::DependencyError(_) => ErrorCategory::Dependency,
            Self::DataProcessingError(_) => ErrorCategory::Data,
            Self::CancellationError(_) => ErrorCategory::Runtime,
            Self::PermissionError(_) => ErrorCategory::Security,
            Self::InternalError(_) => ErrorCategory::Internal,
            Self::UnknownError(_) => ErrorCategory::Unknown,
        }
    }
}

/// エラーカテゴリ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// 設定エラー
    Configuration,
    /// 実行時エラー
    Runtime,
    /// リソース関連エラー
    Resource,
    /// 入出力エラー
    IO,
    /// 依存関係エラー
    Dependency,
    /// データ関連エラー
    Data,
    /// セキュリティ関連エラー
    Security,
    /// 検証エラー
    Validation,
    /// 内部エラー
    Internal,
    /// 不明なエラー
    Unknown,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => write!(f, "Configuration"),
            Self::Runtime => write!(f, "Runtime"),
            Self::Resource => write!(f, "Resource"),
            Self::IO => write!(f, "IO"),
            Self::Dependency => write!(f, "Dependency"),
            Self::Data => write!(f, "Data"),
            Self::Security => write!(f, "Security"),
            Self::Validation => write!(f, "Validation"),
            Self::Internal => write!(f, "Internal"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// エラー詳細情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    /// エラーコード
    pub error_code: String,
    /// エラーメッセージ
    pub message: String,
    /// エラーカテゴリ
    pub category: ErrorCategory,
    /// リカバリー可能かどうか
    pub recoverable: bool,
    /// ユーザーエラーかどうか
    pub user_error: bool,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// パイプラインリカバリーポリシー
#[derive(Debug, Clone)]
pub struct RecoveryPolicy {
    /// 再試行回数
    pub max_retries: usize,
    /// 再試行間隔
    pub retry_interval: Duration,
    /// 指数バックオフを使用するかどうか
    pub use_exponential_backoff: bool,
    /// リカバリー可能なエラーカテゴリ
    pub recoverable_categories: Vec<ErrorCategory>,
    /// リカバリーを続行するかどうかを判断するコールバック
    #[allow(clippy::type_complexity)]
    pub should_retry: Option<Arc<dyn Fn(&PipelineError, usize) -> bool + Send + Sync>>,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_interval: Duration::from_secs(5),
            use_exponential_backoff: true,
            recoverable_categories: vec![
                ErrorCategory::Resource,
                ErrorCategory::IO,
                ErrorCategory::Runtime,
            ],
            should_retry: None,
        }
    }
}

impl RecoveryPolicy {
    /// 指定されたエラーに対して再試行すべきかどうかを判断
    pub fn should_retry_error(&self, error: &PipelineError, attempt: usize) -> bool {
        // 最大再試行回数を超えている場合は再試行しない
        if attempt >= self.max_retries {
            return false;
        }
        
        // カスタムコールバックがある場合はそれを使用
        if let Some(ref callback) = self.should_retry {
            return callback(error, attempt);
        }
        
        // エラーがリカバリー可能でカテゴリが許可されているか確認
        error.is_recoverable() && self.recoverable_categories.contains(&error.category())
    }
    
    /// 次の再試行までの待機時間を計算
    pub fn calculate_retry_delay(&self, attempt: usize) -> Duration {
        if !self.use_exponential_backoff || attempt == 0 {
            return self.retry_interval;
        }
        
        // 指数バックオフを計算（最大64倍まで）
        let factor = (2_u32.pow(attempt as u32)).min(64) as u64;
        self.retry_interval.mul_f64(factor as f64)
    }
}

/// エラー統計
#[derive(Debug, Clone, Default)]
pub struct ErrorStatistics {
    /// エラー総数
    pub total_errors: usize,
    /// カテゴリ別エラー数
    pub errors_by_category: HashMap<ErrorCategory, usize>,
    /// ステージ別エラー数
    pub errors_by_stage: HashMap<StageId, usize>,
    /// 再試行回数
    pub retry_count: usize,
    /// リカバリー成功回数
    pub recovery_success_count: usize,
    /// リカバリー失敗回数
    pub recovery_failure_count: usize,
}

use std::collections::HashMap;

impl ErrorStatistics {
    /// 新しいエラー統計を作成
    pub fn new() -> Self {
        Self::default()
    }
    
    /// エラーを記録
    pub fn record_error(&mut self, error: &PipelineError, stage_id: Option<&StageId>) {
        self.total_errors += 1;
        
        // カテゴリ別にカウント
        let category = error.category();
        *self.errors_by_category.entry(category).or_insert(0) += 1;
        
        // ステージ別にカウント（ステージIDが提供されている場合）
        if let Some(stage_id) = stage_id {
            *self.errors_by_stage.entry(stage_id.clone()).or_insert(0) += 1;
        }
    }
    
    /// 再試行を記録
    pub fn record_retry(&mut self, successful: bool) {
        self.retry_count += 1;
        if successful {
            self.recovery_success_count += 1;
        } else {
            self.recovery_failure_count += 1;
        }
    }
    
    /// 最も頻繁に失敗するステージを取得
    pub fn most_failing_stage(&self) -> Option<(StageId, usize)> {
        self.errors_by_stage.iter()
            .max_by_key(|(_, &count)| count)
            .map(|(stage_id, count)| (stage_id.clone(), *count))
    }
    
    /// 最も一般的なエラーカテゴリを取得
    pub fn most_common_error_category(&self) -> Option<(ErrorCategory, usize)> {
        self.errors_by_category.iter()
            .max_by_key(|(_, &count)| count)
            .map(|(category, count)| (*category, *count))
    }
    
    /// リカバリー成功率を計算
    pub fn recovery_success_rate(&self) -> f64 {
        if self.retry_count == 0 {
            return 0.0;
        }
        self.recovery_success_count as f64 / self.retry_count as f64
    }
}

/// エラートレース（エラー発生コンテキスト）
#[derive(Debug, Clone)]
pub struct ErrorTrace {
    /// パイプラインID
    pub pipeline_id: PipelineId,
    /// ステージID（存在する場合）
    pub stage_id: Option<StageId>,
    /// エラー詳細
    pub details: ErrorDetails,
    /// エラータイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// エラー発生時の関連ファイル
    pub file: Option<PathBuf>,
    /// エラー発生時の行番号
    pub line: Option<u32>,
    /// スタックトレース
    pub stack_trace: Option<String>,
    /// コンテキスト情報
    pub context: HashMap<String, String>,
}

impl ErrorTrace {
    /// 新しいエラートレースを作成
    pub fn new(pipeline_id: PipelineId, error: &PipelineError) -> Self {
        let stage_id = match error {
            PipelineError::StageError { stage_id, .. } => Some(stage_id.clone()),
            _ => None,
        };
        
        Self {
            pipeline_id,
            stage_id,
            details: error.details(),
            timestamp: chrono::Utc::now(),
            file: None,
            line: None,
            stack_trace: None,
            context: HashMap::new(),
        }
    }
    
    /// ファイル情報を追加
    pub fn with_file(mut self, file: PathBuf, line: u32) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self
    }
    
    /// スタックトレースを追加
    pub fn with_stack_trace(mut self, stack_trace: String) -> Self {
        self.stack_trace = Some(stack_trace);
        self
    }
    
    /// コンテキスト情報を追加
    pub fn with_context(mut self, key: &str, value: &str) -> Self {
        self.context.insert(key.to_string(), value.to_string());
        self
    }
    
    /// 複数のコンテキスト情報を追加
    pub fn with_contexts(mut self, contexts: HashMap<String, String>) -> Self {
        self.context.extend(contexts);
        self
    }
    
    /// エラートレースをフォーマット
    pub fn format(&self) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("エラー: {} [{}]\n", self.details.message, self.details.error_code));
        output.push_str(&format!("パイプライン: {}\n", self.pipeline_id));
        
        if let Some(ref stage_id) = self.stage_id {
            output.push_str(&format!("ステージ: {}\n", stage_id));
        }
        
        output.push_str(&format!("カテゴリ: {}\n", self.details.category));
        output.push_str(&format!("タイムスタンプ: {}\n", self.timestamp));
        
        if let Some(ref file) = self.file {
            if let Some(line) = self.line {
                output.push_str(&format!("場所: {}:{}\n", file.display(), line));
            } else {
                output.push_str(&format!("ファイル: {}\n", file.display()));
            }
        }
        
        if !self.context.is_empty() {
            output.push_str("\nコンテキスト:\n");
            for (key, value) in &self.context {
                output.push_str(&format!("  {}: {}\n", key, value));
            }
        }
        
        if let Some(ref stack_trace) = self.stack_trace {
            output.push_str("\nスタックトレース:\n");
            output.push_str(stack_trace);
        }
        
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pipeline_error_categories() {
        let error = PipelineError::BuildError("テストエラー".to_string());
        assert_eq!(error.category(), ErrorCategory::Configuration);
        
        let error = PipelineError::ExecutionError("テストエラー".to_string());
        assert_eq!(error.category(), ErrorCategory::Runtime);
        
        let error = PipelineError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "ファイルが見つかりません"));
        assert_eq!(error.category(), ErrorCategory::IO);
    }
    
    #[test]
    fn test_stage_error_recoverable() {
        let error = StageError::TimeoutError("タイムアウトしました".to_string());
        assert!(error.is_recoverable());
        
        let error = StageError::InitializationError("初期化に失敗しました".to_string());
        assert!(!error.is_recoverable());
    }
    
    #[test]
    fn test_recovery_policy() {
        let policy = RecoveryPolicy::default();
        
        let recoverable_error = PipelineError::TimeoutError("タイムアウト".to_string());
        assert!(policy.should_retry_error(&recoverable_error, 0));
        assert!(policy.should_retry_error(&recoverable_error, 2));
        assert!(!policy.should_retry_error(&recoverable_error, 3));
        
        let non_recoverable_error = PipelineError::BuildError("ビルドエラー".to_string());
        assert!(!policy.should_retry_error(&non_recoverable_error, 0));
    }
    
    #[test]
    fn test_error_statistics() {
        let mut stats = ErrorStatistics::new();
        
        let error1 = PipelineError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "ファイルが見つかりません"));
        let error2 = PipelineError::TimeoutError("タイムアウト".to_string());
        
        let stage_id = StageId::new();
        
        stats.record_error(&error1, Some(&stage_id));
        stats.record_error(&error2, Some(&stage_id));
        stats.record_retry(true);
        
        assert_eq!(stats.total_errors, 2);
        assert_eq!(stats.errors_by_stage.get(&stage_id), Some(&2));
        assert_eq!(stats.retry_count, 1);
        assert_eq!(stats.recovery_success_count, 1);
        assert_eq!(stats.recovery_success_rate(), 1.0);
    }
    
    #[test]
    fn test_error_trace() {
        let pipeline_id = PipelineId::new();
        let error = PipelineError::ExecutionError("実行エラー".to_string());
        
        let trace = ErrorTrace::new(pipeline_id.clone(), &error)
            .with_file(PathBuf::from("/path/to/file.rs"), 42)
            .with_context("user", "test_user")
            .with_context("command", "test_command");
        
        assert_eq!(trace.pipeline_id, pipeline_id);
        assert_eq!(trace.stage_id, None);
        assert_eq!(trace.details.category, ErrorCategory::Runtime);
        assert_eq!(trace.file, Some(PathBuf::from("/path/to/file.rs")));
        assert_eq!(trace.line, Some(42));
        assert_eq!(trace.context.get("user"), Some(&"test_user".to_string()));
        assert_eq!(trace.context.get("command"), Some(&"test_command".to_string()));
    }
} 