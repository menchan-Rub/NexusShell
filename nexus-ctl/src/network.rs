use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::time::SystemTime;

/// ネットワーク情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NetworkInfo {
    pub name: String,
    pub id: String,
    pub driver: String,
    pub subnet: Option<String>,
    pub gateway: Option<IpAddr>,
    pub containers: Vec<String>,
    pub created: SystemTime,
    pub labels: HashMap<String, String>,
}

/// ネットワークヘルパー
#[allow(dead_code)]
pub struct NetworkHelper;

impl NetworkHelper {
    /// IPアドレスの検証
    #[allow(dead_code)]
    pub fn validate_ip(ip_str: &str) -> Result<IpAddr> {
        ip_str.parse::<IpAddr>()
            .map_err(|e| anyhow::anyhow!("Invalid IP address '{}': {}", ip_str, e))
    }
    
    /// CIDR記法の検証
    #[allow(dead_code)]
    pub fn validate_subnet(subnet_str: &str) -> Result<(IpAddr, u8)> {
        let parts: Vec<&str> = subnet_str.split('/').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid subnet format. Expected CIDR notation (e.g., 192.168.1.0/24)"));
        }
        
        let ip = Self::validate_ip(parts[0])?;
        let prefix_len: u8 = parts[1].parse()
            .map_err(|e| anyhow::anyhow!("Invalid prefix length '{}': {}", parts[1], e))?;
        
        // IPv4の場合は/32、IPv6の場合は/128が最大
        let max_prefix = match ip {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        
        if prefix_len > max_prefix {
            return Err(anyhow::anyhow!("Prefix length {} exceeds maximum {} for this IP version", 
                                     prefix_len, max_prefix));
        }
        
        Ok((ip, prefix_len))
    }
    
    /// ポートマッピングの解析
    #[allow(dead_code)]
    pub fn parse_port_mapping(port_str: &str) -> Result<PortMapping> {
        // 形式: [host_ip:]host_port:container_port[/protocol]
        let parts: Vec<&str> = port_str.split(':').collect();
        
        match parts.len() {
            1 => {
                // container_port only
                let (port, protocol) = Self::parse_port_protocol(parts[0])?;
                Ok(PortMapping {
                    host_ip: None,
                    host_port: port,
                    container_port: port,
                    protocol,
                })
            }
            2 => {
                // host_port:container_port
                let host_port = Self::parse_port(parts[0])?;
                let (container_port, protocol) = Self::parse_port_protocol(parts[1])?;
                Ok(PortMapping {
                    host_ip: None,
                    host_port,
                    container_port,
                    protocol,
                })
            }
            3 => {
                // host_ip:host_port:container_port
                let host_ip = Self::validate_ip(parts[0])?;
                let host_port = Self::parse_port(parts[1])?;
                let (container_port, protocol) = Self::parse_port_protocol(parts[2])?;
                Ok(PortMapping {
                    host_ip: Some(host_ip),
                    host_port,
                    container_port,
                    protocol,
                })
            }
            _ => Err(anyhow::anyhow!("Invalid port mapping format: {}", port_str)),
        }
    }
    
    #[allow(dead_code)]
    fn parse_port(port_str: &str) -> Result<u16> {
        port_str.parse::<u16>()
            .map_err(|e| anyhow::anyhow!("Invalid port number '{}': {}", port_str, e))
    }
    
    #[allow(dead_code)]
    fn parse_port_protocol(port_str: &str) -> Result<(u16, Protocol)> {
        if let Some(slash_pos) = port_str.rfind('/') {
            let (port_part, protocol_part) = port_str.split_at(slash_pos);
            let port = Self::parse_port(port_part)?;
            let protocol = match protocol_part[1..].to_lowercase().as_str() {
                "tcp" => Protocol::Tcp,
                "udp" => Protocol::Udp,
                "sctp" => Protocol::Sctp,
                _ => return Err(anyhow::anyhow!("Unsupported protocol: {}", &protocol_part[1..])),
            };
            Ok((port, protocol))
        } else {
            let port = Self::parse_port(port_str)?;
            Ok((port, Protocol::Tcp)) // デフォルトはTCP
        }
    }
    
    /// ネットワーク名の検証
    #[allow(dead_code)]
    pub fn validate_network_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!("Network name cannot be empty"));
        }
        
        if name.len() > 64 {
            return Err(anyhow::anyhow!("Network name too long (max 64 characters)"));
        }
        
        // 英数字、ハイフン、アンダースコア、ピリオドのみ許可
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(anyhow::anyhow!("Network name can only contain alphanumeric characters, hyphens, underscores, and periods"));
        }
        
        // 先頭と末尾は英数字のみ
        if let (Some(first), Some(last)) = (name.chars().next(), name.chars().last()) {
            if !first.is_alphanumeric() || !last.is_alphanumeric() {
                return Err(anyhow::anyhow!("Network name must start and end with an alphanumeric character"));
            }
        }
        
        Ok(())
    }
    
    /// デフォルトゲートウェイの計算
    #[allow(dead_code)]
    pub fn calculate_gateway(subnet: &str) -> Result<IpAddr> {
        let (network_ip, _prefix_len) = Self::validate_subnet(subnet)?;
        
        match network_ip {
            IpAddr::V4(ipv4) => {
                // IPv4の場合、通常は最初のアドレス（.1）をゲートウェイとする
                let octets = ipv4.octets();
                let gateway = Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3] + 1);
                Ok(IpAddr::V4(gateway))
            }
            IpAddr::V6(_) => {
                // IPv6の場合は簡易実装
                // 実際にはより複雑な計算が必要
                Ok(network_ip)
            }
        }
    }
    
    /// IPアドレスがサブネット内にあるかチェック
    #[allow(dead_code)]
    pub fn ip_in_subnet(ip: &IpAddr, subnet: &str) -> Result<bool> {
        let (network_ip, prefix_len) = Self::validate_subnet(subnet)?;
        
        match (ip, network_ip) {
            (IpAddr::V4(ip), IpAddr::V4(network)) => {
                let ip_int = u32::from_be_bytes(ip.octets());
                let network_int = u32::from_be_bytes(network.octets());
                let mask = !((1u32 << (32 - prefix_len)) - 1);
                
                Ok((ip_int & mask) == (network_int & mask))
            }
            (IpAddr::V6(_), IpAddr::V6(_)) => {
                // IPv6の場合は簡易実装
                // 実際にはビット演算が必要
                Ok(false)
            }
            _ => Ok(false), // IPバージョンが違う場合
        }
    }
}

