use libnexuscontainer::{
    Container, ContainerConfig, ContainerState, ContainerStatus,
    NamespaceConfig, CgroupConfig, SecurityPolicy, NetworkConfig,
    VolumeConfig, Result, ContainerError,
};
use libnexuscontainer::container::ContainerConfiguration;
use crate::oci::{OCISpec, OCIState, OCIProcess};
use log::{debug, error, info, warn};
use serde_json;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, create_dir_all};
use std::io::{Read, Write, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

pub struct NexusRuntime {
    #[allow(dead_code)]
    root_dir: PathBuf,
    pub containers: HashMap<String, Container>,
    state_dir: PathBuf,
    #[allow(dead_code)]
    systemd_cgroup: bool,
    pub pid_files: HashMap<String, PathBuf>,
    pub console_sockets: HashMap<String, PathBuf>,
}

impl NexusRuntime {
    pub fn new(root_dir: PathBuf, systemd_cgroup: bool) -> Result<Self> {
        let state_dir = root_dir.join("state");
        
        // 必要なディレクトリを作成
        std::fs::create_dir_all(&state_dir)
            .map_err(|e| ContainerError::Runtime(format!("Failed to create state directory: {}", e)))?;
        
        let mut runtime = NexusRuntime {
            root_dir,
            containers: HashMap::new(),
            state_dir,
            systemd_cgroup,
            pid_files: HashMap::new(),
            console_sockets: HashMap::new(),
        };

        // 既存のコンテナ状態をロード
        runtime.load_existing_containers()?;
        
        Ok(runtime)
    }
    
    /// 既存のコンテナ状態を読み込み
    fn load_existing_containers(&mut self) -> Result<()> {
        if !self.state_dir.exists() {
            return Ok(());
        }
        
        for entry in std::fs::read_dir(&self.state_dir)
            .map_err(|e| ContainerError::Runtime(format!("Failed to read state directory: {}", e)))? {
            
            let entry = entry
                .map_err(|e| ContainerError::Runtime(format!("Failed to read directory entry: {}", e)))?;
            
            if let Some(filename) = entry.file_name().to_str() {
                if filename.ends_with(".json") {
                    let container_id = filename.trim_end_matches(".json");
                    if let Ok(container) = self.load_container_state(container_id) {
                        self.containers.insert(container_id.to_string(), container);
                    }
                }
            }
        }
        
        info!("Loaded {} existing containers", self.containers.len());
        Ok(())
    }
    
    /// コンテナ状態をファイルから読み込み
    fn load_container_state(&self, container_id: &str) -> Result<Container> {
        let state_file = self.state_dir.join(format!("{}.json", container_id));
        let mut file = File::open(&state_file)
            .map_err(|e| ContainerError::Runtime(format!("Failed to open state file: {}", e)))?;
        
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| ContainerError::Runtime(format!("Failed to read state file: {}", e)))?;
        
        let container: Container = serde_json::from_str(&content)
            .map_err(|e| ContainerError::Runtime(format!("Failed to parse state file: {}", e)))?;
        
        Ok(container)
    }
    
    /// コンテナ状態をファイルに保存
    fn save_container_state(&self, container: &Container) -> Result<()> {
        let state_file = self.state_dir.join(format!("{}.json", container.id));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&state_file)
            .map_err(|e| ContainerError::Runtime(format!("Failed to create state file: {}", e)))?;
        
        let content = serde_json::to_string_pretty(container)
            .map_err(|e| ContainerError::Runtime(format!("Failed to serialize container state: {}", e)))?;
        
        file.write_all(content.as_bytes())
            .map_err(|e| ContainerError::Runtime(format!("Failed to write state file: {}", e)))?;
        
        Ok(())
    }
    
    /// コンテナ状態ファイルを削除
    fn remove_container_state(&self, container_id: &str) -> Result<()> {
        let state_file = self.state_dir.join(format!("{}.json", container_id));
        if state_file.exists() {
            std::fs::remove_file(&state_file)
                .map_err(|e| ContainerError::Runtime(format!("Failed to remove state file: {}", e)))?;
        }
        Ok(())
    }
    
    /// コンテナを作成して開始
    pub fn create_and_start(
        &mut self,
        container_id: &str,
        bundle_path: &Path,
        process: Option<&Path>,
        process_stdin: bool,
        tty: bool,
        pid_file: Option<&Path>,
        console_socket: Option<&Path>,
        args: &[String],
    ) -> Result<()> {
        info!("Creating and starting container: {}", container_id);

        // バンドルからspec.jsonを読み込み
        let spec_path = bundle_path.join("config.json");
        let spec_content = fs::read_to_string(&spec_path)
            .map_err(|e| ContainerError::Runtime(format!("Failed to read spec: {}", e)))?;
        
        let oci_spec: OCISpec = serde_json::from_str(&spec_content)
            .map_err(|e| ContainerError::Runtime(format!("Failed to parse spec: {}", e)))?;

        // OCI SpecをContainerConfigに変換
        let container_config = self.convert_oci_to_config(&oci_spec, bundle_path)?;

        // コンテナを作成
        let mut container = Container::new(container_config)?;

        // PIDファイルの処理
        if let Some(pid_path) = pid_file {
            self.pid_files.insert(container_id.to_string(), pid_path.to_path_buf());
        }

        // コンソールソケットの処理  
        if let Some(console_path) = console_socket {
            self.console_sockets.insert(container_id.to_string(), console_path.to_path_buf());
        }

        // コンテナを開始
        container.start()?;

        // PIDファイルを書き込み
        if let Some(pid) = container.pid {
            if let Some(pid_path) = pid_file {
                let mut file = fs::File::create(pid_path)
                    .map_err(|e| ContainerError::Runtime(format!("Failed to create PID file: {}", e)))?;
                write!(file, "{}", pid)
                    .map_err(|e| ContainerError::Runtime(format!("Failed to write PID file: {}", e)))?;
                info!("PID file written: {} -> {}", pid_path.display(), pid);
            }
        }

        // コンテナ状態を保存
        self.save_container_state(&container)?;

        // コンテナを保存
        self.containers.insert(container_id.to_string(), container);

        info!("Container {} created and started successfully", container_id);
        Ok(())
    }

    /// OCI SpecをContainerConfigに変換
    fn convert_oci_to_config(&self, spec: &OCISpec, bundle_path: &Path) -> Result<libnexuscontainer::ContainerConfig> {
        let rootfs_path = bundle_path.join("rootfs");

        let hostname = spec.hostname.clone().unwrap_or_else(|| "container".to_string());
        
        // processフィールドの正しい処理
        let command = if !spec.process.args.is_empty() {
            spec.process.args[0].clone()
        } else {
            "/bin/sh".to_string()
        };
        
        let args = if spec.process.args.len() > 1 {
            spec.process.args[1..].to_vec()
        } else {
            Vec::new()
        };
        
        let envs = spec.process.env.clone();

        let mut config = libnexuscontainer::ContainerConfig::new_simple(hostname, command, args, rootfs_path);

        // 環境変数の設定
        if let Some(env_vars) = envs {
            config.envs = Some(env_vars);
        }

        Ok(config)
    }
    
    /// コンテナを開始
    pub fn start(&mut self, container_id: &str) -> Result<()> {
        info!("Starting container: {}", container_id);
        
        let container_id_owned = container_id.to_string();
        {
            let container = self.containers.get_mut(container_id)
                .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
            
            container.start()?;
        }
        
        // 状態保存を別のスコープで実行
        let container = self.containers.get(&container_id_owned)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id_owned)))?;
        self.save_container_state(container)?;
        
        info!("Container {} started successfully", container_id);
        Ok(())
    }
    
    /// コンテナの情報を取得
    pub fn state(&self, container_id: &str) -> Result<OCIState> {
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;

        Ok(OCIState {
            ociVersion: "1.0.0".to_string(),
            id: container.id.clone(),
            status: match container.state {
                ContainerState::Created => "created".to_string(),
                ContainerState::Running => "running".to_string(),
                ContainerState::Paused => "paused".to_string(),
                ContainerState::Stopping => "stopping".to_string(),
                ContainerState::Exited => "stopped".to_string(),
            },
            pid: container.pid,
            bundle: PathBuf::from("/var/lib/nexus/containers").join(&container.id),
            annotations: HashMap::new(),
        })
    }
    
    /// コンテナにシグナルを送信
    pub fn kill(&mut self, container_id: &str, signal: &str, _all: bool) -> Result<()> {
        info!("Sending signal {} to container {}", signal, container_id);

        {
            let container = self.containers.get_mut(container_id)
                .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;

            match signal {
                "SIGTERM" | "TERM" | "15" => container.stop(false)?,
                "SIGKILL" | "KILL" | "9" => container.stop(true)?,
                _ => {
                    warn!("Unsupported signal: {}", signal);
                    return Err(ContainerError::UnsupportedFeature(format!("Signal {} not supported", signal)));
                }
            }
        }

        // 状態保存を別のスコープで実行
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
        self.save_container_state(container)?;

        info!("Signal {} sent to container {}", signal, container_id);
        Ok(())
    }
    
    /// コンテナを削除
    pub fn delete(&mut self, container_id: &str, force: bool) -> Result<()> {
        info!("Deleting container: {}", container_id);

        let mut container = self.containers.remove(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;

        if !force && matches!(container.state, ContainerState::Running | ContainerState::Paused) {
            return Err(ContainerError::InvalidState(
                format!("Cannot remove running container {} without force", container_id)
            ));
        }

        if matches!(container.state, ContainerState::Running | ContainerState::Paused) {
            container.stop(true)?;
        }

        container.remove(force)?;

        // 状態ファイルの削除
        self.remove_container_state(container_id)?;

        // PIDファイルとコンソールソケットのクリーンアップ
        self.pid_files.remove(container_id);
        self.console_sockets.remove(container_id);

        info!("Container {} deleted successfully", container_id);
        Ok(())
    }
    
    /// コンテナ内でプロセスを実行
    pub fn exec(
        &mut self,
        container_id: &str,
        _process: Option<&Path>,
        _process_stdin: bool,
        _tty: bool,
        _pid_file: Option<&Path>,
        _console_socket: Option<&Path>,
        args: &[String],
    ) -> Result<()> {
        info!("Executing process in container: {}", container_id);
        
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
        
        if container.state != ContainerState::Running {
            return Err(ContainerError::InvalidState(
                format!("Container {} is not running", container_id)
            ));
        }
        
        // 実際のexec実装 - namespace入りでプロセス実行
        if let Some(_pid) = container.pid {
            #[cfg(unix)]
            {
                let nsenter_cmd = format!("/proc/{}/ns", _pid);
                let mut cmd = std::process::Command::new("nsenter");
                cmd.args(&[
                    "-t", &_pid.to_string(),
                    "-m", "-p", "-n", "-u", "-i",
                ]);
                
                if !args.is_empty() {
                    cmd.args(args);
                } else {
                    cmd.arg("/bin/sh");
                }
                
                if _tty {
                    cmd.stdin(std::process::Stdio::inherit());
                    cmd.stdout(std::process::Stdio::inherit());
                    cmd.stderr(std::process::Stdio::inherit());
                }
                
                let status = cmd.status()
                    .map_err(|e| ContainerError::Runtime(format!("Failed to execute nsenter: {}", e)))?;
                
                if !status.success() {
                    return Err(ContainerError::Runtime(format!("Exec failed with exit code: {:?}", status.code())));
                }
            }
            
            #[cfg(not(unix))]
            {
                return Err(ContainerError::UnsupportedFeature("exec not supported on this platform".to_string()));
            }
        } else {
            return Err(ContainerError::InvalidState("Container has no PID".to_string()));
        }
        
        info!("Process executed successfully in container {}", container_id);
        Ok(())
    }
    
    /// コンテナ一覧を表示
    pub fn list(&self, format: &str, quiet: bool) -> Result<()> {
        match format {
            "json" => {
                let containers: Vec<_> = self.containers.values().map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "state": c.state.as_str(),
                        "pid": c.pid,
                        "created": c.created_at,
                        "started": c.started_at,
                        "finished": c.finished_at,
                        "exit_code": c.exit_code
                    })
                }).collect();
                let output = serde_json::to_string_pretty(&containers)
                    .map_err(|e| ContainerError::Runtime(format!("Failed to serialize container list: {}", e)))?;
                println!("{}", output);
            }
            "table" => {
                if !quiet {
                    println!("{:<20} {:<15} {:<10} {:<30}", "CONTAINER ID", "STATUS", "PID", "CREATED");
                }
                
                for container in self.containers.values() {
                    let status = container.state.as_str();
                    let pid = container.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string());
                    let created = format!("{:?}", container.created_at);
                    
                    println!("{:<20} {:<15} {:<10} {:<30}", 
                        &container.id[..12.min(container.id.len())], 
                        status, 
                        pid, 
                        created);
                }
            }
            _ => {
                return Err(ContainerError::InvalidArgument(
                    format!("Unsupported format: {}", format)
                ));
            }
        }
        
        Ok(())
    }
    
    /// コンテナを一時停止
    pub fn pause(&mut self, container_id: &str) -> Result<()> {
        info!("Pausing container: {}", container_id);
        
        let container_id_owned = container_id.to_string();
        {
            let container = self.containers.get_mut(container_id)
                .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
            
            container.pause()?;
        }
        
        // 状態保存を別のスコープで実行
        let container = self.containers.get(&container_id_owned)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id_owned)))?;
        self.save_container_state(container)?;
        
        info!("Container {} paused successfully", container_id);
        Ok(())
    }
    
    /// コンテナを再開
    pub fn resume(&mut self, container_id: &str) -> Result<()> {
        info!("Resuming container: {}", container_id);
        
        let container_id_owned = container_id.to_string();
        {
            let container = self.containers.get_mut(container_id)
                .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
            
            container.unpause()?;
        }
        
        // 状態保存を別のスコープで実行
        let container = self.containers.get(&container_id_owned)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id_owned)))?;
        self.save_container_state(container)?;
        
        info!("Container {} resumed successfully", container_id);
        Ok(())
    }
    
    /// リソース制限を更新
    pub fn update(&mut self, container_id: &str, resources: &Path) -> Result<()> {
        info!("Updating container resources: {}", container_id);
        
        {
            let container = self.containers.get_mut(container_id)
                .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
                
            if container.state != ContainerState::Running {
                return Err(ContainerError::InvalidState(
                    format!("Cannot update resources for container in {} state", container.state.as_str())
                ));
            }
        }
        
        // リソース設定ファイルを読み込み
        let _resource_content = fs::read_to_string(resources)
            .map_err(|e| ContainerError::Runtime(format!("Failed to read resource file: {}", e)))?;
            
        let _resource_config: serde_json::Value = serde_json::from_str(&_resource_content)
            .map_err(|e| ContainerError::Runtime(format!("Failed to parse resource file: {}", e)))?;
        
        // CGROUPSのリソース制限を更新
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
            
        if let Some(_pid) = container.pid {
            #[cfg(target_os = "linux")]
            {
                // メモリ制限の更新
                if let Some(memory_limit) = _resource_config.get("memory") {
                    if let Some(limit_bytes) = memory_limit.as_u64() {
                        let cgroup_path = format!("/sys/fs/cgroup/memory/nexus/{}/memory.limit_in_bytes", container.id);
                        let _ = fs::write(&cgroup_path, limit_bytes.to_string());
                    }
                }
                
                // CPU制限の更新
                if let Some(cpu_config) = _resource_config.get("cpu") {
                    if let Some(shares) = cpu_config.get("shares").and_then(|v| v.as_u64()) {
                        let cgroup_path = format!("/sys/fs/cgroup/cpu/nexus/{}/cpu.shares", container.id);
                        let _ = fs::write(&cgroup_path, shares.to_string());
                    }
                }
            }
        }
        
        // 状態保存
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
        self.save_container_state(container)?;
        
        info!("Container {} resources updated successfully", container_id);
        Ok(())
    }
    
    /// OCI仕様バージョンを表示
    pub fn spec_version(&self) -> Result<()> {
        println!("1.0.0");
        Ok(())
    }
    
    /// コンテナのイベントを監視
    pub fn events(&self, container_id: &str, stats: bool, interval: u64) -> Result<()> {
        info!("Monitoring events for container: {}", container_id);
        
        let container = self.containers.get(container_id)
            .ok_or_else(|| ContainerError::NotFound(format!("Container {} not found", container_id)))?;
        
        if stats {
            // 統計情報の監視
            loop {
                if let Some(_pid) = container.pid {
                    #[cfg(unix)]
                    {
                        // /proc/<pid>/stat から統計情報を取得
                        let stat_path = format!("/proc/{}/stat", _pid);
                        if let Ok(stat_content) = fs::read_to_string(&stat_path) {
                            println!("Stats: {}", stat_content.trim());
                        }
                        
                        // メモリ使用量
                        let status_path = format!("/proc/{}/status", _pid);
                        if let Ok(status_content) = fs::read_to_string(&status_path) {
                            for line in status_content.lines() {
                                if line.starts_with("VmRSS:") {
                                    println!("Memory: {}", line);
                                    break;
                                }
                            }
                        }
                    }
                }
                
                std::thread::sleep(std::time::Duration::from_secs(interval));
            }
        } else {
            // 基本的なイベント監視
            println!("Event monitoring started for container: {}", container_id);
        }
        
        Ok(())
    }
    
    /// 初期化処理
    pub fn initialize(&mut self) -> Result<()> {
        info!("Initializing NexusRuntime");
        
        // ディレクトリ構造の作成
        fs::create_dir_all(&self.root_dir)?;
        fs::create_dir_all(&self.state_dir)?;
        
        // 既存のコンテナ状態をロード
        self.load_existing_containers()?;
        
        info!("NexusRuntime initialized successfully");
        Ok(())
    }
    
    /// クリーンアップ処理
    pub fn cleanup(&mut self) -> Result<()> {
        info!("Cleaning up NexusRuntime");
        
        // 全コンテナの状態を保存
        for container in self.containers.values() {
            self.save_container_state(container)?;
        }
        
        info!("NexusRuntime cleanup completed");
        Ok(())
    }

    /// コンテナを作成（開始は行わない）
    pub fn create(
        &mut self,
        container_id: &str,
        bundle_path: &Path,
        pid_file: Option<&Path>,
        console_socket: Option<&Path>,
    ) -> Result<()> {
        info!("Creating container: {}", container_id);

        // バンドルからspec.jsonを読み込み
        let spec_path = bundle_path.join("config.json");
        let spec_content = fs::read_to_string(&spec_path)
            .map_err(|e| ContainerError::Runtime(format!("Failed to read spec: {}", e)))?;
        
        let oci_spec: OCISpec = serde_json::from_str(&spec_content)
            .map_err(|e| ContainerError::Runtime(format!("Failed to parse spec: {}", e)))?;

        // OCI SpecをContainerConfigに変換
        let container_config = self.convert_oci_to_config(&oci_spec, bundle_path)?;

        // コンテナを作成（開始はしない）
        let container = Container::new(container_config)?;

        // PIDファイルの処理
        if let Some(pid_path) = pid_file {
            self.pid_files.insert(container_id.to_string(), pid_path.to_path_buf());
        }

        // コンソールソケットの処理  
        if let Some(console_path) = console_socket {
            self.console_sockets.insert(container_id.to_string(), console_path.to_path_buf());
        }

        // コンテナ状態を保存
        self.save_container_state(&container)?;

        // コンテナを保存
        self.containers.insert(container_id.to_string(), container);

        info!("Container {} created successfully", container_id);
        Ok(())
    }
} 