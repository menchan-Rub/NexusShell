use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// コンテナ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub id: String,
    pub image: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub working_dir: Option<String>,
    pub user: Option<String>,
    pub hostname: Option<String>,
    pub privileged: bool,
    pub read_only: bool,
    pub network_mode: String,
    pub volumes: Vec<String>,
    pub ports: Vec<String>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
}

/// 名前空間設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    pub pid: bool,
    pub network: bool,
    pub mount: bool,
    pub uts: bool,
    pub ipc: bool,
    pub user: bool,
    pub cgroup: bool,
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            pid: true,
            network: true,
            mount: true,
            uts: true,
            ipc: true,
            user: false,
            cgroup: true,
        }
    }
}

/// Cgroup設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct CgroupConfig {
    pub memory_limit: Option<u64>,
    pub cpu_limit: Option<f64>,
    pub cpu_shares: Option<u64>,
    pub pids_limit: Option<u64>,
    pub devices: Vec<DeviceConfig>,
}


/// デバイス設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub path: String,
    pub device_type: String,
    pub major: i64,
    pub minor: i64,
    pub permissions: String,
}

/// セキュリティポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub no_new_privileges: bool,
    pub readonly_rootfs: bool,
    pub capabilities: CapabilityConfig,
    pub selinux_label: Option<String>,
    pub apparmor_profile: Option<String>,
    pub seccomp_profile: Option<String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            no_new_privileges: true,
            readonly_rootfs: false,
            capabilities: CapabilityConfig::default(),
            selinux_label: None,
            apparmor_profile: None,
            seccomp_profile: None,
        }
    }
}

/// ケイパビリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityConfig {
    pub add: Vec<String>,
    pub drop: Vec<String>,
}

impl Default for CapabilityConfig {
    fn default() -> Self {
        Self {
            add: Vec::new(),
            drop: vec![
                "ALL".to_string(),
            ],
        }
    }
}

/// ネットワーク設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: String,
    pub bridge: Option<String>,
    pub ip_address: Option<String>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
    pub port_mappings: Vec<PortMapping>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: "bridge".to_string(),
            bridge: None,
            ip_address: None,
            gateway: None,
            dns_servers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
            port_mappings: Vec::new(),
        }
    }
}

/// ポートマッピング
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
    pub host_ip: Option<String>,
}

/// ボリューム設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: String,
    pub source: String,
    pub target: String,
    pub readonly: bool,
    pub volume_type: String,
    pub options: Vec<String>,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: String::new(),
            target: String::new(),
            readonly: false,
            volume_type: "bind".to_string(),
            options: Vec::new(),
        }
    }
} 