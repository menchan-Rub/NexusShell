use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// NexusCtl設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// デーモンエンドポイント
    pub daemon_endpoint: String,
    
    /// デフォルトレジストリ
    pub default_registry: String,
    
    /// 認証情報
    pub auth: AuthConfig,
    
    /// ストレージ設定
    pub storage_root: PathBuf,
    
    /// ログ設定
    pub log_level: String,
    
    /// タイムアウト設定
    pub timeout: TimeoutConfig,
    
    /// レジストリ設定
    pub registries: Vec<RegistryConfig>,
    
    /// プロファイル設定
    pub profiles: HashMap<String, ProfileConfig>,
    
    /// エイリアス設定
    pub aliases: HashMap<String, String>,
    
    /// 実験的機能
    pub experimental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct AuthConfig {
    /// 認証トークン
    pub token: Option<String>,
    
    /// 証明書ファイル
    pub cert_file: Option<PathBuf>,
    
    /// 秘密鍵ファイル
    pub key_file: Option<PathBuf>,
    
    /// CA証明書ファイル
    pub ca_file: Option<PathBuf>,
    
    /// TLS検証をスキップ
    pub insecure_skip_tls_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// 接続タイムアウト（秒）
    pub connect: u64,
    
    /// 読み取りタイムアウト（秒）
    pub read: u64,
    
    /// 書き込みタイムアウト（秒）
    pub write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// レジストリ名
    pub name: String,
    
    /// レジストリURL
    pub url: String,
    
    /// 認証情報
    pub auth: Option<RegistryAuth>,
    
    /// TLS設定
    pub tls: Option<RegistryTls>,
    
    /// ミラー設定
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAuth {
    /// ユーザー名
    pub username: String,
    
    /// パスワード
    pub password: String,
    
    /// 認証トークン
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryTls {
    /// 証明書ファイル
    pub cert_file: Option<PathBuf>,
    
    /// 秘密鍵ファイル
    pub key_file: Option<PathBuf>,
    
    /// CA証明書ファイル
    pub ca_file: Option<PathBuf>,
    
    /// TLS検証をスキップ
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// プロファイル説明
    pub description: String,
    
    /// デフォルト設定
    pub defaults: HashMap<String, String>,
    
    /// 環境変数
    pub environment: HashMap<String, String>,
    
    /// ボリュームマウント
    pub volumes: Vec<String>,
    
    /// ポートマッピング
    pub ports: Vec<String>,
    
    /// ネットワーク設定
    pub network: Option<String>,
    
    /// セキュリティ設定
    pub security: Option<SecurityConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// 特権モード
    pub privileged: bool,
    
    /// 読み取り専用ルートファイルシステム
    pub read_only_root_fs: bool,
    
    /// ユーザー
    pub user: Option<String>,
    
    /// グループ
    pub group: Option<String>,
    
    /// 追加のケイパビリティ
    pub cap_add: Vec<String>,
    
    /// 削除するケイパビリティ
    pub cap_drop: Vec<String>,
    
    /// SELinuxラベル
    pub selinux_label: Option<String>,
    
    /// AppArmorプロファイル
    pub apparmor_profile: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon_endpoint: "unix:///var/run/nexusd.sock".to_string(),
            default_registry: "docker.io".to_string(),
            auth: AuthConfig::default(),
            storage_root: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("/var/lib"))
                .join("nexuscontainer"),
            log_level: "info".to_string(),
            timeout: TimeoutConfig::default(),
            registries: vec![
                RegistryConfig {
                    name: "docker.io".to_string(),
                    url: "https://registry-1.docker.io".to_string(),
                    auth: None,
                    tls: None,
                    mirrors: Vec::new(),
                }
            ],
            profiles: HashMap::new(),
            aliases: HashMap::new(),
            experimental: false,
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Ok(Self::default());
        }
        
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
    
    #[allow(dead_code)]
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        
        // 親ディレクトリを作成
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        
        Ok(())
    }
    
    #[allow(dead_code)]
    pub fn create_default_config<P: AsRef<Path>>(path: P) -> Result<()> {
        let config = Self::default();
        config.save(path)
    }
    
    #[allow(dead_code)]
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nexuscontainer")
            .join("config.toml")
    }
    
    #[allow(dead_code)]
    pub fn init_storage_root(&self) -> Result<()> {
        fs::create_dir_all(&self.storage_root)?;
        fs::create_dir_all(self.storage_root.join("containers"))?;
        fs::create_dir_all(self.storage_root.join("images"))?;
        fs::create_dir_all(self.storage_root.join("volumes"))?;
        fs::create_dir_all(self.storage_root.join("networks"))?;
        fs::create_dir_all(self.storage_root.join("tmp"))?;
        Ok(())
    }
    
    #[allow(dead_code)]
    pub fn find_registry(&self, name: &str) -> Option<&RegistryConfig> {
        self.registries.iter().find(|r| r.name == name)
    }
    
    #[allow(dead_code)]
    pub fn add_registry(&mut self, registry: RegistryConfig) {
        // 既存のレジストリを更新または新規追加
        if let Some(existing) = self.registries.iter_mut().find(|r| r.name == registry.name) {
            *existing = registry;
        } else {
            self.registries.push(registry);
        }
    }
    
    #[allow(dead_code)]
    pub fn remove_registry(&mut self, name: &str) -> bool {
        let initial_len = self.registries.len();
        self.registries.retain(|r| r.name != name);
        self.registries.len() < initial_len
    }
    
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        
        // ストレージルートの検証
        if !self.storage_root.exists() {
            warnings.push(format!("Storage root directory does not exist: {}", self.storage_root.display()));
        }
        
        // レジストリの検証
        for registry in &self.registries {
            if registry.url.is_empty() {
                warnings.push(format!("Registry '{}' has empty URL", registry.name));
            }
        }
        
        // 認証ファイルの検証
        if let Some(ref cert_file) = self.auth.cert_file {
            if !cert_file.exists() {
                warnings.push(format!("Certificate file does not exist: {}", cert_file.display()));
            }
        }
        
        if let Some(ref key_file) = self.auth.key_file {
            if !key_file.exists() {
                warnings.push(format!("Key file does not exist: {}", key_file.display()));
            }
        }
        
        if let Some(ref ca_file) = self.auth.ca_file {
            if !ca_file.exists() {
                warnings.push(format!("CA file does not exist: {}", ca_file.display()));
            }
        }
        
        Ok(warnings)
    }
}


impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect: 30,
            read: 300,
            write: 300,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.daemon_endpoint, "unix:///var/run/nexusd.sock");
        assert_eq!(config.default_registry, "docker.io");
        assert_eq!(config.log_level, "info");
        assert!(!config.experimental);
    }
    
    #[test]
    fn test_config_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        let config = Config::default();
        config.save(&config_path).unwrap();
        
        let loaded_config = Config::load(&config_path).unwrap();
        assert_eq!(config.daemon_endpoint, loaded_config.daemon_endpoint);
        assert_eq!(config.default_registry, loaded_config.default_registry);
    }
    
    #[test]
    fn test_registry_management() {
        let mut config = Config::default();
        
        let registry = RegistryConfig {
            name: "test.registry".to_string(),
            url: "https://test.registry.com".to_string(),
            auth: None,
            tls: None,
            mirrors: Vec::new(),
        };
        
        config.add_registry(registry);
        assert_eq!(config.registries.len(), 2); // default + new
        
        assert!(config.find_registry("test.registry").is_some());
        assert!(config.remove_registry("test.registry"));
        assert_eq!(config.registries.len(), 1);
    }
} 