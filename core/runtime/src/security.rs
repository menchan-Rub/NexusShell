/*!
# セキュリティモジュール

シェルのセキュリティ機能を提供します。権限管理、
サンドボックス、セキュリティポリシーなどを実装します。
*/

use anyhow::{Result, anyhow, Context};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{debug, error, info, warn};

/// セキュリティ権限の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    /// ファイル読み取り
    FileRead(PathBuf),
    /// ファイル書き込み
    FileWrite(PathBuf),
    /// ファイル実行
    FileExecute(PathBuf),
    /// コマンド実行
    CommandExecute(String),
    /// ネットワーク接続
    NetworkConnect { host: String, port: u16 },
    /// システム情報参照
    SystemInfoRead,
    /// プロセス生成
    ProcessCreate,
    /// プロセス制御
    ProcessControl,
    /// 環境変数変更
    EnvModify,
    /// 特権操作
    Privileged,
}

/// セキュリティポリシー
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    /// 許可された権限
    allowed: HashSet<Permission>,
    /// 拒否された権限
    denied: HashSet<Permission>,
    /// デフォルトポリシー（許可または拒否）
    default_allow: bool,
}

impl SecurityPolicy {
    /// デフォルトで許可するポリシーを作成
    pub fn allow_by_default() -> Self {
        Self {
            allowed: HashSet::new(),
            denied: HashSet::new(),
            default_allow: true,
        }
    }
    
    /// デフォルトで拒否するポリシーを作成
    pub fn deny_by_default() -> Self {
        Self {
            allowed: HashSet::new(),
            denied: HashSet::new(),
            default_allow: false,
        }
    }
    
    /// 権限を許可
    pub fn allow(&mut self, permission: Permission) {
        self.denied.remove(&permission);
        self.allowed.insert(permission);
    }
    
    /// 権限を拒否
    pub fn deny(&mut self, permission: Permission) {
        self.allowed.remove(&permission);
        self.denied.insert(permission);
    }
    
    /// 権限チェック
    pub fn check(&self, permission: &Permission) -> bool {
        // 明示的に拒否されている場合
        if self.denied.contains(permission) {
            return false;
        }
        
        // 明示的に許可されている場合
        if self.allowed.contains(permission) {
            return true;
        }
        
        // パスベースの権限の場合、親ディレクトリに対する許可も確認
        match permission {
            Permission::FileRead(path) | 
            Permission::FileWrite(path) | 
            Permission::FileExecute(path) => {
                // 親ディレクトリに対する許可を確認
                let mut current = path.as_path();
                while let Some(parent) = current.parent() {
                    let parent_perm = match permission {
                        Permission::FileRead(_) => Permission::FileRead(parent.to_path_buf()),
                        Permission::FileWrite(_) => Permission::FileWrite(parent.to_path_buf()),
                        Permission::FileExecute(_) => Permission::FileExecute(parent.to_path_buf()),
                        _ => unreachable!(),
                    };
                    
                    if self.allowed.contains(&parent_perm) {
                        return true;
                    }
                    
                    current = parent;
                }
            }
            _ => {}
        }
        
        // デフォルトポリシーを返す
        self.default_allow
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        // デフォルトでは安全のため、拒否ポリシーをベースにする
        Self::deny_by_default()
    }
}

/// セキュリティマネージャー
pub struct SecurityManager {
    /// グローバルポリシー
    global_policy: Arc<RwLock<SecurityPolicy>>,
    /// コマンド別ポリシー
    command_policies: Arc<RwLock<HashMap<String, SecurityPolicy>>>,
    /// 権限チェックを無効化するフラグ
    disable_checks: Arc<RwLock<bool>>,
}

impl SecurityManager {
    /// 新しいセキュリティマネージャーを作成
    pub fn new() -> Self {
        Self {
            global_policy: Arc::new(RwLock::new(SecurityPolicy::default())),
            command_policies: Arc::new(RwLock::new(HashMap::new())),
            disable_checks: Arc::new(RwLock::new(false)),
        }
    }
    
    /// 権限チェックが有効かどうか確認
    pub async fn is_enabled(&self) -> bool {
        !(*self.disable_checks.read().await)
    }
    
    /// 権限チェックを有効化
    pub async fn enable(&self) {
        let mut disable = self.disable_checks.write().await;
        *disable = false;
    }
    
    /// 権限チェックを無効化（危険: デバッグ用）
    pub async fn disable(&self) {
        let mut disable = self.disable_checks.write().await;
        *disable = true;
        warn!("セキュリティチェックが無効化されました（危険）");
    }
    
