/*!
# 最先端セキュリティモジュール

シェルのセキュリティ機能を提供します。権限管理、サンドボックス、
セキュリティポリシーなどを堅牢に実装し、ゼロトラスト原則に基づいた
高度な保護機能を提供します。
*/

use anyhow::{Result, anyhow, Context};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use tracing::{debug, error, info, warn, trace};
use dashmap::DashMap;
use async_trait::async_trait;
use chrono::{Utc, DateTime};

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
    /// プラグイン読み込み
    PluginLoad(PathBuf),
    /// デバイスアクセス
    DeviceAccess(String),
    /// IPC通信
    IPCCommunication(String),
}

impl Permission {
    /// 権限が階層的に別の権限を含むかどうか確認
    pub fn contains(&self, other: &Permission) -> bool {
        match (self, other) {
            // パスベースの階層的権限チェック
            (Permission::FileRead(base), Permission::FileRead(path)) |
            (Permission::FileWrite(base), Permission::FileWrite(path)) |
            (Permission::FileExecute(base), Permission::FileExecute(path)) |
            (Permission::PluginLoad(base), Permission::PluginLoad(path)) => {
                path.starts_with(base)
            },
            
            // ホストベースの階層的権限チェック
            (
                Permission::NetworkConnect { host: base_host, port: base_port },
                Permission::NetworkConnect { host: check_host, port: check_port }
            ) => {
                // ワイルドカードホスト
                if base_host == "*" {
                    *base_port == 0 || *base_port == *check_port
                } 
                // サブドメインマッチング
                else if base_host.starts_with("*.") {
                    let domain_suffix = &base_host[2..];
                    (check_host.ends_with(domain_suffix) || check_host == domain_suffix) &&
                    (*base_port == 0 || *base_port == *check_port)
                } 
                // 完全一致
                else {
                    base_host == check_host && (*base_port == 0 || *base_port == *check_port)
                }
            },
            
            // 同じ種類の権限同士は完全一致のみ
            (a, b) => a == b,
        }
    }
    
    /// 権限の文字列表現を取得
    pub fn to_string(&self) -> String {
        match self {
            Permission::FileRead(path) => format!("file:read:{}", path.display()),
            Permission::FileWrite(path) => format!("file:write:{}", path.display()),
            Permission::FileExecute(path) => format!("file:exec:{}", path.display()),
            Permission::CommandExecute(cmd) => format!("cmd:exec:{}", cmd),
            Permission::NetworkConnect { host, port } => format!("net:connect:{}:{}", host, port),
            Permission::SystemInfoRead => "system:info:read".to_string(),
            Permission::ProcessCreate => "process:create".to_string(),
            Permission::ProcessControl => "process:control".to_string(),
            Permission::EnvModify => "env:modify".to_string(),
            Permission::Privileged => "system:privileged".to_string(),
            Permission::PluginLoad(path) => format!("plugin:load:{}", path.display()),
            Permission::DeviceAccess(dev) => format!("device:access:{}", dev),
            Permission::IPCCommunication(channel) => format!("ipc:comm:{}", channel),
        }
    }
    
    /// 文字列から権限を解析
    pub fn from_string(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 {
            return Err(anyhow!("不正な権限文字列: {}", s));
        }
        
        match (parts[0], parts[1]) {
            ("file", "read") => {
                if parts.len() < 3 {
                    Err(anyhow!("パスが指定されていません: {}", s))
                } else {
                    Ok(Permission::FileRead(PathBuf::from(parts[2])))
                }
            },
            ("file", "write") => {
                if parts.len() < 3 {
                    Err(anyhow!("パスが指定されていません: {}", s))
                } else {
                    Ok(Permission::FileWrite(PathBuf::from(parts[2])))
                }
            },
            ("file", "exec") => {
                if parts.len() < 3 {
                    Err(anyhow!("パスが指定されていません: {}", s))
                } else {
                    Ok(Permission::FileExecute(PathBuf::from(parts[2])))
                }
            },
            ("cmd", "exec") => {
                if parts.len() < 3 {
                    Err(anyhow!("コマンドが指定されていません: {}", s))
                } else {
                    Ok(Permission::CommandExecute(parts[2].to_string()))
                }
            },
            ("net", "connect") => {
                if parts.len() < 3 {
                    Err(anyhow!("ホストが指定されていません: {}", s))
                } else {
                    let host_port: Vec<&str> = parts[2].split(':').collect();
                    let host = host_port[0].to_string();
                    let port = if host_port.len() > 1 {
                        host_port[1].parse().unwrap_or(0)
                    } else {
                        0
                    };
                    Ok(Permission::NetworkConnect { host, port })
                }
            },
            ("system", "info") => Ok(Permission::SystemInfoRead),
            ("process", "create") => Ok(Permission::ProcessCreate),
            ("process", "control") => Ok(Permission::ProcessControl),
            ("env", "modify") => Ok(Permission::EnvModify),
            ("system", "privileged") => Ok(Permission::Privileged),
            ("plugin", "load") => {
                if parts.len() < 3 {
                    Err(anyhow!("パスが指定されていません: {}", s))
                } else {
                    Ok(Permission::PluginLoad(PathBuf::from(parts[2])))
                }
            },
            ("device", "access") => {
                if parts.len() < 3 {
                    Err(anyhow!("デバイスが指定されていません: {}", s))
                } else {
                    Ok(Permission::DeviceAccess(parts[2].to_string()))
                }
            },
            ("ipc", "comm") => {
                if parts.len() < 3 {
                    Err(anyhow!("チャンネルが指定されていません: {}", s))
                } else {
                    Ok(Permission::IPCCommunication(parts[2].to_string()))
                }
            },
            _ => Err(anyhow!("不明な権限タイプ: {}:{}", parts[0], parts[1])),
        }
    }
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
    /// 最終更新時刻
    last_modified: Instant,
    /// ポリシー名
    name: String,
    /// ポリシーバージョン
    version: u32,
}