/// ポートマッピング情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PortMapping {
    pub host_ip: Option<IpAddr>,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: Protocol,
}

impl fmt::Display for PortMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let protocol_str = match self.protocol {
            Protocol::Tcp => "",
            Protocol::Udp => "/udp",
            Protocol::Sctp => "/sctp",
        };

        if let Some(host_ip) = self.host_ip {
            write!(f, "{}:{}->{}{}{}",
                host_ip,
                self.host_port,
                self.container_port,
                protocol_str,
                if self.host_port != self.container_port { 
                    format!(":{}", self.container_port) 
                } else { 
                    String::new() 
                }
            )
        } else {
            write!(f, "{}->{}{}",
                self.host_port,
                self.container_port,
                protocol_str
            )
        }
    }
}

/// プロトコル
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Protocol {
    Tcp,
    Udp,
    Sctp,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_ip() {
        assert!(NetworkHelper::validate_ip("192.168.1.1").is_ok());
        assert!(NetworkHelper::validate_ip("::1").is_ok());
        assert!(NetworkHelper::validate_ip("invalid").is_err());
    }
    
    #[test]
    fn test_validate_subnet() {
        assert!(NetworkHelper::validate_subnet("192.168.1.0/24").is_ok());
        assert!(NetworkHelper::validate_subnet("10.0.0.0/8").is_ok());
        assert!(NetworkHelper::validate_subnet("192.168.1.0").is_err());
        assert!(NetworkHelper::validate_subnet("192.168.1.0/33").is_err());
    }
    
    #[test]
    fn test_parse_port_mapping() {
        let mapping = NetworkHelper::parse_port_mapping("80").unwrap();
        assert_eq!(mapping.host_port, 80);
        assert_eq!(mapping.container_port, 80);
        assert_eq!(mapping.protocol, Protocol::Tcp);
        
        let mapping = NetworkHelper::parse_port_mapping("8080:80").unwrap();
        assert_eq!(mapping.host_port, 8080);
        assert_eq!(mapping.container_port, 80);
        
        let mapping = NetworkHelper::parse_port_mapping("192.168.1.1:8080:80/udp").unwrap();
        assert!(mapping.host_ip.is_some());
        assert_eq!(mapping.host_port, 8080);
        assert_eq!(mapping.container_port, 80);
        assert_eq!(mapping.protocol, Protocol::Udp);
    }
    
    #[test]
    fn test_validate_network_name() {
        assert!(NetworkHelper::validate_network_name("my-network").is_ok());
        assert!(NetworkHelper::validate_network_name("network_1").is_ok());
        assert!(NetworkHelper::validate_network_name("bridge0").is_ok());
        
        assert!(NetworkHelper::validate_network_name("").is_err());
        assert!(NetworkHelper::validate_network_name("-invalid").is_err());
        assert!(NetworkHelper::validate_network_name("invalid-").is_err());
        assert!(NetworkHelper::validate_network_name("with spaces").is_err());
    }
    
    #[test]
    fn test_calculate_gateway() {
        let gateway = NetworkHelper::calculate_gateway("192.168.1.0/24").unwrap();
        if let IpAddr::V4(ipv4) = gateway {
            assert_eq!(ipv4, Ipv4Addr::new(192, 168, 1, 1));
        } else {
            panic!("Expected IPv4 address");
        }
    }
    
    #[test]
    fn test_ip_in_subnet() {
        let ip = "192.168.1.100".parse().unwrap();
        assert!(NetworkHelper::ip_in_subnet(&ip, "192.168.1.0/24").unwrap());
        
        let ip = "192.168.2.100".parse().unwrap();
        assert!(!NetworkHelper::ip_in_subnet(&ip, "192.168.1.0/24").unwrap());
    }
    
    #[test]
    fn test_port_mapping_to_string() {
        let mapping = PortMapping {
            host_ip: None,
            host_port: 8080,
            container_port: 80,
            protocol: Protocol::Tcp,
        };
        assert_eq!(mapping.to_string(), "8080:80");
        
        let mapping = PortMapping {
            host_ip: Some("192.168.1.1".parse().unwrap()),
            host_port: 8080,
            container_port: 80,
            protocol: Protocol::Udp,
        };
        assert_eq!(mapping.to_string(), "192.168.1.1:8080:80/udp");
    }
} 