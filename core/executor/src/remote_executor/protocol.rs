use super::error::RemoteExecutorError;
use super::config::RemoteConfig;
use super::{AuthMethod, CommandResult};

use log::{debug, error, info, warn};
use std::time::Duration;
use tokio::time::timeout;
use std::net::TcpStream;
use tokio_ssh2;
use ssh2;
use std::path::Path;

/// SSH接続を管理するプロトコル実装
pub struct SshProtocol {
    /// ホスト名
    host: String,
    /// ポート
    port: u16,
    /// ユーザー名
    username: String,
    /// 認証方法
    auth_method: AuthMethod,
    /// 設定
    config: RemoteConfig,
    /// 接続状態
    connected: bool,
    /// SSHセッション
    session: Option<ssh2::Session>,
}

impl SshProtocol {
    /// 新しいSSHプロトコルインスタンスを作成します
    pub fn new(
        host: &str,
        port: u16,
        username: &str,
        auth_method: AuthMethod,
        config: RemoteConfig,
    ) -> Self {
        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            auth_method,
            config,
            connected: false,
            session: None,
        }
    }

    /// SSHサーバーに接続します
    pub async fn connect(&mut self) -> Result<(), RemoteExecutorError> {
        if self.connected {
            return Ok(());
        }
        let addr = format!("{}:{}", self.host, self.port);
        let tcp = tokio::net::TcpStream::connect(&addr).await.map_err(|e| RemoteExecutorError::ConnectionFailed(e.to_string()))?;
        let tcp = tcp.into_std().map_err(|e| RemoteExecutorError::ConnectionFailed(e.to_string()))?;
        let mut session = ssh2::Session::new().map_err(|e| RemoteExecutorError::ConnectionFailed(e.to_string()))?;
        session.set_tcp_stream(tcp);
        session.handshake().map_err(|e| RemoteExecutorError::ConnectionFailed(e.to_string()))?;
        // 認証
        match &self.auth_method {
            AuthMethod::Password(pass) => {
                session.userauth_password(&self.username, pass).map_err(|e| RemoteExecutorError::AuthenticationFailed(e.to_string()))?;
            },
            AuthMethod::PublicKey(key_path) => {
                let pubkey_path = format!("{}.pub", key_path);
                session.userauth_pubkey_file(&self.username, Some(std::path::Path::new(&pubkey_path)), std::path::Path::new(key_path), None)
                    .map_err(|e| RemoteExecutorError::AuthenticationFailed(e.to_string()))?;
            },
            AuthMethod::SshAgent => {
                session.userauth_agent(&self.username).map_err(|e| RemoteExecutorError::AuthenticationFailed(e.to_string()))?;
            },
            _ => return Err(RemoteExecutorError::AuthenticationFailed("未サポートの認証方式".to_string())),
        }
        if !session.authenticated() {
            return Err(RemoteExecutorError::AuthenticationFailed("認証に失敗しました".to_string()));
        }
        self.session = Some(session);
        self.connected = true;
        Ok(())
    }

    /// SSH接続を切断します
    pub async fn disconnect(&mut self) -> Result<(), RemoteExecutorError> {
        if !self.connected {
            return Ok(());
        }
        if let Some(session) = &mut self.session {
            let _ = session.disconnect(None, "Session closed", None);
        }
        self.connected = false;
        self.session = None;
        Ok(())
    }

    /// リモートコマンドを実行します
    pub async fn execute_command(&mut self, command: &str) -> Result<CommandResult, RemoteExecutorError> {
        if !self.connected {
            return Err(RemoteExecutorError::ConnectionClosed(format!("SSH接続が確立されていません: {}@{}:{}", self.username, self.host, self.port)));
        }
        let session = self.session.as_mut().ok_or(RemoteExecutorError::ConnectionClosed("セッションが存在しません".to_string()))?;
        let mut channel = session.channel_session().map_err(|e| RemoteExecutorError::CommandExecutionFailed(e.to_string()))?;
        channel.exec(command).map_err(|e| RemoteExecutorError::CommandExecutionFailed(e.to_string()))?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        channel.read_to_end(&mut stdout).map_err(|e| RemoteExecutorError::CommandExecutionFailed(e.to_string()))?;
        channel.stderr().read_to_end(&mut stderr).map_err(|e| RemoteExecutorError::CommandExecutionFailed(e.to_string()))?;
        let exit_code = channel.exit_status().map_err(|e| RemoteExecutorError::CommandExecutionFailed(e.to_string()))?;
        channel.close().map_err(|e| RemoteExecutorError::CommandExecutionFailed(e.to_string()))?;
        Ok(CommandResult {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            execution_time_ms: 0,
        })
    }

    /// 接続状態を取得します
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

/// SFTPプロトコルインターフェイス
#[async_trait::async_trait]
pub trait SftpProtocol: Send + Sync {
    /// ファイルをアップロードします
    async fn upload(&self, local_path: &Path, remote_path: &str) -> Result<(), RemoteExecutorError>;
    
    /// ファイルをダウンロードします
    async fn download(&self, remote_path: &str, local_path: &Path) -> Result<(), RemoteExecutorError>;
    
    /// ディレクトリを作成します
    async fn mkdir(&self, path: &str) -> Result<(), RemoteExecutorError>;
    
    /// ディレクトリ内のファイル一覧を取得します
    async fn list_dir(&self, path: &str) -> Result<Vec<String>, RemoteExecutorError>;
    
    /// ファイルが存在するかどうかを確認します
    async fn exists(&self, path: &str) -> Result<bool, RemoteExecutorError>;
    
    /// ファイルを削除します
    async fn remove(&self, path: &str) -> Result<(), RemoteExecutorError>;
    
    /// ファイルの情報を取得します
    async fn stat(&self, path: &str) -> Result<FileInfo, RemoteExecutorError>;
    
    /// ファイルの権限を変更します
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), RemoteExecutorError>;
    
    /// ファイル名を変更します
    async fn rename(&self, from: &str, to: &str) -> Result<(), RemoteExecutorError>;
    
    /// シンボリックリンクを作成します
    async fn symlink(&self, path: &str, target: &str) -> Result<(), RemoteExecutorError>;
    
    /// シンボリックリンクの参照先を取得します
    async fn readlink(&self, path: &str) -> Result<String, RemoteExecutorError>;
}

/// ファイル情報
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// ファイル名
    pub name: String,
    /// ファイルサイズ
    pub size: u64,
    /// 更新日時（Unix時間、秒）
    pub modified_time: u64,
    /// アクセス日時（Unix時間、秒）
    pub access_time: u64,
    /// 作成日時（Unix時間、秒）
    pub creation_time: Option<u64>,
    /// ファイルの種類
    pub file_type: FileType,
    /// ファイルのパーミッション
    pub permissions: u32,
    /// 所有者ID
    pub uid: u32,
    /// グループID
    pub gid: u32,
}

/// ファイルの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    /// 通常ファイル
    Regular,
    /// ディレクトリ
    Directory,
    /// シンボリックリンク
    Symlink,
    /// ブロックデバイス
    BlockDevice,
    /// キャラクタデバイス
    CharDevice,
    /// FIFOパイプ
    Fifo,
    /// ソケット
    Socket,
    /// 不明
    Unknown,
} 