    /// コマンド実行の権限を確認
    pub async fn check_command_permission(&self, command: &str) -> Result<bool> {
        // チェックが無効化されている場合は常に許可
        if !self.is_enabled().await {
            return Ok(true);
        }
        
        let perm = Permission::CommandExecute(command.to_string());
        
        // コマンド固有のポリシーを確認
        let command_policies = self.command_policies.read().await;
        if let Some(policy) = command_policies.get(command) {
            if policy.check(&perm) {
                debug!("コマンド {} の実行が許可されました (コマンド固有ポリシー)", command);
                return Ok(true);
            } else {
                debug!("コマンド {} の実行が拒否されました (コマンド固有ポリシー)", command);
                return Ok(false);
            }
        }
        
        // グローバルポリシーを確認
        let global_policy = self.global_policy.read().await;
        let allowed = global_policy.check(&perm);
        
        if allowed {
            debug!("コマンド {} の実行が許可されました (グローバルポリシー)", command);
        } else {
            debug!("コマンド {} の実行が拒否されました (グローバルポリシー)", command);
        }
        
        Ok(allowed)
    }
    
    /// ファイルアクセスの権限を確認
    pub async fn check_file_permission(&self, path: &Path, read: bool, write: bool, execute: bool) -> Result<bool> {
        // チェックが無効化されている場合は常に許可
        if !self.is_enabled().await {
            return Ok(true);
        }
        
        let global_policy = self.global_policy.read().await;
        let mut allowed = true;
        
        // パスを正規化して絶対パスに変換
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        // 読み取り権限
        if read {
            let perm = Permission::FileRead(abs_path.clone());
            allowed = allowed && global_policy.check(&perm);
            if !allowed {
                return Ok(false);
            }
        }
        
        // 書き込み権限
        if write {
            let perm = Permission::FileWrite(abs_path.clone());
            allowed = allowed && global_policy.check(&perm);
            if !allowed {
                return Ok(false);
            }
        }
        
        // 実行権限
        if execute {
            let perm = Permission::FileExecute(abs_path);
            allowed = allowed && global_policy.check(&perm);
        }
        
        Ok(allowed)
    }
    
    /// ネットワークアクセスの権限を確認
    pub async fn check_network_permission(&self, host: &str, port: u16) -> Result<bool> {
        // チェックが無効化されている場合は常に許可
        if !self.is_enabled().await {
            return Ok(true);
        }
        
        let perm = Permission::NetworkConnect { 
            host: host.to_string(), 
            port 
        };
        
        let global_policy = self.global_policy.read().await;
        let allowed = global_policy.check(&perm);
        
        if allowed {
            debug!("ネットワーク接続 {}:{} が許可されました", host, port);
        } else {
            debug!("ネットワーク接続 {}:{} が拒否されました", host, port);
        }
        
        Ok(allowed)
    }
    
    /// グローバルポリシーを設定
    pub async fn set_global_policy(&self, policy: SecurityPolicy) {
        let mut global = self.global_policy.write().await;
        *global = policy;
        info!("グローバルセキュリティポリシーが更新されました");
    }
    
    /// コマンド固有のポリシーを設定
    pub async fn set_command_policy(&self, command: &str, policy: SecurityPolicy) {
        let mut policies = self.command_policies.write().await;
        policies.insert(command.to_string(), policy);
        info!("コマンド {} のセキュリティポリシーが更新されました", command);
    }
    
    /// コマンド固有のポリシーを削除
    pub async fn remove_command_policy(&self, command: &str) -> bool {
        let mut policies = self.command_policies.write().await;
        let removed = policies.remove(command).is_some();
        if removed {
            info!("コマンド {} のセキュリティポリシーが削除されました", command);
        }
        removed
    }
    
    /// 特定のパスに対する読み取り権限を許可
    pub async fn allow_file_read(&self, path: &Path) -> Result<()> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        let mut global = self.global_policy.write().await;
        global.allow(Permission::FileRead(abs_path));
        Ok(())
    }
    
    /// 特定のパスに対する書き込み権限を許可
    pub async fn allow_file_write(&self, path: &Path) -> Result<()> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        let mut global = self.global_policy.write().await;
        global.allow(Permission::FileWrite(abs_path));
        Ok(())
    }
    
    /// 特定のパスに対する実行権限を許可
    pub async fn allow_file_execute(&self, path: &Path) -> Result<()> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        let mut global = self.global_policy.write().await;
        global.allow(Permission::FileExecute(abs_path));
        Ok(())
    }
    
    /// 特定のコマンドの実行を許可
    pub async fn allow_command(&self, command: &str) -> Result<()> {
        let mut global = self.global_policy.write().await;
        global.allow(Permission::CommandExecute(command.to_string()));
        Ok(())
    }
    
    /// 特定のホスト:ポートへのネットワーク接続を許可
    pub async fn allow_network(&self, host: &str, port: u16) -> Result<()> {
        let mut global = self.global_policy.write().await;
        global.allow(Permission::NetworkConnect { 
            host: host.to_string(), 
            port 
        });
        Ok(())
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
} 