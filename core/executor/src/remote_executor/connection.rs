use super::error::RemoteExecutorError;
use super::config::RemoteConfig;
use super::{AuthMethod, CommandResult};
use super::protocol::{RemoteProtocol, SshProtocol, SftpProtocol};

use std::io::{Read, Write};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tokio::sync::{RwLock, Mutex, mpsc};
use tokio::time::{timeout, Duration};
use log::{debug, error, info, warn, trace};
use metrics::{counter, gauge, histogram};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Instant;
use rand::{thread_rng, Rng};
use std::collections::HashMap;

use ssh2::{Channel, Session, Sftp, DisconnectCode, KnownHostFileKind, KeyboardInteractivePrompt};
use ssh2::FingerprintHash;

/// セキュリティ設定
#[derive(Debug, Clone)]
pub struct SecuritySettings {
    /// ホスト鍵の検証を行うかどうか
    verify_host_key: bool,
    /// 既知のホストファイル
    known_hosts_file: Option<PathBuf>,
    /// 許可するキー交換アルゴリズム
    allowed_kex: Vec<String>,
    /// 許可する暗号化アルゴリズム
    allowed_ciphers: Vec<String>,
    /// 許可するMAC（メッセージ認証コード）アルゴリズム
    allowed_macs: Vec<String>,
    /// ホスト鍵検証コールバック
    host_key_callback: Option<Arc<dyn Fn(&[u8], &str) -> bool + Send + Sync>>,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            verify_host_key: true,
            known_hosts_file: None,
            allowed_kex: vec![
                "curve25519-sha256".to_string(),
                "diffie-hellman-group-exchange-sha256".to_string(),
            ],
            allowed_ciphers: vec![
                "chacha20-poly1305@openssh.com".to_string(),
                "aes256-gcm@openssh.com".to_string(),
                "aes128-gcm@openssh.com".to_string(),
            ],
            allowed_macs: vec![
                "hmac-sha2-512-etm@openssh.com".to_string(),
                "hmac-sha2-256-etm@openssh.com".to_string(),
            ],
            host_key_callback: None,
        }
    }
}

/// 接続メトリクス
#[derive(Debug, Clone, Default)]
pub struct ConnectionMetrics {
    /// 接続が確立された時刻
    pub connection_start_time: Option<Instant>,
    /// 最後に使用された時刻
    pub last_used_time: Option<Instant>,
    /// 接続の再試行回数
    pub reconnect_attempts: u32,
    /// 実行されたコマンドの総数
    pub commands_executed: u64,
    /// 転送されたバイト数（アップロード）
    pub bytes_uploaded: u64,
    /// 転送されたバイト数（ダウンロード）
    pub bytes_downloaded: u64,
    /// RTT（往復時間）ミリ秒
    pub rtt_ms: f64,
    /// RTTサンプル数
    pub rtt_samples: u32,
    /// 最後のエラー
    pub last_error: Option<String>,
}

/// リモートマシンへの接続
pub struct RemoteConnection {
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
    /// SSH接続セッション
    session: Arc<RwLock<Option<Session>>>,
    /// 接続状態
    connected: Arc<RwLock<bool>>,
    /// セキュリティ設定
    security: Arc<RwLock<SecuritySettings>>,
    /// メトリクス
    metrics: Arc<RwLock<ConnectionMetrics>>,
    /// キープアライブタスクの送信チャネル
    keepalive_tx: Option<mpsc::Sender<()>>,
    /// 再接続バックオフ
    reconnect_backoff: Arc<RwLock<u64>>,
    /// SFTPセッション
    sftp_session: Arc<Mutex<Option<Sftp>>>,
}

impl RemoteConnection {
    /// 新しいリモート接続を作成します
    pub fn new(
        host: &str,
        username: &str,
        auth_method: AuthMethod,
        config: RemoteConfig,
    ) -> Self {
        let (hostname, port) = if let Some(idx) = host.find(':') {
            let (hostname, port_str) = host.split_at(idx);
            let port = port_str[1..].parse::<u16>().unwrap_or(config.default_port());
            (hostname.to_string(), port)
        } else {
            (host.to_string(), config.default_port())
        };

        let connection = Self {
            host: hostname,
            port,
            username: username.to_string(),
            auth_method,
            config,
            session: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
            security: Arc::new(RwLock::new(SecuritySettings::default())),
            metrics: Arc::new(RwLock::new(ConnectionMetrics::default())),
            keepalive_tx: None,
            reconnect_backoff: Arc::new(RwLock::new(500)),
            sftp_session: Arc::new(Mutex::new(None)),
        };
        
        connection
    }

    /// ホスト名を取得します
    pub fn host(&self) -> &str {
        &self.host
    }

    /// ポートを取得します
    pub fn port(&self) -> u16 {
        self.port
    }

    /// ユーザー名を取得します
    pub fn username(&self) -> &str {
        &self.username
    }
    
    /// セキュリティ設定を設定します
    pub async fn set_security_settings(&self, settings: SecuritySettings) {
        let mut security_settings = self.security.write().await;
        *security_settings = settings;
    }

