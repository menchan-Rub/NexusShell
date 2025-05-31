use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(unix)]
use nix::sched::{unshare, CloneFlags};
#[cfg(unix)]
use nix::unistd::{setresuid, setresgid, Uid, Gid};
#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::io::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    pub enable_pid: bool,
    pub enable_net: bool,
    pub enable_mount: bool,
    pub enable_uts: bool,
    pub enable_ipc: bool,
    pub enable_user: bool,
    pub enable_cgroup: bool,
    
    // User名前空間のマッピング設定
    pub user_mappings: Vec<UserMapping>,
    pub group_mappings: Vec<UserMapping>,
    
    // UTS名前空間の設定
    pub hostname: Option<String>,
    pub domainname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMapping {
    pub container_id: u32,
    pub host_id: u32,
    pub size: u32,
}

pub struct NamespaceManager {
    config: NamespaceConfig,
    original_pid: Option<i32>,
}

impl NamespaceManager {
    pub fn new(config: NamespaceConfig) -> Self {
        Self {
            config,
            original_pid: None,
        }
    }
    
    /// 名前空間を作成・設定する
    pub fn setup_namespaces(&mut self) -> Result<()> {
        log::debug!("Setting up namespaces: {:?}", self.config);
        
        #[cfg(unix)]
        {
            // 作成する名前空間のフラグを構築
            let mut flags = CloneFlags::empty();
            
            if self.config.enable_pid {
                flags |= CloneFlags::CLONE_NEWPID;
            }
            if self.config.enable_net {
                flags |= CloneFlags::CLONE_NEWNET;
            }
            if self.config.enable_mount {
                flags |= CloneFlags::CLONE_NEWNS;
            }
            if self.config.enable_uts {
                flags |= CloneFlags::CLONE_NEWUTS;
            }
            if self.config.enable_ipc {
                flags |= CloneFlags::CLONE_NEWIPC;
            }
            if self.config.enable_user {
                flags |= CloneFlags::CLONE_NEWUSER;
            }
            if self.config.enable_cgroup {
                flags |= CloneFlags::CLONE_NEWCGROUP;
            }
            
            // 名前空間の分離
            if !flags.is_empty() {
                log::info!("Creating namespaces with flags: {:?}", flags);
                unshare(flags)
                    .map_err(|e| ContainerError::Namespace(format!("Failed to unshare namespaces: {}", e)))?;
            }
            
            // User名前空間の設定
            if self.config.enable_user {
                self.setup_user_namespace()?;
            }
            
            // UTS名前空間の設定
            if self.config.enable_uts {
                self.setup_uts_namespace()?;
            }
            
            // Mount名前空間の設定
            if self.config.enable_mount {
                self.setup_mount_namespace()?;
            }
            
            // Network名前空間の設定
            if self.config.enable_net {
                self.setup_network_namespace()?;
            }
            
            log::info!("Namespaces setup completed");
        }
        
        #[cfg(not(unix))]
        {
            log::warn!("Namespace isolation is not supported on this platform");
            Err(ContainerError::UnsupportedFeature("Namespace isolation not supported on this platform".to_string()))
        }
    }
    
