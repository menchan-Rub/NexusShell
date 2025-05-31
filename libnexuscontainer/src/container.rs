use crate::config::ContainerConfig;
use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::time::SystemTime;
use uuid::Uuid;

/// コンテナログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLog {
    pub timestamp: SystemTime,
    pub message: String,
}

/// コンテナの実行状態
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContainerState {
    Created,
    Running,
    Stopped,
    Paused,
}

impl ContainerState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContainerState::Created => "created",
            ContainerState::Running => "running",
            ContainerState::Stopped => "stopped",
            ContainerState::Paused => "paused",
        }
    }
}

/// コンテナステータス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatus {
    pub id: String,
    pub state: ContainerState,
    pub pid: Option<u32>,
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub exit_code: Option<i32>,
}

/// コンテナインスタンス
#[derive(Debug, Serialize, Deserialize)]
pub struct Container {
    config: ContainerConfig,
    pub state: ContainerState,
    pub pid: Option<u32>,
    pub id: String,
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub exit_code: Option<i32>,
    #[serde(skip)]
    #[allow(dead_code)]
    process: Option<Child>,
    pub logs: Vec<ContainerLog>,
}

/// コンテナ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ContainerConfiguration {
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

impl Container {
    /// 新しいコンテナを作成
    pub fn new(config: ContainerConfig) -> Result<Self> {
        let id = Uuid::new_v4().to_string();
        Ok(Self {
            config,
            state: ContainerState::Created,
            pid: None,
            id,
            created_at: SystemTime::now(),
            started_at: None,
            finished_at: None,
            exit_code: None,
            process: None,
            logs: Vec::new(),
        })
    }

    /// コンテナを開始
    pub fn start(&mut self) -> Result<()> {
        if self.state != ContainerState::Created {
            return Err(ContainerError::InvalidState(format!(
                "Cannot start container in state: {:?}",
                self.state
            )));
        }

        // プラットフォーム固有の実装
        #[cfg(unix)]
        {
            self.start_unix()?;
        }

        #[cfg(windows)]
        {
            self.start_windows()?;
        }

        #[cfg(not(any(unix, windows)))]
        {
            return Err(ContainerError::UnsupportedFeature(
                "Container start not supported on this platform".to_string()
            ));
        }

        self.state = ContainerState::Running;
        self.started_at = Some(SystemTime::now());

        Ok(())
    }

    /// コンテナを停止
    pub fn stop(&mut self) -> Result<()> {
        if self.state != ContainerState::Running {
            return Err(ContainerError::InvalidState(format!(
                "Cannot stop container in state: {:?}",
                self.state
            )));
        }

        // プロセスを終了
        if let Some(ref mut process) = self.process {
            let _ = process.kill();
            let _ = process.wait();
        }

        self.state = ContainerState::Stopped;
        self.finished_at = Some(SystemTime::now());

        Ok(())
    }

    /// コンテナを削除
    pub fn remove(&mut self) -> Result<()> {
        if self.state == ContainerState::Running {
            self.stop()?;
        }

        // クリーンアップ処理
        self.cleanup()?;

        Ok(())
    }

    /// コンテナの状態を取得
    pub fn get_state(&self) -> Result<ContainerState> {
        Ok(self.state.clone())
    }

    /// コンテナIDを取得
    pub fn get_id(&self) -> &str {
        &self.id
    }

    /// コンテナ設定を取得
    pub fn get_config(&self) -> &ContainerConfig {
        &self.config
    }

    /// Unix系でのコンテナ開始
    #[cfg(unix)]
    fn start_unix(&mut self) -> Result<()> {
        use std::os::unix::process::CommandExt;

        let mut cmd = Command::new(&self.config.command[0]);
        
        if self.config.command.len() > 1 {
            cmd.args(&self.config.command[1..]);
        }

        // 環境変数を設定
        for env in &self.config.env {
            if let Some((key, value)) = env.split_once('=') {
                cmd.env(key, value);
            }
        }

        // 作業ディレクトリを設定
        if let Some(ref working_dir) = self.config.working_dir {
            cmd.current_dir(working_dir);
        }

        // 標準入出力を設定
        cmd.stdin(Stdio::null())
           .stdout(Stdio::null())
           .stderr(Stdio::null());

        // プロセスを起動
        let child = cmd.spawn()
            .map_err(|e| ContainerError::Runtime(format!("Failed to start process: {}", e)))?;

        self.pid = Some(child.id());
        self.process = Some(child);

        Ok(())
    }

    /// Windows系でのコンテナ開始
    #[cfg(windows)]
    fn start_windows(&mut self) -> Result<()> {
        let mut cmd = Command::new(&self.config.command[0]);
        
        if self.config.command.len() > 1 {
            cmd.args(&self.config.command[1..]);
        }

        // 環境変数を設定
        for env in &self.config.env {
            if let Some((key, value)) = env.split_once('=') {
                cmd.env(key, value);
            }
        }

        // 作業ディレクトリを設定
        if let Some(ref working_dir) = self.config.working_dir {
            cmd.current_dir(working_dir);
        }

        // 標準入出力を設定
        cmd.stdin(Stdio::null())
           .stdout(Stdio::null())
           .stderr(Stdio::null());

        // プロセスを起動
        let child = cmd.spawn()
            .map_err(|e| ContainerError::Runtime(format!("Failed to start process: {}", e)))?;

        self.pid = Some(child.id());
        self.process = Some(child);

        Ok(())
    }

