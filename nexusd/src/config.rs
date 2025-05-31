use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// データディレクトリ
    pub data_root: PathBuf,
    
    /// gRPCサーバーの設定
    pub grpc_listen: String,
    
    /// HTTPサーバーの設定
    pub http_listen: String,
    
    /// Unixソケットパス
    pub unix_socket: PathBuf,
    
    /// PIDファイルのパス
    pub pid_file: Option<PathBuf>,
    
    /// ログ設定
    pub log_config: LogConfig,
    
    /// セキュリティ設定
    pub security_config: SecurityConfig,
    
    /// ネットワーク設定
    pub network_config: NetworkConfig,
    
    /// ストレージ設定
    pub storage_config: StorageConfig,
    
    /// ランタイム設定
    pub runtime_config: RuntimeConfig,
    
    /// メトリクス設定
    pub metrics_config: MetricsConfig,
    
    /// イベント設定
    pub event_config: EventConfig,
    
    /// systemdによる管理
    pub systemd: bool,
    
    /// デバッグモード
    pub debug: bool,
    
    /// ユーザー（デーモン実行時）
    pub user: Option<String>,
    
    /// グループ（デーモン実行時）
    pub group: Option<String>,
    
    /// 追加の環境変数
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// ログレベル
    pub level: String,
    
    /// ログファイルのパス
    pub file: Option<PathBuf>,
    
    /// ログローテーション設定
    pub rotation: Option<LogRotationConfig>,
    
    /// 構造化ログ（JSON形式）
    pub structured: bool,
    
    /// ログフィルター
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// 最大ファイルサイズ（バイト）
    pub max_size: u64,
    
    /// 保持するファイル数
    pub max_files: u32,
    
    /// 圧縮するかどうか
    pub compress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// TLS設定
    pub tls: Option<TlsConfig>,
    
    /// 認証設定
    pub auth: AuthConfig,
    
    /// リソース制限
    pub limits: ResourceLimits,
    
    /// セキュリティプロファイル
    pub default_security_profile: String,
    
    /// rootlessモード
    pub rootless: bool,
    
    /// user namespace のマッピング
    pub user_namespace_mappings: Vec<UserNamespaceMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// 証明書ファイル
    pub cert_file: PathBuf,
    
    /// 秘密鍵ファイル
    pub key_file: PathBuf,
    
    /// CA証明書ファイル
    pub ca_file: Option<PathBuf>,
    
    /// クライアント証明書を要求するか
    pub client_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// 認証が必要かどうか
    pub enabled: bool,
    
    /// 認証方式
    pub methods: Vec<AuthMethod>,
    
    /// JWTトークン設定
    pub jwt: Option<JwtConfig>,
    
    /// ユーザーディレクトリ
    pub users_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    None,
    Basic,
    Jwt,
    OAuth2,
    Ldap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// JWTシークレット
    pub secret: String,
    
    /// トークンの有効期限（秒）
    pub expiration: u64,
    
    /// リフレッシュトークンを使用するか
    pub refresh_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// 最大コンテナ数
    pub max_containers: Option<u32>,
    
    /// 最大メモリ使用量（バイト）
    pub max_memory: Option<u64>,
    
    /// 最大CPU使用量（コア数）
    pub max_cpu: Option<f64>,
    
    /// 最大ディスク使用量（バイト）
    pub max_disk: Option<u64>,
    
    /// 最大同時実行数
    pub max_concurrent_operations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserNamespaceMapping {
    /// ホスト側のUID/GID
    pub host_id: u32,
    
    /// コンテナ側のUID/GID
    pub container_id: u32,
    
    /// マッピング範囲
    pub range: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// デフォルトネットワークドライバー
    pub default_driver: String,
    
    /// ブリッジネットワーク設定
    pub bridge: BridgeNetworkConfig,
    
    /// DNS設定
    pub dns: DnsConfig,
    
    /// ポートフォワーディング設定
    pub port_forwarding: PortForwardingConfig,
    
    /// CNI設定ディレクトリ
    pub cni_config_dir: Option<PathBuf>,
    
    /// CNIバイナリディレクトリ
    pub cni_bin_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeNetworkConfig {
    /// ブリッジ名
    pub name: String,
    
    /// ネットワークサブネット
    pub subnet: String,
    
    /// ゲートウェイアドレス
    pub gateway: String,
    
    /// IPアドレス範囲
    pub ip_range: Option<String>,
    
    /// MTUサイズ
    pub mtu: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    /// DNSサーバー
    pub servers: Vec<String>,
    
    /// 検索ドメイン
    pub search_domains: Vec<String>,
    
    /// オプション
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardingConfig {
    /// 有効かどうか
    pub enabled: bool,
    
    /// 利用可能なポート範囲
    pub port_range: Option<(u16, u16)>,
    
    /// iptablesを使用するか
    pub use_iptables: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// ストレージドライバー
    pub driver: String,
    
    /// イメージディレクトリ
    pub images_dir: PathBuf,
    
    /// コンテナディレクトリ
    pub containers_dir: PathBuf,
    
    /// ボリュームディレクトリ
    pub volumes_dir: PathBuf,
    
    /// 一時ディレクトリ
    pub tmp_dir: PathBuf,
    
    /// OverlayFS設定
    pub overlayfs: Option<OverlayFsConfig>,
    
    /// ガベージコレクション設定
    pub gc_config: GcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayFsConfig {
    /// lower dirディレクトリ
    pub lower_dir: PathBuf,
    
    /// upper dirディレクトリ
    pub upper_dir: PathBuf,
    
    /// work dirディレクトリ
    pub work_dir: PathBuf,
    
    /// merged dirディレクトリ
    pub merged_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcConfig {
    /// 自動ガベージコレクション
    pub auto_gc: bool,
    
    /// GC実行間隔（秒）
    pub interval: u64,
    
    /// 未使用リソースの保持期間（秒）
    pub retention_period: u64,
    
    /// ディスク使用量の閾値（割合）
    pub disk_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// デフォルトランタイム
    pub default_runtime: String,
    
    /// 利用可能なランタイム
    pub runtimes: HashMap<String, RuntimeInfo>,
    
    /// OCI仕様バージョン
    pub oci_version: String,
    
    /// cgroup設定
    pub cgroup: CgroupConfig,
    
    /// namespace設定
    pub namespace: NamespaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    /// ランタイムのパス
    pub path: PathBuf,
    
    /// ランタイム引数
    pub args: Vec<String>,
    
    /// 環境変数
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupConfig {
    /// cgroupバージョン（v1 or v2）
    pub version: String,
    
    /// cgroupマウントポイント
    pub mount_point: PathBuf,
    
    /// systemdによる管理
    pub systemd: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    /// デフォルトで有効にするnamespace
    pub default_namespaces: Vec<String>,
    
    /// user namespaceの使用
    pub use_user_namespace: bool,
    
    /// network namespaceの共有
    pub share_network_namespace: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// メトリクス収集が有効かどうか
    pub enabled: bool,
    
    /// Prometheusエンドポイント
    pub prometheus_endpoint: Option<String>,
    
    /// メトリクス収集間隔（秒）
    pub collection_interval: u64,
    
    /// 保持期間（秒）
    pub retention_period: u64,
    
    /// 収集するメトリクス
    pub metrics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// イベント保存が有効かどうか
    pub enabled: bool,
    
    /// イベントバッファサイズ
    pub buffer_size: usize,
    
    /// イベント保持期間（秒）
    pub retention_period: u64,
    
    /// 外部へのイベント送信
    pub webhooks: Vec<WebhookConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// webhook URL
    pub url: String,
    
    /// 認証ヘッダー
    pub headers: HashMap<String, String>,
    
    /// フィルター条件
    pub filters: Vec<String>,
    
    /// リトライ設定
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大リトライ回数
    pub max_attempts: u32,
    
    /// リトライ間隔（秒）
    pub interval: u64,
    
    /// 指数バックオフを使用するか
    pub exponential_backoff: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            data_root: PathBuf::from("/var/lib/nexusd"),
            grpc_listen: "127.0.0.1:7890".to_string(),
            http_listen: "127.0.0.1:7891".to_string(),
            unix_socket: PathBuf::from("/var/run/nexusd.sock"),
            pid_file: Some(PathBuf::from("/var/run/nexusd.pid")),
            log_config: LogConfig::default(),
            security_config: SecurityConfig::default(),
            network_config: NetworkConfig::default(),
            storage_config: StorageConfig::default(),
            runtime_config: RuntimeConfig::default(),
            metrics_config: MetricsConfig::default(),
            event_config: EventConfig::default(),
            systemd: false,
            debug: false,
            user: None,
            group: None,
            environment: HashMap::new(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: None,
            rotation: None,
            structured: false,
            filters: Vec::new(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            tls: None,
            auth: AuthConfig::default(),
            limits: ResourceLimits::default(),
            default_security_profile: "default".to_string(),
            rootless: false,
            user_namespace_mappings: Vec::new(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            methods: vec![AuthMethod::None],
            jwt: None,
            users_file: None,
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_containers: None,
            max_memory: None,
            max_cpu: None,
            max_disk: None,
            max_concurrent_operations: 100,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            default_driver: "bridge".to_string(),
            bridge: BridgeNetworkConfig::default(),
            dns: DnsConfig::default(),
            port_forwarding: PortForwardingConfig::default(),
            cni_config_dir: Some(PathBuf::from("/etc/cni/net.d")),
            cni_bin_dir: Some(PathBuf::from("/opt/cni/bin")),
        }
    }
}

impl Default for BridgeNetworkConfig {
    fn default() -> Self {
        Self {
            name: "nexus0".to_string(),
            subnet: "172.17.0.0/16".to_string(),
            gateway: "172.17.0.1".to_string(),
            ip_range: None,
            mtu: 1500,
        }
    }
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            servers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
            search_domains: Vec::new(),
            options: Vec::new(),
        }
    }
}

impl Default for PortForwardingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port_range: Some((10000, 65535)),
            use_iptables: true,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        let data_root = PathBuf::from("/var/lib/nexusd");
        Self {
            driver: "overlayfs".to_string(),
            images_dir: data_root.join("images"),
            containers_dir: data_root.join("containers"),
            volumes_dir: data_root.join("volumes"),
            tmp_dir: data_root.join("tmp"),
            overlayfs: None,
            gc_config: GcConfig::default(),
        }
    }
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            auto_gc: true,
            interval: 3600, // 1 hour
            retention_period: 86400, // 24 hours
            disk_threshold: 0.9, // 90%
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let mut runtimes = HashMap::new();
        runtimes.insert("nexus-runtime".to_string(), RuntimeInfo {
            path: PathBuf::from("/usr/local/bin/nexus-runtime"),
            args: Vec::new(),
            env: HashMap::new(),
        });
        
        Self {
            default_runtime: "nexus-runtime".to_string(),
            runtimes,
            oci_version: "1.0.0".to_string(),
            cgroup: CgroupConfig::default(),
            namespace: NamespaceConfig::default(),
        }
    }
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            version: "v2".to_string(),
            mount_point: PathBuf::from("/sys/fs/cgroup"),
            systemd: false,
        }
    }
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            default_namespaces: vec![
                "pid".to_string(),
                "net".to_string(),
                "mount".to_string(),
                "uts".to_string(),
                "ipc".to_string(),
            ],
            use_user_namespace: false,
            share_network_namespace: false,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prometheus_endpoint: Some("/metrics".to_string()),
            collection_interval: 30,
            retention_period: 86400, // 24 hours
            metrics: vec![
                "container_count".to_string(),
                "image_count".to_string(),
                "volume_count".to_string(),
                "memory_usage".to_string(),
                "cpu_usage".to_string(),
            ],
        }
    }
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: 1000,
            retention_period: 86400, // 24 hours
            webhooks: Vec::new(),
        }
    }
}

