use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// OCI Runtime Specification準拠のコンテナ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OCISpec {
    #[serde(rename = "ociVersion")]
    pub oci_version: String,
    pub root: OCIRoot,
    pub mounts: Option<Vec<OCIMount>>,
    pub process: OCIProcess,
    pub hostname: Option<String>,
    pub domainname: Option<String>,
    pub platform: Option<OCIPlatform>,
    pub linux: Option<OCILinux>,
    pub solaris: Option<OCISolaris>,
    pub windows: Option<OCIWindows>,
    pub vm: Option<OCIVM>,
    pub annotations: Option<HashMap<String, String>>,
    pub hooks: Option<OCIHooks>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIRoot {
    pub path: String,
    pub readonly: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIMount {
    pub destination: String,
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub mount_type: Option<String>,
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIProcess {
    pub terminal: Option<bool>,
    pub consoleSize: Option<OCIConsoleSize>,
    pub user: Option<OCIUser>,
    pub args: Vec<String>,
    pub env: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub capabilities: Option<OCICapabilities>,
    pub rlimits: Option<Vec<OCIRlimit>>,
    pub noNewPrivileges: Option<bool>,
    pub apparmorProfile: Option<String>,
    pub oomScoreAdj: Option<i32>,
    pub selinuxLabel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIConsoleSize {
    pub height: u32,
    pub width: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIUser {
    pub uid: u32,
    pub gid: u32,
    pub umask: Option<u32>,
    pub additionalGids: Option<Vec<u32>>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCICapabilities {
    pub bounding: Option<Vec<String>>,
    pub effective: Option<Vec<String>>,
    pub inheritable: Option<Vec<String>>,
    pub permitted: Option<Vec<String>>,
    pub ambient: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIRlimit {
    #[serde(rename = "type")]
    pub limit_type: String,
    pub hard: u64,
    pub soft: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIPlatform {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinux {
    pub namespaces: Option<Vec<OCINamespace>>,
    pub uidMappings: Option<Vec<OCIIDMapping>>,
    pub gidMappings: Option<Vec<OCIIDMapping>>,
    pub devices: Option<Vec<OCIDevice>>,
    pub cgroupsPath: Option<String>,
    pub resources: Option<OCILinuxResources>,
    pub intelRdt: Option<OCIIntelRdt>,
    pub sysctl: Option<HashMap<String, String>>,
    pub seccomp: Option<OCISeccomp>,
    pub rootfsPropagation: Option<String>,
    pub maskedPaths: Option<Vec<String>>,
    pub readonlyPaths: Option<Vec<String>>,
    pub mountLabel: Option<String>,
    pub personality: Option<OCIPersonality>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCINamespace {
    #[serde(rename = "type")]
    pub namespace_type: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIIDMapping {
    pub containerID: u32,
    pub hostID: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIDevice {
    #[serde(rename = "type")]
    pub device_type: String,
    pub path: String,
    pub major: Option<i64>,
    pub minor: Option<i64>,
    pub fileMode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCILinuxResources {
    pub devices: Option<Vec<OCIDeviceCgroup>>,
    pub memory: Option<OCIMemory>,
    pub cpu: Option<OCICPU>,
    pub blockIO: Option<OCIBlockIO>,
    pub hugepageLimits: Option<Vec<OCIHugepageLimit>>,
    pub network: Option<OCINetwork>,
    pub pids: Option<OCIPids>,
    pub rdma: Option<HashMap<String, OCIRdma>>,
    pub unified: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIDeviceCgroup {
    pub allow: bool,
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    pub major: Option<i64>,
    pub minor: Option<i64>,
    pub access: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIMemory {
    pub limit: Option<i64>,
    pub reservation: Option<i64>,
    pub swap: Option<i64>,
    pub kernel: Option<i64>,
    pub kernelTCP: Option<i64>,
    pub swappiness: Option<u64>,
    pub disableOOMKiller: Option<bool>,
    pub useHierarchy: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCICPU {
    pub shares: Option<u64>,
    pub quota: Option<i64>,
    pub period: Option<u64>,
    pub realtimeRuntime: Option<i64>,
    pub realtimePeriod: Option<u64>,
    pub cpus: Option<String>,
    pub mems: Option<String>,
    pub idle: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIBlockIO {
    pub weight: Option<u16>,
    pub leafWeight: Option<u16>,
    pub weightDevice: Option<Vec<OCIWeightDevice>>,
    pub throttleReadBpsDevice: Option<Vec<OCIThrottleDevice>>,
    pub throttleWriteBpsDevice: Option<Vec<OCIThrottleDevice>>,
    pub throttleReadIOPSDevice: Option<Vec<OCIThrottleDevice>>,
    pub throttleWriteIOPSDevice: Option<Vec<OCIThrottleDevice>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWeightDevice {
    pub major: i64,
    pub minor: i64,
    pub weight: Option<u16>,
    pub leafWeight: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIThrottleDevice {
    pub major: i64,
    pub minor: i64,
    pub rate: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIHugepageLimit {
    pub pageSize: String,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCINetwork {
    pub classID: Option<u32>,
    pub priorities: Option<Vec<OCIInterfacePriority>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIInterfacePriority {
    pub name: String,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIPids {
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIRdma {
    pub hcaHandles: Option<u32>,
    pub hcaObjects: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIIntelRdt {
    pub closID: Option<String>,
    pub l3CacheSchema: Option<String>,
    pub memBwSchema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCISeccomp {
    pub defaultAction: String,
    pub defaultErrnoRet: Option<u32>,
    pub architectures: Option<Vec<String>>,
    pub flags: Option<Vec<String>>,
    pub listenerPath: Option<String>,
    pub listenerMetadata: Option<String>,
    pub syscalls: Option<Vec<OCISyscall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCISyscall {
    pub names: Vec<String>,
    pub action: String,
    pub errnoRet: Option<u32>,
    pub args: Option<Vec<OCIArg>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIArg {
    pub index: u32,
    pub value: u64,
    pub valueTwo: Option<u64>,
    pub op: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIPersonality {
    pub domain: String,
    pub flags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCISolaris {
    pub milestone: Option<String>,
    pub limitpriv: Option<String>,
    pub maxShmMemory: Option<String>,
    pub anet: Option<Vec<OCIAnet>>,
    pub cappedCPU: Option<OCICappedCPU>,
    pub cappedMemory: Option<OCICappedMemory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIAnet {
    pub linkname: Option<String>,
    pub lowerLink: Option<String>,
    pub allowedAddress: Option<String>,
    pub configureAllowedAddress: Option<String>,
    pub defrouter: Option<String>,
    pub linkProtection: Option<String>,
    pub macAddress: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCICappedCPU {
    pub ncpus: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCICappedMemory {
    pub physical: Option<String>,
    pub swap: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindows {
    pub layerFolders: Vec<String>,
    pub devices: Option<Vec<OCIWindowsDevice>>,
    pub resources: Option<OCIWindowsResources>,
    pub credentialSpec: Option<OCICredentialSpec>,
    pub servicing: Option<bool>,
    pub ignoreFlushesDuringBoot: Option<bool>,
    pub hyperv: Option<OCIHyperV>,
    pub network: Option<OCIWindowsNetwork>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindowsDevice {
    pub id: String,
    pub idType: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindowsResources {
    pub memory: Option<OCIWindowsMemory>,
    pub cpu: Option<OCIWindowsCPU>,
    pub storage: Option<OCIWindowsStorage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindowsMemory {
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindowsCPU {
    pub count: Option<u64>,
    pub shares: Option<u16>,
    pub maximum: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindowsStorage {
    pub iops: Option<u64>,
    pub bps: Option<u64>,
    pub sandboxSize: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCICredentialSpec {
    pub config: Option<String>,
    pub file: Option<String>,
    pub registry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIHyperV {
    pub utilityVMPath: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIWindowsNetwork {
    pub endpointList: Option<Vec<String>>,
    pub allowUnqualifiedDNSQuery: Option<bool>,
    pub DNSSearchList: Option<Vec<String>>,
    pub networkSharedContainerName: Option<String>,
    pub networkNamespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIVM {
    pub hypervisor: OCIVMHypervisor,
    pub kernel: OCIVMKernel,
    pub image: Option<OCIVMImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIVMHypervisor {
    pub path: String,
    pub parameters: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIVMKernel {
    pub path: String,
    pub parameters: Option<Vec<String>>,
    pub initrd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIVMImage {
    pub path: String,
    pub format: String,
}

/// OCI Runtime State
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCIState {
    #[serde(rename = "ociVersion")]
    pub ociVersion: String,
    pub id: String,
    pub status: String,
    pub pid: Option<u32>,
    pub bundle: PathBuf,
    pub annotations: HashMap<String, String>,
}

/// OCI Hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct OCIHooks {
    pub prestart: Option<Vec<OCIHook>>,
    pub createRuntime: Option<Vec<OCIHook>>,
    pub createContainer: Option<Vec<OCIHook>>,
    pub startContainer: Option<Vec<OCIHook>>,
    pub poststart: Option<Vec<OCIHook>>,
    pub poststop: Option<Vec<OCIHook>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct OCIHook {
    pub path: String,
    pub args: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub timeout: Option<u32>,
}

impl Default for OCISpec {
    fn default() -> Self {
        Self {
            oci_version: "1.0.0".to_string(),
            root: OCIRoot {
                path: "rootfs".to_string(),
                readonly: Some(false),
            },
            mounts: None,
            process: OCIProcess {
                terminal: Some(false),
                consoleSize: None,
                user: Some(OCIUser {
                    uid: 0,
                    gid: 0,
                    umask: None,
                    additionalGids: None,
                    username: None,
                }),
                args: vec!["/bin/sh".to_string()],
                env: Some(vec![
                    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()
                ]),
                cwd: Some("/".to_string()),
                capabilities: None,
                rlimits: None,
                noNewPrivileges: Some(true),
                apparmorProfile: None,
                oomScoreAdj: None,
                selinuxLabel: None,
            },
            hostname: None,
            domainname: None,
            platform: Some(OCIPlatform {
                os: "linux".to_string(),
                arch: "amd64".to_string(),
            }),
            linux: Some(OCILinux {
                namespaces: Some(vec![
                    OCINamespace { namespace_type: "pid".to_string(), path: None },
                    OCINamespace { namespace_type: "network".to_string(), path: None },
                    OCINamespace { namespace_type: "ipc".to_string(), path: None },
                    OCINamespace { namespace_type: "uts".to_string(), path: None },
                    OCINamespace { namespace_type: "mount".to_string(), path: None },
                ]),
                uidMappings: None,
                gidMappings: None,
                devices: None,
                cgroupsPath: None,
                resources: None,
                intelRdt: None,
                sysctl: None,
                seccomp: None,
                rootfsPropagation: Some("private".to_string()),
                maskedPaths: Some(vec![
                    "/proc/acpi".to_string(),
                    "/proc/asound".to_string(),
                    "/proc/kcore".to_string(),
                    "/proc/keys".to_string(),
                    "/proc/latency_stats".to_string(),
                    "/proc/timer_list".to_string(),
                    "/proc/timer_stats".to_string(),
                    "/proc/sched_debug".to_string(),
                    "/sys/firmware".to_string(),
                    "/proc/scsi".to_string(),
                ]),
                readonlyPaths: Some(vec![
                    "/proc/bus".to_string(),
                    "/proc/fs".to_string(),
                    "/proc/irq".to_string(),
                    "/proc/sys".to_string(),
                    "/proc/sysrq-trigger".to_string(),
                ]),
                mountLabel: None,
                personality: None,
            }),
            solaris: None,
            windows: None,
            vm: None,
            annotations: None,
            hooks: None,
        }
    }
} 