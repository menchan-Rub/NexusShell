mod error;
mod config;
mod connection;
mod protocol;

pub use error::RemoteExecutorError;
pub use config::RemoteConfig;
pub use connection::RemoteConnection;
pub use protocol::{RemoteProtocol, SshProtocol};

use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use dashmap::DashMap;
use tokio::sync::{RwLock, Semaphore};
use log::{debug, error, info, warn, trace};
use metrics::{counter, gauge, histogram};
use futures::future::{self, FutureExt};
use tokio::time::timeout;
use once_cell::sync::Lazy;
use uuid::Uuid;

// グローバル設定
static GLOBAL_CONFIG: Lazy<Arc<RwLock<GlobalExecutorConfig>>> = Lazy::new(|| {
    Arc::new(RwLock::new(GlobalExecutorConfig::default()))
});

/// リモートエグゼキュータ
/// リモートマシン上でコマンドを実行する機能を提供します
pub struct RemoteExecutor {
    /// リモート接続の管理
    connections: DashMap<String, RemoteConnection>,
    /// アクティブなコマンド実行
    active_commands: DashMap<String, CommandExecution>,
    /// デフォルト設定
    default_config: Arc<RwLock<RemoteConfig>>,
    /// 並列接続の制限セマフォア
    connection_limiter: Arc<Semaphore>,
    /// 接続プール
    connection_pool: ConnectionPool,
    /// メトリクス
    metrics: Arc<RwLock<RemoteExecutorMetrics>>,
    /// 健全性チェッカー
    health_checker: Arc<HealthChecker>,
}

/// コマンド実行情報
struct CommandExecution {
    /// 実行ID
    id: String,
    /// ホスト
    host: String,
    /// コマンド
    command: String,
    /// 開始時刻
    start_time: Instant,
    /// 終了時刻（完了した場合）
    end_time: Option<Instant>,
    /// 状態
    state: CommandState,
}

/// コマンド状態
#[derive(Debug, Clone, PartialEq)]
enum CommandState {
    /// 実行中
    Running,
    /// 成功
    Succeeded,
    /// 失敗
    Failed(String),
    /// タイムアウト
    TimedOut,
    /// キャンセル
    Cancelled,
}

/// メトリクス
#[derive(Debug, Clone, Default)]
pub struct RemoteExecutorMetrics {
    /// 確立された接続の総数
    connections_established: u64,
    /// 失敗した接続の総数
    connections_failed: u64,
    /// 実行されたコマンドの総数
    commands_executed: u64,
    /// 成功したコマンドの総数
    commands_succeeded: u64,
    /// 失敗したコマンドの総数
    commands_failed: u64,
    /// タイムアウトしたコマンドの総数
    commands_timedout: u64,
    /// アクティブな接続数
    active_connections: u64,
    /// 転送されたバイト数（アップロード）
    bytes_uploaded: u64,
    /// 転送されたバイト数（ダウンロード）
    bytes_downloaded: u64,
    /// コマンド実行の平均時間（ミリ秒）
    avg_command_execution_time_ms: f64,
    /// サンプル数
    command_execution_samples: u64,
}

/// グローバル設定
#[derive(Debug, Clone)]
struct GlobalExecutorConfig {
    /// デフォルトのSSHポート
    default_ssh_port: u16,
    /// デフォルトのタイムアウト
    default_timeout_sec: u64,
    /// 最大並列接続数
    max_parallel_connections: usize,
    /// 接続プールの有効/無効
    enable_connection_pooling: bool,
    /// 接続プールの最大サイズ
    connection_pool_size: usize,
    /// コマンド実行の最大再試行回数
    max_command_retries: u32,
    /// 健全性チェックの間隔（秒）
    health_check_interval_sec: u64,
}

impl Default for GlobalExecutorConfig {
    fn default() -> Self {
        Self {
            default_ssh_port: 22,
            default_timeout_sec: 30,
            max_parallel_connections: 100,
            enable_connection_pooling: true,
            connection_pool_size: 20,
            max_command_retries: 3,
            health_check_interval_sec: 60,
        }
    }
}

