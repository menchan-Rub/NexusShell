use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

/// OCI Image Manifest Specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    
    #[serde(rename = "mediaType")]
    pub media_type: String,
    
    pub config: OCIDescriptor,
    pub layers: Vec<OCIDescriptor>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

/// OCI Descriptor for referencing content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    
    pub digest: String,
    pub size: u64,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

/// OCI Image Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIImage {
    pub created: Option<String>,
    pub author: Option<String>,
    pub architecture: String,
    pub os: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_features: Option<Vec<String>>,
    
    pub config: OCIImageConfig,
    pub rootfs: OCIRootFS,
    pub history: Vec<OCIHistory>,
}

/// OCI Image Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIImageConfig {
    #[serde(rename = "User")]
    pub user: Option<String>,
    
    #[serde(rename = "ExposedPorts")]
    pub exposed_ports: Option<HashMap<String, serde_json::Value>>,
    
    #[serde(rename = "Env")]
    pub env: Option<Vec<String>>,
    
    #[serde(rename = "Entrypoint")]
    pub entrypoint: Option<Vec<String>>,
    
    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,
    
    #[serde(rename = "Volumes")]
    pub volumes: Option<HashMap<String, serde_json::Value>>,
    
    #[serde(rename = "WorkingDir")]
    pub working_dir: Option<String>,
    
    #[serde(rename = "Labels")]
    pub labels: Option<HashMap<String, String>>,
    
    #[serde(rename = "StopSignal")]
    pub stop_signal: Option<String>,
    
    #[serde(rename = "StopTimeout")]
    pub stop_timeout: Option<u32>,
}

/// OCI RootFS Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIRootFS {
    #[serde(rename = "type")]
    pub fs_type: String,
    
    #[serde(rename = "diff_ids")]
    pub diff_ids: Vec<String>,
}

/// OCI History Entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIHistory {
    pub created: Option<String>,
    pub created_by: Option<String>,
    pub author: Option<String>,
    pub comment: Option<String>,
    pub empty_layer: Option<bool>,
}

/// OCI Layer Information
#[derive(Debug, Clone)]
pub struct OCILayer {
    pub digest: String,
    pub size: u64,
    pub media_type: String,
    pub diff_id: String,
    pub blob_path: PathBuf,
}

/// OCI Index for multi-platform images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    
    #[serde(rename = "mediaType")]
    pub media_type: String,
    
    pub manifests: Vec<OCIManifestDescriptor>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

/// OCI Manifest Descriptor with platform information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIManifestDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    
    pub digest: String,
    pub size: u64,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<OCIPlatform>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

/// OCI Platform specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIPlatform {
    pub architecture: String,
    pub os: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_features: Option<Vec<String>>,
}

/// OCI Container Runtime Specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCISpec {
    #[serde(rename = "ociVersion")]
    pub oci_version: String,
    
    pub process: OCIProcess,
    pub root: OCIRoot,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounts: Option<Vec<OCIMount>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<OCIHooks>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux: Option<OCILinux>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<serde_json::Value>,
}

/// OCI Process specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIProcess {
    pub terminal: Option<bool>,
    pub console_size: Option<OCIConsoleSize>,
    pub user: OCIUser,
    pub args: Vec<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<OCICapabilities>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rlimits: Option<Vec<OCIRlimit>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_new_privileges: Option<bool>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apparmor_profile: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_score_adj: Option<i32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selinux_label: Option<String>,
}

/// OCI Console Size
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIConsoleSize {
    pub height: u32,
    pub width: u32,
}

/// OCI User specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIUser {
    pub uid: u32,
    pub gid: u32,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_gids: Option<Vec<u32>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// OCI Capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCICapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounding: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inheritable: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permitted: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient: Option<Vec<String>>,
}

/// OCI Resource Limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIRlimit {
    #[serde(rename = "type")]
    pub limit_type: String,
    pub hard: u64,
    pub soft: u64,
}

/// OCI Root filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIRoot {
    pub path: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

/// OCI Mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIMount {
    pub destination: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

/// OCI Hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIHooks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prestart: Option<Vec<OCIHook>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_runtime: Option<Vec<OCIHook>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_container: Option<Vec<OCIHook>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_container: Option<Vec<OCIHook>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poststart: Option<Vec<OCIHook>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poststop: Option<Vec<OCIHook>>,
}

/// OCI Hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIHook {
    pub path: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

/// OCI Linux-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinux {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid_mappings: Option<Vec<OCIIDMapping>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid_mappings: Option<Vec<OCIIDMapping>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sysctl: Option<HashMap<String, String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<OCILinuxResources>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroups_path: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<Vec<OCILinuxNamespace>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<OCILinuxDevice>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seccomp: Option<serde_json::Value>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs_propagation: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_paths: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly_paths: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_label: Option<String>,
}

