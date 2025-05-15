use thiserror::Error;

/// リモート実行に関するエラー
#[derive(Debug, Error)]
pub enum RemoteExecutorError {
    /// 接続エラー
    #[error("リモートホストへの接続に失敗しました: {0}")]
    ConnectionFailed(String),

    /// 認証エラー
    #[error("認証に失敗しました: {0}")]
    AuthenticationFailed(String),

    /// 接続が見つからない
    #[error("指定された接続が見つかりません: {0}")]
    ConnectionNotFound(String),

    /// 接続が切断された
    #[error("接続が切断されました: {0}")]
    ConnectionClosed(String),

    /// コマンド実行エラー
    #[error("リモートコマンドの実行に失敗しました: {0}")]
    CommandExecutionFailed(String),

    /// チャネルオープンエラー
    #[error("チャネルのオープンに失敗しました: {0}")]
    ChannelOpenFailed(String),

    /// データ転送エラー
    #[error("データ転送に失敗しました: {0}")]
    DataTransferFailed(String),

    /// I/Oエラー
    #[error("I/Oエラー: {0}")]
    IoError(#[from] std::io::Error),

    /// タイムアウト
    #[error("リモート操作がタイムアウトしました")]
    Timeout,

    /// プロトコルエラー
    #[error("プロトコルエラー: {0}")]
    ProtocolError(String),

    /// 設定エラー
    #[error("設定エラー: {0}")]
    ConfigurationError(String),

    /// その他のエラー
    #[error("リモート実行エラー: {0}")]
    Other(String),
} 