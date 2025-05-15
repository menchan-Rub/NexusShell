mod error;
mod config;
mod policy;
mod container;

pub use error::SandboxError;
pub use config::SandboxConfig;
pub use policy::SandboxPolicy;
pub use container::Container;

use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::RwLock;
use log::{debug, info, error, warn};

/// サンドボックス実行環境
/// 安全にコードを実行するための隔離環境を提供します
pub struct Sandbox {
    /// 現在のコンテナ
    containers: DashMap<String, Container>,
    /// デフォルト設定
    default_config: Arc<RwLock<SandboxConfig>>,
    /// デフォルトポリシー
    default_policy: Arc<RwLock<SandboxPolicy>>,
}

impl Sandbox {
    /// 新しいサンドボックスを作成します
    pub fn new() -> Self {
        Self {
            containers: DashMap::new(),
            default_config: Arc::new(RwLock::new(SandboxConfig::default())),
            default_policy: Arc::new(RwLock::new(SandboxPolicy::default())),
        }
    }

    /// 新しいコンテナを作成します
    pub async fn create_container(&self, name: &str) -> Result<String, SandboxError> {
        // 既存のコンテナをチェック
        if self.containers.contains_key(name) {
            return Err(SandboxError::ContainerAlreadyExists(name.to_string()));
        }

        // デフォルト設定とポリシーを取得
        let config = self.default_config.read().await.clone();
        let policy = self.default_policy.read().await.clone();
        
        debug!("新しいサンドボックスコンテナを作成します: {}", name);
        
        // コンテナを作成
        let container = Container::new(name, config, policy);
        let container_id = container.id().to_string();
        
        // コンテナを初期化
        match container.init().await {
            Ok(_) => {
                info!("サンドボックスコンテナを作成しました: {} (ID: {})", name, container_id);
                self.containers.insert(container_id.clone(), container);
                Ok(container_id)
            }
            Err(e) => {
                error!("サンドボックスコンテナの作成に失敗しました: {} - {}", name, e);
                Err(e)
            }
        }
    }

    /// コンテナ内でコマンドを実行します
    pub async fn execute_command(
        &self,
        container_id: &str,
        command: &str,
    ) -> Result<ExecutionResult, SandboxError> {
        // コンテナを取得
        let container = match self.containers.get(container_id) {
            Some(container) => container,
            None => return Err(SandboxError::ContainerNotFound(container_id.to_string())),
        };
        
        debug!("サンドボックスコンテナでコマンドを実行します: {} - {}", container_id, command);
        
        // コマンドを実行
        container.execute(command).await
    }

    /// コンテナに設定を適用します
    pub async fn apply_config(
        &self,
        container_id: &str,
        config: SandboxConfig,
    ) -> Result<(), SandboxError> {
        // コンテナを取得
        let container = match self.containers.get(container_id) {
            Some(container) => container,
            None => return Err(SandboxError::ContainerNotFound(container_id.to_string())),
        };
        
        debug!("サンドボックスコンテナに設定を適用します: {}", container_id);
        
        // 設定を適用
        container.apply_config(config).await
    }

    /// コンテナにポリシーを適用します
    pub async fn apply_policy(
        &self,
        container_id: &str,
        policy: SandboxPolicy,
    ) -> Result<(), SandboxError> {
        // コンテナを取得
        let container = match self.containers.get(container_id) {
            Some(container) => container,
            None => return Err(SandboxError::ContainerNotFound(container_id.to_string())),
        };
        
        debug!("サンドボックスコンテナにポリシーを適用します: {}", container_id);
        
        // ポリシーを適用
        container.apply_policy(policy).await
    }

    /// コンテナを破棄します
    pub async fn destroy_container(&self, container_id: &str) -> Result<(), SandboxError> {
        // コンテナを取得して削除
        if let Some((_, container)) = self.containers.remove(container_id) {
            debug!("サンドボックスコンテナを破棄します: {}", container_id);
            
            // コンテナを破棄
            match container.destroy().await {
                Ok(_) => {
                    info!("サンドボックスコンテナを破棄しました: {}", container_id);
                    Ok(())
                }
                Err(e) => {
                    warn!("サンドボックスコンテナの破棄に失敗しました: {} - {}", container_id, e);
                    Err(e)
                }
            }
        } else {
            Err(SandboxError::ContainerNotFound(container_id.to_string()))
        }
    }