    /// リモートサーバーに接続します
    pub async fn connect(&self) -> Result<(), RemoteExecutorError> {
        let mut connected = self.connected.write().await;
        
        if *connected {
            debug!("既に接続されています: {}@{}:{}", self.username, self.host, self.port);
            return Ok(());
        }

        debug!("リモートサーバーに接続しています: {}@{}:{}", self.username, self.host, self.port);
        
        // メトリクスを初期化
        {
            let mut metrics = self.metrics.write().await;
            metrics.connection_start_time = Some(Instant::now());
            metrics.last_used_time = Some(Instant::now());
        }

        // 接続タイムアウトの設定
        let connect_timeout_duration = self.config.connect_timeout();
        let connect_result = timeout(connect_timeout_duration, self.connect_internal()).await;

        match connect_result {
            Ok(result) => {
                match result {
                    Ok(_) => {
                        *connected = true;
                        
                        // 接続が成功したらキープアライブを開始
                        if self.config.enable_keepalive() {
                            self.start_keepalive().await;
                        }
                        
                        // バックオフをリセット
                        {
                            let mut backoff = self.reconnect_backoff.write().await;
                            *backoff = 500; // ミリ秒
                        }
                        
                        info!("SSH接続が確立されました: {}@{}:{}", self.username, self.host, self.port);
                        
                        // プロメテウスメトリクスを更新
                        counter!("nexusshell_remote_connections_established", "host" => self.host.clone(), "user" => self.username.clone()).increment(1);
                        
                        Ok(())
                    }
                    Err(e) => {
                        // メトリクスを更新
                        {
                            let mut metrics = self.metrics.write().await;
                            metrics.last_error = Some(e.to_string());
                        }
                        
                        error!("SSH接続に失敗しました: {}@{}:{} - {}", self.username, self.host, self.port, e);
                        Err(e)
                    }
                }
            }
            Err(_) => {
                // メトリクスを更新
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.last_error = Some("接続タイムアウト".to_string());
                }
                
                error!("SSH接続がタイムアウトしました: {}@{}:{}", self.username, self.host, self.port);
                Err(RemoteExecutorError::Timeout)
            }
        }
    }

    /// 内部接続処理
    async fn connect_internal(&self) -> Result<(), RemoteExecutorError> {
        // tokioブロッキングコンテキストで実行
        let host = self.host.clone();
        let port = self.port;
        let username = self.username.clone();
        let auth_method = self.auth_method.clone();
        let config = self.config.clone();
        let session_arc = self.session.clone();
        let security = self.security.read().await.clone();
        let start_time = Instant::now();

        // TCPストリームの接続とSSHセッションのハンドシェイクはブロッキング操作であるため
        // トークンのブロッキングタスクで実行
        let result = tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            // TCPストリームの接続
            let socket_addr = format!("{}:{}", host, port)
                .to_socket_addrs()
                .map_err(|e| RemoteExecutorError::ConnectionFailed(format!("アドレス解決に失敗: {}", e)))?
                .next()
                .ok_or_else(|| RemoteExecutorError::ConnectionFailed("有効なアドレスが見つかりません".to_string()))?;

            let tcp = TcpStream::connect_timeout(
                &socket_addr,
                std::time::Duration::from_secs(config.connect_timeout().as_secs()),
            )
            .map_err(|e| RemoteExecutorError::ConnectionFailed(format!("TCPストリームの接続に失敗: {}", e)))?;

            // TCPキープアライブを設定
            if let Some(keepalive) = config.tcp_keepalive() {
                tcp.set_keepalive(Some(std::time::Duration::from_secs(keepalive.as_secs())))
                    .map_err(|e| RemoteExecutorError::ConnectionFailed(format!("TCPキープアライブの設定に失敗: {}", e)))?;
            }
            
            // TCP_NODELAYを設定（Nagleアルゴリズムを無効化）
            tcp.set_nodelay(true)
                .map_err(|e| RemoteExecutorError::ConnectionFailed(format!("TCP_NODELAYの設定に失敗: {}", e)))?;

            // SSH2セッションの作成
            let mut session = Session::new()
                .map_err(|e| RemoteExecutorError::ConnectionFailed(format!("SSHセッションの作成に失敗: {}", e)))?;
            
            // セキュリティオプションを設定
            if !security.allowed_kex.is_empty() {
                session.method_pref(ssh2::MethodType::KEX, &security.allowed_kex.join(","))
                    .map_err(|e| RemoteExecutorError::ConfigurationError(
                        format!("キー交換アルゴリズムの設定に失敗: {}", e)
                    ))?;
            }
            
            if !security.allowed_ciphers.is_empty() {
                session.method_pref(ssh2::MethodType::CRYPT_CS, &security.allowed_ciphers.join(","))
                    .map_err(|e| RemoteExecutorError::ConfigurationError(
                        format!("暗号化アルゴリズム（クライアント->サーバー）の設定に失敗: {}", e)
                    ))?;
                    
                session.method_pref(ssh2::MethodType::CRYPT_SC, &security.allowed_ciphers.join(","))
                    .map_err(|e| RemoteExecutorError::ConfigurationError(
                        format!("暗号化アルゴリズム（サーバー->クライアント）の設定に失敗: {}", e)
                    ))?;
            }
            
            if !security.allowed_macs.is_empty() {
                session.method_pref(ssh2::MethodType::MAC_CS, &security.allowed_macs.join(","))
                    .map_err(|e| RemoteExecutorError::ConfigurationError(
                        format!("MACアルゴリズム（クライアント->サーバー）の設定に失敗: {}", e)
                    ))?;
                    
                session.method_pref(ssh2::MethodType::MAC_SC, &security.allowed_macs.join(","))
                    .map_err(|e| RemoteExecutorError::ConfigurationError(
                        format!("MACアルゴリズム（サーバー->クライアント）の設定に失敗: {}", e)
                    ))?;
            }
            
            session.set_tcp_stream(tcp);
            session.handshake().map_err(|e| RemoteExecutorError::ConnectionFailed(format!("SSHハンドシェイクに失敗: {}", e)))?;
            
            // ホスト鍵の検証
            if security.verify_host_key {
                let server_key = session.host_key()
                    .map_err(|e| RemoteExecutorError::SecurityError(format!("ホスト鍵の取得に失敗: {}", e)))?;
                    
                let hash = session.host_key_hash(FingerprintHash::SHA256)
                    .ok_or_else(|| RemoteExecutorError::SecurityError("ホスト鍵のハッシュ生成に失敗".to_string()))?;
                
                // コールバックがあれば使用
                if let Some(callback) = &security.host_key_callback {
                    if !callback(hash, &host) {
                        return Err(RemoteExecutorError::SecurityError(
                            format!("ホスト鍵の検証に失敗しました: {}@{}", username, host)
                        ));
                    }
                } else if let Some(known_hosts_file) = &security.known_hosts_file {
                    // 既知のホストファイルがあれば検証
                    let mut known_hosts = session.known_hosts()
                        .map_err(|e| RemoteExecutorError::SecurityError(
                            format!("既知のホストの初期化に失敗: {}", e)
                        ))?;
                        
                    known_hosts.read_file(known_hosts_file, KnownHostFileKind::OpenSSH)
                        .map_err(|e| RemoteExecutorError::SecurityError(
                            format!("既知のホストファイルの読み込みに失敗: {}", e)
                        ))?;
                        
                    let check_result = known_hosts.check_port(&host, port, server_key);
                    if let ssh2::CheckResult::Failure = check_result {
                        return Err(RemoteExecutorError::SecurityError(
                            format!("ホスト鍵の検証に失敗しました: {}@{}", username, host)
                        ));
                    } else if let ssh2::CheckResult::NotFound = check_result {
                        // オプションで新規ホストを自動的に追加
                        if config.auto_add_host_key() {
                            known_hosts.add(server_key, &host, None, port)
                                .map_err(|e| RemoteExecutorError::SecurityError(
                                    format!("ホスト鍵の追加に失敗: {}", e)
                                ))?;
                                
                            known_hosts.write_file(known_hosts_file, KnownHostFileKind::OpenSSH)
                                .map_err(|e| RemoteExecutorError::SecurityError(
                                    format!("既知のホストファイルの書き込みに失敗: {}", e)
                                ))?;
                        } else {
                            return Err(RemoteExecutorError::SecurityError(
                                format!("未知のホスト鍵です: {}@{}", username, host)
                            ));
                        }
                    }
                }
            }

            // 認証
            match auth_method {
                AuthMethod::Password(pass) => {
                    session.userauth_password(&username, &pass)
                        .map_err(|e| RemoteExecutorError::AuthenticationFailed(format!("パスワード認証に失敗: {}", e)))?;
                }
                AuthMethod::PublicKey(key_path) => {
                    let key_path = Path::new(&key_path);
                    let pubkey_path = format!("{}.pub", key_path.to_string_lossy());
                    
                    session.userauth_pubkey_file(
                        &username,
                        Some(Path::new(&pubkey_path)),
                        key_path,
                        None
                    ).map_err(|e| RemoteExecutorError::AuthenticationFailed(format!("公開鍵認証に失敗: {}", e)))?;
                }
                AuthMethod::SshAgent => {
                    session.userauth_agent(&username)
                        .map_err(|e| RemoteExecutorError::AuthenticationFailed(format!("SSHエージェント認証に失敗: {}", e)))?;
                }
                AuthMethod::HostBased(host_key_path) => {
                    // ホストベース認証（現在のlibssh2ではサポートされていないため、エラーを返す）
                    return Err(RemoteExecutorError::AuthenticationFailed(
                        "ホストベース認証は現在サポートされていません".to_string()
                    ));
                }
                AuthMethod::KeyboardInteractive => {
                    // キーボードインタラクティブ認証の実装
                    debug!("キーボードインタラクティブ認証を開始: {}@{}", &self.username, &self.host);
                    
                    // 認証チャレンジを処理
                    let prompts = session.keyboard_interactive_prompts()?;
                    let mut responses = Vec::new();
                    
                    // セキュリティ設定を非同期的にロック
                    let security = self.security.read().await;
                    
                    for prompt in prompts {
                        // プロンプトをユーザーに表示
                        let response = if let Some(callback) = &security.host_key_callback {
                            // 同期コールバックを呼び出し
                            (callback)(prompt.text.clone(), prompt.echo)
                        } else {
                            // コールバックが設定されていない場合はデフォルトの処理
                            use std::io::{stdin, stdout, Write};
                            print!("{}", prompt.text);
                            stdout().flush()?;
                            
                            let mut response = String::new();
                            stdin().read_line(&mut response)?;
                            response.trim().to_string()
                        };
                        
                        responses.push(response);
                    }
                    
                    // セキュリティ設定のロックを解放
                    drop(security);
                    
                    // レスポンスを送信
                    session.keyboard_interactive_authenticate(responses)?;
                    debug!("キーボードインタラクティブ認証成功: {}@{}", &self.username, &self.host);
                }
                AuthMethod::Kerberos => {
                    // Kerberos認証（現在のlibssh2ではサポートされていないため、エラーを返す）
                    return Err(RemoteExecutorError::AuthenticationFailed(
                        "Kerberos認証は現在サポートされていません".to_string()
                    ));
                }
            }

            // 認証確認
            if !session.authenticated() {
                return Err(RemoteExecutorError::AuthenticationFailed("認証に失敗しました".to_string()));
            }

            // セッションをMutexで包む
            let mut session_guard = session_arc.blocking_lock();
            *session_guard = Some(session);
            
            Ok(())
        }).await.map_err(|e| RemoteExecutorError::ConnectionFailed(format!("接続タスクがパニックしました: {}", e)))?;
        
        // RTTメトリクスを更新
        {
            let connection_time = start_time.elapsed().as_millis() as f64;
            let mut metrics = self.metrics.write().await;
            
            // RTT（往復時間）を更新
            metrics.rtt_ms = ((metrics.rtt_ms * metrics.rtt_samples as f64) + connection_time) / 
                             (metrics.rtt_samples as f64 + 1.0);
            metrics.rtt_samples += 1;
            
            // プロメテウスメトリクスを更新
            histogram!("nexusshell_remote_connection_time_ms", "host" => self.host.clone()).record(connection_time);
        }

        result
    }
    
    /// 内部接続状態を取得します
    async fn get_session(&self) -> Result<Arc<RwLock<Option<Session>>>, RemoteExecutorError> {
        if !*self.connected.read().await {
            return Err(RemoteExecutorError::ConnectionClosed("接続が確立されていません".to_string()));
        }
        
        Ok(Arc::clone(&self.session))
    }
    
    /// 再接続を試みます
    pub async fn reconnect(&self) -> Result<(), RemoteExecutorError> {
        let mut connected = self.connected.write().await;
        
        // すでに接続されていれば何もしない
        if *connected {
            return Ok(());
        }
        
        debug!("リモートサーバーに再接続しています: {}@{}:{}", self.username, self.host, self.port);
        
        // 再接続の試行回数を更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.reconnect_attempts += 1;
        }
        
        // セッションを一度クリア
        {
            let mut session = self.session.write().await;
            *session = None;
        }
        
        // 指数バックオフ付きの再接続
        let backoff = {
            let backoff = *self.reconnect_backoff.read().await;
            
            // 次回のバックオフを更新（上限5秒）
            let mut next_backoff = self.reconnect_backoff.write().await;
            *next_backoff = std::cmp::min(backoff * 2, 5000);
            
            backoff
        };
        
        // ジッタを加えたバックオフ（0-20%のランダム変動）
        let jitter = thread_rng().gen_range(0..=20) as f64 / 100.0;
        let backoff_with_jitter = (backoff as f64 * (1.0 + jitter)) as u64;
        
        // バックオフ時間待機
        if backoff_with_jitter > 0 {
            debug!("再接続前に{}ミリ秒待機します", backoff_with_jitter);
            tokio::time::sleep(Duration::from_millis(backoff_with_jitter)).await;
        }
        
        // 再接続処理
        match self.connect_internal().await {
            Ok(_) => {
                *connected = true;
                
                // 接続が成功したらキープアライブを開始
                if self.config.enable_keepalive() {
                    self.start_keepalive().await;
                }
                
                info!("SSH再接続に成功しました: {}@{}:{}", self.username, self.host, self.port);
                
                // プロメテウスメトリクスを更新
                counter!("nexusshell_remote_reconnections_succeeded", "host" => self.host.clone()).increment(1);
                
                Ok(())
            }
            Err(e) => {
                error!("SSH再接続に失敗しました: {}@{}:{} - {}", self.username, self.host, self.port, e);
                
                // プロメテウスメトリクスを更新
                counter!("nexusshell_remote_reconnections_failed", "host" => self.host.clone()).increment(1);
                
                Err(e)
            }
        }
    }

    /// SSH接続を切断します
    pub async fn disconnect(&mut self) -> Result<(), RemoteExecutorError> {
        let mut connected = self.connected.write().await;
        if !*connected {
            return Ok(());
        }

        debug!("リモートサーバーから切断しています: {}@{}:{}", self.username, self.host, self.port);
        
        // キープアライブを停止
        if let Some(tx) = &self.keepalive_tx {
            let _ = tx.send(()).await;
            self.keepalive_tx = None;
        }

        let mut session_lock = self.session.write().await;
        if let Some(session) = session_lock.take() {
            // ssh2のdisconnect操作はブロッキングなので、spawn_blockingで実行
            let disconnect_result = tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
                session.disconnect(Some(DisconnectCode::ByApplication), "セッション終了", None)
                    .map_err(|e| RemoteExecutorError::ConnectionClosed(format!("切断処理中にエラーが発生: {}", e)))?;
                Ok(())
            }).await;

            match disconnect_result {
                Ok(result) => {
                    *connected = false;
                    debug!("リモートサーバーから切断しました: {}@{}:{}", self.username, self.host, self.port);
                    
                    // メトリクスを更新
                    counter!("nexusshell_remote_connections_closed", "host" => self.host.clone()).increment(1);
                    
                    result
                }
                Err(e) => {
                    error!("切断処理中にタスクがパニックしました: {}", e);
                    *connected = false; // セッションは既にdropされているので切断されたと見なす
                    Ok(())
                }
            }
        } else {
            *connected = false;
            Ok(())
        }
    }
    
    /// キープアライブタスクを開始します
    async fn start_keepalive(&self) {
        let keepalive_interval = match self.config.keepalive_interval() {
            Some(interval) => interval,
            None => return, // キープアライブが無効
        };
        
        // 既存のキープアライブタスクを終了
        if let Some(tx) = &self.keepalive_tx {
            let _ = tx.send(()).await;
        }
        
        let (tx, mut rx) = mpsc::channel::<()>(1);
        self.keepalive_tx = Some(tx);
        
        let session_arc = self.session.clone();
        let connected_arc = self.connected.clone();
        let host = self.host.clone();
        let username = self.username.clone();
        let port = self.port;
        
        // キープアライブタスクを開始
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(keepalive_interval);
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 接続チェック
                        if !*connected_arc.read().await {
                            debug!("キープアライブタスクを終了します: 接続が閉じられています");
                            break;
                        }
                        
                        // キープアライブを送信
                        trace!("キープアライブを送信: {}@{}:{}", username, host, port);
                        
                        // ブロッキング操作なのでspawn_blockingで実行
                        let session_arc_clone = session_arc.clone();
                        let result = tokio::task::spawn_blocking(move || -> Result<bool, RemoteExecutorError> {
                            let session_guard = session_arc_clone.blocking_lock();
                            if let Some(session) = &*session_guard {
                                match session.keepalive_send() {
                                    Ok(needs_reply) => Ok(needs_reply),
                                    Err(e) => Err(RemoteExecutorError::ConnectionClosed(
                                        format!("キープアライブの送信に失敗: {}", e)
                                    ))
                                }
                            } else {
                                Ok(false)
                            }
                        }).await;
                        
                        // エラーがあれば接続切断とみなす
                        if let Ok(Err(e)) = result {
                            error!("キープアライブエラー: {} - {}", host, e);
                            *connected_arc.write().await = false;
                            break;
                        }
                    }
                    _ = rx.recv() => {
                        debug!("キープアライブタスクを停止します: {}@{}:{}", username, host, port);
                        break;
                    }
                }
            }
        });
    }

    /// リモートコマンドを実行します
    pub async fn execute_command(&self, command: &str) -> Result<CommandResult, RemoteExecutorError> {
        let start_time = Instant::now();
        let session_arc = self.get_session().await?;
        
        // メトリクスを更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.commands_executed += 1;
            metrics.last_used_time = Some(Instant::now());
        }

        debug!("リモートコマンドを実行します: {}", command);
        
        // コマンド実行のタイムアウト
        let timeout_duration = self.config.command_timeout();
        let execution_result = timeout(
            timeout_duration,
            self.execute_command_internal(&session_arc, command),
        ).await;
        
        match execution_result {
            Ok(result) => {
                // 実行時間を計算
                let execution_time = start_time.elapsed().as_millis() as u64;
                
                match result {
                    Ok(mut cmd_result) => {
                        // 成功
                        trace!("コマンド実行結果: exit_code={}, stdout={}, stderr={}",
                            cmd_result.exit_code, cmd_result.stdout, cmd_result.stderr);
                        
                        // 実行時間を設定
                        cmd_result.execution_time_ms = execution_time;
                        
                        // プロメテウスメトリクスを更新
                        counter!("nexusshell_remote_commands_executed", "host" => self.host.clone()).increment(1);
                        histogram!("nexusshell_remote_command_time_ms", "host" => self.host.clone()).record(execution_time as f64);
                        
                        Ok(cmd_result)
                    }
                    Err(e) => {
                        // コマンド実行エラー
                        error!("コマンド実行に失敗しました: {} - {}", command, e);
                        
                        // メトリクスを更新
                        {
                            let mut metrics = self.metrics.write().await;
                            metrics.last_error = Some(e.to_string());
                        }
                        
                        // プロメテウスメトリクスを更新
                        counter!("nexusshell_remote_command_errors", "host" => self.host.clone(), "error" => e.to_string()).increment(1);
                        
                        Err(e)
                    }
                }
            }
            Err(_) => {
                // タイムアウト
                error!("コマンド実行がタイムアウトしました: {}", command);
                
                // メトリクスを更新
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.last_error = Some("コマンド実行タイムアウト".to_string());
                }
                
                // プロメテウスメトリクスを更新
                counter!("nexusshell_remote_command_timeouts", "host" => self.host.clone()).increment(1);
                
                Err(RemoteExecutorError::Timeout)
            }
        }
    }

    /// リモートコマンドを実行します（内部実装）
    async fn execute_command_internal(
        &self,
        session_arc: Arc<RwLock<Option<Session>>>,
        command: &str,
    ) -> Result<CommandResult, RemoteExecutorError> {
        let command_str = command.to_string();
        
        // セッションを非同期に取得して複製
        let session_guard = session_arc.read().await;
        let session_opt = session_guard.as_ref();
        
        // Sessionのクローンを作成
        let session = match session_opt {
            Some(s) => s.clone(),
            None => return Err(RemoteExecutorError::ConnectionClosed("セッションが存在しません".to_string())),
        };
        
        // ブロッキング操作をspawn_blockingで実行
        tokio::task::spawn_blocking(move || -> Result<CommandResult, RemoteExecutorError> {
            // チャネルを開く
            let mut channel = session.channel_session()
                .map_err(|e| RemoteExecutorError::ChannelOpenFailed(format!("チャネルのオープンに失敗: {}", e)))?;
            
            // 擬似ターミナルを要求（オプション）
            if let Some(term) = session.blocking_getenv("TERM") {
                let term_str = term.unwrap_or_else(|| "xterm".to_string());
                let _ = channel.request_pty(&term_str, None, None);
            }
            
            // コマンドを実行
            channel.exec(&command_str)
                .map_err(|e| RemoteExecutorError::CommandExecutionFailed(format!("コマンド実行に失敗: {}", e)))?;
            
            // 標準出力を読み取り
            let mut stdout = Vec::new();
            channel.read_to_end(&mut stdout)
                .map_err(|e| RemoteExecutorError::IoError(e))?;
            
            // 標準エラー出力を読み取り
            let mut stderr = Vec::new();
            channel.stderr().read_to_end(&mut stderr)
                .map_err(|e| RemoteExecutorError::IoError(e))?;
            
            // チャネルを閉じて終了コードを取得
            channel.send_eof()
                .map_err(|e| RemoteExecutorError::IoError(e))?;
                
            channel.wait_close()
                .map_err(|e| RemoteExecutorError::IoError(e))?;
                
            let exit_code = channel.exit_status()
                .map_err(|e| RemoteExecutorError::CommandExecutionFailed(format!("終了コードの取得に失敗: {}", e)))?;
            
            // 結果を返す
            let stdout_str = String::from_utf8_lossy(&stdout).to_string();
            let stderr_str = String::from_utf8_lossy(&stderr).to_string();
            
            Ok(CommandResult {
                exit_code,
                stdout: stdout_str,
                stderr: stderr_str,
                execution_time_ms: 0, // 呼び出し元で設定
            })
        }).await.map_err(|e| RemoteExecutorError::CommandExecutionFailed(format!("コマンド実行タスクがパニックしました: {}", e)))?
    }

    /// 接続が確立されているかどうかを確認します
    pub async fn is_connected(&self) -> bool {
        // 接続フラグをチェック
        if !*self.connected.read().await {
            return false;
        }
        
        // セッションがあるか確認
        let session_exists = {
            let session = self.session.read().await;
            session.is_some()
        };
        
        if !session_exists {
            return false;
        }
        
        // 高度な接続確認プロトコルを実行
        const MAX_PING_RETRIES: u8 = 3;
        const PING_TIMEOUT_MS: u64 = 500;
        
        let mut connected = false;
        let mut latency_ms = 0.0;
        let mut retry_count = 0;
        
        while retry_count < MAX_PING_RETRIES && !connected {
            // タイムアウト付きで軽量なコマンドを実行
            let start_time = Instant::now();
            let ping_result = tokio::time::timeout(
                Duration::from_millis(PING_TIMEOUT_MS),
                self.execute_remote_command("echo NEXUSSHELL_PING")
            ).await;
            
            match ping_result {
                // タイムアウトなし、コマンド成功
                Ok(Ok(_)) => {
                    latency_ms = start_time.elapsed().as_millis() as f64;
                    connected = true;
                    
                    // メトリクスを更新
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.rtt_ms = (metrics.rtt_ms * metrics.rtt_samples as f64 + latency_ms) 
                                      / (metrics.rtt_samples as f64 + 1.0);
                        metrics.rtt_samples += 1;
                    }
                    
                    trace!("接続確認成功: ホスト={}, RTT={:.2}ms", self.host, latency_ms);
                }
                // タイムアウトなし、コマンド失敗
                Ok(Err(err)) => {
                    warn!("接続確認コマンドが失敗: ホスト={}, エラー={}", self.host, err);
                    retry_count += 1;
                    
                    // 短い待機時間を入れて再試行
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    
                    // 障害の詳細を記録
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.last_error = Some(format!("接続確認コマンド失敗: {}", err));
                    }
                }
                // タイムアウト発生
                Err(_) => {
                    warn!("接続確認がタイムアウト: ホスト={}, タイムアウト={}ms", self.host, PING_TIMEOUT_MS);
                    retry_count += 1;
                    
                    // より長い待機時間を入れて再試行
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    
                    // 障害の詳細を記録
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.last_error = Some(format!("接続確認タイムアウト: {}ms", PING_TIMEOUT_MS));
                    }
                }
            }
        }
        
        // すべての再試行が失敗した場合は、再接続を試みる
        if !connected {
            debug!("接続が不安定のため再接続を試みます: ホスト={}", self.host);
            
            // 再接続を試行
            match self.reconnect().await {
                Ok(_) => {
                    info!("ホスト {}への再接続に成功しました", self.host);
                    connected = true;
                }
                Err(err) => {
                    error!("ホスト {}への再接続に失敗しました: {}", self.host, err);
                    
                    // 障害情報を更新
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.last_error = Some(format!("再接続失敗: {}", err));
                    }
                    
                    // 接続フラグを更新
                    *self.connected.write().await = false;
                }
            }
        }
        
        connected
    }
    
    /// 接続状態を確認します
    async fn check_connection(&self) -> Result<bool, RemoteExecutorError> {
        let session_arc = self.session.clone();
        
        // ブロッキング操作をspawn_blockingで実行
        let result = tokio::task::spawn_blocking(move || -> Result<bool, RemoteExecutorError> {
            let session_guard = session_arc.blocking_lock();
            if let Some(session) = &*session_guard {
                // アライブかどうかを単純な操作で確認
                match session.authenticated() {
                    true => Ok(true),
                    false => Err(RemoteExecutorError::ConnectionClosed("認証されていません".to_string())),
                }
            } else {
                Err(RemoteExecutorError::ConnectionClosed("セッションが存在しません".to_string()))
            }
        }).await;
        
        match result {
            Ok(Ok(true)) => Ok(true),
            Ok(Ok(false)) => Ok(false),
            Ok(Err(_)) => {
                // 接続フラグを更新
                *self.connected.write().await = false;
                Ok(false)
            }
            Err(_) => {
                // 接続フラグを更新
                *self.connected.write().await = false;
                Ok(false)
            }
        }
    }

    /// 設定を取得します
    pub fn config(&self) -> RemoteConfig {
        self.config.clone()
    }
    
    /// ファイルをリモートホストにアップロードします
    pub async fn upload_file(&self, local_path: &Path, remote_path: &str) -> Result<(), RemoteExecutorError> {
        let start_time = Instant::now();
        debug!("ファイルをアップロードします: {} -> {}", local_path.display(), remote_path);
        
        // ローカルファイルのサイズを取得
        let file_size = std::fs::metadata(local_path)
            .map_err(|e| RemoteExecutorError::IoError(e))?
            .len();
            
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let remote_path_str = remote_path.to_string();
        let local_path_buf = local_path.to_path_buf();
        
        // ブロッキング操作をspawn_blockingで実行
        let result = tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            // ローカルファイルを開く
            let mut local_file = std::fs::File::open(&local_path_buf)
                .map_err(|e| RemoteExecutorError::IoError(e))?;
                
            // リモートファイルを作成
            let mut remote_file = sftp.create(&remote_path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("リモートファイルの作成に失敗: {}", e)
                ))?;
                
            // バッファサイズ（8 MiB）
            const BUFFER_SIZE: usize = 8 * 1024 * 1024;
            let mut buffer = vec![0u8; BUFFER_SIZE];
            let mut total_bytes = 0;
            
            // ファイルをチャンクで転送
            loop {
                let bytes_read = local_file.read(&mut buffer)
                    .map_err(|e| RemoteExecutorError::IoError(e))?;
                    
                if bytes_read == 0 {
                    break;
                }
                
                remote_file.write_all(&buffer[..bytes_read])
                    .map_err(|e| RemoteExecutorError::DataTransferFailed(
                        format!("データの書き込みに失敗: {}", e)
                    ))?;
                    
                total_bytes += bytes_read;
            }
            
            // ファイルを閉じる
            remote_file.close()
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("リモートファイルのクローズに失敗: {}", e)
                ))?;
                
            Ok(())
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル転送タスクがパニックしました: {}", e)
        ))?;
        
        // 実行時間を計算
        let transfer_time = start_time.elapsed().as_millis() as f64;
        
        // メトリクスを更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.bytes_uploaded += file_size;
            metrics.last_used_time = Some(Instant::now());
        }
        
        // プロメテウスメトリクスを更新
        counter!("nexusshell_remote_files_uploaded", "host" => self.host.clone()).increment(1);
        counter!("nexusshell_remote_bytes_uploaded", "host" => self.host.clone()).increment(file_size as f64);
        
        if file_size > 0 {
            let transfer_rate = (file_size as f64 / 1024.0) / (transfer_time / 1000.0); // KB/s
            histogram!("nexusshell_remote_upload_rate_kbps", "host" => self.host.clone()).record(transfer_rate);
        }
        
        info!("ファイルアップロード完了: {} -> {} ({:.2} KB/s)",
            local_path.display(), remote_path, 
            if transfer_time > 0.0 { (file_size as f64 / 1024.0) / (transfer_time / 1000.0) } else { 0.0 });
            
        result
    }
    
    /// ファイルをリモートホストからダウンロードします
    pub async fn download_file(&self, remote_path: &str, local_path: &Path) -> Result<(), RemoteExecutorError> {
        let start_time = Instant::now();
        debug!("ファイルをダウンロードします: {} -> {}", remote_path, local_path.display());
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let remote_path_str = remote_path.to_string();
        let local_path_buf = local_path.to_path_buf();
        
        // ブロッキング操作をspawn_blockingで実行
        let (file_size, result) = tokio::task::spawn_blocking(move || -> Result<u64, RemoteExecutorError> {
            // リモートファイルを開く
            let remote_file_attrs = sftp.stat(&remote_path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("リモートファイルの情報取得に失敗: {}", e)
                ))?;
                
            let file_size = remote_file_attrs.size.unwrap_or(0);
                
            let mut remote_file = sftp.open(&remote_path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("リモートファイルのオープンに失敗: {}", e)
                ))?;
                
            // ローカルファイルを作成
            let mut local_file = std::fs::File::create(&local_path_buf)
                .map_err(|e| RemoteExecutorError::IoError(e))?;
                
            // バッファサイズ（8 MiB）
            const BUFFER_SIZE: usize = 8 * 1024 * 1024;
            let mut buffer = vec![0u8; BUFFER_SIZE];
            let mut total_bytes = 0;
            
            // ファイルをチャンクで転送
            loop {
                let bytes_read = remote_file.read(&mut buffer)
                    .map_err(|e| RemoteExecutorError::IoError(e))?;
                    
                if bytes_read == 0 {
                    break;
                }
                
                local_file.write_all(&buffer[..bytes_read])
                    .map_err(|e| RemoteExecutorError::IoError(e))?;
                    
                total_bytes += bytes_read;
            }
            
            Ok(file_size)
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル転送タスクがパニックしました: {}", e)
        ))??;
        
        // 実行時間を計算
        let transfer_time = start_time.elapsed().as_millis() as f64;
        
        // メトリクスを更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.bytes_downloaded += file_size;
            metrics.last_used_time = Some(Instant::now());
        }
        
        // プロメテウスメトリクスを更新
        counter!("nexusshell_remote_files_downloaded", "host" => self.host.clone()).increment(1);
        counter!("nexusshell_remote_bytes_downloaded", "host" => self.host.clone()).increment(file_size as f64);
        
        if file_size > 0 {
            let transfer_rate = (file_size as f64 / 1024.0) / (transfer_time / 1000.0); // KB/s
            histogram!("nexusshell_remote_download_rate_kbps", "host" => self.host.clone()).record(transfer_rate);
        }
        
        info!("ファイルダウンロード完了: {} -> {} ({:.2} KB/s)",
            remote_path, local_path.display(), 
            if transfer_time > 0.0 { (file_size as f64 / 1024.0) / (transfer_time / 1000.0) } else { 0.0 });
            
        Ok(())
    }
    
    /// SFTPセッションを取得します
    async fn get_sftp_session(&self) -> Result<Sftp, RemoteExecutorError> {
        // 既存のSFTPセッションを確認
        {
            let sftp_guard = self.sftp_session.lock().await;
            if let Some(sftp) = &*sftp_guard {
                return Ok(sftp.clone());
            }
        }
        
        // セッションを取得
        let session_arc = self.get_session().await?;
        
        // セッションを非同期に取得して複製
        let session_guard = session_arc.read().await;
        let session_opt = session_guard.as_ref();
        
        // Sessionのクローンを作成
        let session = match session_opt {
            Some(s) => s.clone(),
            None => return Err(RemoteExecutorError::ConnectionClosed("セッションが存在しません".to_string())),
        };
        
        // 新しいSFTPセッションを作成
        let sftp = tokio::task::spawn_blocking(move || -> Result<Sftp, RemoteExecutorError> {
            session.sftp()
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("SFTPセッションの作成に失敗: {}", e)
                ))
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("SFTPセッション作成タスクがパニックしました: {}", e)
        ))??;
        
        // SFTPセッションを保存
        {
            let mut sftp_guard = self.sftp_session.lock().await;
            *sftp_guard = Some(sftp.clone());
        }
        
        debug!("新しいSFTPセッションを作成しました: {}@{}", self.username, self.host);
        Ok(sftp)
    }
    
    /// リモートホストにディレクトリを作成します
    pub async fn create_directory(&self, path: &str) -> Result<(), RemoteExecutorError> {
        debug!("ディレクトリを作成します: {}", path);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            sftp.mkdir(&path_str, 0o755)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("ディレクトリの作成に失敗: {}", e)
                ))
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ディレクトリ作成タスクがパニックしました: {}", e)
        ))??;
        
        info!("ディレクトリを作成しました: {}", path);
        Ok(())
    }
    
    /// リモートホストのディレクトリ内のファイル一覧を取得します
    pub async fn list_directory(&self, path: &str) -> Result<Vec<String>, RemoteExecutorError> {
        debug!("ディレクトリ内のファイル一覧を取得します: {}", path);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        let files = tokio::task::spawn_blocking(move || -> Result<Vec<String>, RemoteExecutorError> {
            let dir = sftp.opendir(&path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("ディレクトリのオープンに失敗: {}", e)
                ))?;
                
            let mut file_names = Vec::new();
            
            for entry in dir {
                let entry = entry.map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("ディレクトリエントリの読み取りに失敗: {}", e)
                ))?;
                
                if let Some(filename) = entry.filename {
                    let name = String::from_utf8_lossy(&filename).to_string();
                    if name != "." && name != ".." {
                        file_names.push(name);
                    }
                }
            }
            
            Ok(file_names)
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ディレクトリ一覧取得タスクがパニックしました: {}", e)
        ))??;
        
        debug!("ディレクトリ内のファイル一覧を取得しました: {} ({}ファイル)", path, files.len());
        Ok(files)
    }
    
    /// リモートホストのファイルが存在するかどうかを確認します
    pub async fn file_exists(&self, path: &str) -> Result<bool, RemoteExecutorError> {
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        let exists = tokio::task::spawn_blocking(move || -> Result<bool, RemoteExecutorError> {
            match sftp.stat(&path_str) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル存在確認タスクがパニックしました: {}", e)
        ))??;
        
        trace!("ファイル存在確認: {} - {}", path, if exists { "存在します" } else { "存在しません" });
        Ok(exists)
    }
    
    /// リモートホストのファイルを削除します
    pub async fn remove_file(&self, path: &str) -> Result<(), RemoteExecutorError> {
        debug!("ファイルを削除します: {}", path);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            sftp.unlink(&path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("ファイルの削除に失敗: {}", e)
                ))
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル削除タスクがパニックしました: {}", e)
        ))??;
        
        info!("ファイルを削除しました: {}", path);
        Ok(())
    }
    
    /// 接続のメトリクスを取得します
    pub async fn metrics(&self) -> ConnectionMetrics {
        self.metrics.read().await.clone()
    }

    /// リモートホストのファイル情報を取得します
    pub async fn stat_file(&self, path: &str) -> Result<super::protocol::FileInfo, RemoteExecutorError> {
        debug!("ファイル情報を取得します: {}", path);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        let file_info = tokio::task::spawn_blocking(move || -> Result<super::protocol::FileInfo, RemoteExecutorError> {
            // ファイルの情報を取得
            let stat = sftp.stat(&path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("ファイル情報の取得に失敗: {}", e)
                ))?;
            
            // ファイル名を取得（パスから抽出）
            let name = match std::path::Path::new(&path_str).file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => path_str.clone(),
            };
            
            // ファイル種別を解析
            let file_type = match stat.file_type() {
                Some(ssh2::FileType::S_IFDIR) => super::protocol::FileType::Directory,
                Some(ssh2::FileType::S_IFLNK) => super::protocol::FileType::Symlink,
                Some(ssh2::FileType::S_IFBLK) => super::protocol::FileType::BlockDevice,
                Some(ssh2::FileType::S_IFCHR) => super::protocol::FileType::CharDevice,
                Some(ssh2::FileType::S_IFIFO) => super::protocol::FileType::Fifo,
                Some(ssh2::FileType::S_IFSOCK) => super::protocol::FileType::Socket,
                Some(ssh2::FileType::S_IFREG) => super::protocol::FileType::Regular,
                _ => super::protocol::FileType::Unknown,
            };
            
            // unix時間に変換
            let mtime = stat.mtime.unwrap_or(0);
            let atime = stat.atime.unwrap_or(0);
            
            // ファイル情報を構築
            let info = super::protocol::FileInfo {
                name,
                size: stat.size.unwrap_or(0),
                modified_time: mtime,
                access_time: atime,
                creation_time: None, // SSH2/SFTPはcreation_timeを提供しない
                file_type,
                permissions: stat.perm.unwrap_or(0),
                uid: stat.uid.unwrap_or(0),
                gid: stat.gid.unwrap_or(0),
            };
            
            Ok(info)
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル情報取得タスクがパニックしました: {}", e)
        ))??;
        
        trace!("ファイル情報を取得しました: {} (type={:?}, size={})", 
               path, file_info.file_type, file_info.size);
        Ok(file_info)
    }

    /// リモートホストのファイル権限を変更します
    pub async fn chmod_file(&self, path: &str, mode: u32) -> Result<(), RemoteExecutorError> {
        debug!("ファイル権限を変更します: {} -> {:o}", path, mode);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            sftp.setstat(&path_str, ssh2::FileStat {
                size: None,
                uid: None,
                gid: None,
                perm: Some(mode),
                atime: None,
                mtime: None,
            })
            .map_err(|e| RemoteExecutorError::DataTransferFailed(
                format!("ファイル権限の変更に失敗: {}", e)
            ))
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル権限変更タスクがパニックしました: {}", e)
        ))??;
        
        info!("ファイル権限を変更しました: {} -> {:o}", path, mode);
        Ok(())
    }

    /// リモートホストのファイル名を変更します
    pub async fn rename_file(&self, from: &str, to: &str) -> Result<(), RemoteExecutorError> {
        debug!("ファイル名を変更します: {} -> {}", from, to);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let from_str = from.to_string();
        let to_str = to.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            sftp.rename(&from_str, &to_str, None)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("ファイル名の変更に失敗: {}", e)
                ))
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("ファイル名変更タスクがパニックしました: {}", e)
        ))??;
        
        info!("ファイル名を変更しました: {} -> {}", from, to);
        Ok(())
    }

    /// リモートホストにシンボリックリンクを作成します
    pub async fn create_symlink(&self, path: &str, target: &str) -> Result<(), RemoteExecutorError> {
        debug!("シンボリックリンクを作成します: {} -> {}", path, target);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        let target_str = target.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        tokio::task::spawn_blocking(move || -> Result<(), RemoteExecutorError> {
            sftp.symlink(&target_str, &path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("シンボリックリンクの作成に失敗: {}", e)
                ))
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("シンボリックリンク作成タスクがパニックしました: {}", e)
        ))??;
        
        info!("シンボリックリンクを作成しました: {} -> {}", path, target);
        Ok(())
    }

    /// リモートホストのシンボリックリンクの参照先を取得します
    pub async fn read_link(&self, path: &str) -> Result<String, RemoteExecutorError> {
        debug!("シンボリックリンクの参照先を取得します: {}", path);
        
        // SFTPセッションを取得
        let sftp = self.get_sftp_session().await?;
        let path_str = path.to_string();
        
        // ブロッキング操作をspawn_blockingで実行
        let target = tokio::task::spawn_blocking(move || -> Result<String, RemoteExecutorError> {
            sftp.readlink(&path_str)
                .map_err(|e| RemoteExecutorError::DataTransferFailed(
                    format!("シンボリックリンクの読み取りに失敗: {}", e)
                ))
                .map(|p| p.to_string_lossy().to_string())
        }).await.map_err(|e| RemoteExecutorError::DataTransferFailed(
            format!("シンボリックリンク読み取りタスクがパニックしました: {}", e)
        ))??;
        
        debug!("シンボリックリンクの参照先を取得しました: {} -> {}", path, target);
        Ok(target)
    }

    /// リモートコマンドを実行します
    async fn execute_remote_command(&self, command: &str) -> Result<(), RemoteExecutorError> {
        let start_time = Instant::now();
        let session_arc = self.get_session().await?;
        
        // メトリクスを更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.commands_executed += 1;
            metrics.last_used_time = Some(Instant::now());
        }

        debug!("リモートコマンドを実行します: {}", command);
        
        // コマンド実行のタイムアウト
        let timeout_duration = self.config.command_timeout();
        let execution_result = timeout(
            timeout_duration,
            self.execute_command_internal(session_arc, command),
        ).await;
        
        match execution_result {
            Ok(result) => {
                // 実行時間を計算
                let execution_time = start_time.elapsed().as_millis() as u64;
                
                match result {
                    Ok(mut cmd_result) => {
                        // 成功
                        trace!("コマンド実行結果: exit_code={}, stdout={}, stderr={}",
                            cmd_result.exit_code, cmd_result.stdout, cmd_result.stderr);
                        
                        // 実行時間を設定
                        cmd_result.execution_time_ms = execution_time;
                        
                        // プロメテウスメトリクスを更新
                        counter!("nexusshell_remote_commands_executed", "host" => self.host.clone()).increment(1);
                        histogram!("nexusshell_remote_command_time_ms", "host" => self.host.clone()).record(execution_time as f64);
                        
                        Ok(())
                    }
                    Err(e) => {
                        // コマンド実行エラー
                        error!("コマンド実行に失敗しました: {} - {}", command, e);
                        
                        // メトリクスを更新
                        {
                            let mut metrics = self.metrics.write().await;
                            metrics.last_error = Some(e.to_string());
                        }
                        
                        // プロメテウスメトリクスを更新
                        counter!("nexusshell_remote_command_errors", "host" => self.host.clone(), "error" => e.to_string()).increment(1);
                        
                        Err(e)
                    }
                }
            }
            Err(_) => {
                // タイムアウト
                error!("コマンド実行がタイムアウトしました: {}", command);
                
                // メトリクスを更新
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.last_error = Some("コマンド実行タイムアウト".to_string());
                }
                
                // プロメテウスメトリクスを更新
                counter!("nexusshell_remote_command_timeouts", "host" => self.host.clone()).increment(1);
                
                Err(RemoteExecutorError::Timeout)
            }
        }
    }
}