    /// クリーンアップ処理
    fn cleanup(&mut self) -> Result<()> {
        // プロセスが残っている場合は終了
        if let Some(ref mut process) = self.process {
            let _ = process.kill();
            let _ = process.wait();
        }

        // 一時ファイルやディレクトリの削除
        // TODO: 実際のクリーンアップ処理を実装

        Ok(())
    }

    /// コンテナ内でコマンドを実行
    #[allow(dead_code)]
    pub fn exec(&self, _command: &[String]) -> Result<()> {
        if self.state != ContainerState::Running {
            return Err(ContainerError::InvalidState(format!(
                "Cannot exec in container in state: {:?}",
                self.state
            )));
        }

        // TODO: 実際のexec実装
        Ok(())
    }

    /// コンテナを一時停止
    #[allow(dead_code)]
    pub fn pause(&mut self) -> Result<()> {
        if self.state != ContainerState::Running {
            return Err(ContainerError::InvalidState(format!(
                "Cannot pause container in state: {:?}",
                self.state
            )));
        }

        // TODO: 実際のpause実装
        self.state = ContainerState::Paused;
        Ok(())
    }

    /// コンテナの一時停止を解除
    #[allow(dead_code)]
    pub fn unpause(&mut self) -> Result<()> {
        if self.state != ContainerState::Paused {
            return Err(ContainerError::InvalidState(format!(
                "Cannot unpause container in state: {:?}",
                self.state
            )));
        }

        // TODO: 実際のunpause実装
        self.state = ContainerState::Running;
        Ok(())
    }

    /// コンテナの統計情報を取得
    #[allow(dead_code)]
    pub fn get_stats(&self) -> Result<ContainerStats> {
        if self.state != ContainerState::Running {
            return Err(ContainerError::InvalidState(format!(
                "Cannot get stats for container in state: {:?}",
                self.state
            )));
        }

        // TODO: 実際の統計情報取得
        Ok(ContainerStats {
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_limit: 0,
            network_rx: 0,
            network_tx: 0,
            block_read: 0,
            block_write: 0,
        })
    }

    /// コンテナのログを取得
    #[allow(dead_code)]
    pub fn get_logs(&self, _follow: bool, _tail: Option<usize>) -> Result<Vec<String>> {
        // TODO: 実際のログ取得実装
        Ok(Vec::new())
    }

    /// コンテナのログを追加
    pub fn add_log(&mut self, message: String) {
        let timestamp = SystemTime::now();
        self.logs.push(ContainerLog {
            timestamp,
            message,
        });
    }

    /// コンテナのログを取得
    pub fn get_logs_struct(&self, since: Option<SystemTime>, lines: Option<usize>) -> Vec<ContainerLog> {
        let mut logs = self.logs.clone();
        
        // since以降のログをフィルタ
        if let Some(since_time) = since {
            logs.retain(|log| log.timestamp >= since_time);
        }
        
        // 行数制限
        if let Some(max_lines) = lines {
            if logs.len() > max_lines {
                logs = logs.into_iter().rev().take(max_lines).rev().collect();
            }
        }
        
        logs
    }
}

/// コンテナ統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ContainerStats {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub block_read: u64,
    pub block_write: u64,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_creation() {
        let config = ContainerConfig {
            id: "test-container".to_string(),
            image: "test-image".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
            env: vec!["TEST=1".to_string()],
            working_dir: Some("/tmp".to_string()),
            user: None,
            hostname: Some("test-host".to_string()),
            privileged: false,
            read_only: false,
            network_mode: "default".to_string(),
            volumes: Vec::new(),
            ports: Vec::new(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
        };

        let container = Container::new(config).unwrap();
        assert_eq!(container.get_state().unwrap(), ContainerState::Created);
        assert_eq!(container.get_id(), "test-container");
    }

    #[test]
    fn test_container_state_transitions() {
        let config = ContainerConfig {
            id: "test-container".to_string(),
            image: "test-image".to_string(),
            command: vec!["sleep".to_string(), "1".to_string()],
            env: Vec::new(),
            working_dir: None,
            user: None,
            hostname: None,
            privileged: false,
            read_only: false,
            network_mode: "default".to_string(),
            volumes: Vec::new(),
            ports: Vec::new(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
        };

        let mut container = Container::new(config).unwrap();
        
        // 初期状態はCreated
        assert_eq!(container.get_state().unwrap(), ContainerState::Created);
        
        // 開始
        if container.start().is_ok() {
            assert_eq!(container.get_state().unwrap(), ContainerState::Running);
            
            // 停止
            container.stop().unwrap();
            assert_eq!(container.get_state().unwrap(), ContainerState::Stopped);
        }
    }
} 