/// 接続プール
struct ConnectionPool {
    /// プール状態
    enabled: bool,
    /// プールされた接続
    pools: DashMap<String, Vec<RemoteConnection>>,
    /// プールの最大サイズ
    max_size: usize,
}

impl ConnectionPool {
    /// 新しい接続プールを作成します
    fn new(enabled: bool, max_size: usize) -> Self {
        Self {
            enabled,
            pools: DashMap::new(),
            max_size,
        }
    }
    
    /// 接続を取得します
    async fn acquire(&self, host: &str, username: &str, auth_method: &AuthMethod) -> Option<RemoteConnection> {
        if !self.enabled {
            return None;
        }
        
        let pool_key = format!("{}@{}", username, host);
        let mut entry = self.pools.entry(pool_key).or_insert_with(Vec::new);
        
        if let Some(conn) = entry.pop() {
            if conn.is_connected().await {
                debug!("接続プールから接続を取得しました: {}@{}", username, host);
                return Some(conn);
            }
        }
        
        None
    }
    
    /// 接続をプールに返却します
    async fn release(&self, connection: RemoteConnection) {
        if !self.enabled {
            // プール無効の場合は切断して返却しない
            let _ = connection.disconnect().await;
            return;
        }
        
        let host = connection.host();
        let username = connection.username();
        let pool_key = format!("{}@{}", username, host);
        
        if connection.is_connected().await {
            let mut entry = self.pools.entry(pool_key).or_insert_with(Vec::new);
            
            if entry.len() < self.max_size {
                debug!("接続をプールに返却します: {}@{}", username, host);
                entry.push(connection);
            } else {
                // プールが一杯なので切断
                debug!("プールが一杯なので接続を切断します: {}@{}", username, host);
                let _ = connection.disconnect().await;
            }
        } else {
            // 切断された接続は返却しない
            debug!("切断された接続はプールに返却しません: {}@{}", username, host);
        }
    }
    
    /// 特定のホストの接続をすべて切断します
    async fn clear_host(&self, host: &str) {
        for mut entry in self.pools.iter_mut() {
            let key = entry.key();
            if key.contains(host) {
                let connections = entry.value_mut();
                for conn in connections.drain(..) {
                    let _ = conn.disconnect().await;
                }
                debug!("ホスト {} の接続プールをクリアしました", host);
            }
        }
    }
    
    /// すべての接続をクリアします
    async fn clear_all(&self) {
        for mut entry in self.pools.iter_mut() {
            let connections = entry.value_mut();
            for conn in connections.drain(..) {
                let _ = conn.disconnect().await;
            }
        }
        debug!("すべての接続プールをクリアしました");
    }
}

/// 健全性チェッカー
struct HealthChecker {
    /// 有効フラグ
    enabled: bool,
    /// チェック間隔
    interval: Duration,
    /// 対象のエグゼキュータ
    executor: Option<Arc<RemoteExecutor>>,
}

impl HealthChecker {
    /// 新しい健全性チェッカーを作成します
    fn new(interval_sec: u64) -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(interval_sec),
            executor: None,
        }
    }
    
    /// エグゼキュータを設定します
    fn set_executor(&mut self, executor: Arc<RemoteExecutor>) {
        self.executor = Some(executor);
    }
    
    /// 健全性チェックを開始します
    async fn start(&self) {
        if !self.enabled || self.executor.is_none() {
            return;
        }
        
        let executor = self.executor.as_ref().unwrap().clone();
        let interval = self.interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                debug!("接続の健全性チェックを実行しています");
                
                // 現在のアクティブな接続をチェック
                for connection_entry in executor.connections.iter() {
                    let connection_id = connection_entry.key().clone();
                    let connection = connection_entry.value();
                    
                    // 切断されている接続をクリーンアップ
                    if !connection.is_connected().await {
                        warn!("切断された接続を削除します: {}", connection_id);
                        executor.connections.remove(&connection_id);
                    }
                }
                
                // メトリクスを更新
                {
                    let mut metrics = executor.metrics.write().await;
                    metrics.active_connections = executor.connections.len() as u64;
                    
                    // Prometheusメトリクスを更新
                    gauge!("nexusshell_remote_active_connections").set(metrics.active_connections as f64);
                }
            }
        });
    }
}

