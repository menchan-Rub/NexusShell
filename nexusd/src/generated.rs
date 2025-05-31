// Generated protobuf stubs for NexusContainer daemon
// This file contains stub implementations for protobuf-generated types

// use super::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tonic::{Request, Response, Status, Result as TonicResult};

// Container Service Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContainerRequest {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub volumes: Vec<String>,
    pub ports: Vec<String>,
    pub workdir: Option<String>,
    pub user: Option<String>,
    pub hostname: Option<String>,
    pub privileged: bool,
    pub read_only: bool,
    pub network: Option<String>,
    pub security_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContainerResponse {
    pub id: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartContainerRequest {
    pub id: String,
    pub detach_keys: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopContainerRequest {
    pub id: String,
    pub timeout: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartContainerRequest {
    pub id: String,
    pub timeout: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveContainerRequest {
    pub id: String,
    pub force: bool,
    pub remove_volumes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListContainersRequest {
    pub all: bool,
    pub limit: Option<i32>,
    pub size: bool,
    pub filters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListContainersResponse {
    pub containers: Vec<ContainerSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLogsRequest {
    pub container_id: String,
    pub follow: bool,
    pub stdout: bool,
    pub stderr: bool,
    pub since: Option<String>,
    pub until: Option<String>,
    pub timestamps: bool,
    pub tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLogsResponse {
    pub data: Vec<u8>,
    pub stream_type: i32, // 1 = stdout, 2 = stderr
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecInContainerRequest {
    pub container_id: String,
    pub cmd: Vec<String>,
    pub env: HashMap<String, String>,
    pub workdir: Option<String>,
    pub user: Option<String>,
    pub privileged: bool,
    pub tty: bool,
    pub attach_stdin: bool,
    pub attach_stdout: bool,
    pub attach_stderr: bool,
    pub detach_keys: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecInContainerResponse {
    pub exec_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseContainerRequest {
    pub container_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpauseContainerRequest {
    pub container_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpauseContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateContainerRequest {
    pub container_id: String,
    pub memory: Option<i64>,
    pub memory_swap: Option<i64>,
    pub cpu_shares: Option<i64>,
    pub cpu_period: Option<i64>,
    pub cpu_quota: Option<i64>,
    pub cpuset_cpus: Option<String>,
    pub cpuset_mems: Option<String>,
    pub blkio_weight: Option<i32>,
    pub restart_policy: Option<RestartPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateContainerResponse {
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContainerStatsRequest {
    pub container_id: String,
    pub stream: bool,
    pub one_shot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContainerStatsResponse {
    pub stats: Option<ContainerStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStats {
    pub read: String,
    pub preread: String,
    pub pids_stats: Option<PidsStats>,
    pub blkio_stats: Option<BlkioStats>,
    pub num_procs: u32,
    pub storage_stats: Option<StorageStats>,
    pub cpu_stats: Option<CpuStats>,
    pub precpu_stats: Option<CpuStats>,
    pub memory_stats: Option<MemoryStats>,
    pub name: String,
    pub id: String,
    pub networks: HashMap<String, NetworkStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidsStats {
    pub current: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlkioStats {
    pub io_service_bytes_recursive: Vec<BlkioStatEntry>,
    pub io_serviced_recursive: Vec<BlkioStatEntry>,
    pub io_queue_recursive: Vec<BlkioStatEntry>,
    pub io_service_time_recursive: Vec<BlkioStatEntry>,
    pub io_wait_time_recursive: Vec<BlkioStatEntry>,
    pub io_merged_recursive: Vec<BlkioStatEntry>,
    pub io_time_recursive: Vec<BlkioStatEntry>,
    pub sectors_recursive: Vec<BlkioStatEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlkioStatEntry {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub read_count_normalized: Option<u64>,
    pub read_size_bytes: Option<u64>,
    pub write_count_normalized: Option<u64>,
    pub write_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuStats {
    pub cpu_usage: CpuUsage,
    pub system_cpu_usage: Option<u64>,
    pub online_cpus: Option<u32>,
    pub throttling_data: ThrottlingData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuUsage {
    pub total_usage: u64,
    pub percpu_usage: Vec<u64>,
    pub usage_in_kernelmode: u64,
    pub usage_in_usermode: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottlingData {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub usage: Option<u64>,
    pub max_usage: Option<u64>,
    pub stats: HashMap<String, u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub rx_errors: u64,
    pub rx_dropped: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub tx_errors: u64,
    pub tx_dropped: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSummary {
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    pub image_id: String,
    pub command: String,
    pub created: i64,
    pub state: String,
    pub status: String,
    pub ports: Vec<Port>,
    pub size_rw: Option<i64>,
    pub size_root_fs: Option<i64>,
    pub labels: HashMap<String, String>,
    pub network_mode: String,
    pub mounts: Vec<MountPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub ip: Option<String>,
    pub private_port: u32,
    pub public_port: Option<u32>,
    pub port_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPoint {
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub rw: bool,
    pub propagation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectContainerRequest {
    pub id: String,
    pub size: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectContainerResponse {
    pub container: Option<ContainerInspect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInspect {
    pub id: String,
    pub created: String,
    pub path: String,
    pub args: Vec<String>,
    pub state: ContainerState,
    pub image: String,
    pub name: String,
    pub restart_count: i32,
    pub driver: String,
    pub platform: String,
    pub mount_label: String,
    pub process_label: String,
    pub app_armor_profile: String,
    pub exec_ids: Vec<String>,
    pub host_config: HostConfig,
    pub graph_driver: GraphDriverData,
    pub size_rw: Option<i64>,
    pub size_root_fs: Option<i64>,
    pub mounts: Vec<MountPoint>,
    pub config: ContainerConfig,
    pub network_settings: NetworkSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerState {
    pub status: String,
    pub running: bool,
    pub paused: bool,
    pub restarting: bool,
    pub oom_killed: bool,
    pub dead: bool,
    pub pid: i32,
    pub exit_code: i32,
    pub error: String,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub binds: Vec<String>,
    pub container_id_file: String,
    pub log_config: LogConfig,
    pub network_mode: String,
    pub port_bindings: HashMap<String, Vec<PortBinding>>,
    pub restart_policy: RestartPolicy,
    pub auto_remove: bool,
    pub volume_driver: String,
    pub volumes_from: Vec<String>,
    pub cap_add: Vec<String>,
    pub cap_drop: Vec<String>,
    pub dns: Vec<String>,
    pub dns_options: Vec<String>,
    pub dns_search: Vec<String>,
    pub extra_hosts: Vec<String>,
    pub group_add: Vec<String>,
    pub ipc_mode: String,
    pub cgroup: String,
    pub links: Vec<String>,
    pub oom_score_adj: i32,
    pub pid_mode: String,
    pub privileged: bool,
    pub publish_all_ports: bool,
    pub readonly_rootfs: bool,
    pub security_opt: Vec<String>,
    pub tmpfs: HashMap<String, String>,
    pub uts_mode: String,
    pub userns_mode: String,
    pub shm_size: i64,
    pub sysctls: HashMap<String, String>,
    pub runtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    pub log_type: String,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortBinding {
    pub host_ip: String,
    pub host_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicy {
    pub name: String,
    pub maximum_retry_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDriverData {
    pub name: String,
    pub data: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub hostname: String,
    pub domainname: String,
    pub user: String,
    pub attach_stdin: bool,
    pub attach_stdout: bool,
    pub attach_stderr: bool,
    pub exposed_ports: HashMap<String, serde_json::Value>,
    pub tty: bool,
    pub open_stdin: bool,
    pub stdin_once: bool,
    pub env: Vec<String>,
    pub cmd: Vec<String>,
    pub image: String,
    pub volumes: HashMap<String, serde_json::Value>,
    pub working_dir: String,
    pub entrypoint: Vec<String>,
    pub network_disabled: bool,
    pub mac_address: String,
    pub on_build: Vec<String>,
    pub labels: HashMap<String, String>,
    pub stop_signal: String,
    pub stop_timeout: Option<i32>,
    pub shell: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub bridge: String,
    pub sandbox_id: String,
    pub hairpin_mode: bool,
    pub link_local_ipv6_address: String,
    pub link_local_ipv6_prefix_len: i32,
    pub ports: HashMap<String, Vec<PortBinding>>,
    pub sandbox_key: String,
    pub secondary_ip_addresses: Vec<String>,
    pub secondary_ipv6_addresses: Vec<String>,
    pub endpoint_id: String,
    pub gateway: String,
    pub global_ipv6_address: String,
    pub global_ipv6_prefix_len: i32,
    pub ip_address: String,
    pub ip_prefix_len: i32,
    pub ipv6_gateway: String,
    pub mac_address: String,
    pub networks: HashMap<String, EndpointSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSettings {
    pub ipam_config: Option<EndpointIpamConfig>,
    pub links: Vec<String>,
    pub aliases: Vec<String>,
    pub network_id: String,
    pub endpoint_id: String,
    pub gateway: String,
    pub ip_address: String,
    pub ip_prefix_len: i32,
    pub ipv6_gateway: String,
    pub global_ipv6_address: String,
    pub global_ipv6_prefix_len: i32,
    pub mac_address: String,
    pub driver_opts: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointIpamConfig {
    pub ipv4_address: String,
    pub ipv6_address: String,
    pub link_local_ips: Vec<String>,
}

// Image Service Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListImagesRequest {
    pub all: bool,
    pub filters: HashMap<String, String>,
    pub digests: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListImagesResponse {
    pub images: Vec<ImageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSummary {
    pub id: String,
    pub parent_id: String,
    pub repo_tags: Vec<String>,
    pub repo_digests: Vec<String>,
    pub created: i64,
    pub size: i64,
    pub virtual_size: i64,
    pub shared_size: i64,
    pub labels: HashMap<String, String>,
    pub containers: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullImageRequest {
    pub image: String,
    pub tag: Option<String>,
    pub platform: Option<String>,
    pub all_tags: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullImageResponse {
    pub status: String,
    pub progress: String,
    pub progress_detail: Option<ProgressDetail>,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressDetail {
    pub current: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushImageRequest {
    pub image: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushImageResponse {
    pub status: String,
    pub progress: String,
    pub progress_detail: Option<ProgressDetail>,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveImageRequest {
    pub image: String,
    pub force: bool,
    pub no_prune: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveImageResponse {
    pub deleted: Vec<ImageDeleteResponseItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageDeleteResponseItem {
    pub untagged: Option<String>,
    pub deleted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectImageRequest {
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectImageResponse {
    pub image: Option<ImageInspect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInspect {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub repo_digests: Vec<String>,
    pub parent: String,
    pub comment: String,
    pub created: String,
    pub container: String,
    pub container_config: ContainerConfig,
    pub docker_version: String,
    pub author: String,
    pub config: ContainerConfig,
    pub architecture: String,
    pub os: String,
    pub os_version: String,
    pub size: i64,
    pub virtual_size: i64,
    pub graph_driver: GraphDriverData,
    pub root_fs: RootFs,
    pub metadata: ImageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootFs {
    pub fs_type: String,
    pub layers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub last_tag_time: String,
}

// Volume Service Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVolumeRequest {
    pub name: String,
    pub driver: String,
    pub driver_opts: HashMap<String, String>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVolumeResponse {
    pub volume: Option<Volume>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: String,
    pub status: HashMap<String, String>,
    pub labels: HashMap<String, String>,
    pub scope: String,
    pub options: HashMap<String, String>,
    pub usage_data: Option<VolumeUsageData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeUsageData {
    pub size: i64,
    pub ref_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVolumesRequest {
    pub filters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVolumesResponse {
    pub volumes: Vec<Volume>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveVolumeRequest {
    pub name: String,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveVolumeResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectVolumeRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectVolumeResponse {
    pub volume: Option<Volume>,
}

// Network Service Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNetworkRequest {
    pub name: String,
    pub driver: String,
    pub options: HashMap<String, String>,
    pub ipam: Option<Ipam>,
    pub enable_ipv6: bool,
    pub internal: bool,
    pub attachable: bool,
    pub ingress: bool,
    pub config_only: bool,
    pub config_from: Option<ConfigReference>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ipam {
    pub driver: String,
    pub config: Vec<IpamConfig>,
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpamConfig {
    pub subnet: String,
    pub ip_range: String,
    pub gateway: String,
    pub aux_addresses: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReference {
    pub network: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNetworkResponse {
    pub id: String,
    pub warning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNetworksRequest {
    pub filters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNetworksResponse {
    pub networks: Vec<Network>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
    pub id: String,
    pub created: String,
    pub scope: String,
    pub driver: String,
    pub enable_ipv6: bool,
    pub ipam: Ipam,
    pub internal: bool,
    pub attachable: bool,
    pub ingress: bool,
    pub config_from: Option<ConfigReference>,
    pub config_only: bool,
    pub containers: HashMap<String, EndpointResource>,
    pub options: HashMap<String, String>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointResource {
    pub name: String,
    pub endpoint_id: String,
    pub mac_address: String,
    pub ipv4_address: String,
    pub ipv6_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveNetworkRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveNetworkResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectNetworkRequest {
    pub id: String,
    pub verbose: bool,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectNetworkResponse {
    pub network: Option<Network>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectContainerRequest {
    pub network: String,
    pub container: String,
    pub endpoint_config: Option<EndpointSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectContainerResponse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectContainerRequest {
    pub network: String,
    pub container: String,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectContainerResponse {}

// System Service Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVersionRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVersionResponse {
    pub version: String,
    pub api_version: String,
    pub min_api_version: String,
    pub git_commit: String,
    pub go_version: String,
    pub os: String,
    pub arch: String,
    pub kernel_version: String,
    pub build_time: String,
    pub experimental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetInfoRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetInfoResponse {
    pub info: Option<SystemInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub id: String,
    pub containers: i32,
    pub containers_running: i32,
    pub containers_paused: i32,
    pub containers_stopped: i32,
    pub images: i32,
    pub driver: String,
    pub driver_status: Vec<Vec<String>>,
    pub docker_root_dir: String,
    pub system_status: Vec<Vec<String>>,
    pub plugins: Plugins,
    pub memory_limit: bool,
    pub swap_limit: bool,
    pub kernel_memory: bool,
    pub cpu_cfs_period: bool,
    pub cpu_cfs_quota: bool,
    pub cpu_shares: bool,
    pub cpu_set: bool,
    pub pids_limit: bool,
    pub ipv4_forwarding: bool,
    pub bridge_nf_iptables: bool,
    pub bridge_nf_ip6tables: bool,
    pub debug: bool,
    pub nfd: i32,
    pub oom_kill_disable: bool,
    pub n_goroutines: i32,
    pub system_time: String,
    pub logging_driver: String,
    pub cgroup_driver: String,
    pub n_events_listener: i32,
    pub kernel_version: String,
    pub operating_system: String,
    pub os_type: String,
    pub architecture: String,
    pub index_server_address: String,
    pub registry_config: RegistryServiceConfig,
    pub ncpu: i32,
    pub mem_total: i64,
    pub generic_resources: Vec<GenericResource>,
    pub docker_version: String,
    pub http_proxy: String,
    pub https_proxy: String,
    pub no_proxy: String,
    pub name: String,
    pub labels: Vec<String>,
    pub experimental_build: bool,
    pub server_version: String,
    pub cluster_store: String,
    pub cluster_advertise: String,
    pub runtimes: HashMap<String, Runtime>,
    pub default_runtime: String,
    pub swarm: SwarmInfo,
    pub live_restore_enabled: bool,
    pub isolation: String,
    pub init_binary: String,
    pub containerd_commit: Commit,
    pub runc_commit: Commit,
    pub init_commit: Commit,
    pub security_options: Vec<String>,
    pub product_license: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugins {
    pub volume: Vec<String>,
    pub network: Vec<String>,
    pub authorization: Vec<String>,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryServiceConfig {
    pub allow_nondistributable_artifacts_cidrs: Vec<String>,
    pub allow_nondistributable_artifacts_hostnames: Vec<String>,
    pub insecure_registry_cidrs: Vec<String>,
    pub index_configs: HashMap<String, IndexInfo>,
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub mirrors: Vec<String>,
    pub secure: bool,
    pub official: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericResource {
    pub named_resource_spec: Option<NamedGenericResource>,
    pub discrete_resource_spec: Option<DiscreteGenericResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedGenericResource {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscreteGenericResource {
    pub kind: String,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    pub path: String,
    pub runtime_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmInfo {
    pub node_id: String,
    pub node_addr: String,
    pub local_node_state: String,
    pub control_available: bool,
    pub error: String,
    pub remote_managers: Vec<PeerNode>,
    pub nodes: i32,
    pub managers: i32,
    pub cluster: ClusterInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerNode {
    pub node_id: String,
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub id: String,
    pub version: ObjectVersion,
    pub created_at: String,
    pub updated_at: String,
    pub spec: SwarmSpec,
    pub tls_info: TlsInfo,
    pub root_rotation_in_progress: bool,
    pub default_addr_pool: Vec<String>,
    pub subnet_size: i32,
    pub data_path_port: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectVersion {
    pub index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmSpec {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub orchestration: OrchestrationConfig,
    pub raft: RaftConfig,
    pub dispatcher: DispatcherConfig,
    pub ca_config: CaConfig,
    pub encryption_config: EncryptionConfig,
    pub task_defaults: TaskDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub task_history_retention_limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    pub snapshot_interval: i64,
    pub keep_old_snapshots: i64,
    pub log_entries_for_slow_followers: i64,
    pub election_tick: i32,
    pub heartbeat_tick: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatcherConfig {
    pub heartbeat_period: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaConfig {
    pub node_cert_expiry: i64,
    pub external_cas: Vec<ExternalCa>,
    pub signing_ca_cert: String,
    pub signing_ca_key: String,
    pub force_rotate: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalCa {
    pub protocol: String,
    pub url: String,
    pub options: HashMap<String, String>,
    pub ca_cert: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub auto_lock_managers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefaults {
    pub log_driver: Option<Driver>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Driver {
    pub name: String,
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsInfo {
    pub trust_root: String,
    pub cert_issuer_subject: String,
    pub cert_issuer_public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub expected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEventsRequest {
    pub since: Option<String>,
    pub until: Option<String>,
    pub filters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEventsResponse {
    pub event: Option<SystemEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub event_type: String,
    pub action: String,
    pub actor: EventActor,
    pub time: i64,
    pub time_nano: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActor {
    pub id: String,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResponse {
    pub api_version: String,
    pub docker_experimental: bool,
    pub os_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDiskUsageRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDiskUsageResponse {
    pub disk_usage: Option<DiskUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub layers_size: i64,
    pub images: Vec<ImageSummary>,
    pub containers: Vec<ContainerSummary>,
    pub volumes: Vec<Volume>,
    pub build_cache: Vec<BuildCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCache {
    pub id: String,
    pub parent: String,
    pub cache_type: String,
    pub description: String,
    pub in_use: bool,
    pub shared: bool,
    pub size: i64,
    pub created_at: String,
    pub last_used_at: String,
    pub usage_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneSystemRequest {
    pub filters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneSystemResponse {
    pub containers_deleted: Vec<String>,
    pub space_reclaimed: i64,
}

// gRPC Service Traits
#[tonic::async_trait]
#[allow(dead_code)]
pub trait ContainerService: Send + Sync + 'static {
    type ContainerLogsStream: futures::Stream<Item = Result<ContainerLogsResponse, Status>>
        + Send
        + 'static;

    async fn create_container(
        &self,
        request: Request<CreateContainerRequest>,
    ) -> TonicResult<Response<CreateContainerResponse>>;

    async fn start_container(
        &self,
        request: Request<StartContainerRequest>,
    ) -> TonicResult<Response<StartContainerResponse>>;

    async fn stop_container(
        &self,
        request: Request<StopContainerRequest>,
    ) -> TonicResult<Response<StopContainerResponse>>;

    async fn restart_container(
        &self,
        request: Request<RestartContainerRequest>,
    ) -> TonicResult<Response<RestartContainerResponse>>;

    async fn remove_container(
        &self,
        request: Request<RemoveContainerRequest>,
    ) -> TonicResult<Response<RemoveContainerResponse>>;

    async fn list_containers(
        &self,
        request: Request<ListContainersRequest>,
    ) -> TonicResult<Response<ListContainersResponse>>;

    async fn inspect_container(
        &self,
        request: Request<InspectContainerRequest>,
    ) -> TonicResult<Response<InspectContainerResponse>>;

    async fn container_logs(
        &self,
        request: Request<ContainerLogsRequest>,
    ) -> TonicResult<Response<Self::ContainerLogsStream>>;

    async fn exec_in_container(
        &self,
        request: Request<ExecInContainerRequest>,
    ) -> TonicResult<Response<ExecInContainerResponse>>;

    async fn pause_container(
        &self,
        request: Request<PauseContainerRequest>,
    ) -> TonicResult<Response<PauseContainerResponse>>;

    async fn unpause_container(
        &self,
        request: Request<UnpauseContainerRequest>,
    ) -> TonicResult<Response<UnpauseContainerResponse>>;

    async fn update_container(
        &self,
        request: Request<UpdateContainerRequest>,
    ) -> TonicResult<Response<UpdateContainerResponse>>;

    async fn get_container_stats(
        &self,
        request: Request<GetContainerStatsRequest>,
    ) -> TonicResult<Response<GetContainerStatsResponse>>;
}

#[tonic::async_trait]
#[allow(dead_code)]
pub trait ImageService: Send + Sync + 'static {
    async fn list_images(
        &self,
        request: Request<ListImagesRequest>,
    ) -> TonicResult<Response<ListImagesResponse>>;

    async fn pull_image(
        &self,
        request: Request<PullImageRequest>,
    ) -> TonicResult<Response<PullImageResponse>>;

    async fn push_image(
        &self,
        request: Request<PushImageRequest>,
    ) -> TonicResult<Response<PushImageResponse>>;

    async fn remove_image(
        &self,
        request: Request<RemoveImageRequest>,
    ) -> TonicResult<Response<RemoveImageResponse>>;

    async fn inspect_image(
        &self,
        request: Request<InspectImageRequest>,
    ) -> TonicResult<Response<InspectImageResponse>>;
}

#[tonic::async_trait]
#[allow(dead_code)]
pub trait VolumeService: Send + Sync + 'static {
    async fn create_volume(
        &self,
        request: Request<CreateVolumeRequest>,
    ) -> TonicResult<Response<CreateVolumeResponse>>;

    async fn list_volumes(
        &self,
        request: Request<ListVolumesRequest>,
    ) -> TonicResult<Response<ListVolumesResponse>>;

    async fn remove_volume(
        &self,
        request: Request<RemoveVolumeRequest>,
    ) -> TonicResult<Response<RemoveVolumeResponse>>;

    async fn inspect_volume(
        &self,
        request: Request<InspectVolumeRequest>,
    ) -> TonicResult<Response<InspectVolumeResponse>>;
}

#[tonic::async_trait]
#[allow(dead_code)]
pub trait NetworkService: Send + Sync + 'static {
    async fn create_network(
        &self,
        request: Request<CreateNetworkRequest>,
    ) -> TonicResult<Response<CreateNetworkResponse>>;

    async fn list_networks(
        &self,
        request: Request<ListNetworksRequest>,
    ) -> TonicResult<Response<ListNetworksResponse>>;

    async fn remove_network(
        &self,
        request: Request<RemoveNetworkRequest>,
    ) -> TonicResult<Response<RemoveNetworkResponse>>;

    async fn inspect_network(
        &self,
        request: Request<InspectNetworkRequest>,
    ) -> TonicResult<Response<InspectNetworkResponse>>;

    async fn connect_container(
        &self,
        request: Request<ConnectContainerRequest>,
    ) -> TonicResult<Response<ConnectContainerResponse>>;

    async fn disconnect_container(
        &self,
        request: Request<DisconnectContainerRequest>,
    ) -> TonicResult<Response<DisconnectContainerResponse>>;
}

#[tonic::async_trait]
#[allow(dead_code)]
pub trait SystemService: Send + Sync + 'static {
    type GetEventsStream: futures::Stream<Item = Result<GetEventsResponse, Status>>
        + Send
        + 'static;

    async fn get_version(
        &self,
        request: Request<GetVersionRequest>,
    ) -> TonicResult<Response<GetVersionResponse>>;

    async fn get_info(
        &self,
        request: Request<GetInfoRequest>,
    ) -> TonicResult<Response<GetInfoResponse>>;

    async fn get_events(
        &self,
        request: Request<GetEventsRequest>,
    ) -> TonicResult<Response<Self::GetEventsStream>>;

    async fn ping(
        &self,
        request: Request<PingRequest>,
    ) -> TonicResult<Response<PingResponse>>;

    async fn get_disk_usage(
        &self,
        request: Request<GetDiskUsageRequest>,
    ) -> TonicResult<Response<GetDiskUsageResponse>>;

    async fn prune_system(
        &self,
        request: Request<PruneSystemRequest>,
    ) -> TonicResult<Response<PruneSystemResponse>>;
}

// Server modules for compatibility
pub mod container_service_server {
    // use super::*;

    pub use super::ContainerService;

    #[allow(dead_code)]
    pub struct ContainerServiceServer<T> {
        inner: T,
    }

    impl<T> ContainerServiceServer<T> {
        #[allow(dead_code)]
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
}

pub mod image_service_server {
    // use super::*;

    pub use super::ImageService;

    #[allow(dead_code)]
    pub struct ImageServiceServer<T> {
        inner: T,
    }

    impl<T> ImageServiceServer<T> {
        #[allow(dead_code)]
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
}

pub mod volume_service_server {
    // use super::*;

    pub use super::VolumeService;

    #[allow(dead_code)]
    pub struct VolumeServiceServer<T> {
        inner: T,
    }

    impl<T> VolumeServiceServer<T> {
        #[allow(dead_code)]
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
}

pub mod network_service_server {
    // use super::*;

    pub use super::NetworkService;

    #[allow(dead_code)]
    pub struct NetworkServiceServer<T> {
        inner: T,
    }

    impl<T> NetworkServiceServer<T> {
        #[allow(dead_code)]
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
}

pub mod system_service_server {
    // use super::*;

    pub use super::SystemService;

    #[allow(dead_code)]
    pub struct SystemServiceServer<T> {
        inner: T,
    }

    impl<T> SystemServiceServer<T> {
        #[allow(dead_code)]
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
} 