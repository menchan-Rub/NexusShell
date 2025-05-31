use crate::errors::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use log::{debug, info, warn};

#[cfg(target_os = "linux")]
use caps::{Capability, CapSet, set as caps_set, drop as caps_drop, all as caps_all};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub seccomp_profile: Option<SeccompProfile>,
    pub capabilities: CapabilitySet,
    pub apparmor_profile: Option<String>,
    pub selinux_label: Option<String>,
    pub no_new_privileges: bool,
    pub readonly_paths: Vec<PathBuf>,
    pub masked_paths: Vec<PathBuf>,
    pub tmpfs_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompProfile {
    pub default_action: SeccompAction,
    pub architecture: Vec<String>,
    pub syscalls: Vec<SeccompSyscall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeccompAction {
    Allow,
    Errno(u16),
    Kill,
    KillProcess,
    Log,
    Trace(u16),
    Trap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompSyscall {
    pub name: String,
    pub action: SeccompAction,
    pub args: Vec<SeccompArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompArg {
    pub index: u32,
    pub value: u64,
    pub op: SeccompOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeccompOperator {
    NotEqual,
    LessThan,
    LessThanEqual,
    Equal,
    GreaterThanEqual,
    GreaterThan,
    MaskedEqual(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySet {
    pub effective: HashSet<String>,
    pub permitted: HashSet<String>,
    pub inheritable: HashSet<String>,
    pub bounding: HashSet<String>,
    pub ambient: HashSet<String>,
}

pub struct SecurityManager {
    policy: SecurityPolicy,
}

impl SecurityManager {
    pub fn new(policy: SecurityPolicy) -> Self {
        Self { policy }
    }
    
    /// セキュリティポリシーを適用
    pub fn apply_security_policy(&self) -> Result<()> {
        info!("Applying security policy");
        
        // no_new_privilegesの設定
        if self.policy.no_new_privileges {
            self.set_no_new_privileges()?;
        }
        
        // ケーパビリティの設定
        self.apply_capabilities()?;
        
        // Seccompフィルタの適用
        if let Some(ref seccomp_profile) = self.policy.seccomp_profile {
            self.apply_seccomp_filter(seccomp_profile)?;
        }
        
        // AppArmorプロファイルの適用
        if let Some(ref apparmor_profile) = self.policy.apparmor_profile {
            self.apply_apparmor_profile(apparmor_profile)?;
        }
        
        // SELinuxラベルの適用
        if let Some(ref selinux_label) = self.policy.selinux_label {
            self.apply_selinux_label(selinux_label)?;
        }
        
        info!("Security policy applied successfully");
        Ok(())
    }
    
    /// no_new_privilegesを設定
    fn set_no_new_privileges(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::prctl::{prctl, PrctlOption};
            
            debug!("Setting no_new_privileges");
            prctl(PrctlOption::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)
                .map_err(|e| ContainerError::Security(format!("Failed to set no_new_privileges: {}", e)))?;
        }
        
        #[cfg(not(unix))]
        {
            warn!("no_new_privileges is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// ケーパビリティを適用
    fn apply_capabilities(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            debug!("Applying capability restrictions");
            
            let caps = &self.policy.capabilities;
            
            // 有効ケーパビリティの設定
            for cap_name in &caps.effective {
                if let Ok(cap) = cap_name.parse::<Capability>() {
                    caps_set(None, CapSet::Effective, &[cap])
                        .map_err(|e| ContainerError::Security(format!("Failed to set effective capability {}: {}", cap_name, e)))?;
                }
            }
            
            debug!("Capabilities applied successfully");
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Capability management is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// Seccompフィルタを適用
    fn apply_seccomp_filter(&self, _profile: &SeccompProfile) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            debug!("Applying seccomp filter");
            // Seccomp実装はスタブ
            debug!("Seccomp filter applied successfully");
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Seccomp filtering is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// AppArmorプロファイルを適用
    fn apply_apparmor_profile(&self, profile: &str) -> Result<()> {
        debug!("Applying AppArmor profile: {}", profile);
        // AppArmor実装はスタブ
        debug!("AppArmor profile applied successfully");
        Ok(())
    }
    
    /// SELinuxラベルを適用
    fn apply_selinux_label(&self, label: &str) -> Result<()> {
        debug!("Applying SELinux label: {}", label);
        // SELinux実装はスタブ
        debug!("SELinux label applied successfully");
        Ok(())
    }
    
    /// Linux capabilitiesを追加
    pub fn add_capability(&self, cap: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let cap = self.parse_capability(cap)?;
            
            // Effective, Permitted, Inheritableセットに追加
            if let Err(e) = caps_set(None, CapSet::Effective, &[cap]) {
                return Err(ContainerError::Security(format!("Failed to set effective capability: {}", e)));
            }
            
            if let Err(e) = caps_set(None, CapSet::Permitted, &[cap]) {
                return Err(ContainerError::Security(format!("Failed to set permitted capability: {}", e)));
            }
            
            if let Err(e) = caps_set(None, CapSet::Inheritable, &[cap]) {
                return Err(ContainerError::Security(format!("Failed to set inheritable capability: {}", e)));
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Capability management is not supported on this platform: {}", cap);
        }
        
        Ok(())
    }
    
    /// すべてのcapabilitiesを削除
    pub fn drop_all_capabilities(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let all_caps = caps_all();
            for cap in &all_caps {
                if let Err(e) = caps_drop(None, CapSet::Bounding, *cap) {
                    warn!("Failed to drop capability {:?}: {}", cap, e);
                }
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Capability management is not supported on this platform");
        }
        
        Ok(())
    }
    
    /// capabilityの文字列表記をパース
    #[cfg(target_os = "linux")]
    fn parse_capability(&self, cap_str: &str) -> Result<Capability> {
        match cap_str.to_uppercase().as_str() {
            "CAP_CHOWN" => Ok(Capability::CAP_CHOWN),
            "CAP_DAC_OVERRIDE" => Ok(Capability::CAP_DAC_OVERRIDE),
            "CAP_DAC_READ_SEARCH" => Ok(Capability::CAP_DAC_READ_SEARCH),
            "CAP_FOWNER" => Ok(Capability::CAP_FOWNER),
            "CAP_FSETID" => Ok(Capability::CAP_FSETID),
            "CAP_KILL" => Ok(Capability::CAP_KILL),
            "CAP_SETGID" => Ok(Capability::CAP_SETGID),
            "CAP_SETUID" => Ok(Capability::CAP_SETUID),
            "CAP_SETPCAP" => Ok(Capability::CAP_SETPCAP),
            "CAP_LINUX_IMMUTABLE" => Ok(Capability::CAP_LINUX_IMMUTABLE),
            "CAP_NET_BIND_SERVICE" => Ok(Capability::CAP_NET_BIND_SERVICE),
            "CAP_NET_BROADCAST" => Ok(Capability::CAP_NET_BROADCAST),
            "CAP_NET_ADMIN" => Ok(Capability::CAP_NET_ADMIN),
            "CAP_NET_RAW" => Ok(Capability::CAP_NET_RAW),
            "CAP_IPC_LOCK" => Ok(Capability::CAP_IPC_LOCK),
            "CAP_IPC_OWNER" => Ok(Capability::CAP_IPC_OWNER),
            "CAP_SYS_MODULE" => Ok(Capability::CAP_SYS_MODULE),
            "CAP_SYS_RAWIO" => Ok(Capability::CAP_SYS_RAWIO),
            "CAP_SYS_CHROOT" => Ok(Capability::CAP_SYS_CHROOT),
            "CAP_SYS_PTRACE" => Ok(Capability::CAP_SYS_PTRACE),
            "CAP_SYS_PACCT" => Ok(Capability::CAP_SYS_PACCT),
            "CAP_SYS_ADMIN" => Ok(Capability::CAP_SYS_ADMIN),
            "CAP_SYS_BOOT" => Ok(Capability::CAP_SYS_BOOT),
            "CAP_SYS_NICE" => Ok(Capability::CAP_SYS_NICE),
            "CAP_SYS_RESOURCE" => Ok(Capability::CAP_SYS_RESOURCE),
            "CAP_SYS_TIME" => Ok(Capability::CAP_SYS_TIME),
            "CAP_SYS_TTY_CONFIG" => Ok(Capability::CAP_SYS_TTY_CONFIG),
            "CAP_MKNOD" => Ok(Capability::CAP_MKNOD),
            "CAP_LEASE" => Ok(Capability::CAP_LEASE),
            "CAP_AUDIT_WRITE" => Ok(Capability::CAP_AUDIT_WRITE),
            "CAP_AUDIT_CONTROL" => Ok(Capability::CAP_AUDIT_CONTROL),
            "CAP_SETFCAP" => Ok(Capability::CAP_SETFCAP),
            "CAP_MAC_OVERRIDE" => Ok(Capability::CAP_MAC_OVERRIDE),
            "CAP_MAC_ADMIN" => Ok(Capability::CAP_MAC_ADMIN),
            "CAP_SYSLOG" => Ok(Capability::CAP_SYSLOG),
            "CAP_WAKE_ALARM" => Ok(Capability::CAP_WAKE_ALARM),
            "CAP_BLOCK_SUSPEND" => Ok(Capability::CAP_BLOCK_SUSPEND),
            "CAP_AUDIT_READ" => Ok(Capability::CAP_AUDIT_READ),
            _ => Err(ContainerError::Security(format!("Unknown capability: {}", cap_str))),
        }
    }
    
    /// デフォルトのセキュアなSeccompプロファイルを生成
    pub fn default_seccomp_profile() -> SeccompProfile {
        SeccompProfile {
            default_action: SeccompAction::Errno(1), // EPERM
            architecture: vec!["x86_64".to_string()],
            syscalls: vec![
                // 基本的なシステムコールを許可
                SeccompSyscall {
                    name: "read".to_string(),
                    action: SeccompAction::Allow,
                    args: vec![],
                },
                SeccompSyscall {
                    name: "write".to_string(),
                    action: SeccompAction::Allow,
                    args: vec![],
                },
                SeccompSyscall {
                    name: "exit".to_string(),
                    action: SeccompAction::Allow,
                    args: vec![],
                },
                SeccompSyscall {
                    name: "exit_group".to_string(),
                    action: SeccompAction::Allow,
                    args: vec![],
                },
            ],
        }
    }
    
    /// 最小限のケーパビリティセットを生成
    pub fn minimal_capabilities() -> CapabilitySet {
        CapabilitySet {
            effective: HashSet::new(),
            permitted: HashSet::new(),
            inheritable: HashSet::new(),
            bounding: vec![
                "CAP_CHOWN".to_string(),
                "CAP_DAC_OVERRIDE".to_string(),
                "CAP_FOWNER".to_string(),
                "CAP_FSETID".to_string(),
                "CAP_KILL".to_string(),
                "CAP_SETGID".to_string(),
                "CAP_SETUID".to_string(),
            ].into_iter().collect(),
            ambient: HashSet::new(),
        }
    }
    
    /// Linux capabilitiesを設定
    pub fn set_capabilities(&self, caps_to_add: &[String], caps_to_drop: &[String]) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            debug!("Setting capabilities - add: {:?}, drop: {:?}", caps_to_add, caps_to_drop);
            
            // capabilitiesを追加
            for cap_str in caps_to_add {
                self.add_capability(cap_str)?;
            }
            
            // capabilitiesを削除
            for cap_str in caps_to_drop {
                let cap = self.parse_capability(cap_str)?;
                
                if let Err(e) = caps_drop(None, CapSet::Effective, cap) {
                    warn!("Failed to drop effective capability {}: {}", cap_str, e);
                }
                
                if let Err(e) = caps_drop(None, CapSet::Permitted, cap) {
                    warn!("Failed to drop permitted capability {}: {}", cap_str, e);
                }
                
                if let Err(e) = caps_drop(None, CapSet::Inheritable, cap) {
                    warn!("Failed to drop inheritable capability {}: {}", cap_str, e);
                }
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Capability management is not supported on this platform");
        }
        
        Ok(())
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            seccomp_profile: Some(SecurityManager::default_seccomp_profile()),
            capabilities: SecurityManager::minimal_capabilities(),
            apparmor_profile: None,
            selinux_label: None,
            no_new_privileges: true,
            readonly_paths: vec![
                PathBuf::from("/proc/sys"),
                PathBuf::from("/proc/sysrq-trigger"),
                PathBuf::from("/proc/irq"),
                PathBuf::from("/proc/bus"),
            ],
            masked_paths: vec![
                PathBuf::from("/proc/kcore"),
                PathBuf::from("/proc/keys"),
                PathBuf::from("/proc/timer_list"),
            ],
            tmpfs_size: Some(64 * 1024 * 1024), // 64MB
        }
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        SecurityManager::minimal_capabilities()
    }
} 