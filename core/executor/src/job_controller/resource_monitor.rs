use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use log::{debug, error, warn, trace};
use metrics::{gauge, histogram};
use sysinfo::{System, SystemExt, Process, ProcessExt, ProcessorExt, CpuExt, DiskExt, NetworkExt};
use tokio::time::timeout;

use super::job::Job;

/// システムリソースモニター
/// システムとジョブのリソース使用状況をモニタリングするコンポーネント
pub struct ResourceMonitor {
    /// システム情報
    system: Arc<RwLock<System>>,
    /// 最終更新時刻
    last_update: Arc<RwLock<Instant>>,
    /// CPU使用率の履歴
    cpu_usage_history: Arc<RwLock<Vec<f32>>>,
    /// メモリ使用率の履歴
    memory_usage_history: Arc<RwLock<Vec<f32>>>,
    /// ディスクI/O統計
    disk_io_stats: Arc<RwLock<HashMap<String, (u64, u64)>>>, // (読み取り, 書き込み)
    /// ネットワークI/O統計
    network_io_stats: Arc<RwLock<HashMap<String, (u64, u64)>>>, // (受信, 送信)
    /// 更新間隔
    update_interval: Duration,
    /// 更新タスクの実行状態
    is_running: Arc<RwLock<bool>>,
    /// 履歴の最大サイズ
    history_size: usize,
}