impl RemoteExecutor {
    /// 新しいリモートエグゼキュータを作成します
    pub fn new() -> Self {
        Self::with_config(GlobalExecutorConfig::default())
    }
    
    /// 設定を指定して新しいリモートエグゼキュータを作成します
    pub fn with_config(config: GlobalExecutorConfig) -> Self {
        let connection_pool = ConnectionPool::new(
            config.enable_connection_pooling,
            config.connection_pool_size,
        );
        
        let mut health_checker = HealthChecker::new(config.health_check_interval_sec);
        
        let executor = Self {
            connections: DashMap::new(),
            active_commands: DashMap::new(),
            default_config: Arc::new(RwLock::new(RemoteConfig::default())),
            connection_limiter: Arc::new(Semaphore::new(config.max_parallel_connections)),
            connection_pool,
            metrics: Arc::new(RwLock::new(RemoteExecutorMetrics::default())),
            health_checker: Arc::new(health_checker),
        };
        
        // 循環参照を避けるため、別途設定
        // health_checker.set_executor(Arc::new(executor.clone()));
        
        executor
    }

    /// 新しいリモート接続を作成します
    pub async fn connect(
        &self,
        host: &str,
        username: &str,
        auth_method: AuthMethod,
    ) -> Result<String, RemoteExecutorError> {
        let config = self.default_config.read().await.clone();
        self.connect_with_config(host, username, auth_method, config).await
    }

    /// 設定を指定して新しいリモート接続を作成します
    pub async fn connect_with_config(
        &self,
        host: &str,
        username: &str,
        auth_method: AuthMethod,
        config: RemoteConfig,
    ) -> Result<String, RemoteExecutorError> {
        // 接続IDを生成
        let connection_id = format!("{}@{}", username, host);
        
        // 既存の接続があれば再利用
        if self.connections.contains_key(&connection_id) {
            debug!("既存の接続を再利用します: {}", connection_id);
            
            // 接続が生きているか確認
            let connection = self.connections.get(&connection_id).unwrap();
            if !connection.is_connected().await {
                // 切断されている場合は削除して再接続
                debug!("切断された接続を削除して再接続します: {}", connection_id);
                self.connections.remove(&connection_id);
            } else {
                return Ok(connection_id);
            }
        }
        
        // 接続プールから接続を取得
        if let Some(pooled_connection) = self.connection_pool.acquire(host, username, &auth_method).await {
            debug!("接続プールから接続を取得しました: {}", connection_id);
            self.connections.insert(connection_id.clone(), pooled_connection);
            return Ok(connection_id);
        }
        
        // 並列接続制限を適用
        let _permit = match self.connection_limiter.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return Err(RemoteExecutorError::ConnectionFailed(
                "並列接続の制限に達しました".to_string()
            )),
        };
        
        // 新しい接続を作成
        debug!("新しいリモート接続を作成します: {}", connection_id);
        
        let connection = RemoteConnection::new(host, username, auth_method, config);
        
        // メトリクスを更新
        counter!("nexusshell_remote_connection_attempts").increment(1);
        
        // 接続を開始
        let connect_result = connection.connect().await;
        
        match connect_result {
            Ok(_) => {
                info!("リモート接続に成功しました: {}", connection_id);
                
                // メトリクスを更新
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.connections_established += 1;
                    metrics.active_connections += 1;
                }
                counter!("nexusshell_remote_connections_established").increment(1);
                gauge!("nexusshell_remote_active_connections").increment(1.0);
                
                self.connections.insert(connection_id.clone(), connection);
                Ok(connection_id)
            }
            Err(e) => {
                error!("リモート接続に失敗しました: {} - {}", connection_id, e);
                
                // メトリクスを更新
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.connections_failed += 1;
                }
                counter!("nexusshell_remote_connections_failed").increment(1);
                
