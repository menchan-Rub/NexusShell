use anyhow::Result;
use log::{info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::process::Command as TokioCommand;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub socket_path: PathBuf,
    pub pid_file: PathBuf,
    pub log_file: Option<PathBuf>,
    pub log_level: String,
    pub data_root: PathBuf,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let runtime_dir = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        
        Self {
            socket_path: runtime_dir.join("nexusd.sock"),
            pid_file: runtime_dir.join("nexusd.pid"),
            log_file: None,
            log_level: "info".to_string(),
            data_root: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("/var/lib"))
                .join("nexuscontainer"),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DaemonManager {
    config: DaemonConfig,
}

impl DaemonManager {
    #[allow(dead_code)]
    pub fn new(config: DaemonConfig) -> Self {
        Self { config }
    }
    
    /// デーモンが動作中かチェック
    #[allow(dead_code)]
    pub fn is_running(&self) -> Result<bool> {
        if !self.config.pid_file.exists() {
            return Ok(false);
        }
        
        let pid_str = fs::read_to_string(&self.config.pid_file)?;
        let _pid: u32 = pid_str.trim().parse()
            .map_err(|e| anyhow::anyhow!("Invalid PID in file: {}", e))?;
        
        // プロセスが存在するかチェック
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            
            match kill(Pid::from_raw(_pid as i32), Signal::SIGTERM) {
                Ok(_) => Ok(true),
                Err(nix::errno::Errno::ESRCH) => {
                    // プロセスが存在しない
                    let _ = fs::remove_file(&self.config.pid_file);
                    Ok(false)
                }
                Err(e) => Err(anyhow::anyhow!("Failed to check process: {}", e)),
            }
        }
        
        #[cfg(not(unix))]
        {
            // WindowsなどのUnix以外では簡易実装
            Ok(true)
        }
    }
    
    /// デーモンを開始
    #[allow(dead_code)]
    pub async fn start_daemon(&self, binary_path: Option<PathBuf>) -> Result<()> {
        if self.is_running()? {
            return Err(anyhow::anyhow!("Daemon is already running"));
        }
        
        let daemon_binary = binary_path.unwrap_or_else(|| {
            // デフォルトのバイナリパスを推測
            std::env::current_exe()
                .unwrap_or_else(|_| PathBuf::from("nexusd"))
                .parent()
                .unwrap_or(&PathBuf::from("."))
                .join("nexusd")
        });
        
        if !daemon_binary.exists() {
            return Err(anyhow::anyhow!("Daemon binary not found: {}", daemon_binary.display()));
        }
        
        // 必要なディレクトリを作成
        if let Some(parent) = self.config.socket_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.config.pid_file.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // デーモンを起動
        let mut cmd = TokioCommand::new(&daemon_binary);
        cmd.arg("--socket").arg(&self.config.socket_path)
           .arg("--pid-file").arg(&self.config.pid_file)
           .arg("--log-level").arg(&self.config.log_level)
           .arg("--data-root").arg(&self.config.data_root);
        
        if let Some(ref log_file) = self.config.log_file {
            cmd.arg("--log-file").arg(log_file);
        }
        
        // バックグラウンドで実行
        cmd.stdin(Stdio::null())
           .stdout(Stdio::null())
           .stderr(Stdio::null());
        
        let child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        
        info!("Started daemon with PID: {}", pid);
        
        // PIDファイルに書き込み
        fs::write(&self.config.pid_file, pid.to_string())?;
        
        // デーモンが起動するまで少し待つ
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        if !self.is_running()? {
            return Err(anyhow::anyhow!("Failed to start daemon"));
        }
        
        Ok(())
    }
    
    /// デーモンを停止
    #[allow(dead_code)]
    pub fn stop_daemon(&self, force: bool) -> Result<()> {
        if !self.is_running()? {
            warn!("Daemon is not running");
            return Ok(());
        }
        
        let pid_str = fs::read_to_string(&self.config.pid_file)?;
        let pid: u32 = pid_str.trim().parse()
            .map_err(|e| anyhow::anyhow!("Invalid PID in file: {}", e))?;
        
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            
            let signal = if force { Signal::SIGKILL } else { Signal::SIGTERM };
            
            kill(Pid::from_raw(pid as i32), signal)
                .map_err(|e| anyhow::anyhow!("Failed to kill process {}: {}", pid, e))?;
            
            info!("Sent {} signal to daemon (PID: {})", 
                  if force { "SIGKILL" } else { "SIGTERM" }, pid);
        }
        
        #[cfg(not(unix))]
        {
            // WindowsなどのUnix以外では簡易実装
            let output = Command::new("taskkill")
                .arg(if force { "/F" } else { "/T" })
                .arg("/PID")
                .arg(pid.to_string())
                .output()?;
            
            if !output.status.success() {
                return Err(anyhow::anyhow!("Failed to terminate process {}", pid));
            }
        }
        
        // PIDファイルを削除
        let _ = fs::remove_file(&self.config.pid_file);
        
        // ソケットファイルも削除
        if self.config.socket_path.exists() {
            let _ = fs::remove_file(&self.config.socket_path);
        }
        
        Ok(())
    }
    
    /// デーモンを再起動
    #[allow(dead_code)]
    pub async fn restart_daemon(&self, binary_path: Option<PathBuf>) -> Result<()> {
        info!("Restarting daemon...");
        
        // 停止
        if self.is_running()? {
            self.stop_daemon(false)?;
            
            // 停止するまで待つ
            for _ in 0..10 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                if !self.is_running()? {
                    break;
                }
            }
            
            if self.is_running()? {
                warn!("Daemon did not stop gracefully, forcing stop");
                self.stop_daemon(true)?;
            }
        }
        
        // 開始
        self.start_daemon(binary_path).await
    }
    
    /// デーモンの状態を取得
    #[allow(dead_code)]
    pub fn get_status(&self) -> Result<DaemonStatus> {
        let running = self.is_running()?;
        let pid = if running && self.config.pid_file.exists() {
            let pid_str = fs::read_to_string(&self.config.pid_file)?;
            Some(pid_str.trim().parse::<u32>().unwrap_or(0))
        } else {
            None
        };
        
        Ok(DaemonStatus {
            running,
            pid,
            socket_path: self.config.socket_path.clone(),
            pid_file: self.config.pid_file.clone(),
            data_root: self.config.data_root.clone(),
        })
    }
    
    /// デーモンのログを取得
    #[allow(dead_code)]
    pub fn get_logs(&self, lines: Option<usize>) -> Result<Vec<String>> {
        if let Some(ref log_file) = self.config.log_file {
            if log_file.exists() {
                let content = fs::read_to_string(log_file)?;
                let mut log_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                
                if let Some(n) = lines {
                    if log_lines.len() > n {
                        log_lines = log_lines.split_off(log_lines.len() - n);
                    }
                }
                
                Ok(log_lines)
            } else {
                Ok(vec!["Log file not found".to_string()])
            }
        } else {
            Ok(vec!["No log file configured".to_string()])
        }
    }
    
    /// ソケット接続をテスト
    #[allow(dead_code)]
    pub async fn test_connection(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use tokio::net::UnixStream;
            
            if !self.config.socket_path.exists() {
                return Err(anyhow::anyhow!("Socket file does not exist"));
            }
            
            match UnixStream::connect(&self.config.socket_path).await {
                Ok(_) => {
                    info!("Successfully connected to daemon socket");
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!("Failed to connect to daemon: {}", e)),
            }
        }
        
        #[cfg(not(unix))]
        {
            // Unix以外では簡易実装
            if self.is_running()? {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Daemon is not running"))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub socket_path: PathBuf,
    pub pid_file: PathBuf,
    pub data_root: PathBuf,
}

/// デーモン関連のヘルパー関数
pub mod helpers {
    use super::*;
    
    /// デフォルト設定でデーモンマネージャーを作成
    #[allow(dead_code)]
    pub fn create_default_manager() -> DaemonManager {
        DaemonManager::new(DaemonConfig::default())
    }
    
    /// システムサービスとしてデーモンをインストール
    #[allow(dead_code)]
    pub fn install_systemd_service(_config: &DaemonConfig, _binary_path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            let service_content = format!(
                r#"[Unit]
Description=NexusContainer Daemon
After=network.target
Wants=network.target

[Service]
Type=forking
ExecStart={} --socket {} --pid-file {} --log-level {} --data-root {}
PIDFile={}
KillMode=process
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
"#,
                _binary_path.display(),
                _config.socket_path.display(),
                _config.pid_file.display(),
                _config.log_level,
                _config.data_root.display(),
                _config.pid_file.display()
            );
            
            let service_file = PathBuf::from("/etc/systemd/system/nexusd.service");
            fs::write(&service_file, service_content)?;
            
            info!("Systemd service file created: {}", service_file.display());
            
            // systemctl daemon-reload
            let output = Command::new("systemctl")
                .arg("daemon-reload")
                .output()?;
            
            if !output.status.success() {
                return Err(anyhow::anyhow!("Failed to reload systemd daemon"));
            }
            
            info!("Systemd daemon reloaded");
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Err(anyhow::anyhow!("Systemd service installation is only supported on Unix systems"))
        }
    }
    
    /// システムサービスを削除
    #[allow(dead_code)]
    pub fn uninstall_systemd_service() -> Result<()> {
        #[cfg(unix)]
        {
            let service_file = PathBuf::from("/etc/systemd/system/nexusd.service");
            
            if service_file.exists() {
                // サービスを停止
                let _ = Command::new("systemctl")
                    .arg("stop")
                    .arg("nexusd")
                    .output();
                
                // サービスを無効化
                let _ = Command::new("systemctl")
                    .arg("disable")
                    .arg("nexusd")
                    .output();
                
                // ファイルを削除
                fs::remove_file(&service_file)?;
                
                // reload
                let _ = Command::new("systemctl")
                    .arg("daemon-reload")
                    .output();
                
                info!("Systemd service uninstalled");
            }
            
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Err(anyhow::anyhow!("Systemd service uninstallation is only supported on Unix systems"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();
        assert!(!config.socket_path.as_os_str().is_empty());
        assert!(!config.pid_file.as_os_str().is_empty());
        assert_eq!(config.log_level, "info");
    }
    
    #[test]
    fn test_daemon_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = DaemonConfig {
            socket_path: temp_dir.path().join("test.sock"),
            pid_file: temp_dir.path().join("test.pid"),
            log_file: None,
            log_level: "debug".to_string(),
            data_root: temp_dir.path().to_path_buf(),
        };
        
        let manager = DaemonManager::new(config);
        assert!(!manager.is_running().unwrap());
    }
    
    #[tokio::test]
    async fn test_daemon_status() {
        let temp_dir = TempDir::new().unwrap();
        let config = DaemonConfig {
            socket_path: temp_dir.path().join("test.sock"),
            pid_file: temp_dir.path().join("test.pid"),
            log_file: None,
            log_level: "debug".to_string(),
            data_root: temp_dir.path().to_path_buf(),
        };
        
        let manager = DaemonManager::new(config);
        let status = manager.get_status().unwrap();
        
        assert!(!status.running);
        assert!(status.pid.is_none());
    }
} 