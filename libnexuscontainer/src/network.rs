use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    pub bridge_name: Option<String>,
    pub interface_name: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub gateway: Option<IpAddr>,
    pub dns_servers: Vec<IpAddr>,
    pub port_mappings: Vec<PortMapping>,
    pub hostname: Option<String>,
    pub domain: Option<String>,
    pub extra_hosts: HashMap<String, IpAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMode {
    Bridge,
    Host,
    None,
    Container(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_ip: Option<IpAddr>,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Protocol {
    TCP,
    UDP,
}

impl Protocol {
    pub fn to_lowercase(&self) -> &'static str {
        match self {
            Protocol::TCP => "tcp",
            Protocol::UDP => "udp",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub ip_address: IpAddr,
    pub netmask: IpAddr,
    pub mac_address: String,
    pub mtu: u32,
    pub state: InterfaceState,
}

#[derive(Debug, Clone)]
pub enum InterfaceState {
    Up,
    Down,
}

pub struct NetworkManager {
    bridge_name: String,
    subnet: String,
    gateway: IpAddr,
    dns_servers: Vec<IpAddr>,
    allocated_ips: HashMap<String, IpAddr>,
    next_ip: u32,
}

impl NetworkManager {
    pub fn new(bridge_name: String, subnet: String, gateway: IpAddr) -> Result<Self> {
        let manager = Self {
            bridge_name,
            subnet,
            gateway,
            dns_servers: vec![
                IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
                IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4)),
            ],
            allocated_ips: HashMap::new(),
            next_ip: 2, // .1 はゲートウェイなので .2 から開始
        };
        
        manager.ensure_bridge_exists()?;
        Ok(manager)
    }
    
    /// ブリッジネットワークの存在確認・作成
    fn ensure_bridge_exists(&self) -> Result<()> {
        log::debug!("Ensuring bridge {} exists", self.bridge_name);
        
        // ブリッジの存在確認
        let output = Command::new("ip")
            .args(["link", "show", &self.bridge_name])
            .output()
            .map_err(|e| ContainerError::Network(format!("Failed to check bridge: {}", e)))?;
        
        if !output.status.success() {
            // ブリッジを作成
            log::info!("Creating bridge: {}", self.bridge_name);
            self.create_bridge()?;
        } else {
            log::debug!("Bridge {} already exists", self.bridge_name);
        }
        
        Ok(())
    }
    
    /// ブリッジを作成
    fn create_bridge(&self) -> Result<()> {
        // ブリッジインターフェースの作成
        let status = Command::new("ip")
            .args(["link", "add", "name", &self.bridge_name, "type", "bridge"])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to create bridge: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to create bridge {}", self.bridge_name)));
        }
        
        // ブリッジにIPアドレスを設定
        let status = Command::new("ip")
            .args(["addr", "add", &self.subnet, "dev", &self.bridge_name])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to assign IP to bridge: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to assign IP to bridge {}", self.bridge_name)));
        }
        
        // ブリッジを有効化
        let status = Command::new("ip")
            .args(["link", "set", "dev", &self.bridge_name, "up"])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to bring up bridge: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to bring up bridge {}", self.bridge_name)));
        }
        
        log::info!("Bridge {} created successfully", self.bridge_name);
        Ok(())
    }
    
    /// コンテナのネットワークを設定
    pub fn setup_container_network(&mut self, container_id: &str, config: &NetworkConfig) -> Result<NetworkInterface> {
        match &config.mode {
            NetworkMode::Bridge => self.setup_bridge_network(container_id, config),
            NetworkMode::Host => self.setup_host_network(container_id, config),
            NetworkMode::None => self.setup_null_network(container_id, config),
            NetworkMode::Container(target_container) => self.setup_shared_network(container_id, target_container, config),
        }
    }
    
    /// ブリッジネットワークの設定
    fn setup_bridge_network(&mut self, container_id: &str, config: &NetworkConfig) -> Result<NetworkInterface> {
        log::debug!("Setting up bridge network for container {}", container_id);
        
        // コンテナ用の veth ペアを作成
        let host_veth = format!("veth-{}", &container_id[..8]);
        let container_veth = "eth0".to_string();
        
        self.create_veth_pair(&host_veth, &container_veth)?;
        
        // ホスト側 veth をブリッジに接続
        self.attach_to_bridge(&host_veth)?;
        
        // IPアドレスの割り当て
        let ip_address = if let Some(ip) = config.ip_address {
            ip
        } else {
            self.allocate_ip(container_id)?
        };
        
        // コンテナ内でのネットワーク設定（実際には setns システムコールが必要）
        // 現在は簡易実装
        let netif = NetworkInterface {
            name: container_veth,
            ip_address,
            netmask: IpAddr::V4(Ipv4Addr::new(255, 255, 255, 0)),
            mac_address: self.generate_mac_address(container_id),
            mtu: 1500,
            state: InterfaceState::Up,
        };
        
        log::info!("Bridge network configured for container {}: IP {}", container_id, ip_address);
        Ok(netif)
    }
    
    /// ホストネットワークの設定
    fn setup_host_network(&self, container_id: &str, _config: &NetworkConfig) -> Result<NetworkInterface> {
        log::debug!("Setting up host network for container {}", container_id);
        
        // ホストネットワークを使用する場合、特別な設定は不要
        let netif = NetworkInterface {
            name: "host".to_string(),
            ip_address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), // プレースホルダー
            netmask: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            mac_address: "00:00:00:00:00:00".to_string(),
            mtu: 1500,
            state: InterfaceState::Up,
        };
        
        log::info!("Host network configured for container {}", container_id);
        Ok(netif)
    }
    
    /// ネットワークなしの設定
    fn setup_null_network(&self, container_id: &str, _config: &NetworkConfig) -> Result<NetworkInterface> {
        log::debug!("Setting up null network for container {}", container_id);
        
        // ループバックインターフェースのみ
        let netif = NetworkInterface {
            name: "lo".to_string(),
            ip_address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            netmask: IpAddr::V4(Ipv4Addr::new(255, 0, 0, 0)),
            mac_address: "00:00:00:00:00:00".to_string(),
            mtu: 65536,
            state: InterfaceState::Up,
        };
        
        log::info!("Null network configured for container {}", container_id);
        Ok(netif)
    }
    
    /// 共有ネットワークの設定
    fn setup_shared_network(&self, container_id: &str, target_container: &str, _config: &NetworkConfig) -> Result<NetworkInterface> {
        log::debug!("Setting up shared network for container {} with {}", container_id, target_container);
        
        // 他のコンテナのネットワーク名前空間を共有
        // 実装は複雑になるため、現在は簡易実装
        let netif = NetworkInterface {
            name: "shared".to_string(),
            ip_address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            netmask: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            mac_address: "00:00:00:00:00:00".to_string(),
            mtu: 1500,
            state: InterfaceState::Up,
        };
        
        log::info!("Shared network configured for container {} with {}", container_id, target_container);
        Ok(netif)
    }
    
    /// veth ペアを作成
    fn create_veth_pair(&self, host_veth: &str, container_veth: &str) -> Result<()> {
        let status = Command::new("ip")
            .args([
                "link", "add", host_veth,
                "type", "veth",
                "peer", "name", container_veth
            ])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to create veth pair: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to create veth pair: {} <-> {}", host_veth, container_veth)));
        }
        
        // ホスト側 veth を有効化
        let status = Command::new("ip")
            .args(["link", "set", "dev", host_veth, "up"])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to bring up host veth: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to bring up host veth: {}", host_veth)));
        }
        
        log::debug!("Created veth pair: {} <-> {}", host_veth, container_veth);
        Ok(())
    }
    
    /// veth をブリッジに接続
    fn attach_to_bridge(&self, veth_name: &str) -> Result<()> {
        let status = Command::new("ip")
            .args(["link", "set", "dev", veth_name, "master", &self.bridge_name])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to attach veth to bridge: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to attach {} to bridge {}", veth_name, self.bridge_name)));
        }
        
        log::debug!("Attached {} to bridge {}", veth_name, self.bridge_name);
        Ok(())
    }
    
    /// IPアドレスを割り当て
    fn allocate_ip(&mut self, container_id: &str) -> Result<IpAddr> {
        // 簡易的な IP 割り当て（実際にはより複雑なアルゴリズムが必要）
        let base_ip = Ipv4Addr::new(172, 17, 0, 0);
        let ip = Ipv4Addr::new(172, 17, 0, self.next_ip as u8);
        
        self.next_ip += 1;
        if self.next_ip > 254 {
            return Err(ContainerError::Network("No more IP addresses available".to_string()));
        }
        
        let ip_addr = IpAddr::V4(ip);
        self.allocated_ips.insert(container_id.to_string(), ip_addr);
        
        log::debug!("Allocated IP {} for container {}", ip_addr, container_id);
        Ok(ip_addr)
    }
    
    /// MACアドレスを生成
    fn generate_mac_address(&self, container_id: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        container_id.hash(&mut hasher);
        let hash = hasher.finish();
        
        format!(
            "02:42:{:02x}:{:02x}:{:02x}:{:02x}",
            (hash >> 24) & 0xff,
            (hash >> 16) & 0xff,
            (hash >> 8) & 0xff,
            hash & 0xff
        )
    }
    
    /// ポートマッピングを設定
    pub fn setup_port_mappings(&self, container_id: &str, port_mappings: &[PortMapping]) -> Result<()> {
        let container_ip = self.allocated_ips.get(container_id)
            .ok_or_else(|| ContainerError::Network(format!("No IP allocated for container {}", container_id)))?;
        
        for mapping in port_mappings {
            self.setup_port_mapping(container_ip, mapping)?;
        }
        
        Ok(())
    }
    
    /// 個別のポートマッピングを設定
    fn setup_port_mapping(&self, container_ip: &IpAddr, mapping: &PortMapping) -> Result<()> {
        let protocol = match mapping.protocol {
            Protocol::TCP => "tcp",
            Protocol::UDP => "udp",
        };
        
        let host_ip = mapping.host_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());
        
        // iptables を使用してポートフォワーディングを設定
        let status = Command::new("iptables")
            .args([
                "-t", "nat",
                "-A", "PREROUTING",
                "-p", protocol,
                "-d", &host_ip,
                "--dport", &mapping.host_port.to_string(),
                "-j", "DNAT",
                "--to-destination", &format!("{}:{}", container_ip, mapping.container_port)
            ])
            .status()
            .map_err(|e| ContainerError::Network(format!("Failed to setup port mapping: {}", e)))?;
        
        if !status.success() {
            return Err(ContainerError::Network(format!("Failed to setup port mapping: {}:{} -> {}:{}", 
                host_ip, mapping.host_port, container_ip, mapping.container_port)));
        }
        
        log::info!("Port mapping configured: {}:{} -> {}:{} ({})", 
            host_ip, mapping.host_port, container_ip, mapping.container_port, protocol);
        
        Ok(())
    }
    
    /// DNS設定を作成
    pub fn setup_dns(&self, container_rootfs: &Path, config: &NetworkConfig) -> Result<()> {
        let resolv_conf_path = container_rootfs.join("etc/resolv.conf");
        
        // etc ディレクトリが存在しない場合は作成
        if let Some(parent) = resolv_conf_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ContainerError::Network(format!("Failed to create /etc directory: {}", e)))?;
        }
        
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&resolv_conf_path)
            .map_err(|e| ContainerError::Network(format!("Failed to create resolv.conf: {}", e)))?;
        
        // ドメイン設定
        if let Some(ref domain) = config.domain {
            writeln!(file, "domain {}", domain)
                .map_err(|e| ContainerError::Network(format!("Failed to write domain to resolv.conf: {}", e)))?;
        }
        
        // DNS サーバー設定
        let dns_servers = if config.dns_servers.is_empty() {
            &self.dns_servers
        } else {
            &config.dns_servers
        };
        
        for dns in dns_servers {
            writeln!(file, "nameserver {}", dns)
                .map_err(|e| ContainerError::Network(format!("Failed to write nameserver to resolv.conf: {}", e)))?;
        }
        
        log::debug!("DNS configuration written to resolv.conf");
        Ok(())
    }
    
    /// hosts ファイルを設定
    pub fn setup_hosts(&self, container_rootfs: &Path, config: &NetworkConfig) -> Result<()> {
        let hosts_path = container_rootfs.join("etc/hosts");
        
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&hosts_path)
            .map_err(|e| ContainerError::Network(format!("Failed to create hosts file: {}", e)))?;
        
        // デフォルトエントリ
        writeln!(file, "127.0.0.1\tlocalhost")
            .map_err(|e| ContainerError::Network(format!("Failed to write localhost to hosts: {}", e)))?;
        
        writeln!(file, "::1\tlocalhost ip6-localhost ip6-loopback")
            .map_err(|e| ContainerError::Network(format!("Failed to write localhost to hosts: {}", e)))?;
        
        // ホスト名の設定
        if let Some(ref hostname) = config.hostname {
            writeln!(file, "127.0.1.1\t{}", hostname)
                .map_err(|e| ContainerError::Network(format!("Failed to write hostname to hosts: {}", e)))?;
        }
        
        // 追加ホストエントリ
        for (hostname, ip) in &config.extra_hosts {
            writeln!(file, "{}\t{}", ip, hostname)
                .map_err(|e| ContainerError::Network(format!("Failed to write extra host to hosts: {}", e)))?;
        }
        
        log::debug!("Hosts configuration written to /etc/hosts");
        Ok(())
    }
    
    /// コンテナネットワークのクリーンアップ
    pub fn cleanup_container_network(&mut self, container_id: &str) -> Result<()> {
        log::info!("Cleaning up network for container {}", container_id);
        
        // IPアドレスの解放
        if let Some(ip) = self.allocated_ips.remove(container_id) {
            log::info!("Released IP {} for container {}", ip, container_id);
        }
        
        // veth インターフェースの削除
        let host_veth = format!("veth-{}", &container_id[..8]);
        let output = Command::new("ip")
            .args(["link", "delete", &host_veth])
            .output();
            
        match output {
            Ok(output) => {
                if output.status.success() {
                    log::info!("Deleted veth interface: {}", host_veth);
                } else {
                    log::warn!("Failed to delete veth interface {}: {}", 
                        host_veth, String::from_utf8_lossy(&output.stderr));
                }
            }
            Err(e) => {
                log::warn!("Failed to execute ip command for veth deletion: {}", e);
            }
        }
        
        // ポートマッピングの削除
        if let Some(container_ip) = self.allocated_ips.get(container_id) {
            self.cleanup_port_mappings(container_id, container_ip)?;
        }
        
        log::info!("Network cleanup completed for container {}", container_id);
        Ok(())
    }
    
    /// コンテナのポートマッピングをクリーンアップ
    fn cleanup_port_mappings(&self, container_id: &str, container_ip: &IpAddr) -> Result<()> {
        // iptablesのNATテーブルからコンテナ関連のルールを削除
        #[cfg(unix)]
        {
            let output = Command::new("iptables")
                .args(&["-t", "nat", "-L", "PREROUTING", "-n", "--line-numbers"])
                .output();
                
            if let Ok(output) = output {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    let mut rules_to_delete = Vec::new();
                    
                    // コンテナIPを含むルールを特定
                    for line in output_str.lines() {
                        if line.contains(&container_ip.to_string()) {
                            if let Some(line_num) = line.split_whitespace().next() {
                                if let Ok(num) = line_num.parse::<u32>() {
                                    rules_to_delete.push(num);
                                }
                            }
                        }
                    }
                    
                    // ルールを逆順で削除（行番号の変更を防ぐため）
                    rules_to_delete.sort_by(|a, b| b.cmp(a));
                    for rule_num in rules_to_delete {
                        let _ = Command::new("iptables")
                            .args(&["-t", "nat", "-D", "PREROUTING", &rule_num.to_string()])
                            .status();
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// ネットワーク統計を取得
    pub fn get_basic_network_stats(&self) -> NetworkStats {
        NetworkStats {
            network_name: self.bridge_name.clone(),
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            rx_errors: 0,
            tx_errors: 0,
            rx_dropped: 0,
            tx_dropped: 0,
        }
    }

    /// ポートマッピングを削除
    pub fn remove_port_mapping(&mut self, _network_name: &str, container_id: &str, mapping: &PortMapping) -> Result<()> {
        let container_ip = self.allocated_ips.get(container_id)
            .ok_or_else(|| ContainerError::Network(format!("No IP allocated for container {}", container_id)))?;
            
        log::info!("Removing port mapping for container {}: {}:{} -> {}:{}", 
                   container_id, 
                   mapping.host_ip.map(|ip| ip.to_string()).unwrap_or_else(|| "0.0.0.0".to_string()), 
                   mapping.host_port, 
                   container_ip,
                   mapping.container_port);

        #[cfg(unix)]
        {
            let protocol = match mapping.protocol {
                Protocol::TCP => "tcp",
                Protocol::UDP => "udp",
            };
            
            let host_ip = mapping.host_ip
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "0.0.0.0".to_string());
            
            // iptablesルールを削除
            let output = Command::new("iptables")
                .args(&[
                    "-t", "nat",
                    "-D", "PREROUTING",
                    "-p", protocol,
                    "-d", &host_ip,
                    "--dport", &mapping.host_port.to_string(),
                    "-j", "DNAT",
                    "--to-destination", &format!("{}:{}", container_ip, mapping.container_port)
                ])
                .output()
                .map_err(|e| ContainerError::Network(format!("Failed to execute iptables: {}", e)))?;

            if !output.status.success() {
                log::warn!("Failed to remove iptables rule: {}", String::from_utf8_lossy(&output.stderr));
            } else {
                log::info!("Port mapping removed successfully");
            }
        }

        #[cfg(not(unix))]
        {
            log::warn!("Port mapping removal not supported on this platform");
        }

        Ok(())
    }

    /// ネットワークトラフィック統計を取得
    pub fn get_network_stats(&self, _network_name: &str) -> Result<NetworkStats> {
        log::debug!("Getting network statistics for bridge: {}", self.bridge_name);

        #[cfg(unix)]
        {
            // ネットワークインターフェースの統計を取得
            let rx_bytes = self.get_interface_stat(&self.bridge_name, "rx_bytes")?;
            let tx_bytes = self.get_interface_stat(&self.bridge_name, "tx_bytes")?;
            let rx_packets = self.get_interface_stat(&self.bridge_name, "rx_packets")?;
            let tx_packets = self.get_interface_stat(&self.bridge_name, "tx_packets")?;
            let rx_errors = self.get_interface_stat(&self.bridge_name, "rx_errors")?;
            let tx_errors = self.get_interface_stat(&self.bridge_name, "tx_errors")?;

            Ok(NetworkStats {
                network_name: self.bridge_name.clone(),
                rx_bytes,
                tx_bytes,
                rx_packets,
                tx_packets,
                rx_errors,
                tx_errors,
                rx_dropped: self.get_interface_stat(&self.bridge_name, "rx_dropped").unwrap_or(0),
                tx_dropped: self.get_interface_stat(&self.bridge_name, "tx_dropped").unwrap_or(0),
            })
        }

        #[cfg(not(unix))]
        {
            // Windows等では簡易的な統計を返す
            Ok(NetworkStats {
                network_name: self.bridge_name.clone(),
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
                rx_errors: 0,
                tx_errors: 0,
                rx_dropped: 0,
                tx_dropped: 0,
            })
        }
    }

    /// インターフェース統計値を取得
    #[cfg(unix)]
    fn get_interface_stat(&self, interface: &str, stat_name: &str) -> Result<u64> {
        let stat_path = format!("/sys/class/net/{}/statistics/{}", interface, stat_name);
        
        match std::fs::read_to_string(&stat_path) {
            Ok(content) => {
                content.trim().parse::<u64>()
                    .map_err(|e| ContainerError::Network(format!("Failed to parse stat {}: {}", stat_name, e)))
            }
            Err(_) => {
                // ファイルが存在しない場合は0を返す
                Ok(0)
            }
        }
    }

    /// ネットワーク設定を検証
    pub fn validate_network_config(&self, config: &NetworkConfig) -> Result<Vec<String>> {
        let warnings = Vec::new();
        
        // 基本的な検証のみ実装
        if let Some(ref bridge_name) = config.bridge_name {
            if bridge_name.is_empty() {
                return Err(ContainerError::Network("Bridge name cannot be empty".to_string()));
            }
        }
        
        Ok(warnings)
    }

    /// ネットワーク使用量をクリーンアップ
    pub fn cleanup_unused_networks(&mut self) -> Result<Vec<String>> {
        let removed_networks = Vec::new();
        
        // 簡易実装：実際の使用ではより詳細な削除ロジックが必要
        log::info!("Network cleanup completed");
        
        Ok(removed_networks)
    }
}

/// 2つのサブネットが重複するかチェック
fn networks_overlap(subnet1: &str, subnet2: &str) -> bool {
    // 簡易的なチェック実装
    // 実際の実装では、IPアドレス範囲の重複を正確に計算する必要がある
    subnet1 == subnet2
}

/// ネットワーク統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub network_name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Bridge,
            bridge_name: Some("nexus0".to_string()),
            interface_name: Some("eth0".to_string()),
            ip_address: None,
            gateway: Some(IpAddr::V4(Ipv4Addr::new(172, 17, 0, 1))),
            dns_servers: vec![
                IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
                IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4)),
            ],
            port_mappings: Vec::new(),
            hostname: None,
            domain: None,
            extra_hosts: HashMap::new(),
        }
    }
}

impl PortMapping {
    pub fn new(host_port: u16, container_port: u16, protocol: Protocol) -> Self {
        Self {
            host_ip: None,
            host_port,
            container_port,
            protocol,
        }
    }
    
    pub fn tcp(host_port: u16, container_port: u16) -> Self {
        Self::new(host_port, container_port, Protocol::TCP)
    }
    
    pub fn udp(host_port: u16, container_port: u16) -> Self {
        Self::new(host_port, container_port, Protocol::UDP)
    }
} 