                Err(e)
            }
        }
    }

    /// リモートコマンドを実行します
    pub async fn execute_command(
        &self,
        connection_id: &str,
        command: &str,
    ) -> Result<CommandResult, RemoteExecutorError> {
        // コマンド実行IDを生成
        let execution_id = Uuid::new_v4().to_string();
        
        // 接続を取得
        let connection = match self.connections.get(connection_id) {
            Some(conn) => conn,
            None => return Err(RemoteExecutorError::ConnectionNotFound(connection_id.to_string())),
        };
        
        // コマンド実行情報を登録
        let execution = CommandExecution {
            id: execution_id.clone(),
            host: connection.host().to_string(),
            command: command.to_string(),
            start_time: Instant::now(),
            end_time: None,
            state: CommandState::Running,
        };
        
        self.active_commands.insert(execution_id.clone(), execution);
        
        // メトリクスを更新
        {
            let mut metrics = self.metrics.write().await;
            metrics.commands_executed += 1;
        }
        counter!("nexusshell_remote_commands_total").increment(1);
        
        // コマンドを実行
        debug!("リモートコマンドを実行します: {} - {}", connection_id, command);
        
        let config = connection.config();
        let command_retries = config.command_retries();
        let mut last_error = None;
        
        // 再試行ループ
        for attempt in 1..=command_retries + 1 {
            // 接続が生きているか確認
            if !connection.is_connected().await {
                // 再接続を試みる
                debug!("接続が切断されています。再接続を試みます: {}", connection_id);
                if let Err(e) = connection.reconnect().await {
                    let err = RemoteExecutorError::ConnectionClosed(
                        format!("接続が切断され、再接続に失敗しました: {}", e)
                    );
                    self.update_command_state(&execution_id, CommandState::Failed(err.to_string()));
                    return Err(err);
                }
            }
            
            let result = connection.execute_command(command).await;
            
            match result {
                Ok(cmd_result) => {
                    // 成功
                    let execution_time = Instant::now().duration_since(
                        self.active_commands.get(&execution_id).unwrap().start_time
                    );
                    
                    // メトリクスを更新
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.commands_succeeded += 1;
                        
                        // 平均実行時間を更新
                        let exec_time_ms = execution_time.as_millis() as f64;
                        metrics.avg_command_execution_time_ms = (
                            metrics.avg_command_execution_time_ms * metrics.command_execution_samples as f64
                            + exec_time_ms
                        ) / (metrics.command_execution_samples + 1) as f64;
                        metrics.command_execution_samples += 1;
                    }
                    
                    // Prometheusメトリクス更新
                    counter!("nexusshell_remote_commands_succeeded").increment(1);
                    histogram!("nexusshell_remote_command_execution_time_ms").record(execution_time.as_millis() as f64);
                    
                    // コマンド実行状態を更新
                    self.update_command_state(&execution_id, CommandState::Succeeded);
                    
                    debug!("リモートコマンドの実行に成功しました: {} - {}", connection_id, command);
                    return Ok(cmd_result);
                },
                Err(e) => {
                    // エラー
                    if attempt <= command_retries {
                        warn!("リモートコマンドの実行に失敗しました。再試行します ({}/{}): {} - {} - {}",
                              attempt, command_retries, connection_id, command, e);
                        
                        // 再試行前に少し待機
                        tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                        
                        last_error = Some(e);
                        continue;
                    }
                    
                    error!("リモートコマンドの実行に失敗しました: {} - {} - {}", connection_id, command, e);
                    
                    // メトリクスを更新
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.commands_failed += 1;
                    }
                    counter!("nexusshell_remote_commands_failed").increment(1);
                    
                    // コマンド実行状態を更新
                    self.update_command_state(&execution_id, CommandState::Failed(e.to_string()));
                    
                    return Err(e);
                }
            }
        }
        
        // ここに到達するのは再試行が失敗した場合のみ
        let error = last_error.unwrap_or_else(|| 
            RemoteExecutorError::CommandExecutionFailed("不明なエラー".to_string())
        );
        
        self.update_command_state(&execution_id, CommandState::Failed(error.to_string()));
        Err(error)
    }
    
    /// 複数のホストで同じコマンドを実行します
    pub async fn execute_command_on_hosts(
        &self,
        hosts: &[&str],
        username: &str,
        auth_method: AuthMethod,
        command: &str,
    ) -> HashMap<String, Result<CommandResult, RemoteExecutorError>> {
        let mut results = HashMap::new();
        let mut futures = Vec::new();
        
        // 各ホストへの接続と実行をセットアップ
        for host in hosts {
            let host_str = host.to_string();
            let username_str = username.to_string();
            let auth_clone = auth_method.clone();
            let command_str = command.to_string();
            let executor = self.clone();
            
            let future = async move {
                let connection_result = executor.connect(&host_str, &username_str, auth_clone).await;
                
                match connection_result {
                    Ok(connection_id) => {
                        let cmd_result = executor.execute_command(&connection_id, &command_str).await;
                        (host_str, cmd_result)
                    },
                    Err(e) => {
                        (host_str, Err(e))
                    }
                }
            };
            
            futures.push(future);
        }
        
        // 並列実行して結果を待機
        let results_vec = future::join_all(futures).await;
        
        // 結果をマップに格納
        for (host, result) in results_vec {
            results.insert(host, result);
        }
        
        results
    }
    
    /// コマンド実行状態を更新します
    fn update_command_state(&self, execution_id: &str, state: CommandState) {
        if let Some(mut exec) = self.active_commands.get_mut(execution_id) {
            if state != CommandState::Running {
                exec.end_time = Some(Instant::now());
            }
            exec.state = state;
        }
    }
    
    /// アクティブなコマンド実行をキャンセルします
    pub async fn cancel_command(&self, execution_id: &str) -> Result<(), RemoteExecutorError> {
        // 現在は直接サポートしていないので、状態だけ更新する
        if let Some(mut exec) = self.active_commands.get_mut(execution_id) {
            exec.state = CommandState::Cancelled;
            exec.end_time = Some(Instant::now());
            
            debug!("コマンド実行をキャンセルしました: {}", execution_id);
            Ok(())
        } else {
            Err(RemoteExecutorError::Other(format!("実行IDが見つかりません: {}", execution_id)))
        }
    }

    /// リモート接続を切断します
    pub async fn disconnect(&self, connection_id: &str) -> Result<(), RemoteExecutorError> {
        // 接続を取得して削除
        if let Some((_, mut connection)) = self.connections.remove(connection_id) {
            // 切断処理
            debug!("リモート接続を切断します: {}", connection_id);
            connection.disconnect().await?;
            
            // プールに返却
            self.connection_pool.release(connection).await;
            
            // メトリクスを更新
            {
                let mut metrics = self.metrics.write().await;
                metrics.active_connections = self.connections.len() as u64;
            }
            gauge!("nexusshell_remote_active_connections").set(self.connections.len() as f64);
            
            Ok(())
        } else {
            Err(RemoteExecutorError::ConnectionNotFound(connection_id.to_string()))
        }
    }

    /// 接続が存在するかチェックします
    pub fn has_connection(&self, connection_id: &str) -> bool {
        self.connections.contains_key(connection_id)
    }

    /// 接続のリストを取得します
    pub fn list_connections(&self) -> Vec<String> {
        self.connections.iter().map(|entry| entry.key().clone()).collect()
    }

    /// デフォルト設定を取得します
    pub async fn default_config(&self) -> RemoteConfig {
        self.default_config.read().await.clone()
    }

    /// デフォルト設定を設定します
    pub async fn set_default_config(&self, config: RemoteConfig) {
        let mut default_config = self.default_config.write().await;
        *default_config = config;
    }
    
    /// メトリクスを取得します
    pub async fn metrics(&self) -> RemoteExecutorMetrics {
        self.metrics.read().await.clone()
    }
    
    /// ファイルをリモートホストにアップロードします
    pub async fn upload_file(
        &self,
        connection_id: &str,
        local_path: &std::path::Path,
        remote_path: &str,
    ) -> Result<(), RemoteExecutorError> {
        // 接続を取得
        let connection = match self.connections.get(connection_id) {
            Some(conn) => conn,
            None => return Err(RemoteExecutorError::ConnectionNotFound(connection_id.to_string())),
        };
        
        // ファイルアップロード
        debug!("ファイルをアップロードします: {} -> {}", local_path.display(), remote_path);
        let result = connection.upload_file(local_path, remote_path).await;
        
        // メトリクスを更新
        if result.is_ok() {
            if let Ok(metadata) = std::fs::metadata(local_path) {
                let file_size = metadata.len();
                
                let mut metrics = self.metrics.write().await;
                metrics.bytes_uploaded += file_size;
                
                // Prometheusメトリクス更新
                counter!("nexusshell_remote_bytes_uploaded").increment(file_size as f64);
            }
        }
        
        result
    }
    
    /// ファイルをリモートホストからダウンロードします
    pub async fn download_file(
        &self,
        connection_id: &str,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> Result<(), RemoteExecutorError> {
        // 接続を取得
        let connection = match self.connections.get(connection_id) {
            Some(conn) => conn,
            None => return Err(RemoteExecutorError::ConnectionNotFound(connection_id.to_string())),
        };
        
        // ファイルダウンロード
        debug!("ファイルをダウンロードします: {} -> {}", remote_path, local_path.display());
        let result = connection.download_file(remote_path, local_path).await;
        
        // メトリクスを更新
        if result.is_ok() {
            if let Ok(metadata) = std::fs::metadata(local_path) {
                let file_size = metadata.len();
                
                let mut metrics = self.metrics.write().await;
                metrics.bytes_downloaded += file_size;
                
                // Prometheusメトリクス更新
                counter!("nexusshell_remote_bytes_downloaded").increment(file_size as f64);
            }
        }
        
        result
    }
    
    /// すべての接続を切断します
    pub async fn disconnect_all(&self) -> Result<(), RemoteExecutorError> {
        debug!("すべての接続を切断します");
        
        let connection_ids: Vec<String> = self.connections.iter()
            .map(|entry| entry.key().clone())
            .collect();
        
        for connection_id in connection_ids {
            let _ = self.disconnect(&connection_id).await;
        }
        
        // 接続プールもクリア
        self.connection_pool.clear_all().await;
        
        Ok(())
    }
    
    /// 接続の健全性チェックを実行します
    pub async fn check_connections_health(&self) -> HashMap<String, bool> {
        debug!("すべての接続の健全性をチェックします");
        
        let mut health_status = HashMap::new();
        
        for entry in self.connections.iter() {
            let connection_id = entry.key().clone();
            let connection = entry.value();
            
            let is_healthy = connection.is_connected().await;
            health_status.insert(connection_id, is_healthy);
        }
        
        health_status
    }
    
    /// グローバル設定を取得します
    pub async fn global_config() -> GlobalExecutorConfig {
        GLOBAL_CONFIG.read().await.clone()
    }
    
    /// グローバル設定を更新します
    pub async fn update_global_config(config: GlobalExecutorConfig) {
        let mut global_config = GLOBAL_CONFIG.write().await;
        *global_config = config;
    }
}

impl Default for RemoteExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RemoteExecutor {
    fn clone(&self) -> Self {
        Self {
            connections: DashMap::new(), // 新しい接続マップを作成
            active_commands: DashMap::new(),
            default_config: Arc::clone(&self.default_config),
            connection_limiter: Arc::clone(&self.connection_limiter),
            connection_pool: ConnectionPool::new(
                self.connection_pool.enabled,
                self.connection_pool.max_size,
            ),
            metrics: Arc::clone(&self.metrics),
            health_checker: Arc::clone(&self.health_checker),
        }
    }
}

/// 認証方法
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// パスワード認証
    Password(String),
    /// 公開鍵認証
    PublicKey(String),
    /// SSH Agent認証
    SshAgent,
    /// ホストベース認証
    HostBased(String),
    /// キーボードインタラクティブ認証
    KeyboardInteractive,
    /// Kerberos認証（GSSAPi）
    Kerberos,
}

/// コマンド実行結果
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// 終了コード
    pub exit_code: i32,
    /// 標準出力
    pub stdout: String,
    /// 標準エラー出力
    pub stderr: String,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
} 