impl DaemonConfig {
    /// ファイルから設定を読み込み
    pub async fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        
        // YAML、TOML、JSONのいずれかの形式を自動判定
        let config = if content.trim_start().starts_with('{') {
            // JSON形式
            serde_json::from_str(&content)?
        } else if content.contains("---") || content.contains(":") {
            // YAML形式
            serde_yaml::from_str(&content)?
        } else {
            // TOML形式
            toml::from_str(&content)?
        };
        
        // Configuration loaded from file
        Ok(config)
    }
    
    /// 設定をファイルに保存
    #[allow(dead_code)]
    pub async fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }
    
    /// 設定の検証
    pub fn validate(&self) -> Result<Vec<String>, Vec<String>> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        
        // データディレクトリの存在チェック
        if !self.data_root.exists() {
            warnings.push(format!("Data root directory does not exist: {}", self.data_root.display()));
        }
        
        // ネットワーク設定の検証
        if let Err(e) = self.validate_network_config() {
            errors.push(format!("Network configuration error: {}", e));
        }
        
        // ストレージ設定の検証
        if let Err(e) = self.validate_storage_config() {
            errors.push(format!("Storage configuration error: {}", e));
        }
        
        // セキュリティ設定の検証
        if let Err(e) = self.validate_security_config() {
            errors.push(format!("Security configuration error: {}", e));
        }
        
        // リソース制限の検証
        if let Err(e) = self.validate_resource_limits() {
            errors.push(format!("Resource limits error: {}", e));
        }
        
        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }
    }
    
    fn validate_network_config(&self) -> Result<()> {
        // サブネットの検証
        if !is_valid_cidr(&self.network_config.bridge.subnet) {
            anyhow::bail!("Invalid bridge subnet: {}", self.network_config.bridge.subnet);
        }
        
        // ゲートウェイの検証
        if !is_valid_ip(&self.network_config.bridge.gateway) {
            anyhow::bail!("Invalid bridge gateway: {}", self.network_config.bridge.gateway);
        }
        
        Ok(())
    }
    
    fn validate_storage_config(&self) -> Result<()> {
        // ディレクトリが絶対パスかチェック
        if !self.storage_config.images_dir.is_absolute() {
            anyhow::bail!("Images directory must be absolute path");
        }
        
        if !self.storage_config.containers_dir.is_absolute() {
            anyhow::bail!("Containers directory must be absolute path");
        }
        
        Ok(())
    }
    
    fn validate_security_config(&self) -> Result<()> {
        // TLS設定の検証
        if let Some(ref tls) = self.security_config.tls {
            if !tls.cert_file.exists() {
                anyhow::bail!("TLS certificate file not found: {}", tls.cert_file.display());
            }
            
            if !tls.key_file.exists() {
                anyhow::bail!("TLS key file not found: {}", tls.key_file.display());
            }
        }
        
        Ok(())
    }
    
    fn validate_resource_limits(&self) -> Result<()> {
        let limits = &self.security_config.limits;
        
        if let Some(max_containers) = limits.max_containers {
            if max_containers == 0 {
                anyhow::bail!("Max containers must be greater than 0");
            }
        }
        
        if let Some(max_memory) = limits.max_memory {
            if max_memory < 1024 * 1024 { // 1MB最小
                anyhow::bail!("Max memory must be at least 1MB");
            }
        }
        
        Ok(())
    }
}

fn is_valid_cidr(cidr: &str) -> bool {
    cidr.contains('/') && {
        let parts: Vec<&str> = cidr.split('/').collect();
        parts.len() == 2 && is_valid_ip(parts[0]) && parts[1].parse::<u8>().is_ok()
    }
}

fn is_valid_ip(ip: &str) -> bool {
    ip.parse::<std::net::IpAddr>().is_ok()
} 