    /// デフォルト設定を取得します
    pub async fn default_config(&self) -> SandboxConfig {
        self.default_config.read().await.clone()
    }

    /// デフォルト設定を設定します
    pub async fn set_default_config(&self, config: SandboxConfig) {
        let mut default_config = self.default_config.write().await;
        *default_config = config;
    }

    /// デフォルトポリシーを取得します
    pub async fn default_policy(&self) -> SandboxPolicy {
        self.default_policy.read().await.clone()
    }

    /// デフォルトポリシーを設定します
    pub async fn set_default_policy(&self, policy: SandboxPolicy) {
        let mut default_policy = self.default_policy.write().await;
        *default_policy = policy;
    }

    /// 全てのコンテナを一覧表示します
    pub fn list_containers(&self) -> Vec<String> {
        self.containers.iter().map(|entry| entry.key().clone()).collect()
    }
    
    /// 特定のコンテナが存在するか確認します
    pub fn container_exists(&self, container_id: &str) -> bool {
        self.containers.contains_key(container_id)
    }
    
    /// コンテナの情報を取得します
    pub fn get_container_info(&self, container_id: &str) -> Option<ContainerInfo> {
        self.containers.get(container_id).map(|container| {
            ContainerInfo {
                id: container.id().to_string(),
                name: container.name().to_string(),
            }
        })
    }
    
    /// 全てのコンテナを破棄します
    pub async fn destroy_all_containers(&self) -> Result<(), SandboxError> {
        let container_ids: Vec<String> = self.list_containers();
        
        let mut errors = Vec::new();
        for id in container_ids {
            if let Err(e) = self.destroy_container(&id).await {
                errors.push(format!("コンテナ {} の破棄に失敗: {}", id, e));
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(SandboxError::MultipleErrors(errors.join("; ")))
        }
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// コンテナ情報
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// コンテナID
    pub id: String,
    /// コンテナ名
    pub name: String,
}

/// 実行結果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// 終了コード
    pub exit_code: i32,
    /// 標準出力
    pub stdout: String,
    /// 標準エラー出力
    pub stderr: String,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
    /// メモリ使用量（バイト）
    pub memory_usage_bytes: u64,
    /// CPU使用率（パーセント）
    pub cpu_usage_percent: f64,
    /// プロセスID
    pub pid: Option<u32>,
    /// シグナルで終了したかどうか
    pub signaled: bool,
    /// 送信されたシグナル（シグナルで終了した場合）
    pub signal: Option<i32>,
    /// コマンド文字列
    pub command: String,
    /// 開始時刻（UNIXタイムスタンプ）
    pub start_time: u64,
    /// 終了時刻（UNIXタイムスタンプ）
    pub end_time: u64,
}

impl ExecutionResult {
    /// 新しい実行結果を作成します
    pub fn new(command: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        Self {
            exit_code: -1,
            stdout: String::new(),
            stderr: String::new(),
            execution_time_ms: 0,
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
            pid: None,
            signaled: false,
            signal: None,
            command: command.to_string(),
            start_time: now,
            end_time: now,
        }
    }
    
    /// 実行は成功したか
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
    
    /// 標準出力と標準エラー出力を結合
    pub fn combined_output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
    
    /// 実行結果の概要を取得
    pub fn summary(&self) -> String {
        format!(
            "終了コード: {}, 実行時間: {}ms, メモリ使用量: {}バイト, CPU使用率: {:.2}%",
            self.exit_code,
            self.execution_time_ms,
            self.memory_usage_bytes,
            self.cpu_usage_percent
        )
    }
    
    /// 実行がシグナルで終了したかどうか
    pub fn was_signaled(&self) -> bool {
        self.signaled
    }
    
    /// 開始時刻と終了時刻から実行時間を計算
    pub fn calculate_execution_time(&mut self) {
        if self.end_time > self.start_time {
            self.execution_time_ms = ((self.end_time - self.start_time) * 1000) as u64;
        }
    }
} 