impl SecurityPolicy {
    /// デフォルトで許可するポリシーを作成
    pub fn allow_by_default(name: &str) -> Self {
        Self {
            allowed: HashSet::new(),
            denied: HashSet::new(),
            default_allow: true,
            last_modified: Instant::now(),
            name: name.to_string(),
            version: 1,
        }
    }
    
    /// デフォルトで拒否するポリシーを作成
    pub fn deny_by_default(name: &str) -> Self {
        Self {
            allowed: HashSet::new(),
            denied: HashSet::new(),
            default_allow: false,
            last_modified: Instant::now(),
            name: name.to_string(),
            version: 1,
        }
    }
    
    /// 権限を許可
    pub fn allow(&mut self, permission: Permission) {
        self.denied.remove(&permission);
        self.allowed.insert(permission);
        self.last_modified = Instant::now();
        self.version += 1;
    }
    
    /// 権限を拒否
    pub fn deny(&mut self, permission: Permission) {
        self.allowed.remove(&permission);
        self.denied.insert(permission);
        self.last_modified = Instant::now();
        self.version += 1;
    }

    /// ポリシー名を取得
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// ポリシーバージョンを取得
    pub fn version(&self) -> u32 {
        self.version
    }
    
    /// 最終更新時刻を取得
    pub fn last_modified(&self) -> Instant {
        self.last_modified
    }
    
    /// 権限チェック
    pub fn check(&self, permission: &Permission) -> bool {
        // まず直接のマッチングを試行
        
        // 明示的に拒否されている場合
        if self.denied.contains(permission) {
            trace!("権限 {:?} は明示的に拒否", permission);
            return false;
        }
        
        // 明示的に許可されている場合
        if self.allowed.contains(permission) {
            trace!("権限 {:?} は明示的に許可", permission);
            return true;
        }
        
        // 階層的な権限チェック
        for denied in &self.denied {
            if denied.contains(permission) {
                trace!("権限 {:?} は {:?} によって拒否", permission, denied);
                return false;
            }
        }
        
        for allowed in &self.allowed {
            if allowed.contains(permission) {
                trace!("権限 {:?} は {:?} によって許可", permission, allowed);
                return true;
            }
        }
        
        // デフォルトポリシーを返す
        trace!("権限 {:?} はデフォルトポリシーを使用: {}", permission, self.default_allow);
        self.default_allow
    }
    
    /// ポリシーを別のポリシーとマージ
    pub fn merge(&mut self, other: &SecurityPolicy) {
        // 拒否リストをマージ
        for perm in &other.denied {
            self.deny(perm.clone());
        }
        
        // 許可リストをマージ
        for perm in &other.allowed {
            self.allow(perm.clone());
        }
        
        // バージョンを更新
        self.version += 1;
        self.last_modified = Instant::now();
    }
    
    /// ポリシーをJSONに変換
    pub fn to_json(&self) -> Result<String> {
        let mut map = serde_json::Map::new();
        
        // 基本情報
        map.insert("name".to_string(), serde_json::Value::String(self.name.clone()));
        map.insert("version".to_string(), serde_json::Value::Number(serde_json::Number::from(self.version)));
        map.insert("default_allow".to_string(), serde_json::Value::Bool(self.default_allow));
        
        // 許可リスト
        let allowed: Vec<String> = self.allowed.iter()
            .map(|p| p.to_string())
            .collect();
        map.insert("allowed".to_string(), serde_json::Value::Array(
            allowed.into_iter().map(serde_json::Value::String).collect()
        ));
        
        // 拒否リスト
        let denied: Vec<String> = self.denied.iter()
            .map(|p| p.to_string())
            .collect();
        map.insert("denied".to_string(), serde_json::Value::Array(
            denied.into_iter().map(serde_json::Value::String).collect()
        ));
        
        // JSONに変換
        serde_json::to_string_pretty(&serde_json::Value::Object(map))
            .map_err(|e| anyhow!("JSONへの変換エラー: {}", e))
    }
    
    /// JSONからポリシーを構築
    pub fn from_json(json: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| anyhow!("JSONの解析エラー: {}", e))?;
        
        let obj = value.as_object()
            .ok_or_else(|| anyhow!("JSONがオブジェクトではありません"))?;
        
        // 基本情報を抽出
        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("imported_policy")
            .to_string();
            
        let version = obj.get("version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
            
        let default_allow = obj.get("default_allow")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
            
        let mut policy = if default_allow {
            Self::allow_by_default(&name)
        } else {
            Self::deny_by_default(&name)
        };
        
        policy.version = version;
        
        // 許可リストを処理
        if let Some(allowed) = obj.get("allowed").and_then(|v| v.as_array()) {
            for item in allowed {
                if let Some(perm_str) = item.as_str() {
                    if let Ok(perm) = Permission::from_string(perm_str) {
                        policy.allowed.insert(perm);
                    }
                }
            }
        }
        
        // 拒否リストを処理
        if let Some(denied) = obj.get("denied").and_then(|v| v.as_array()) {
            for item in denied {
                if let Some(perm_str) = item.as_str() {
                    if let Ok(perm) = Permission::from_string(perm_str) {
                        policy.denied.insert(perm);
                    }
                }
            }
        }
        
        Ok(policy)
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        // デフォルトでは安全のため、拒否ポリシーをベースにする
        Self::deny_by_default("default_policy")
    }
}

/// セキュリティイベントリスナー
#[async_trait::async_trait]
pub trait SecurityEventListener: Send + Sync {
    /// 権限チェック時に呼び出される
    async fn on_permission_check(&self, permission: &Permission, allowed: bool);
    