impl ResourceMonitor {
    /// 新しいリソースモニターを作成します
    pub fn new() -> Self {
        let monitor = Self {
            system: Arc::new(RwLock::new(System::new_all())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            cpu_usage_history: Arc::new(RwLock::new(Vec::with_capacity(60))),
            memory_usage_history: Arc::new(RwLock::new(Vec::with_capacity(60))),
            disk_io_stats: Arc::new(RwLock::new(HashMap::new())),
            network_io_stats: Arc::new(RwLock::new(HashMap::new())),
            update_interval: Duration::from_secs(1),
            is_running: Arc::new(RwLock::new(false)),
            history_size: 60, // 1分間の履歴を保持
        };
        
        // 更新タスクを開始
        monitor.start_update_task();
        
        monitor
    }
    
    /// 更新間隔を秒単位で設定します
    pub fn set_update_interval(&mut self, seconds: u64) {
        self.update_interval = Duration::from_secs(seconds);
    }
    
    /// 履歴サイズを設定します
    pub fn set_history_size(&mut self, size: usize) {
        self.history_size = size;
    }
    
    /// 更新タスクを開始します
    fn start_update_task(&self) {
        let system = self.system.clone();
        let last_update = self.last_update.clone();
        let cpu_history = self.cpu_usage_history.clone();
        let memory_history = self.memory_usage_history.clone();
        let disk_stats = self.disk_io_stats.clone();
        let network_stats = self.network_io_stats.clone();
        let interval = self.update_interval;
        let history_size = self.history_size;
        let is_running = self.is_running.clone();
        
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            
            // 実行状態を更新
            {
                let mut running = is_running.write().await;
                *running = true;
            }
            
            loop {
                timer.tick().await;
                
                // システム情報を更新
                {
                    let mut sys = system.write().await;
                    sys.refresh_all();
                    
                    // 最終更新時刻を記録
                    let mut update_time = last_update.write().await;
                    *update_time = Instant::now();
                }
                
                // CPU使用率を記録
                {
                    let sys = system.read().await;
                    let cpu_usage = sys.global_cpu_info().cpu_usage();
                    
                    let mut history = cpu_history.write().await;
                    history.push(cpu_usage);
                    
                    // 履歴サイズを制限
                    if history.len() > history_size {
                        history.remove(0);
                    }
                    
                    // メトリクスを更新
                    gauge!("nexusshell_system_cpu_usage").set(cpu_usage as f64);
                }
                
                // メモリ使用率を記録
                {
                    let sys = system.read().await;
                    let total_memory = sys.total_memory();
                    let used_memory = sys.used_memory();
                    
                    let memory_usage = if total_memory > 0 {
                        (used_memory as f32 / total_memory as f32) * 100.0
                    } else {
                        0.0
                    };
                    
                    let mut history = memory_history.write().await;
                    history.push(memory_usage);
                    
                    // 履歴サイズを制限
                    if history.len() > history_size {
                        history.remove(0);
                    }
                    
                    // メトリクスを更新
                    gauge!("nexusshell_system_memory_usage").set(memory_usage as f64);
                    gauge!("nexusshell_system_memory_total").set(total_memory as f64);
                    gauge!("nexusshell_system_memory_used").set(used_memory as f64);
                }
                
                // ディスクI/O統計を更新
                {
                    let sys = system.read().await;
                    let mut current_stats = disk_stats.write().await;
                    
                    for disk in sys.disks() {
                        let name = disk.name().to_string_lossy().to_string();
                        let read_bytes = disk.total_read_bytes();
                        let write_bytes = disk.total_written_bytes();
                        
                        current_stats.insert(name.clone(), (read_bytes, write_bytes));
                        
                        // メトリクスを更新
                        gauge!("nexusshell_system_disk_read_bytes", "disk" => name.clone()).set(read_bytes as f64);
                        gauge!("nexusshell_system_disk_write_bytes", "disk" => name.clone()).set(write_bytes as f64);
                    }
                }
                
                // ネットワークI/O統計を更新
                {
                    let sys = system.read().await;
                    let mut current_stats = network_stats.write().await;
                    
                    for (interface_name, data) in sys.networks() {
                        let rx_bytes = data.total_received();
                        let tx_bytes = data.total_transmitted();
                        
                        current_stats.insert(interface_name.clone(), (rx_bytes, tx_bytes));
                        
                        // メトリクスを更新
                        gauge!("nexusshell_system_network_rx_bytes", "interface" => interface_name.clone()).set(rx_bytes as f64);
                        gauge!("nexusshell_system_network_tx_bytes", "interface" => interface_name.clone()).set(tx_bytes as f64);
                    }
                }
                
                // 実行状態をチェック
                {
                    let running = is_running.read().await;
                    if !*running {
                        break;
                    }
                }
            }
            
            debug!("リソースモニターの更新タスクを終了しました");
        });
    }
    
    /// システムのリソース使用状況を取得します
    pub async fn system_usage(&self) -> HashMap<String, f64> {
        let mut usage = HashMap::new();
        
        // システム情報を読み取り
        let sys = self.system.read().await;
        
        // CPU使用率
        usage.insert("cpu_usage".to_string(), sys.global_cpu_info().cpu_usage() as f64);
        
        // メモリ使用率
        let total_memory = sys.total_memory();
        let used_memory = sys.used_memory();
        
        usage.insert("memory_total".to_string(), total_memory as f64);
        usage.insert("memory_used".to_string(), used_memory as f64);
        
        if total_memory > 0 {
            usage.insert("memory_usage".to_string(), (used_memory as f64 / total_memory as f64) * 100.0);
        } else {
            usage.insert("memory_usage".to_string(), 0.0);
        }
        
        // スワップ使用率
        let total_swap = sys.total_swap();
        let used_swap = sys.used_swap();
        
        usage.insert("swap_total".to_string(), total_swap as f64);
        usage.insert("swap_used".to_string(), used_swap as f64);
        
        if total_swap > 0 {
            usage.insert("swap_usage".to_string(), (used_swap as f64 / total_swap as f64) * 100.0);
        } else {
            usage.insert("swap_usage".to_string(), 0.0);
        }
        
        usage
    }
    
    /// システムの負荷状況を取得します
    pub async fn system_load(&self) -> HashMap<String, f64> {
        let mut load = HashMap::new();
        
        // CPU負荷
        {
            let history = self.cpu_usage_history.read().await;
            if !history.is_empty() {
                let avg_cpu_load = history.iter().sum::<f32>() / history.len() as f32;
                load.insert("cpu_load".to_string(), avg_cpu_load as f64);
            } else {
                let sys = self.system.read().await;
                load.insert("cpu_load".to_string(), sys.global_cpu_info().cpu_usage() as f64);
            }
        }
        
        // メモリ負荷
        {
            let history = self.memory_usage_history.read().await;
            if !history.is_empty() {
                let avg_memory_load = history.iter().sum::<f32>() / history.len() as f32;
                load.insert("memory_load".to_string(), avg_memory_load as f64);
            } else {
                let sys = self.system.read().await;
                let total_memory = sys.total_memory();
                let used_memory = sys.used_memory();
                
                if total_memory > 0 {
                    load.insert("memory_load".to_string(), (used_memory as f64 / total_memory as f64) * 100.0);
                } else {
                    load.insert("memory_load".to_string(), 0.0);
                }
            }
        }
        
        load
    }
    
    /// 指定されたジョブが実行可能かどうかをリソース状況からチェックします
    pub async fn can_execute(&self, job: &Job) -> bool {
        // システムリソース使用状況を取得
        let system_usage = self.system_usage().await;
        
        // ジョブのリソース制限を取得
        if let Some(limits) = job.resource_limits() {
            // CPU使用率のチェック
            if let Some(max_cpu) = limits.max_cpu_percent {
                let current_cpu = system_usage.get("cpu_usage").cloned().unwrap_or(0.0);
                if current_cpu > max_cpu as f64 {
                    warn!("CPU使用率が高すぎるためジョブを実行できません: {}% > {}%", 
                          current_cpu, max_cpu);
                    return false;
                }
            }
            
            // メモリ使用率のチェック
            if let Some(max_memory) = limits.max_memory_percent {
                let current_memory = system_usage.get("memory_usage").cloned().unwrap_or(0.0);
                if current_memory > max_memory as f64 {
                    warn!("メモリ使用率が高すぎるためジョブを実行できません: {}% > {}%", 
                          current_memory, max_memory);
                    return false;
                }
            }
        }
        
        true
    }
    
    /// 指定されたプロセスのリソース使用状況を取得します
    pub async fn process_usage(&self, pid: u32) -> Option<HashMap<String, f64>> {
        let sys = self.system.read().await;
        
        if let Some(process) = sys.process(sysinfo::Pid::from_u32(pid)) {
            let mut usage = HashMap::new();
            
            // CPU使用率
            usage.insert("cpu_usage".to_string(), process.cpu_usage() as f64);
            
            // メモリ使用量
            usage.insert("memory_used".to_string(), process.memory() as f64);
            
            // 仮想メモリ使用量
            usage.insert("virtual_memory_used".to_string(), process.virtual_memory() as f64);
            
            // ディスク読み取り
            if let Some(read_bytes) = process.disk_usage().total_read_bytes {
                usage.insert("disk_read_bytes".to_string(), read_bytes as f64);
            }
            
            // ディスク書き込み
            if let Some(write_bytes) = process.disk_usage().total_written_bytes {
                usage.insert("disk_write_bytes".to_string(), write_bytes as f64);
            }
            
            Some(usage)
        } else {
            None
        }
    }
    
    /// 指定されたプロセスの子プロセスのリソース使用状況を含めた合計を取得します
    pub async fn process_tree_usage(&self, pid: u32) -> HashMap<String, f64> {
        let mut total_usage = HashMap::new();
        
        // ルートプロセスの使用状況
        if let Some(usage) = self.process_usage(pid).await {
            for (key, value) in usage {
                total_usage.insert(key, value);
            }
        }
        
        // 子プロセスのリストを取得
        let child_pids = self.find_child_processes(pid).await;
        
        // 子プロセスの使用状況を合計
        for child_pid in child_pids {
            if let Some(usage) = self.process_usage(child_pid).await {
                for (key, value) in usage {
                    let total = total_usage.entry(key.clone()).or_insert(0.0);
                    *total += value;
                }
            }
        }
        
        total_usage
    }
    
    /// 指定されたプロセスのすべての子プロセスのPIDを取得します
    async fn find_child_processes(&self, parent_pid: u32) -> Vec<u32> {
        let sys = self.system.read().await;
        let mut child_pids = Vec::new();
        
        // すべてのプロセスをチェック
        for (pid, process) in sys.processes() {
            // 親プロセスIDをチェック
            if let Some(ppid) = process.parent() {
                if ppid.as_u32() == parent_pid {
                    child_pids.push(pid.as_u32());
                    
                    // 孫プロセスも再帰的に追加
                    let grandchild_pids = self.find_child_processes(pid.as_u32()).await;
                    child_pids.extend(grandchild_pids);
                }
            }
        }
        
        child_pids
    }
    
    /// リソース使用状況の監視を停止します
    pub async fn shutdown(&self) {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        debug!("リソースモニターをシャットダウンしました");
    }
    
    /// CPU使用率の履歴を取得します
    pub async fn cpu_usage_history(&self) -> Vec<f32> {
        self.cpu_usage_history.read().await.clone()
    }
    
    /// メモリ使用率の履歴を取得します
    pub async fn memory_usage_history(&self) -> Vec<f32> {
        self.memory_usage_history.read().await.clone()
    }
    
    /// 最後のシステム情報更新からの経過時間を取得します
    pub async fn time_since_last_update(&self) -> Duration {
        self.last_update.read().await.elapsed()
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_resource_monitor_basics() {
        let monitor = ResourceMonitor::new();
        
        // 更新が行われるのを少し待つ
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // システム使用状況が取得できることを確認
        let usage = monitor.system_usage().await;
        assert!(usage.contains_key("cpu_usage"));
        assert!(usage.contains_key("memory_usage"));
        
        // システム負荷が取得できることを確認
        let load = monitor.system_load().await;
        assert!(load.contains_key("cpu_load"));
        
        // モニターをシャットダウン
        monitor.shutdown().await;
    }
} 