/// OCI ID Mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIIDMapping {
    #[serde(rename = "containerID")]
    pub container_id: u32,
    
    #[serde(rename = "hostID")]
    pub host_id: u32,
    
    pub size: u32,
}

/// OCI Linux Resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxResources {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<OCILinuxDeviceCgroup>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<OCILinuxMemory>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<OCILinuxCPU>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<OCILinuxPids>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_io: Option<serde_json::Value>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hugepage_limits: Option<Vec<serde_json::Value>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<serde_json::Value>,
}

/// OCI Linux Device Cgroup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxDeviceCgroup {
    pub allow: bool,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor: Option<i64>,
    
    pub access: String,
}

/// OCI Linux Memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxMemory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservation: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_tcp: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swappiness: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_oom_killer: Option<bool>,
}

/// OCI Linux CPU
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxCPU {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_runtime: Option<i64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_period: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mems: Option<String>,
}

/// OCI Linux PIDs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxPids {
    pub limit: i64,
}

/// OCI Linux Namespace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxNamespace {
    #[serde(rename = "type")]
    pub ns_type: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// OCI Linux Device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxDevice {
    pub path: String,
    
    #[serde(rename = "type")]
    pub device_type: String,
    
    pub major: i64,
    pub minor: i64,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_mode: Option<u32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
}

/// OCI State for runtime queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIState {
    #[serde(rename = "ociVersion")]
    pub oci_version: String,
    
    pub id: String,
    pub status: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    
    pub bundle: PathBuf,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

impl OCIDescriptor {
    /// Create a new descriptor from content
    pub fn from_content(content: &[u8], media_type: String) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let digest = format!("sha256:{:x}", hasher.finalize());
        
        Self {
            media_type,
            digest,
            size: content.len() as u64,
            urls: None,
            annotations: None,
        }
    }
    
    /// Verify content against descriptor
    pub fn verify_content(&self, content: &[u8]) -> Result<()> {
        if content.len() != self.size as usize {
            return Err(ContainerError::InvalidDigest(
                format!("Size mismatch: expected {}, got {}", self.size, content.len())
            ));
        }
        
        let expected_digest = &self.digest;
        if let Some(expected_hash) = expected_digest.strip_prefix("sha256:") {
            let mut hasher = Sha256::new();
            hasher.update(content);
            let computed_hash = format!("{:x}", hasher.finalize());
            
            if computed_hash != expected_hash {
                return Err(ContainerError::InvalidDigest(
                    format!("Hash mismatch: expected {}, got {}", expected_hash, computed_hash)
                ));
            }
        } else {
            return Err(ContainerError::InvalidDigest(
                format!("Unsupported digest algorithm: {}", expected_digest)
            ));
        }
        
        Ok(())
    }
}

impl Default for OCIManifest {
    fn default() -> Self {
        Self {
            schema_version: 2,
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            config: OCIDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: String::new(),
                size: 0,
                urls: None,
                annotations: None,
            },
            layers: Vec::new(),
            annotations: None,
        }
    }
}

impl Default for OCIImage {
    fn default() -> Self {
        Self {
            created: Some(chrono::Utc::now().to_rfc3339()),
            author: None,
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            variant: None,
            os_version: None,
            os_features: None,
            config: OCIImageConfig::default(),
            rootfs: OCIRootFS {
                fs_type: "layers".to_string(),
                diff_ids: Vec::new(),
            },
            history: Vec::new(),
        }
    }
}

impl Default for OCIImageConfig {
    fn default() -> Self {
        Self {
            user: None,
            exposed_ports: None,
            env: None,
            entrypoint: None,
            cmd: Some(vec!["/bin/sh".to_string()]),
            volumes: None,
            working_dir: None,
            labels: None,
            stop_signal: Some("SIGTERM".to_string()),
            stop_timeout: Some(10),
        }
    }
}

impl Default for OCISpec {
    fn default() -> Self {
        Self {
            oci_version: "1.0.0".to_string(),
            process: OCIProcess::default(),
            root: OCIRoot {
                path: "rootfs".to_string(),
                readonly: Some(false),
            },
            hostname: None,
            mounts: None,
            hooks: None,
            annotations: None,
            linux: None,
            windows: None,
        }
    }
}

impl Default for OCIProcess {
    fn default() -> Self {
        Self {
            terminal: Some(false),
            console_size: None,
            user: OCIUser {
                uid: 0,
                gid: 0,
                additional_gids: None,
                username: None,
            },
            args: vec!["/bin/sh".to_string()],
            env: None,
            cwd: Some("/".to_string()),
            capabilities: None,
            rlimits: None,
            no_new_privileges: Some(true),
            apparmor_profile: None,
            oom_score_adj: None,
            selinux_label: None,
        }
    }
} 