    /// セキュリティポリシーが変更されたときに呼び出される
    async fn on_policy_changed(&self, policy_name: &str);
    
    /// セキュリティ違反が検出されたときに呼び出される
    async fn on_security_violation(&self, details: &SecurityViolationDetails);
}

/// セキュリティ違反の詳細
#[derive(Debug, Clone)]
pub struct SecurityViolationDetails {
    /// 違反の種類
    pub violation_type: SecurityViolationType,
    /// 対象のリソース
    pub resource: String,
    /// 試行された操作
    pub attempted_operation: String,
    /// ソースの識別子（プロセスID、コマンド名など）
    pub source: String,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 追加情報
    pub additional_info: HashMap<String, String>,
}

/// セキュリティ違反の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityViolationType {
    /// 権限不足
    PermissionDenied,
    /// リソース使用量制限を超過
    ResourceLimitExceeded,
    /// 不正なデータアクセス
    InvalidDataAccess,
    /// ポリシー違反
    PolicyViolation,
    /// 不正な操作
    InvalidOperation,
}

/// セキュリティマネージャー
pub struct SecurityManager {
    /// グローバルポリシー
    global_policy: Arc<RwLock<SecurityPolicy>>,
    /// コマンド別ポリシー
    command_policies: Arc<DashMap<String, SecurityPolicy>>,
    /// ドメイン別ポリシー
    domain_policies: Arc<DashMap<String, SecurityPolicy>>,
    /// 権限チェックを無効化するフラグ
    disable_checks: Arc<RwLock<bool>>,
    /// セキュリティリスナー
    listeners: Arc<RwLock<Vec<Arc<dyn SecurityEventListener>>>>,
    /// セキュリティキャッシュ
    permission_cache: Arc<dashmap::DashMap<String, CachedPermission>>,
    /// キャッシュの有効期間
    cache_ttl: Duration,
    /// 監査ログ
    audit_log: Arc<RwLock<Vec<AuditLogEntry>>>,
    /// 監査ログの最大サイズ
    max_audit_log_size: usize,
}

/// キャッシュされた権限
#[derive(Debug, Clone)]
struct CachedPermission {
    /// 権限
    permission: Permission,
    /// 許可/拒否
    allowed: bool,
    /// キャッシュされた時刻
    timestamp: Instant,
    /// ポリシーバージョン（変更検出用）
    policy_version: u32,
}

/// 監査ログエントリ
#[derive(Debug, Clone)]
struct AuditLogEntry {
    /// タイムスタンプ
    timestamp: chrono::DateTime<chrono::Utc>,
    /// イベントタイプ
    event_type: AuditEventType,
    /// 関連するリソース
    resource: String,
    /// 操作結果
    result: AuditResult,
    /// ソース情報
    source: String,
    /// 追加情報
    details: HashMap<String, String>,
}

/// 監査イベントタイプ
#[derive(Debug, Clone, PartialEq, Eq)]
enum AuditEventType {
    /// 権限チェック
    PermissionCheck,
    /// ポリシー変更
    PolicyChange,
    /// セキュリティ違反
    SecurityViolation,
    /// システム操作
    SystemOperation,
}

/// 監査結果
#[derive(Debug, Clone, PartialEq, Eq)]
enum AuditResult {
    /// 許可
    Allowed,
    /// 拒否
    Denied,
    /// 成功
    Success,
    /// 失敗
    Failure,
}

impl std::fmt::Debug for SecurityManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityManager")
            .field("global_policy", &format!("<RwLock<{:?}>>", self.global_policy))
            .field("command_policies", &format!("<DashMap<{} entries>>", self.command_policies.len()))
            .field("domain_policies", &format!("<DashMap<{} entries>>", self.domain_policies.len()))
            .field("disable_checks", &format!("<RwLock<{}>>", self.disable_checks.try_read().map(|f| *f).unwrap_or(false)))
            .field("listeners", &format!("<RwLock<{} listeners>>", self.listeners.try_read().map(|l| l.len()).unwrap_or(0)))
            .field("permission_cache", &format!("<DashMap<{} entries>>", self.permission_cache.len()))
            .field("cache_ttl", &self.cache_ttl)
            .field("audit_log", &format!("<RwLock<{} entries>>", self.audit_log.try_read().map(|l| l.len()).unwrap_or(0)))
            .field("max_audit_log_size", &self.max_audit_log_size)
            .finish()
    }
}

