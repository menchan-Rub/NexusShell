use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupConfig {
    pub enabled: bool,
    pub version: CgroupVersion,
    pub name: String,
    pub resource_limits: ResourceLimits,
    pub custom_settings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CgroupVersion {
    V1,
    V2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ResourceLimits {
    // CPU制限
    pub cpu_shares: Option<u64>,           // CPU相対重み
    pub cpu_quota: Option<i64>,            // CPU時間クォータ (マイクロ秒)
    pub cpu_period: Option<u64>,           // CPU期間 (マイクロ秒)
    pub cpu_max: Option<String>,           // cgroups v2のCPU制限
    pub cpuset_cpus: Option<String>,       // 利用可能CPUコア
    pub cpuset_mems: Option<String>,       // 利用可能メモリノード
    
    // メモリ制限
    pub memory_limit: Option<u64>,         // メモリ制限 (バイト)
    pub memory_soft_limit: Option<u64>,    // メモリソフト制限
    pub memory_swap_limit: Option<u64>,    // スワップ制限
    pub memory_swappiness: Option<u64>,    // スワップ積極性 (0-100)
    pub memory_oom_kill_disable: Option<bool>, // OOM Killerの無効化
    
    // I/O制限
    pub blkio_weight: Option<u64>,         // I/O重み (10-1000)
    pub blkio_device_read_bps: Vec<DeviceLimit>, // デバイス読み取り制限
    pub blkio_device_write_bps: Vec<DeviceLimit>, // デバイス書き込み制限
    pub blkio_device_read_iops: Vec<DeviceLimit>, // デバイス読み取りIOPS制限
    pub blkio_device_write_iops: Vec<DeviceLimit>, // デバイス書き込みIOPS制限
    
    // PID制限
    pub pids_limit: Option<u64>,           // 最大プロセス数
    
    // ネットワーク制限
    pub net_class_id: Option<u32>,         // ネットワーククラスID
    
    // その他
    pub devices_allow: Vec<DeviceRule>,    // デバイスアクセス許可
    pub devices_deny: Vec<DeviceRule>,     // デバイスアクセス拒否
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLimit {
    pub major: u32,
    pub minor: u32,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRule {
    pub device_type: String,  // "c" (character) or "b" (block) or "a" (all)
    pub major: Option<u32>,
    pub minor: Option<u32>,
    pub permissions: String,  // "r", "w", "m" の組み合わせ
}

pub struct CgroupManager {
    config: CgroupConfig,
    cgroup_path: PathBuf,
    cgroup_root: PathBuf,
}

impl CgroupManager {
    pub fn new(config: CgroupConfig) -> Result<Self> {
        let cgroup_root = Self::detect_cgroup_root()?;
        let cgroup_path = cgroup_root.join(&config.name);
        
        Ok(Self {
            config,
            cgroup_path,
            cgroup_root,
        })
    }
    
    /// Cgroupルートディレクトリを検出
    fn detect_cgroup_root() -> Result<PathBuf> {
        // cgroups v2の統合階層を優先
        let v2_root = PathBuf::from("/sys/fs/cgroup");
        if v2_root.join("cgroup.controllers").exists() {
            return Ok(v2_root);
        }
        
        // cgroups v1のlegacy階層
        let v1_root = PathBuf::from("/sys/fs/cgroup");
        if v1_root.exists() {
            return Ok(v1_root);
        }
        
        Err(ContainerError::Cgroup("Cgroup filesystem not found".to_string()))
    }
    
    /// Cgroupを作成・設定する
    pub fn setup_cgroup(&self, pid: u32) -> Result<()> {
        if !self.config.enabled {
            log::debug!("Cgroups disabled, skipping setup");
            return Ok(());
        }
        
        log::info!("Setting up cgroup: {}", self.config.name);
        
        // Cgroupディレクトリの作成
        std::fs::create_dir_all(&self.cgroup_path)
            .map_err(|e| ContainerError::Cgroup(format!("Failed to create cgroup directory: {}", e)))?;
        
        // プロセスをCgroupに追加
        self.add_process(pid)?;
        
        // リソース制限の設定
        self.apply_resource_limits()?;
        
        // カスタム設定の適用
        self.apply_custom_settings()?;
        
        log::info!("Cgroup setup completed: {}", self.config.name);
        Ok(())
    }
    
    /// プロセスをCgroupに追加
    fn add_process(&self, pid: u32) -> Result<()> {
        let procs_file = self.cgroup_path.join("cgroup.procs");
        let mut file = OpenOptions::new()
            .write(true)
            .open(&procs_file)
            .map_err(|e| ContainerError::Cgroup(format!("Failed to open cgroup.procs: {}", e)))?;
        
        write!(file, "{}", pid)
            .map_err(|e| ContainerError::Cgroup(format!("Failed to add process to cgroup: {}", e)))?;
        
        log::debug!("Added process {} to cgroup {}", pid, self.config.name);
        Ok(())
    }
    
    /// リソース制限を適用
    fn apply_resource_limits(&self) -> Result<()> {
        let limits = &self.config.resource_limits;
        
        // CPU制限
        if let Some(cpu_max) = &limits.cpu_max {
            self.write_cgroup_file("cpu.max", cpu_max)?;
        }
        
        if let Some(cpu_shares) = limits.cpu_shares {
            self.write_cgroup_file("cpu.weight", &cpu_shares.to_string())?;
        }
        
        if let Some(cpuset_cpus) = &limits.cpuset_cpus {
            self.write_cgroup_file("cpuset.cpus", cpuset_cpus)?;
        }
        
        if let Some(cpuset_mems) = &limits.cpuset_mems {
            self.write_cgroup_file("cpuset.mems", cpuset_mems)?;
        }
        
        // メモリ制限
        if let Some(memory_limit) = limits.memory_limit {
            self.write_cgroup_file("memory.max", &memory_limit.to_string())?;
        }
        
        if let Some(memory_soft_limit) = limits.memory_soft_limit {
            self.write_cgroup_file("memory.high", &memory_soft_limit.to_string())?;
        }
        
        if let Some(memory_swap_limit) = limits.memory_swap_limit {
            self.write_cgroup_file("memory.swap.max", &memory_swap_limit.to_string())?;
        }
        
        // I/O制限
        if let Some(blkio_weight) = limits.blkio_weight {
            self.write_cgroup_file("io.weight", &blkio_weight.to_string())?;
        }
        
        // PID制限
        if let Some(pids_limit) = limits.pids_limit {
            self.write_cgroup_file("pids.max", &pids_limit.to_string())?;
        }
        
        log::debug!("Resource limits applied for cgroup {}", self.config.name);
        Ok(())
    }
    
    /// カスタム設定を適用
    fn apply_custom_settings(&self) -> Result<()> {
        for (key, value) in &self.config.custom_settings {
            self.write_cgroup_file(key, value)?;
        }
        Ok(())
    }
    
    /// Cgroupファイルに値を書き込み
    fn write_cgroup_file(&self, filename: &str, value: &str) -> Result<()> {
        let file_path = self.cgroup_path.join(filename);
        
        if !file_path.exists() {
            log::warn!("Cgroup file {} does not exist, skipping", filename);
            return Ok(());
        }
        
        let mut file = OpenOptions::new()
            .write(true)
            .open(&file_path)
            .map_err(|e| ContainerError::Cgroup(format!("Failed to open {}: {}", filename, e)))?;
        
        file.write_all(value.as_bytes())
            .map_err(|e| ContainerError::Cgroup(format!("Failed to write to {}: {}", filename, e)))?;
        
        log::debug!("Set {} = {} for cgroup {}", filename, value, self.config.name);
        Ok(())
    }
    
    /// Cgroupファイルから値を読み取り
    fn read_cgroup_file(&self, filename: &str) -> Result<String> {
        let file_path = self.cgroup_path.join(filename);
        let mut file = File::open(&file_path)
            .map_err(|e| ContainerError::Cgroup(format!("Failed to open {}: {}", filename, e)))?;
        
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| ContainerError::Cgroup(format!("Failed to read {}: {}", filename, e)))?;
        
        Ok(content.trim().to_string())
    }
    
    /// リソース使用統計を取得
    pub fn get_stats(&self) -> Result<CgroupStats> {
        let mut stats = CgroupStats::default();
        
        // CPU統計
        if let Ok(cpu_stat) = self.read_cgroup_file("cpu.stat") {
            stats.cpu_stats = Self::parse_cpu_stat(&cpu_stat);
        }
        
        // メモリ統計
        if let Ok(memory_current) = self.read_cgroup_file("memory.current") {
            stats.memory_usage = memory_current.parse().unwrap_or(0);
        }
        
        if let Ok(memory_stat) = self.read_cgroup_file("memory.stat") {
            stats.memory_stats = Self::parse_memory_stat(&memory_stat);
        }
        
        // PID統計
        if let Ok(pids_current) = self.read_cgroup_file("pids.current") {
            stats.pids_current = pids_current.parse().unwrap_or(0);
        }
        
        // I/O統計
        if let Ok(io_stat) = self.read_cgroup_file("io.stat") {
            stats.io_stats = Self::parse_io_stat(&io_stat);
        }
        
        Ok(stats)
    }
    
    /// CPU統計をパース
    fn parse_cpu_stat(content: &str) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                if let Ok(value) = parts[1].parse::<u64>() {
                    stats.insert(parts[0].to_string(), value);
                }
            }
        }
        stats
    }
    
    /// メモリ統計をパース
    fn parse_memory_stat(content: &str) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                if let Ok(value) = parts[1].parse::<u64>() {
                    stats.insert(parts[0].to_string(), value);
                }
            }
        }
        stats
    }
    
    /// I/O統計をパース
    fn parse_io_stat(content: &str) -> HashMap<String, HashMap<String, u64>> {
        let mut stats = HashMap::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let device = parts[0].to_string();
                let mut device_stats = HashMap::new();
                
                for stat in &parts[1..] {
                    if let Some((key, value)) = stat.split_once('=') {
                        if let Ok(value) = value.parse::<u64>() {
                            device_stats.insert(key.to_string(), value);
                        }
                    }
                }
                
                stats.insert(device, device_stats);
            }
        }
        stats
    }
    
    /// Cgroupを削除
    pub fn cleanup(&self) -> Result<()> {
        if self.cgroup_path.exists() {
            std::fs::remove_dir(&self.cgroup_path)
                .map_err(|e| ContainerError::Cgroup(format!("Failed to remove cgroup directory: {}", e)))?;
            log::info!("Cleaned up cgroup: {}", self.config.name);
        }
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CgroupStats {
    pub cpu_stats: HashMap<String, u64>,
    pub memory_usage: u64,
    pub memory_stats: HashMap<String, u64>,
    pub pids_current: u64,
    pub io_stats: HashMap<String, HashMap<String, u64>>,
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            version: CgroupVersion::V2,
            name: "nexuscontainer".to_string(),
            resource_limits: ResourceLimits::default(),
            custom_settings: HashMap::new(),
        }
    }
}


impl DeviceLimit {
    pub fn new(major: u32, minor: u32, limit: u64) -> Self {
        Self { major, minor, limit }
    }
}

impl DeviceRule {
    pub fn new(device_type: String, major: Option<u32>, minor: Option<u32>, permissions: String) -> Self {
        Self {
            device_type,
            major,
            minor,
            permissions,
        }
    }
    
    /// すべてのデバイスアクセスを許可
    pub fn allow_all() -> Self {
        Self::new("a".to_string(), None, None, "rwm".to_string())
    }
    
    /// すべてのデバイスアクセスを拒否
    pub fn deny_all() -> Self {
        Self::new("a".to_string(), None, None, "".to_string())
    }
} 