    /// User名前空間のUID/GIDマッピングを設定
    fn setup_user_namespace(&self) -> Result<()> {
        #[cfg(unix)]
        {
            log::debug!("Setting up user namespace");
            
            let pid = nix::unistd::getpid().as_raw();
            
            // UID マッピングの設定
            if !self.config.user_mappings.is_empty() {
                let uid_map_path = format!("/proc/{}/uid_map", pid);
                let mut uid_map = OpenOptions::new()
                    .write(true)
                    .open(&uid_map_path)
                    .map_err(|e| ContainerError::Namespace(format!("Failed to open uid_map: {}", e)))?;
                
                for mapping in &self.config.user_mappings {
                    let line = format!("{} {} {}\n", mapping.container_id, mapping.host_id, mapping.size);
                    uid_map.write_all(line.as_bytes())
                        .map_err(|e| ContainerError::Namespace(format!("Failed to write uid_map: {}", e)))?;
                }
            }
            
            // setgroups の無効化（必要な場合）
            let setgroups_path = format!("/proc/{}/setgroups", pid);
            if PathBuf::from(&setgroups_path).exists() {
                let mut setgroups = OpenOptions::new()
                    .write(true)
                    .open(&setgroups_path)
                    .map_err(|e| ContainerError::Namespace(format!("Failed to open setgroups: {}", e)))?;
                
                setgroups.write_all(b"deny\n")
                    .map_err(|e| ContainerError::Namespace(format!("Failed to write setgroups: {}", e)))?;
            }
            
            // GID マッピングの設定
            if !self.config.group_mappings.is_empty() {
                let gid_map_path = format!("/proc/{}/gid_map", pid);
                let mut gid_map = OpenOptions::new()
                    .write(true)
                    .open(&gid_map_path)
                    .map_err(|e| ContainerError::Namespace(format!("Failed to open gid_map: {}", e)))?;
                
                for mapping in &self.config.group_mappings {
                    let line = format!("{} {} {}\n", mapping.container_id, mapping.host_id, mapping.size);
                    gid_map.write_all(line.as_bytes())
                        .map_err(|e| ContainerError::Namespace(format!("Failed to write gid_map: {}", e)))?;
                }
            }
            
            log::debug!("User namespace configured successfully");
        }
        
        #[cfg(not(unix))]
        {
            log::warn!("User namespace is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// UTS名前空間の設定（ホスト名、ドメイン名）
    fn setup_uts_namespace(&self) -> Result<()> {
        #[cfg(unix)]
        {
            log::debug!("Setting up UTS namespace");
            
            if let Some(ref hostname) = self.config.hostname {
                nix::unistd::sethostname(hostname)
                    .map_err(|e| ContainerError::Namespace(format!("Failed to set hostname: {}", e)))?;
                log::debug!("Hostname set to: {}", hostname);
            }
            
            if let Some(ref domainname) = self.config.domainname {
                nix::unistd::setdomainname(domainname)
                    .map_err(|e| ContainerError::Namespace(format!("Failed to set domainname: {}", e)))?;
                log::debug!("Domainname set to: {}", domainname);
            }
        }
        
        #[cfg(not(unix))]
        {
            log::warn!("UTS namespace is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// Mount名前空間の基本設定
    fn setup_mount_namespace(&self) -> Result<()> {
        #[cfg(unix)]
        {
            log::debug!("Setting up mount namespace");
            
            // マウント伝播の設定
            use nix::mount::{mount, MsFlags};
            
            // プライベートマウント伝播に設定
            mount(
                None::<&str>,
                "/",
                None::<&str>,
                MsFlags::MS_SLAVE | MsFlags::MS_REC,
                None::<&str>
            ).map_err(|e| ContainerError::Namespace(format!("Failed to set mount propagation: {}", e)))?;
            
            log::debug!("Mount namespace configured");
        }
        
        #[cfg(not(unix))]
        {
            log::warn!("Mount namespace is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// Network名前空間の基本設定
    fn setup_network_namespace(&self) -> Result<()> {
        log::debug!("Setting up network namespace");
        
        // ネットワーク名前空間では、ループバックインターフェースを有効化
        // 実際の実装はnetwork.rsで詳細化される
        
        log::debug!("Network namespace configured");
        Ok(())
    }
    
    /// 現在のプロセスのPIDを保存
    pub fn save_original_pid(&mut self) {
        #[cfg(unix)]
        {
            self.original_pid = Some(nix::unistd::getpid().as_raw());
        }
        
        #[cfg(not(unix))]
        {
            self.original_pid = Some(std::process::id() as i32);
        }
    }
    
    /// ルートレスコンテナ用のデフォルト設定を生成
    pub fn rootless_default() -> Self {
        #[cfg(unix)]
        {
            let current_uid = nix::unistd::getuid().as_raw();
            let current_gid = nix::unistd::getgid().as_raw();
            
            Self::new(NamespaceConfig {
                enable_pid: true,
                enable_net: true,
                enable_mount: true,
                enable_uts: true,
                enable_ipc: true,
                enable_user: true,
                enable_cgroup: false, // ルートレスではCgroupは通常利用できない
                user_mappings: vec![
                    UserMapping {
                        container_id: 0,
                        host_id: current_uid,
                        size: 1,
                    }
                ],
                group_mappings: vec![
                    UserMapping {
                        container_id: 0,
                        host_id: current_gid,
                        size: 1,
                    }
                ],
                hostname: Some("nexuscontainer".to_string()),
                domainname: None,
            })
        }
        
        #[cfg(not(unix))]
        {
            Self::new(NamespaceConfig {
                enable_pid: false,
                enable_net: false,
                enable_mount: false,
                enable_uts: false,
                enable_ipc: false,
                enable_user: false,
                enable_cgroup: false,
                user_mappings: Vec::new(),
                group_mappings: Vec::new(),
                hostname: Some("nexuscontainer".to_string()),
                domainname: None,
            })
        }
    }
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            enable_pid: true,
            enable_net: true,
            enable_mount: true,
            enable_uts: true,
            enable_ipc: true,
            enable_user: false, // デフォルトでは無効（セキュリティ考慮）
            enable_cgroup: true,
            user_mappings: Vec::new(),
            group_mappings: Vec::new(),
            hostname: None,
            domainname: None,
        }
    }
}

impl UserMapping {
    pub fn new(container_id: u32, host_id: u32, size: u32) -> Self {
        Self {
            container_id,
            host_id,
            size,
        }
    }
}

/// 名前空間の操作を行うヘルパー関数群
pub mod namespace_utils {
    use super::*;
    
    /// 指定されたPIDの名前空間情報を取得
    pub fn get_namespace_info(pid: i32) -> Result<HashMap<String, String>> {
        #[cfg(unix)]
        {
            let mut namespaces = HashMap::new();
            let ns_dir = PathBuf::from(format!("/proc/{}/ns", pid));
            
            if !ns_dir.exists() {
                return Err(ContainerError::Namespace(
                    format!("Namespace directory not found for PID {}", pid)
                ));
            }
            
            let ns_types = ["pid", "net", "mnt", "uts", "ipc", "user", "cgroup"];
            
            for ns_type in &ns_types {
                let ns_path = ns_dir.join(ns_type);
                if ns_path.exists() {
                    if let Ok(link) = std::fs::read_link(&ns_path) {
                        namespaces.insert(ns_type.to_string(), link.to_string_lossy().to_string());
                    }
                }
            }
            
            Ok(namespaces)
        }
        
        #[cfg(not(unix))]
        {
            Err(ContainerError::UnsupportedFeature("Namespace information not available on this platform".to_string()))
        }
    }
    
    /// 現在のプロセスの名前空間情報を取得
    pub fn get_current_namespaces() -> Result<HashMap<String, String>> {
        #[cfg(unix)]
        {
            get_namespace_info(nix::unistd::getpid().as_raw())
        }
        
        #[cfg(not(unix))]
        {
            get_namespace_info(std::process::id() as i32)
        }
    }
    
    /// 2つの名前空間が同じかどうかを判定
    pub fn is_same_namespace(ns1: &str, ns2: &str) -> bool {
        // 名前空間のinode番号を比較
        ns1 == ns2
    }
} 