use std::fmt;

/// 非同期ランタイムのエラー
#[derive(Debug)]
pub enum AsyncRuntimeError {
    /// ランタイムが初期化されていません
    RuntimeNotInitialized,
    /// ドメインが見つかりません
    DomainNotFound,
    /// セマフォ取得に失敗しました
    SemaphoreAcquisitionFailed,
    /// タスク実行に失敗しました
    TaskExecutionFailed(String),
    /// タイムアウトが発生しました
    TaskTimedOut(String),
    /// タスクがキャンセルされました
    TaskCancelled(String),
    /// スレッドプールエラー
    ThreadPoolError(String),
    /// メトリクス収集エラー
    MetricsError(String),
    /// システムエラー
    SystemError(String),
}

impl fmt::Display for AsyncRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeNotInitialized => write!(f, "ランタイムが初期化されていません"),
            Self::DomainNotFound => write!(f, "指定された実行ドメインが見つかりません"),
            Self::SemaphoreAcquisitionFailed => write!(f, "セマフォ取得に失敗しました"),
            Self::TaskExecutionFailed(msg) => write!(f, "タスク実行に失敗しました: {}", msg),
            Self::TaskTimedOut(task_name) => write!(f, "タスク '{}' がタイムアウトしました", task_name),
            Self::TaskCancelled(task_name) => write!(f, "タスク '{}' がキャンセルされました", task_name),
            Self::ThreadPoolError(msg) => write!(f, "スレッドプールエラー: {}", msg),
            Self::MetricsError(msg) => write!(f, "メトリクス収集エラー: {}", msg),
            Self::SystemError(msg) => write!(f, "システムエラー: {}", msg),
        }
    }
}

impl std::error::Error for AsyncRuntimeError {}

impl From<tokio::sync::AcquireError> for AsyncRuntimeError {
    fn from(_: tokio::sync::AcquireError) -> Self {
        Self::SemaphoreAcquisitionFailed
    }
}

impl From<std::io::Error> for AsyncRuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::SystemError(error.to_string())
    }
}

impl From<tokio::task::JoinError> for AsyncRuntimeError {
    fn from(error: tokio::task::JoinError) -> Self {
        if error.is_cancelled() {
            Self::TaskCancelled("不明なタスク".to_string())
        } else {
            Self::TaskExecutionFailed(error.to_string())
        }
    }
} 