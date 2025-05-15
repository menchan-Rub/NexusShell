use thiserror::Error;

/// ジョブ処理に関するエラー
#[derive(Debug, Error)]
pub enum JobError {
    /// ジョブが見つからない
    #[error("ジョブが見つかりません: {0}")]
    JobNotFound(String),

    /// ジョブ実行エラー
    #[error("ジョブの実行に失敗しました: {0}")]
    ExecutionFailed(String),

    /// ジョブスケジューリングエラー
    #[error("ジョブのスケジューリングに失敗しました: {0}")]
    SchedulingFailed(String),

    /// キャンセルエラー
    #[error("ジョブのキャンセルに失敗しました: {0}")]
    CancellationFailed(String),

    /// 実行中のジョブが多すぎる
    #[error("実行中のジョブが多すぎます")]
    TooManyRunningJobs,

    /// プロセス起動エラー
    #[error("プロセスの起動に失敗しました: {0}")]
    ProcessStartFailed(String),

    /// プロセス通信エラー
    #[error("プロセスとの通信に失敗しました: {0}")]
    ProcessCommunicationFailed(String),

    /// I/Oエラー
    #[error("I/Oエラー: {0}")]
    IoError(#[from] std::io::Error),

    /// タイムアウトエラー
    #[error("ジョブの実行がタイムアウトしました")]
    Timeout,

    /// リソース制限エラー
    #[error("リソース制限に達しました: {0}")]
    ResourceLimitReached(String),

    /// パーミッションエラー
    #[error("実行権限がありません: {0}")]
    PermissionDenied(String),

    /// シェル環境エラー
    #[error("シェル環境エラー: {0}")]
    ShellEnvironmentError(String),

    /// その他のエラー
    #[error("ジョブエラー: {0}")]
    Other(String),
} 