/// SFTPプロトコルの実装
pub struct SftpProtocol {
    connection: RemoteConnection,
}

impl SftpProtocol {
    /// 新しいSFTPプロトコルインスタンスを作成します
    pub fn new(connection: RemoteConnection) -> Self {
        Self { connection }
    }
    
    /// 接続を取得します
    pub fn connection(&self) -> &RemoteConnection {
        &self.connection
    }
}

// SFTPプロトコルトレイトの実装
#[async_trait::async_trait]
impl super::protocol::SftpProtocol for SftpProtocol {
    /// ファイルをアップロードします
    async fn upload(&self, local_path: &Path, remote_path: &str) -> Result<(), RemoteExecutorError> {
        self.connection.upload_file(local_path, remote_path).await
    }
    
    /// ファイルをダウンロードします
    async fn download(&self, remote_path: &str, local_path: &Path) -> Result<(), RemoteExecutorError> {
        self.connection.download_file(remote_path, local_path).await
    }
    
    /// ディレクトリを作成します
    async fn mkdir(&self, path: &str) -> Result<(), RemoteExecutorError> {
        self.connection.create_directory(path).await
    }
    
    /// ディレクトリ内のファイル一覧を取得します
    async fn list_dir(&self, path: &str) -> Result<Vec<String>, RemoteExecutorError> {
        self.connection.list_directory(path).await
    }
    
    /// ファイルが存在するかどうかを確認します
    async fn exists(&self, path: &str) -> Result<bool, RemoteExecutorError> {
        self.connection.file_exists(path).await
    }
    
    /// ファイルを削除します
    async fn remove(&self, path: &str) -> Result<(), RemoteExecutorError> {
        self.connection.remove_file(path).await
    }
    
    /// ファイルの情報を取得します
    async fn stat(&self, path: &str) -> Result<super::protocol::FileInfo, RemoteExecutorError> {
        self.connection.stat_file(path).await
    }
    
    /// ファイルの権限を変更します
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), RemoteExecutorError> {
        self.connection.chmod_file(path, mode).await
    }
    
    /// ファイル名を変更します
    async fn rename(&self, from: &str, to: &str) -> Result<(), RemoteExecutorError> {
        self.connection.rename_file(from, to).await
    }
    
    /// シンボリックリンクを作成します
    async fn symlink(&self, path: &str, target: &str) -> Result<(), RemoteExecutorError> {
        self.connection.create_symlink(path, target).await
    }
    
    /// シンボリックリンクの参照先を取得します
    async fn readlink(&self, path: &str) -> Result<String, RemoteExecutorError> {
        self.connection.read_link(path).await
    }
} 