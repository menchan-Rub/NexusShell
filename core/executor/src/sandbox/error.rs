use thiserror::Error;

/// サンドボックスに関するエラー
#[derive(Debug, Error)]
pub enum SandboxError {
    /// コンテナが見つからない
    #[error("指定されたコンテナが見つかりません: {0}")]
    ContainerNotFound(String),

    /// コンテナが既に存在する
    #[error("指定された名前のコンテナは既に存在します: {0}")]
    ContainerAlreadyExists(String),

    /// コンテナの初期化に失敗
    #[error("コンテナの初期化に失敗しました: {0}")]
    ContainerInitializationFailed(String),

    /// コンテナの破棄に失敗
    #[error("コンテナの破棄に失敗しました: {0}")]
    ContainerDestructionFailed(String),

    /// コマンド実行エラー
    #[error("コマンドの実行に失敗しました: {0}")]
    CommandExecutionFailed(String),

    /// リソース制限超過
    #[error("リソース制限を超過しました: {0}")]
    ResourceLimitExceeded(String),

    /// ポリシー違反
    #[error("セキュリティポリシー違反: {0}")]
    PolicyViolation(String),

    /// ファイルシステムエラー
    #[error("ファイルシステムエラー: {0}")]
    FileSystemError(String),

    /// ネットワークエラー
    #[error("ネットワークエラー: {0}")]
    NetworkError(String),

    /// 外部ツールエラー
    #[error("外部ツールエラー: {0}")]
    ExternalToolError(String),

    /// 権限エラー
    #[error("権限が不足しています: {0}")]
    PermissionDenied(String),

    /// タイムアウト
    #[error("操作がタイムアウトしました: {0}")]
    Timeout(String),

    /// I/Oエラー
    #[error("I/Oエラー: {0}")]
    IoError(#[from] std::io::Error),

    /// その他のエラー
    #[error("サンドボックスエラー: {0}")]
    Other(String),
} 