impl SecurityManager {
    /// 新しいセキュリティマネージャーを作成
    pub fn new() -> Self {
        Self {
            global_policy: Arc::new(RwLock::new(SecurityPolicy::default())),
            command_policies: Arc::new(DashMap::new()),
            domain_policies: Arc::new(DashMap::new()),
            disable_checks: Arc::new(RwLock::new(false)),
            listeners: Arc::new(RwLock::new(Vec::new())),
            permission_cache: Arc::new(dashmap::DashMap::new()),
            cache_ttl: Duration::from_secs(30), // デフォルトの30秒キャッシュ
            audit_log: Arc::new(RwLock::new(Vec::new())),
            max_audit_log_size: 10000, // デフォルトの最大ログサイズ
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
        debug!("セキュリティチェックが有効化されました");
        
        // 監査ログにイベントを記録
        self.log_audit_event(
            AuditEventType::SystemOperation,
            "security",
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "enable_security".to_string())])
        ).await;
    }
    
    /// 権限チェックを無効化（危険: デバッグ用）
    pub async fn disable(&self) {
        let mut disable = self.disable_checks.write().await;
        *disable = true;
        warn!("セキュリティチェックが無効化されました（危険）");
        
        // 監査ログにイベントを記録
        self.log_audit_event(
            AuditEventType::SystemOperation,
            "security",
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "disable_security".to_string())])
        ).await;
    }
    
    /// セキュリティリスナーを追加
    pub async fn add_listener(&self, listener: Arc<dyn SecurityEventListener>) {
        let mut listeners = self.listeners.write().await;
        listeners.push(listener);
        debug!("セキュリティリスナーが追加されました");
    }
    
    /// セキュリティリスナーを削除
    pub async fn remove_listener(&self, listener: &Arc<dyn SecurityEventListener>) {
        let mut listeners = self.listeners.write().await;
        
        // ポインタ比較でリスナーを検索し削除
        if let Some(index) = listeners.iter().position(|l| Arc::ptr_eq(l, listener)) {
            listeners.remove(index);
            debug!("セキュリティリスナーが削除されました");
        }
    }
    
    /// キャッシュTTLを設定
    pub fn set_cache_ttl(&mut self, ttl: Duration) {
        self.cache_ttl = ttl;
        debug!("セキュリティキャッシュTTLが更新されました: {:?}", ttl);
    }
    
    /// 監査ログの最大サイズを設定
    pub fn set_max_audit_log_size(&mut self, size: usize) {
        self.max_audit_log_size = size;
        debug!("監査ログの最大サイズが更新されました: {}", size);
    }
    
    /// 権限をキャッシュキーに変換
    fn permission_to_cache_key(permission: &Permission) -> String {
        permission.to_string()
    }
    
    /// 監査ログにイベントを記録
    async fn log_audit_event(
        &self,
        event_type: AuditEventType,
        resource: &str,
        result: AuditResult,
        source: &str,
        details: HashMap<String, String>
    ) {
        let entry = AuditLogEntry {
            timestamp: chrono::Utc::now(),
            event_type,
            resource: resource.to_string(),
            result,
            source: source.to_string(),
            details,
        };
        
        let mut log = self.audit_log.write().await;
        log.push(entry);
        
        // ログサイズが最大値を超えたら古いエントリを削除
        if log.len() > self.max_audit_log_size {
            let excess = log.len() - self.max_audit_log_size;
            log.drain(0..excess);
        }
    }
    
    /// 監査ログを取得
    pub async fn get_audit_log(&self, max_entries: Option<usize>) -> Vec<HashMap<String, String>> {
        let log = self.audit_log.read().await;
        let max = max_entries.unwrap_or(self.max_audit_log_size);
        
        log.iter()
            .rev() // 最新のエントリから
            .take(max)
            .map(|entry| {
                let mut map = HashMap::new();
                map.insert("timestamp".to_string(), entry.timestamp.to_rfc3339());
                map.insert("event_type".to_string(), format!("{:?}", entry.event_type));
                map.insert("resource".to_string(), entry.resource.clone());
                map.insert("result".to_string(), format!("{:?}", entry.result));
                map.insert("source".to_string(), entry.source.clone());
                
                // 追加詳細情報
                for (key, value) in &entry.details {
                    map.insert(format!("detail_{}", key), value.clone());
                }
                
                map
            })
            .collect()
    }
    
    /// セキュリティ違反を報告
    pub async fn report_security_violation(&self, details: SecurityViolationDetails) {
        // リスナーに違反を通知
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_security_violation(&details).await;
        }
        
        // 監査ログに記録
        let mut audit_details = HashMap::new();
        audit_details.insert("violation_type".to_string(), format!("{:?}", details.violation_type));
        audit_details.insert("attempted_operation".to_string(), details.attempted_operation.clone());
        
        for (key, value) in &details.additional_info {
            audit_details.insert(key.clone(), value.clone());
        }
        
        self.log_audit_event(
            AuditEventType::SecurityViolation,
            &details.resource,
            AuditResult::Denied,
            &details.source,
            audit_details
        ).await;
        
        // ログにも記録
        warn!(
            "セキュリティ違反: {:?} - リソース: {}, 操作: {}, ソース: {}",
            details.violation_type, details.resource, details.attempted_operation, details.source
        );
    }
    
    /// 権限チェックの結果をリスナーに通知
    async fn notify_permission_check(&self, permission: &Permission, allowed: bool) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_permission_check(permission, allowed).await;
        }
    }
    
    /// ポリシー変更をリスナーに通知
    async fn notify_policy_changed(&self, policy_name: &str) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_policy_changed(policy_name).await;
        }
    }
    
    /// キャッシュをクリア
    pub async fn clear_cache(&self) {
        self.permission_cache.clear();
        debug!("セキュリティ権限キャッシュがクリアされました");
    }
    
    /// 古いキャッシュエントリを削除
    pub async fn prune_cache(&self) {
        let now = Instant::now();
        let mut expired_keys = Vec::new();
        
        // 期限切れのエントリを収集
        for entry in self.permission_cache.iter() {
            if now.duration_since(entry.timestamp) > self.cache_ttl {
                expired_keys.push(entry.key().clone());
            }
        }
        
        // 期限切れのエントリを削除
        for key in expired_keys {
            self.permission_cache.remove(&key);
        }
    }
    
    /// コマンド実行の権限を確認
    pub async fn check_command_permission(&self, command: &str) -> Result<bool> {
        // チェックが無効化されている場合は常に許可
        if !self.is_enabled().await {
            return Ok(true);
        }
        
        let perm = Permission::CommandExecute(command.to_string());
        
        // キャッシュをチェック
        let cache_key = Self::permission_to_cache_key(&perm);
        if let Some(cached) = self.permission_cache.get(&cache_key) {
            // キャッシュが有効期限内かつポリシーバージョンが同じかを確認
            let global_policy = self.global_policy.read().await;
            if Instant::now().duration_since(cached.timestamp) < self.cache_ttl &&
               cached.policy_version == global_policy.version() {
                trace!("キャッシュヒット: コマンド {} の実行が {} されました", 
                      command, if cached.allowed { "許可" } else { "拒否" });
                
                // 権限チェック結果をリスナーに通知
                self.notify_permission_check(&perm, cached.allowed).await;
                
                // 監査ログに記録
                self.log_audit_event(
                    AuditEventType::PermissionCheck,
                    command,
                    if cached.allowed { AuditResult::Allowed } else { AuditResult::Denied },
                    "command",
                    HashMap::from([("cached".to_string(), "true".to_string())])
                ).await;
                
                return Ok(cached.allowed);
            }
        }
        
        // コマンド固有のポリシーを確認
        if let Some(policy) = self.command_policies.get(command) {
            let allowed = policy.check(&perm);
            
            // キャッシュを更新
            let global_policy = self.global_policy.read().await;
            self.permission_cache.insert(cache_key, CachedPermission {
                permission: perm.clone(),
                allowed,
                timestamp: Instant::now(),
                policy_version: global_policy.version(),
            });
            
            // 権限チェック結果をリスナーに通知
            self.notify_permission_check(&perm, allowed).await;
            
            // 監査ログに記録
            self.log_audit_event(
                AuditEventType::PermissionCheck,
                command,
                if allowed { AuditResult::Allowed } else { AuditResult::Denied },
                "command",
                HashMap::from([("policy".to_string(), format!("command:{}", command))])
            ).await;
            
            debug!("コマンド {} の実行が {} されました (コマンド固有ポリシー)", 
                  command, if allowed { "許可" } else { "拒否" });
            
            return Ok(allowed);
        }
        
        // グローバルポリシーを確認
        let global_policy = self.global_policy.read().await;
        let allowed = global_policy.check(&perm);
        
        // キャッシュを更新
        self.permission_cache.insert(cache_key, CachedPermission {
            permission: perm.clone(),
            allowed,
            timestamp: Instant::now(),
            policy_version: global_policy.version(),
        });
        
        // 権限チェック結果をリスナーに通知
        self.notify_permission_check(&perm, allowed).await;
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PermissionCheck,
            command,
            if allowed { AuditResult::Allowed } else { AuditResult::Denied },
            "command",
            HashMap::from([("policy".to_string(), "global".to_string())])
        ).await;
        
        debug!("コマンド {} の実行が {} されました (グローバルポリシー)", 
              command, if allowed { "許可" } else { "拒否" });
        
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
        
        // 各権限タイプをチェック
        if read {
            let perm = Permission::FileRead(abs_path.clone());
            let cache_key = Self::permission_to_cache_key(&perm);
            
            // キャッシュをチェック
            if let Some(cached) = self.permission_cache.get(&cache_key) {
                if Instant::now().duration_since(cached.timestamp) < self.cache_ttl &&
                   cached.policy_version == global_policy.version() {
                    allowed = allowed && cached.allowed;
                    
                    // キャッシュヒットの監査ログ
                    self.log_audit_event(
                        AuditEventType::PermissionCheck,
                        &abs_path.display().to_string(),
                        if cached.allowed { AuditResult::Allowed } else { AuditResult::Denied },
                        "file:read",
                        HashMap::from([("cached".to_string(), "true".to_string())])
                    ).await;
                    
                    if !cached.allowed {
                        return Ok(false);
                    }
                    continue;
                }
            }
            
            // ポリシーチェック
            let file_read_allowed = global_policy.check(&perm);
            allowed = allowed && file_read_allowed;
            
            // キャッシュ更新
            self.permission_cache.insert(cache_key, CachedPermission {
                permission: perm.clone(),
                allowed: file_read_allowed,
                timestamp: Instant::now(),
                policy_version: global_policy.version(),
            });
            
            // 権限チェック結果をリスナーに通知
            self.notify_permission_check(&perm, file_read_allowed).await;
            
            // 監査ログを記録
            self.log_audit_event(
                AuditEventType::PermissionCheck,
                &abs_path.display().to_string(),
                if file_read_allowed { AuditResult::Allowed } else { AuditResult::Denied },
                "file:read",
                HashMap::new()
            ).await;
            
            if !file_read_allowed {
                debug!("ファイル読み取り権限が拒否されました: {:?}", abs_path);
                return Ok(false);
            }
        }
        
        // 書き込み権限
        if write {
            let perm = Permission::FileWrite(abs_path.clone());
            let cache_key = Self::permission_to_cache_key(&perm);
            
            // キャッシュをチェック
            if let Some(cached) = self.permission_cache.get(&cache_key) {
                if Instant::now().duration_since(cached.timestamp) < self.cache_ttl &&
                   cached.policy_version == global_policy.version() {
                    allowed = allowed && cached.allowed;
                    
                    // キャッシュヒットの監査ログ
                    self.log_audit_event(
                        AuditEventType::PermissionCheck,
                        &abs_path.display().to_string(),
                        if cached.allowed { AuditResult::Allowed } else { AuditResult::Denied },
                        "file:write",
                        HashMap::from([("cached".to_string(), "true".to_string())])
                    ).await;
                    
                    if !cached.allowed {
                        return Ok(false);
                    }
                    continue;
                }
            }
            
            // ポリシーチェック
            let file_write_allowed = global_policy.check(&perm);
            allowed = allowed && file_write_allowed;
            
            // キャッシュ更新
            self.permission_cache.insert(cache_key, CachedPermission {
                permission: perm.clone(),
                allowed: file_write_allowed,
                timestamp: Instant::now(),
                policy_version: global_policy.version(),
            });
            
            // 権限チェック結果をリスナーに通知
            self.notify_permission_check(&perm, file_write_allowed).await;
            
            // 監査ログを記録
            self.log_audit_event(
                AuditEventType::PermissionCheck,
                &abs_path.display().to_string(),
                if file_write_allowed { AuditResult::Allowed } else { AuditResult::Denied },
                "file:write",
                HashMap::new()
            ).await;
            
            if !file_write_allowed {
                debug!("ファイル書き込み権限が拒否されました: {:?}", abs_path);
                return Ok(false);
            }
        }
        
        // 実行権限
        if execute {
            let perm = Permission::FileExecute(abs_path.clone());
            let cache_key = Self::permission_to_cache_key(&perm);
            
            // キャッシュをチェック
            if let Some(cached) = self.permission_cache.get(&cache_key) {
                if Instant::now().duration_since(cached.timestamp) < self.cache_ttl &&
                   cached.policy_version == global_policy.version() {
                    allowed = allowed && cached.allowed;
                    
                    // キャッシュヒットの監査ログ
                    self.log_audit_event(
                        AuditEventType::PermissionCheck,
                        &abs_path.display().to_string(),
                        if cached.allowed { AuditResult::Allowed } else { AuditResult::Denied },
                        "file:execute",
                        HashMap::from([("cached".to_string(), "true".to_string())])
                    ).await;
                    
                    if !cached.allowed {
                        return Ok(false);
                    }
                    continue;
                }
            }
            
            // ポリシーチェック
            let file_exec_allowed = global_policy.check(&perm);
            allowed = allowed && file_exec_allowed;
            
            // キャッシュ更新
            self.permission_cache.insert(cache_key, CachedPermission {
                permission: perm.clone(),
                allowed: file_exec_allowed,
                timestamp: Instant::now(),
                policy_version: global_policy.version(),
            });
            
            // 権限チェック結果をリスナーに通知
            self.notify_permission_check(&perm, file_exec_allowed).await;
            
            // 監査ログを記録
            self.log_audit_event(
                AuditEventType::PermissionCheck,
                &abs_path.display().to_string(),
                if file_exec_allowed { AuditResult::Allowed } else { AuditResult::Denied },
                "file:execute",
                HashMap::new()
            ).await;
            
            if !file_exec_allowed {
                debug!("ファイル実行権限が拒否されました: {:?}", abs_path);
                return Ok(false);
            }
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
        
        // キャッシュをチェック
        let cache_key = Self::permission_to_cache_key(&perm);
        if let Some(cached) = self.permission_cache.get(&cache_key) {
            let global_policy = self.global_policy.read().await;
            if Instant::now().duration_since(cached.timestamp) < self.cache_ttl &&
               cached.policy_version == global_policy.version() {
                
                // 権限チェック結果をリスナーに通知
                self.notify_permission_check(&perm, cached.allowed).await;
                
                // 監査ログに記録
                self.log_audit_event(
                    AuditEventType::PermissionCheck,
                    &format!("{}:{}", host, port),
                    if cached.allowed { AuditResult::Allowed } else { AuditResult::Denied },
                    "network",
                    HashMap::from([("cached".to_string(), "true".to_string())])
                ).await;
                
                if cached.allowed {
                    trace!("キャッシュヒット: ネットワーク接続 {}:{} が許可されました", host, port);
                } else {
                    trace!("キャッシュヒット: ネットワーク接続 {}:{} が拒否されました", host, port);
                }
                
                return Ok(cached.allowed);
            }
        }
        
        // グローバルポリシーを確認
        let global_policy = self.global_policy.read().await;
        let allowed = global_policy.check(&perm);
        
        // キャッシュを更新
        self.permission_cache.insert(cache_key, CachedPermission {
            permission: perm.clone(),
            allowed,
            timestamp: Instant::now(),
            policy_version: global_policy.version(),
        });
        
        // 権限チェック結果をリスナーに通知
        self.notify_permission_check(&perm, allowed).await;
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PermissionCheck,
            &format!("{}:{}", host, port),
            if allowed { AuditResult::Allowed } else { AuditResult::Denied },
            "network",
            HashMap::new()
        ).await;
        
        if allowed {
            debug!("ネットワーク接続 {}:{} が許可されました", host, port);
        } else {
            debug!("ネットワーク接続 {}:{} が拒否されました", host, port);
        }
        
        Ok(allowed)
    }
    
    /// コマンド実行の検証
    pub async fn validate_command_execution(
        &self,
        cmd_path: &Path, 
        working_dir: &Path, 
        security_context: &crate::execution::SecurityContext
    ) -> Result<()> {
        // セキュリティチェックが無効の場合は常に許可
        if !self.is_enabled().await {
            return Ok(());
        }
        
        let cmd_str = cmd_path.display().to_string();
        let workdir_str = working_dir.display().to_string();
        
        // ファイル実行権限をチェック
        let can_execute = self.check_file_permission(cmd_path, true, false, true).await?;
        if !can_execute {
            let details = SecurityViolationDetails {
                violation_type: SecurityViolationType::PermissionDenied,
                resource: cmd_str.clone(),
                attempted_operation: "execute".to_string(),
                source: "command_execution".to_string(),
                timestamp: chrono::Utc::now(),
                additional_info: HashMap::from([
                    ("working_dir".to_string(), workdir_str.clone())
                ]),
            };
            
            self.report_security_violation(details).await;
            return Err(anyhow!("コマンド実行権限がありません: {:?}", cmd_path));
        }
        
        // 作業ディレクトリのアクセス権限をチェック
        let can_access_workdir = self.check_file_permission(working_dir, true, false, false).await?;
        if !can_access_workdir {
            let details = SecurityViolationDetails {
                violation_type: SecurityViolationType::PermissionDenied,
                resource: workdir_str.clone(),
                attempted_operation: "access_directory".to_string(),
                source: "command_execution".to_string(),
                timestamp: chrono::Utc::now(),
                additional_info: HashMap::from([
                    ("command".to_string(), cmd_str.clone())
                ]),
            };
            
            self.report_security_violation(details).await;
            return Err(anyhow!("作業ディレクトリへのアクセス権限がありません: {:?}", working_dir));
        }
        
        // ファイルシステム制限をチェック
        if let Some(fs_restrictions) = &security_context.filesystem_restrictions {
            // 実行ファイルが許可パスに含まれているか確認
            let cmd_allowed = fs_restrictions.exec_allowed_paths.iter()
                .any(|allowed_path| cmd_path.starts_with(allowed_path));
                
            if !cmd_allowed {
                let details = SecurityViolationDetails {
                    violation_type: SecurityViolationType::PolicyViolation,
                    resource: cmd_str.clone(),
                    attempted_operation: "execute_restricted_path".to_string(),
                    source: "command_execution".to_string(),
                    timestamp: chrono::Utc::now(),
                    additional_info: HashMap::from([
                        ("working_dir".to_string(), workdir_str.clone()),
                        ("allowed_paths".to_string(), format!("{:?}", fs_restrictions.exec_allowed_paths))
                    ]),
                };
                
                self.report_security_violation(details).await;
                return Err(anyhow!("コマンド実行が許可されていないパスにあります: {:?}", cmd_path));
            }
            
            // 作業ディレクトリが許可パスに含まれているか確認
            let workdir_allowed = fs_restrictions.read_allowed_paths.iter()
                .any(|allowed_path| working_dir.starts_with(allowed_path));
                
            if !workdir_allowed {
                let details = SecurityViolationDetails {
                    violation_type: SecurityViolationType::PolicyViolation,
                    resource: workdir_str.clone(),
                    attempted_operation: "access_restricted_directory".to_string(),
                    source: "command_execution".to_string(),
                    timestamp: chrono::Utc::now(),
                    additional_info: HashMap::from([
                        ("command".to_string(), cmd_str.clone()),
                        ("allowed_paths".to_string(), format!("{:?}", fs_restrictions.read_allowed_paths))
                    ]),
                };
                
                self.report_security_violation(details).await;
                return Err(anyhow!("作業ディレクトリが許可されていないパスにあります: {:?}", working_dir));
            }
        }
        
        // ネットワークアクセス制限をチェック
        if !security_context.allow_network {
            // 必要に応じてネットワーク機能を使用するコマンドを制限
            let network_commands = ["curl", "wget", "ssh", "nc", "netcat", "ping", "telnet", "ftp", "sftp"];
            let cmd_name = cmd_path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
                
            if network_commands.contains(&cmd_name) {
                let details = SecurityViolationDetails {
                    violation_type: SecurityViolationType::PolicyViolation,
                    resource: cmd_str,
                    attempted_operation: "network_access".to_string(),
                    source: "command_execution".to_string(),
                    timestamp: chrono::Utc::now(),
                    additional_info: HashMap::from([
                        ("network_allowed".to_string(), "false".to_string()),
                        ("command_name".to_string(), cmd_name.to_string())
                    ]),
                };
                
                self.report_security_violation(details).await;
                return Err(anyhow!("ネットワークアクセスが制限されているため、このコマンドは実行できません: {}", cmd_name));
            }
        }
        
        // メモリ使用量制限（実行前の予防的チェックのみ、実行中の制限は別途必要）
        if let Some(memory_limit) = security_context.memory_limit {
            debug!("メモリ使用量制限: {} バイト", memory_limit);
            
            // システムの利用可能メモリをチェック
            if memory_limit > 0 {
                // ここでのチェックは簡易的なものだが、実際の実行時には
                // リソースモニタリングでより厳密に制限を適用する
                self.log_audit_event(
                    AuditEventType::SystemOperation,
                    &cmd_str,
                    AuditResult::Success,
                    "memory_limit",
                    HashMap::from([("limit_bytes".to_string(), memory_limit.to_string())])
                ).await;
            }
        }
        
        // CPU時間制限
        if let Some(cpu_time_limit) = security_context.cpu_time_limit {
            debug!("CPU時間制限: {} 秒", cpu_time_limit);
            
            self.log_audit_event(
                AuditEventType::SystemOperation,
                &cmd_str,
                AuditResult::Success,
                "cpu_limit",
                HashMap::from([("limit_seconds".to_string(), cpu_time_limit.to_string())])
            ).await;
        }
        
        Ok(())
    }
    
    /// グローバルポリシーを設定
    pub async fn set_global_policy(&self, policy: SecurityPolicy) {
        let policy_name = policy.name().to_string();
        
        {
            let mut global = self.global_policy.write().await;
            *global = policy;
        }
        
        // キャッシュを無効化（ポリシーが変わったため）
        self.clear_cache().await;
        
        // リスナーに通知
        self.notify_policy_changed(&policy_name).await;
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            "global_policy",
            AuditResult::Success,
            "system",
            HashMap::from([("policy_name".to_string(), policy_name.clone())])
        ).await;
        
        info!("グローバルセキュリティポリシー '{}' が更新されました", policy_name);
    }
    
    /// コマンド固有のポリシーを設定
    pub async fn set_command_policy(&self, command: &str, policy: SecurityPolicy) {
        let policy_name = policy.name().to_string();
        
        // コマンドポリシーを設定
        self.command_policies.insert(command.to_string(), policy);
        
        // 関連するキャッシュエントリを削除
        let cmd_perm = Permission::CommandExecute(command.to_string());
        let cache_key = Self::permission_to_cache_key(&cmd_perm);
        self.permission_cache.remove(&cache_key);
        
        // リスナーに通知
        self.notify_policy_changed(&format!("command:{}:{}", command, policy_name)).await;
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            &format!("command_policy:{}", command),
            AuditResult::Success,
            "system",
            HashMap::from([
                ("command".to_string(), command.to_string()),
                ("policy_name".to_string(), policy_name.clone())
            ])
        ).await;
        
        info!("コマンド {} のセキュリティポリシー '{}' が更新されました", command, policy_name);
    }
    
    /// ドメイン固有のポリシーを設定
    pub async fn set_domain_policy(&self, domain: &str, policy: SecurityPolicy) {
        let policy_name = policy.name().to_string();
        
        // ドメインポリシーを設定
        self.domain_policies.insert(domain.to_string(), policy);
        
        // リスナーに通知
        self.notify_policy_changed(&format!("domain:{}:{}", domain, policy_name)).await;
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            &format!("domain_policy:{}", domain),
            AuditResult::Success,
            "system",
            HashMap::from([
                ("domain".to_string(), domain.to_string()),
                ("policy_name".to_string(), policy_name.clone())
            ])
        ).await;
        
        info!("ドメイン {} のセキュリティポリシー '{}' が更新されました", domain, policy_name);
    }
    
    /// コマンド固有のポリシーを削除
    pub async fn remove_command_policy(&self, command: &str) -> bool {
        let removed = self.command_policies.remove(command).is_some();
        
        if removed {
            // 関連するキャッシュエントリを削除
            let cmd_perm = Permission::CommandExecute(command.to_string());
            let cache_key = Self::permission_to_cache_key(&cmd_perm);
            self.permission_cache.remove(&cache_key);
            
            // 監査ログに記録
            self.log_audit_event(
                AuditEventType::PolicyChange,
                &format!("command_policy:{}", command),
                AuditResult::Success,
                "system",
                HashMap::from([("operation".to_string(), "remove".to_string())])
            ).await;
            
            info!("コマンド {} のセキュリティポリシーが削除されました", command);
        }
        
        removed
    }
    
    /// ドメイン固有のポリシーを削除
    pub async fn remove_domain_policy(&self, domain: &str) -> bool {
        let removed = self.domain_policies.remove(domain).is_some();
        
        if removed {
            // 監査ログに記録
            self.log_audit_event(
                AuditEventType::PolicyChange,
                &format!("domain_policy:{}", domain),
                AuditResult::Success,
                "system",
                HashMap::from([("operation".to_string(), "remove".to_string())])
            ).await;
            
            info!("ドメイン {} のセキュリティポリシーが削除されました", domain);
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
        
        let perm = Permission::FileRead(abs_path.clone());
        
        // グローバルポリシーを更新
        {
            let mut global = self.global_policy.write().await;
            global.allow(perm.clone());
        }
        
        // キャッシュから関連エントリを削除
        let cache_key = Self::permission_to_cache_key(&perm);
        self.permission_cache.remove(&cache_key);
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            &abs_path.display().to_string(),
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "allow_read".to_string())])
        ).await;
        
        debug!("ファイル読み取り権限が許可されました: {:?}", abs_path);
        Ok(())
    }
    
    /// 特定のパスに対する書き込み権限を許可
    pub async fn allow_file_write(&self, path: &Path) -> Result<()> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        let perm = Permission::FileWrite(abs_path.clone());
        
        // グローバルポリシーを更新
        {
            let mut global = self.global_policy.write().await;
            global.allow(perm.clone());
        }
        
        // キャッシュから関連エントリを削除
        let cache_key = Self::permission_to_cache_key(&perm);
        self.permission_cache.remove(&cache_key);
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            &abs_path.display().to_string(),
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "allow_write".to_string())])
        ).await;
        
        debug!("ファイル書き込み権限が許可されました: {:?}", abs_path);
        Ok(())
    }
    
    /// 特定のパスに対する実行権限を許可
    pub async fn allow_file_execute(&self, path: &Path) -> Result<()> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        let perm = Permission::FileExecute(abs_path.clone());
        
        // グローバルポリシーを更新
        {
            let mut global = self.global_policy.write().await;
            global.allow(perm.clone());
        }
        
        // キャッシュから関連エントリを削除
        let cache_key = Self::permission_to_cache_key(&perm);
        self.permission_cache.remove(&cache_key);
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            &abs_path.display().to_string(),
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "allow_execute".to_string())])
        ).await;
        
        debug!("ファイル実行権限が許可されました: {:?}", abs_path);
        Ok(())
    }
    
    /// 特定のコマンドの実行を許可
    pub async fn allow_command(&self, command: &str) -> Result<()> {
        let perm = Permission::CommandExecute(command.to_string());
        
        // グローバルポリシーを更新
        {
            let mut global = self.global_policy.write().await;
            global.allow(perm.clone());
        }
        
        // キャッシュから関連エントリを削除
        let cache_key = Self::permission_to_cache_key(&perm);
        self.permission_cache.remove(&cache_key);
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            command,
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "allow_command".to_string())])
        ).await;
        
        debug!("コマンド実行権限が許可されました: {}", command);
        Ok(())
    }
    
    /// 特定のホスト:ポートへのネットワーク接続を許可
    pub async fn allow_network(&self, host: &str, port: u16) -> Result<()> {
        let perm = Permission::NetworkConnect { 
            host: host.to_string(), 
            port 
        };
        
        // グローバルポリシーを更新
        {
            let mut global = self.global_policy.write().await;
            global.allow(perm.clone());
        }
        
        // キャッシュから関連エントリを削除
        let cache_key = Self::permission_to_cache_key(&perm);
        self.permission_cache.remove(&cache_key);
        
        // 監査ログに記録
        self.log_audit_event(
            AuditEventType::PolicyChange,
            &format!("{}:{}", host, port),
            AuditResult::Success,
            "system",
            HashMap::from([("operation".to_string(), "allow_network".to_string())])
        ).await;
        
        debug!("ネットワーク接続が許可されました: {}:{}", host, port);
        Ok(())
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
} 