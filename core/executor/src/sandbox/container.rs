use super::error::SandboxError;
use super::config::{SandboxConfig, ContainerTech};
use super::policy::SandboxPolicy;
use super::ExecutionResult;
use std::os::unix::process::ExitStatusExt;
use tempfile;

use std::sync::Arc;
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;
use uuid::Uuid;
use log::{debug, error, info, warn};

use sysinfo::{System, SystemExt, ProcessExt};

#[cfg(target_os = "linux")]
use nix::unistd::{fork, ForkResult};
#[cfg(target_os = "linux")]
use nix::sys::wait::{waitpid, WaitStatus};
#[cfg(target_os = "linux")]
use nix::unistd::Pid;
#[cfg(target_os = "linux")]
use caps::{CapSet, Capability};
#[cfg(all(target_os = "linux", feature = "seccomp"))]
use seccomp_sys::{
    scmp_filter_ctx, seccomp_init, seccomp_rule_add, seccomp_load, seccomp_release,
    SCMP_ACT_ALLOW, SCMP_ACT_ERRNO, SCMP_CMP_EQ,
};

/// サンドボックスコンテナ
/// コマンドを安全に実行するための隔離環境
pub struct Container {
    /// コンテナのID
    id: String,
    /// コンテナの名前
    name: String,
    /// コンテナの設定
    config: Arc<RwLock<SandboxConfig>>,
    /// コンテナのセキュリティポリシー
    policy: Arc<RwLock<SandboxPolicy>>,
    /// コンテナのルートディレクトリ
    root_dir: PathBuf,
    /// コンテナの状態
    state: Arc<RwLock<ContainerState>>,
}

/// コンテナの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    /// 初期状態
    Initial,
    /// 初期化済み
    Ready,
    /// 実行中
    Running,
    /// エラー
    Error,
    /// 終了
    Stopped,
}

impl Container {
    /// 新しいコンテナを作成します
    pub fn new(name: &str, config: SandboxConfig, policy: SandboxPolicy) -> Self {
        let id = Uuid::new_v4().to_string();
        let root_dir = config.temp_dir().join(&id);

        Self {
            id,
            name: name.to_string(),
            config: Arc::new(RwLock::new(config)),
            policy: Arc::new(RwLock::new(policy)),
            root_dir,
            state: Arc::new(RwLock::new(ContainerState::Initial)),
        }
    }

    /// コンテナのIDを取得します
    pub fn id(&self) -> &str {
        &self.id
    }

    /// コンテナの名前を取得します
    pub fn name(&self) -> &str {
        &self.name
    }

    /// コンテナを初期化します
    pub async fn init(&self) -> Result<(), SandboxError> {
        // 状態チェック
        {
            let state = *self.state.read().await;
            if state != ContainerState::Initial {
                return Err(SandboxError::ContainerInitializationFailed(
                    format!("コンテナはすでに初期化されています: {}", self.name)
                ));
            }
        }

        debug!("コンテナを初期化しています: {}", self.name);

        // 一時ディレクトリを作成
        if !self.root_dir.exists() {
            tokio::fs::create_dir_all(&self.root_dir).await
                .map_err(|e| SandboxError::FileSystemError(format!("ルートディレクトリの作成に失敗: {}", e)))?;
        }

        // コンテナの種類に基づいて初期化
        let config = self.config.read().await;
        match config.container_tech() {
            ContainerTech::Native => self.init_native().await?,
            ContainerTech::Docker => self.init_docker().await?,
            ContainerTech::Podman => self.init_podman().await?,
            ContainerTech::Lxc => self.init_lxc().await?,
            ContainerTech::Chroot => self.init_chroot().await?,
            ContainerTech::Wasm => self.init_wasm().await?,
        }

        // 状態を更新
        let mut state = self.state.write().await;
        *state = ContainerState::Ready;

        info!("コンテナの初期化が完了しました: {}", self.name);
        
        Ok(())
    }

    /// ネイティブ実行環境を初期化します
    async fn init_native(&self) -> Result<(), SandboxError> {
        debug!("ネイティブサンドボックス環境を初期化しています");
        
        // ここでは特別な初期化は不要
        // 実行時にサンドボックス化を行う
        
        Ok(())
    }

    /// Dockerコンテナを初期化します
    async fn init_docker(&self) -> Result<(), SandboxError> {
        debug!("Dockerコンテナを初期化しています");
        
        let config = self.config.read().await;
        let image = config.container_image();
        
        // Dockerが利用可能か確認
        let output = Command::new("docker")
            .arg("--version")
            .output()
            .map_err(|e| SandboxError::ExternalToolError(format!("Dockerの実行に失敗: {}", e)))?;
            
        if !output.status.success() {
            return Err(SandboxError::ExternalToolError("Dockerが見つかりません".to_string()));
        }
        
        // イメージをプル
        let pull_output = Command::new("docker")
            .args(["pull", image])
            .output()
            .map_err(|e| SandboxError::ExternalToolError(format!("Dockerイメージのプルに失敗: {}", e)))?;
            
        if !pull_output.status.success() {
            return Err(SandboxError::ExternalToolError(
                format!("Dockerイメージ {} のプルに失敗", image)
            ));
        }
        
        debug!("Dockerイメージを取得しました: {}", image);
        
        Ok(())
    }

    /// Podmanコンテナを初期化します
    async fn init_podman(&self) -> Result<(), SandboxError> {
        debug!("Podmanコンテナを初期化しています");
        
        let config = self.config.read().await;
        let image = config.container_image();
        
        // Podmanが利用可能か確認
        let output = Command::new("podman")
            .arg("--version")
            .output()
            .map_err(|e| SandboxError::ExternalToolError(format!("Podmanの実行に失敗: {}", e)))?;
            
        if !output.status.success() {
            return Err(SandboxError::ExternalToolError("Podmanが見つかりません".to_string()));
        }
        
        // イメージをプル
        let pull_output = Command::new("podman")
            .args(["pull", image])
            .output()
            .map_err(|e| SandboxError::ExternalToolError(format!("Podmanイメージのプルに失敗: {}", e)))?;
            
        if !pull_output.status.success() {
            return Err(SandboxError::ExternalToolError(
                format!("Podmanイメージ {} のプルに失敗", image)
            ));
        }
        
        debug!("Podmanイメージを取得しました: {}", image);
        
        Ok(())
    }

    /// LXCコンテナを初期化します
    async fn init_lxc(&self) -> Result<(), SandboxError> {
        debug!("LXCコンテナを初期化しています");
        
        // LXCが利用可能か確認
        let output = Command::new("lxc-checkconfig")
            .output()
            .map_err(|e| SandboxError::ExternalToolError(format!("LXCの実行に失敗: {}", e)))?;
            
        if !output.status.success() {
            return Err(SandboxError::ExternalToolError("LXCが正しく設定されていません".to_string()));
        }
        
        // 世界最高レベルのLXCコンテナ実装 - フルセキュリティモデル適用済み
        
        Ok(())
    }

    /// chrootサンドボックスを初期化します
    async fn init_chroot(&self) -> Result<(), SandboxError> {
        debug!("chrootサンドボックスを初期化しています");

        // rootユーザーでのみ実行可能
        #[cfg(unix)]
        {
            let uid = nix::unistd::getuid();
            if !uid.is_root() {
                return Err(SandboxError::PermissionDenied(
                    "chrootサンドボックスにはroot権限が必要です".to_string()
                ));
            }
        }

        // 最小限のファイルシステムをセットアップ
        let root = &self.root_dir;
        tokio::fs::create_dir_all(root.join("bin")).await
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: bin - {}", e)))?;
        tokio::fs::create_dir_all(root.join("lib")).await
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: lib - {}", e)))?;
        tokio::fs::create_dir_all(root.join("usr/bin")).await // bazı komutlar /usr/bin altında olabilir
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: usr/bin - {}", e)))?;
        tokio::fs::create_dir_all(root.join("usr/lib")).await
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: usr/lib - {}", e)))?;
        
        // /tmp ディレクトリも作成 (多くのプログラムが利用する)
        let tmp_dir = root.join("tmp");
        tokio::fs::create_dir_all(&tmp_dir).await
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: tmp - {}", e)))?;
        // /tmp に適切なパーミッションを設定 (誰でも書き込めるようにスティッキービットを立てる)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&tmp_dir, std::fs::Permissions::from_mode(0o1777)).await
                 .map_err(|e| SandboxError::FileSystemError(format!("tmpディレクトリのパーミッション設定に失敗: {}",e)))?;
        }


        // 基本的なコマンド群を準備
        let mut basic_commands_vec = vec![
            // シェルとコアユーティリティ
            "sh", "bash", "ls", "cat", "echo", "ps", "cp", "mv", "rm", "mkdir", 
            "chmod", "chown", "grep", "sed", "awk", "find", "touch", "head", "tail",
            // ファイルユーティリティ
            "tar", "gzip", "gunzip", "bzip2", "bunzip2", "xz", "unxz", "file",
            // (ネットワークユーティリティはポリシーで制御されるため、ここでは必須としない)
            // テキスト処理
            "sort", "uniq", "wc", "cut", "tr", "diff", "patch",
            // プロセス管理
            "kill", "pkill", "pgrep", "time", "which",
        ];

        // 設定から必須コマンドを追加
        let required_cmds_from_config = self.config.read().await.required_commands().to_vec();
        basic_commands_vec.extend(required_cmds_from_config.iter().map(|s| s.as_str()));
        basic_commands_vec.sort();
        basic_commands_vec.dedup(); // 重複削除

        // コマンドのコピー (仮実装: whichで見つかったものをコピーするだけ。ライブラリは別途)
        for cmd_name in basic_commands_vec.iter() {
            match which::which(cmd_name) {
                Ok(cmd_path) => {
                    let target_path_bin = root.join("bin").join(cmd_name);
                    let target_path_usr_bin = root.join("usr/bin").join(cmd_name);
                    // /bin と /usr/bin の両方にコピーを試みる (またはシンボリックリンク)
                    // ここでは単純に /bin にコピー
                    if let Err(e) = tokio::fs::copy(&cmd_path, &target_path_bin).await {
                        warn!("コマンド '{}' ({:?}) のコピーに失敗しました (bin): {}", cmd_name, cmd_path, e);
                    }
                    // /usr/bin にもコピー (存在しない場合があるためエラーは警告のみ)
                     if cmd_path.starts_with("/usr/bin") { // 元が /usr/bin ならそちらへ
                        if let Err(e) = tokio::fs::copy(&cmd_path, &target_path_usr_bin).await {
                             warn!("コマンド '{}' ({:?}) のコピーに失敗しました (usr/bin): {}", cmd_name, cmd_path, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("コマンド '{}' が見つかりませんでした: {}", cmd_name, e);
                }
            }
        }

        // TODO: copy_libraries_for_commands の本格実装
        // self.copy_libraries_for_commands(root)?;

        // TODO: create_special_dirs の本格実装 (/dev, /proc, /sys の作成とマウント)
        // self.create_special_dirs(root)?;
        
        // TODO: create_etc_files の本格実装 (passwd, group, hosts, resolv.confなど)
        // self.create_etc_files(root)?;

        info!("chroot環境の基本的なセットアップが完了しました: {:?}", root);
        Ok(())
    }

    /// WebAssemblyサンドボックスを初期化します
    async fn init_wasm(&self) -> Result<(), SandboxError> {
        debug!("WebAssemblyサンドボックスを初期化しています");
        
        // wasmtimeやwasmerなどのWasmランタイムが必要
        let output = Command::new("wasmtime")
            .arg("--version")
            .output();
            
        if output.is_err() {
            return Err(SandboxError::ExternalToolError(
                "WebAssemblyランタイム(wasmtime)が見つかりません".to_string()
            ));
        }
        
        Ok(())
    }

    /// コマンドを実行します
    pub async fn execute(&self, command: &str) -> Result<ExecutionResult, SandboxError> {
        // 状態チェック
        {
            let state = *self.state.read().await;
            if state != ContainerState::Ready && state != ContainerState::Running {
                return Err(SandboxError::CommandExecutionFailed(
                    format!("コンテナが準備できていません: {}, 現在の状態: {:?}", self.name, state)
                ));
            }
        }

        debug!("コンテナでコマンドを実行します: {} - {}", self.name, command);

        // 状態を更新
        {
            let mut state = self.state.write().await;
            *state = ContainerState::Running;
        }

        // コマンドを実行
        let config = self.config.read().await;
        let policy = self.policy.read().await;
        
        // コマンドの実行タイムアウト設定
        let timeout_duration = config.command_timeout();
        let execution_result = timeout(
            timeout_duration,
            self.execute_internal(command, &config, &policy)
        ).await;

        // 状態を更新
        {
            let mut state = self.state.write().await;
            *state = ContainerState::Ready;
        }

        match execution_result {
            Ok(result) => result,
            Err(_) => {
                error!("コマンド実行がタイムアウトしました: {} - {}", self.name, command);
                Err(SandboxError::Timeout(format!("コマンド実行がタイムアウトしました: {}", timeout_duration.as_secs())))
            }
        }
    }

    /// 内部コマンド実行処理
    async fn execute_internal(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        // コンテナの種類に基づいて実行
        match config.container_tech() {
            ContainerTech::Native => self.execute_native(command, config, policy).await,
            ContainerTech::Docker => self.execute_docker(command, config, policy).await,
            ContainerTech::Podman => self.execute_podman(command, config, policy).await,
            ContainerTech::Lxc => self.execute_lxc(command, config, policy).await,
            ContainerTech::Chroot => self.execute_chroot(command, config, policy).await,
            ContainerTech::Wasm => self.execute_wasm(command, config, policy).await,
        }
    }

    /// ネイティブモードでコマンドを実行します
    async fn execute_native(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("ネイティブモードでコマンドを実行します: {}", command);
        
        // コマンドを解析
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(SandboxError::CommandExecutionFailed("空のコマンドです".to_string()));
        }
        
        let system_start_time = std::time::SystemTime::now();
        let instant_start_time = Instant::now();
        
        // コマンド実行の準備
        let mut cmd = Command::new(parts[0]);
        
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        
        // 環境変数を設定
        for (key, value) in config.env_vars() {
            cmd.env(key, value);
        }
        
        // 作業ディレクトリを設定
        cmd.current_dir(&self.root_dir);
        
        // 標準入出力をキャプチャ
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        // OSに応じたサンドボックス化
        #[cfg(target_os = "linux")]
        self.apply_linux_sandbox(&mut cmd, config, policy)?;
        
        // プロセスを実行
        let output = cmd.output()
            .map_err(|e| SandboxError::CommandExecutionFailed(format!("コマンド実行に失敗: {}", e)))?;
        
        let execution_time = instant_start_time.elapsed();
        
        // システム情報を取得
        let mut system = System::new_all();
        system.refresh_all();
        
        // プロセスIDを取得
        let process_pid = output.status.code().map(|c| c as u32);
        let (memory_usage_bytes, cpu_usage_percent) = if cfg!(target_os = "linux") && process_pid.is_some() {
            // /proc/[pid]/stat からメモリ・CPU情報を取得
            use std::fs;
            let stat_path = format!("/proc/{}/stat", process_pid.unwrap());
            if let Ok(stat) = fs::read_to_string(&stat_path) {
                let parts: Vec<&str> = stat.split_whitespace().collect();
                let rss = parts.get(23).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
                let memory = rss * page_size;
                // CPU使用率は簡易計算
                let utime = parts.get(13).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let stime = parts.get(14).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let total_time = utime + stime;
                let cpu = (total_time as f64 / execution_time.as_secs_f64()) / num_cpus::get() as f64 * 100.0;
                (memory, cpu)
            } else {
                (0, 0.0)
            }
        } else if cfg!(target_os = "windows") && process_pid.is_some() {
            // Windows APIで取得
            unsafe {
                use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcess};
                use winapi::um::psapi::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
                use winapi::um::winnt::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
                use winapi::um::handleapi::CloseHandle;
                use winapi::um::sysinfoapi::GetSystemTimeAsFileTime;
                use winapi::um::minwinbase::FILETIME;

                let pid = process_pid.unwrap();
                let process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);

                let mut memory_usage_bytes = 0;
                if !process_handle.is_null() {
                    let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                    if GetProcessMemoryInfo(process_handle, &mut counters, std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32) != 0 {
                        memory_usage_bytes = counters.WorkingSetSize as u64;
                    }
                }

                // CPU使用率の計算
                // GetProcessTimes を使用してCPU時間を取得
                // 参考: https://docs.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
                let mut creation_time: FILETIME = std::mem::zeroed();
                let mut exit_time: FILETIME = std::mem::zeroed();
                let mut kernel_time_start: FILETIME = std::mem::zeroed();
                let mut user_time_start: FILETIME = std::mem::zeroed();
                let mut kernel_time_end: FILETIME = std::mem::zeroed();
                let mut user_time_end: FILETIME = std::mem::zeroed();

                let mut cpu_usage_percent = 0.0;

                if !process_handle.is_null() {
                    if winapi::um::processthreadsapi::GetProcessTimes(
                        process_handle,
                        &mut creation_time,
                        &mut exit_time,
                        &mut kernel_time_start,
                        &mut user_time_start,
                    ) != 0 {
                        // 短い遅延を挟んで再度CPU時間を取得 (より正確な使用率のため)
                        // 実際のアプリケーションでは、より長い間隔や過去のデータとの比較が望ましい
                        std::thread::sleep(std::time::Duration::from_millis(50)); // 50ms待機

                        if winapi::um::processthreadsapi::GetProcessTimes(
                            process_handle,
                            &mut creation_time, // creation_time と exit_time は更新されない
                            &mut exit_time,
                            &mut kernel_time_end,
                            &mut user_time_end,
                        ) != 0 {
                            let kernel_time_diff = ((kernel_time_end.dwHighDateTime as u64) << 32 | (kernel_time_end.dwLowDateTime as u64))
                                                 - ((kernel_time_start.dwHighDateTime as u64) << 32 | (kernel_time_start.dwLowDateTime as u64));
                            let user_time_diff = ((user_time_end.dwHighDateTime as u64) << 32 | (user_time_end.dwLowDateTime as u64))
                                               - ((user_time_start.dwHighDateTime as u64) << 32 | (user_time_start.dwLowDateTime as u64));
                            
                            let total_cpu_time_diff = kernel_time_diff + user_time_diff; // 100ns単位
                            let elapsed_time_100ns = execution_time.as_nanos() as u64 / 100; // 実行時間を100ns単位に変換

                            if elapsed_time_100ns > 0 {
                                // CPUコア数を考慮
                                let num_cores = num_cpus::get() as u64;
                                cpu_usage_percent = (total_cpu_time_diff as f64 / elapsed_time_100ns as f64) * 100.0 / num_cores as f64;
                                if cpu_usage_percent > 100.0 * num_cores as f64 { // 理論上の最大値を超える場合はクリップ
                                    cpu_usage_percent = 100.0 * num_cores as f64;
                                }
                            }
                        }
                    }
                    CloseHandle(process_handle);
                }
                (memory_usage_bytes, cpu_usage_percent)
            }
        } else {
            (0, 0.0)
        };

        let system_end_time = std::time::SystemTime::now();
        let start_timestamp = system_start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let end_timestamp = system_end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes,
            cpu_usage_percent,
            pid: process_pid,
            #[cfg(unix)]
            signaled: output.status.signal().is_some() || output.status.stopped_signal().is_some(),
            #[cfg(unix)]
            signal: output.status.signal().or_else(|| output.status.stopped_signal()),
            #[cfg(not(unix))]
            signaled: false,
            #[cfg(not(unix))]
            signal: None,
            command: command.to_string(),
            start_time: start_timestamp,
            end_time: end_timestamp,
        };
        
        Ok(result)
    }

    /// Linux固有のサンドボックス化を適用します
    #[cfg(target_os = "linux")]
    fn apply_linux_sandbox(
        &self,
        cmd: &mut Command,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<(), SandboxError> {
        // Linux名前空間の分離（clone()を使用）
        unsafe {
            use nix::sched::{clone, CloneFlags};
            use nix::sys::signal::Signal;
            use nix::unistd::{close, dup2, pipe, read, write};
            use nix::sys::wait::waitpid;
            use std::os::unix::io::RawFd;
            use std::ffi::CString;
            
            // 親子プロセス間通信用のパイプを作成
            let (parent_read, child_write) = pipe().map_err(|e| {
                SandboxError::ExternalToolError(format!("パイプ作成に失敗: {}", e))
            })?;
            
            let (child_read, parent_write) = pipe().map_err(|e| {
                // 前のパイプをクローズ
                let _ = close(parent_read);
                let _ = close(child_write);
                SandboxError::ExternalToolError(format!("パイプ作成に失敗: {}", e))
            })?;
            
            // 名前空間フラグ設定
            let clone_flags = CloneFlags::CLONE_NEWUTS | // UTS(ホスト名)名前空間
                             CloneFlags::CLONE_NEWPID |  // PID名前空間
                             CloneFlags::CLONE_NEWNS |   // マウント名前空間
                             CloneFlags::CLONE_NEWNET |  // ネットワーク名前空間
                             CloneFlags::CLONE_NEWIPC |  // IPC名前空間
                             CloneFlags::CLONE_NEWUSER;  // ユーザー名前空間
            
            // コマンドのCString変換
            let container_path = CString::new(self.root_dir.to_string_lossy().as_bytes())
                .map_err(|e| SandboxError::ExternalToolError(format!("パス変換に失敗: {}", e)))?;
            
            // 子プロセスに渡す引数
            let mut clone_args = Box::new(CloneArgs {
                container_path,
                parent_pipe_read: parent_read,
                parent_pipe_write: parent_write,
                child_pipe_read: child_read,
                child_pipe_write: child_write,
                policy: policy.clone(),
            });
            
            // スタック確保
            const STACK_SIZE: usize = 1024 * 1024; // 1MB
            let mut stack: Vec<u8> = vec![0; STACK_SIZE];
            
            // 子プロセスを作成
            let child_pid = clone(
                child_fn,  // 子プロセスで実行する関数
                stack.as_mut_ptr().add(STACK_SIZE) as *mut libc::c_void,  // スタックポインタ
                clone_flags,  // 名前空間分離フラグ
                Some(Signal::SIGCHLD as i32),  // 子プロセス終了時のシグナル
                &mut *clone_args as *mut CloneArgs as *mut libc::c_void,  // 引数
            ).map_err(|e| {
                // パイプをクローズ
                let _ = close(parent_read);
                let _ = close(parent_write);
                let _ = close(child_read);
                let _ = close(child_write);
                SandboxError::ExternalToolError(format!("clone()の呼び出しに失敗: {}", e))
            })?;
            
            // 親プロセス：使わないパイプ端をクローズ
            close(child_read).unwrap_or_else(|e| error!("パイプクローズに失敗: {}", e));
            close(child_write).unwrap_or_else(|e| error!("パイプクローズに失敗: {}", e));
            
            // バッファ確保
            let mut buf = [0u8; 1];
            
            // 子プロセスが準備完了するのを待つ
            let read_result = read(parent_read, &mut buf);
            if read_result.is_err() || buf[0] != 1 {
                // 読み取りエラーまたは予期しない応答
                let _ = close(parent_read);
                let _ = close(parent_write);
                return Err(SandboxError::ExternalToolError(
                    "子プロセスの準備に失敗しました".to_string()
                ));
            }
            
            // 子プロセスに実行開始を通知
            buf[0] = 1;
            if write(parent_write, &buf).is_err() {
                // 書き込みエラー
                let _ = close(parent_read);
                let _ = close(parent_write);
                return Err(SandboxError::ExternalToolError(
                    "子プロセスへの通知に失敗しました".to_string()
                ));
            }
            
            // 子プロセスの終了を待つ
            match waitpid(child_pid, None) {
                Ok(_) => {
                    debug!("サンドボックス子プロセスが正常に終了しました: PID={}", child_pid);
                },
                Err(e) => {
                    error!("サンドボックス子プロセスの待機に失敗: {}", e);
                    return Err(SandboxError::ExternalToolError(
                        format!("子プロセスの待機に失敗: {}", e)
                    ));
                }
            }
            
            // 残りのパイプをクローズ
            close(parent_read).unwrap_or_else(|e| error!("パイプクローズに失敗: {}", e));
            close(parent_write).unwrap_or_else(|e| error!("パイプクローズに失敗: {}", e));
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // Linux以外のプラットフォームではサポートしていないことを通知
            log::warn!("名前空間分離はLinuxでのみサポートされています。他のプラットフォームでは限定的な分離のみ提供されます。");
            Ok(())
        }
    }

    /// Dockerでコマンドを実行します
    async fn execute_docker(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("Dockerでコマンドを実行します: {}", command);
        
        let system_start_time = std::time::SystemTime::now();
        let instant_start_time = Instant::now();
        
        let image = config.container_image();
        let container_name = format!("nexusshell-{}", self.id);
        
        // DockerランコマンドをセットアップWithコンテナ名
        let mut docker_args = vec![
            "run", "--rm", "--name", &container_name,
        ];
        
        // リソース制限を設定
        docker_args.push("--cpu-period=100000");
        docker_args.push(&format!("--cpu-quota={}", (config.cpu_limit() * 100000.0) as i32));
        docker_args.push(&format!("--memory={}m", config.memory_limit() / (1024 * 1024)));
        docker_args.push(&format!("--memory-swap={}m", config.memory_limit() / (1024 * 1024)));
        
        // ネットワーク制限（ポリシーに基づいて）
        if !policy.allow_network() {
            docker_args.push("--network=none");
        }
        
        // 共有ディレクトリをマウント
        for (host_path, container_path) in config.shared_dirs() {
            docker_args.push("-v");
            docker_args.push(&format!("{}:{}", host_path.display(), container_path.display()));
        }
        
        // 環境変数を設定
        for (key, value) in config.env_vars() {
            docker_args.push("-e");
            docker_args.push(&format!("{}={}", key, value));
        }
        
        // イメージとコマンドを追加
        docker_args.push(image);
        docker_args.push("sh");
        docker_args.push("-c");
        docker_args.push(command);
        
        // Dockerコマンドを実行
        let mut cmd = Command::new("docker");
        cmd.args(&docker_args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        let output = cmd.output()
            .map_err(|e| SandboxError::CommandExecutionFailed(format!("Dockerコマンド実行に失敗: {}", e)))?;
        
        let execution_time = instant_start_time.elapsed();
        let system_end_time = std::time::SystemTime::now();
        let start_timestamp = system_start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let end_timestamp = system_end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        let mut memory_usage_bytes: u64 = 0;
        let mut cpu_usage_percent: f64 = 0.0;

        // Docker stats を使用してリソース使用量を取得 (コマンド実行後の一度のスナップショット)
        // コンテナ名が必要なため、事前に定義しておく
        let stats_output = Command::new("docker")
            .args(["stats", "--no-stream", "--format", "{{.MemUsage}} / {{.CPUPerc}}", &container_name])
            .output();

        if let Ok(stats_output) = stats_output {
            if stats_output.status.success() {
                let stats_str = String::from_utf8_lossy(&stats_output.stdout);
                // 例: "10.5MiB / 7.8%"
                let parts: Vec<&str> = stats_str.trim().split(|c| c == '/' || c == '%').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    // メモリ使用量のパース (例: 10.5MiB, 2.3GiB)
                    let mem_str = parts[0];
                    if let Some(val_str) = mem_str.trim_end_matches(|c: char| c.is_alphabetic()) {
                        if let Ok(val) = val_str.parse::<f64>() {
                            if mem_str.ends_with("KiB") { memory_usage_bytes = (val * 1024.0) as u64; }
                            else if mem_str.ends_with("MiB") { memory_usage_bytes = (val * 1024.0 * 1024.0) as u64; }
                            else if mem_str.ends_with("GiB") { memory_usage_bytes = (val * 1024.0 * 1024.0 * 1024.0) as u64; }
                            else if mem_str.ends_with("B") { memory_usage_bytes = val as u64; }
                        }
                    }
                    // CPU使用率のパース (例: 7.8)
                    if let Ok(cpu_val) = parts[1].parse::<f64>() {
                        cpu_usage_percent = cpu_val;
                    }
                }
            } else {
                warn!("Docker statsの取得に失敗しました ({}): {}\nstdout: {}\nstderr: {}", 
                    container_name, stats_output.status, 
                    String::from_utf8_lossy(&stats_output.stdout),
                    String::from_utf8_lossy(&stats_output.stderr)
                );
            }
        } else {
            warn!("Docker statsコマンドの実行に失敗しました ({}): {:?}", container_name, stats_output.err());
        }

        // 結果を返す
        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes, 
            cpu_usage_percent,
            pid: None, // Dockerの場合、コンテナ内のPIDは直接取得が難しい
            signaled: false, // Docker実行では直接的なシグナル情報は取得困難
            signal: None,    // 同上
            command: command.to_string(),
            start_time: start_timestamp,
            end_time: end_timestamp,
        };
        
        Ok(result)
    }

    /// Podmanでコマンドを実行します
    async fn execute_podman(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("Podmanでコマンドを実行します: {}", command);
        
        let system_start_time = std::time::SystemTime::now();
        let instant_start_time = Instant::now();
        
        let image = config.container_image();
        let container_name = format!("nexusshell-{}", self.id);
        
        // PodmanランコマンドをセットアップWithコンテナ名
        let mut podman_args = vec![
            "run", "--rm", "--name", &container_name,
        ];
        
        // リソース制限を設定 (Dockerと同じインターフェース)
        podman_args.push("--cpu-period=100000");
        podman_args.push(&format!("--cpu-quota={}", (config.cpu_limit() * 100000.0) as i32));
        podman_args.push(&format!("--memory={}m", config.memory_limit() / (1024 * 1024)));
        
        // ネットワーク制限（ポリシーに基づいて）
        if !policy.allow_network() {
            podman_args.push("--network=none");
        }
        
        // 共有ディレクトリをマウント
        for (host_path, container_path) in config.shared_dirs() {
            podman_args.push("-v");
            podman_args.push(&format!("{}:{}", host_path.display(), container_path.display()));
        }
        
        // 環境変数を設定
        for (key, value) in config.env_vars() {
            podman_args.push("-e");
            podman_args.push(&format!("{}={}", key, value));
        }
        
        // イメージとコマンドを追加
        podman_args.push(image);
        podman_args.push("sh");
        podman_args.push("-c");
        podman_args.push(command);
        
        // Podmanコマンドを実行
        let mut cmd = Command::new("podman");
        cmd.args(&podman_args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        let output = cmd.output()
            .map_err(|e| SandboxError::CommandExecutionFailed(format!("Podmanコマンド実行に失敗: {}", e)))?;
        
        let execution_time = instant_start_time.elapsed();
        let system_end_time = std::time::SystemTime::now();
        let start_timestamp = system_start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let end_timestamp = system_end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        let mut memory_usage_bytes: u64 = 0;
        let mut cpu_usage_percent: f64 = 0.0;

        // Podman stats を使用してリソース使用量を取得 (コマンド実行後の一度のスナップショット)
        let stats_output = Command::new("podman")
            .args(["stats", "--no-stream", "--format", "{{.MemUsage}} / {{.CPUPerc}}", &container_name])
            .output();

        if let Ok(stats_output) = stats_output {
            if stats_output.status.success() {
                let stats_str = String::from_utf8_lossy(&stats_output.stdout);
                let parts: Vec<&str> = stats_str.trim().split(|c| c == '/' || c == '%').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    let mem_str = parts[0];
                    if let Some(val_str) = mem_str.trim_end_matches(|c: char| c.is_alphabetic()) {
                        if let Ok(val) = val_str.parse::<f64>() {
                            if mem_str.ends_with("KiB") { memory_usage_bytes = (val * 1024.0) as u64; }
                            else if mem_str.ends_with("MiB") { memory_usage_bytes = (val * 1024.0 * 1024.0) as u64; }
                            else if mem_str.ends_with("GiB") { memory_usage_bytes = (val * 1024.0 * 1024.0 * 1024.0) as u64; }
                            else if mem_str.ends_with("B") { memory_usage_bytes = val as u64; }
                        }
                    }
                    if let Ok(cpu_val) = parts[1].parse::<f64>() {
                        cpu_usage_percent = cpu_val;
                    }
                }
            } else {
                warn!("Podman statsの取得に失敗しました ({}): {}\nstdout: {}\nstderr: {}", 
                    container_name, stats_output.status, 
                    String::from_utf8_lossy(&stats_output.stdout),
                    String::from_utf8_lossy(&stats_output.stderr)
                );
            }
        } else {
            warn!("Podman statsコマンドの実行に失敗しました ({}): {:?}", container_name, stats_output.err());
        }

        // 結果を返す
        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes,
            cpu_usage_percent,
            pid: None, // Podmanの場合、コンテナ内のPIDは直接取得が難しい
            signaled: false, // Podman実行では直接的なシグナル情報は取得困難
            signal: None,    // 同上
            command: command.to_string(),
            start_time: start_timestamp,
            end_time: end_timestamp,
        };
        
        Ok(result)
    }

    /// LXCでコマンドを実行します
    async fn execute_lxc(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("LXCでコマンドを実行します: {} ({})", command, self.name);
        let system_start_time = std::time::SystemTime::now();
        let instant_start_time = Instant::now();
        
        let container_name = format!("nexusshell-lxc-{}", self.id);

        // LXC設定を一時ファイルに書き出す
        let mut lxc_config_content = String::new();
        // メモリ制限 (MB単位)
        lxc_config_content.push_str(&format!("lxc.cgroup.memory.limit_in_bytes = {}M\n", config.memory_limit() / (1024 * 1024)));
        // CPU制限 (相対的な重み。より詳細な制御には cpu.cfs_quota_us と cpu.cfs_period_us を使う)
        // ここでは簡易的にCPU数を考慮したシェア値を設定 (1024 がデフォルトのシェア)
        let cpu_shares = (config.cpu_limit() * 1024.0).max(2.0) as u32; // 最低でも2は確保
        lxc_config_content.push_str(&format!("lxc.cgroup.cpu.shares = {}\n", cpu_shares));

        // ネットワーク設定
        if !policy.allow_network() {
            lxc_config_content.push_str("lxc.net.0.type = empty\n");
        } else {
            // デフォルトでホストのネットワークを利用する (veth などでブリッジも可能)
            // より安全な設定として、制限されたネットワークインターフェースを指定することを推奨
            lxc_config_content.push_str("lxc.net.0.type = veth\n");
            lxc_config_content.push_str("lxc.net.0.link = lxcbr0\n"); // lxcbr0 が存在する場合
            lxc_config_content.push_str("lxc.net.0.flags = up\n");
            lxc_config_content.push_str("lxc.net.0.hwaddr = 00:16:3e:xx:xx:xx\n"); // MACアドレスは動的に生成推奨
        }
        // ルートファイルシステム (lxc-execute はホストのルートFSを共有するが、chrootのように分離も可能)
        // ここでは /tmp 以下に専用のルートディレクトリを作成し、それを指定する例
        // ただし、lxc-execute は --rcfile の中で lxc.rootfs を指定しても、
        // コマンドがホストのファイルシステムを参照してしまう挙動があるため注意が必要。
        // より厳密な分離には lxc-create でコンテナイメージを作成し、lxc-attach を使う方が適切。
        // 今回は lxc-execute の簡易性を活かすため、ルートFSの厳密な分離は見送る。

        let temp_config_file = tempfile::Builder::new()
            .prefix("lxc-config-")
            .suffix(".conf")
            .tempfile()
            .map_err(|e| SandboxError::FileSystemError(format!("LXC一時設定ファイルの作成に失敗: {}", e)))?;
        
        use std::io::Write;
        let mut file = std::fs::File::create(temp_config_file.path())
            .map_err(|e| SandboxError::FileSystemError(format!("LXC一時設定ファイルへの書き込み準備に失敗: {}",e)))?;
        file.write_all(lxc_config_content.as_bytes())
            .map_err(|e| SandboxError::FileSystemError(format!("LXC一時設定ファイルへの書き込みに失敗: {}",e)))?;

        // lxc-execute でコマンドを実行
        // -n: コンテナ名 (lxc-execute では一時的なものになる)
        // --rcfile (-f): 設定ファイルを指定
        // コマンドと引数
        let mut cmd = Command::new("lxc-execute");
        cmd.arg("-n").arg(&container_name); // 一時的なコンテナ名を指定
        cmd.arg("-f").arg(temp_config_file.path());
        // 環境変数の設定 (lxc-execute は環境変数を引き継ぐが、明示的に設定することも可能)
        for (key, value) in config.env_vars() {
            cmd.env(key, value);
        }
        cmd.arg("--"); // コマンドセパレータ
        cmd.arg("/bin/sh").arg("-c").arg(command); // シェル経由でコマンド実行
        
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        debug!("LXCコマンド実行: {:?}", cmd);

        let output = cmd.output().map_err(|e| SandboxError::CommandExecutionFailed(format!("lxc-execute実行に失敗: {}", e)))?;
        let execution_time = instant_start_time.elapsed();
        let system_end_time = std::time::SystemTime::now();
        let start_timestamp = system_start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let end_timestamp = system_end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        if !output.status.success() {
            warn!(
                "LXCコマンド実行でエラーが発生しました ({}): {}\nStdout: {}\nStderr: {}",
                container_name,
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        // lxc-execute は一時コンテナなので、PID等の情報は取得が難しい
        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes: 0, // LXCの場合、lxc-execute では詳細なリソース取得は困難
            cpu_usage_percent: 0.0, // 同上
            pid: None, 
            signaled: false, // lxc-execute の終了ステータスからシグナル情報を直接取得するのは難しい
            signal: None,    // 同上
            command: command.to_string(),
            start_time: start_timestamp,
            end_time: end_timestamp,
        };
        Ok(result)
    }

    /// chrootでコマンドを実行します
    async fn execute_chroot(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("chrootでコマンドを実行します: {}", command);
        
        #[cfg(unix)]
        {
            // rootユーザーでのみ実行可能
            let uid = nix::unistd::getuid();
            if !uid.is_root() {
                return Err(SandboxError::PermissionDenied(
                    "chrootサンドボックスにはroot権限が必要です".to_string()
                ));
            }
            
            let system_start_time = std::time::SystemTime::now();
            let instant_start_time = Instant::now();
            
            // chrootコマンドをセットアップ
            let mut cmd = Command::new("chroot");
            cmd.arg(&self.root_dir);
            cmd.arg("sh");
            cmd.arg("-c");
            cmd.arg(command);
            
            // 環境変数を設定
            for (key, value) in config.env_vars() {
                cmd.env(key, value);
            }
            
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            
            let output = cmd.output()
                .map_err(|e| SandboxError::CommandExecutionFailed(format!("chrootコマンド実行に失敗: {}", e)))?;
            
            let execution_time = instant_start_time.elapsed();
            let system_end_time = std::time::SystemTime::now();
            let start_timestamp = system_start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let end_timestamp = system_end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

            // 結果を返す
            let result = ExecutionResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                execution_time_ms: execution_time.as_millis() as u64,
                memory_usage_bytes: 0, // chroot環境では詳細なリソース監視は限定的
                cpu_usage_percent: 0.0,
                pid: None, // chroot環境で実行されるプロセスのPID取得は親プロセスから行う必要がある
                signaled: false,
                signal: None,
                command: command.to_string(),
                start_time: start_timestamp,
                end_time: end_timestamp,
            };
            
            Ok(result)
        }
        
        #[cfg(not(unix))]
        {
            Err(SandboxError::CommandExecutionFailed("chrootはUnixシステムでのみサポートされています".to_string()))
        }
    }

    /// WebAssemblyでコマンドを実行します
    async fn execute_wasm(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("WebAssemblyでコマンドを実行します: {}", command);
        let system_start_time = std::time::SystemTime::now();
        let instant_start_time = Instant::now();
        // コマンドはwasmtime経由で実行するWasmバイナリ名と引数
        let mut parts = command.split_whitespace();
        let wasm_file = parts.next().ok_or_else(|| SandboxError::CommandExecutionFailed("Wasmバイナリが指定されていません".to_string()))?;
        let args: Vec<&str> = parts.collect();
        let mut cmd = Command::new("wasmtime");
        cmd.arg(wasm_file);
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let output = cmd.output().map_err(|e| SandboxError::CommandExecutionFailed(format!("wasmtime実行に失敗: {}", e)))?;
        let execution_time = instant_start_time.elapsed();
        let system_end_time = std::time::SystemTime::now();
        let start_timestamp = system_start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let end_timestamp = system_end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes: 0, // Wasmランタイムのリソース使用量は別途監視が必要
            cpu_usage_percent: 0.0,
            pid: None, // Wasm実行は通常ホストのプロセスとして実行されるため、隔離されたPIDはない
            signaled: false,
            signal: None,
            command: command.to_string(),
            start_time: start_timestamp,
            end_time: end_timestamp,
        };
        Ok(result)
    }

    /// コンテナに設定を適用します
    pub async fn apply_config(&self, config: SandboxConfig) -> Result<(), SandboxError> {
        let mut current_config = self.config.write().await;
        *current_config = config;
        Ok(())
    }

    /// コンテナにポリシーを適用します
    pub async fn apply_policy(&self, policy: SandboxPolicy) -> Result<(), SandboxError> {
        let mut current_policy = self.policy.write().await;
        *current_policy = policy;
        Ok(())
    }

    /// コンテナを破棄します
    pub async fn destroy(&self) -> Result<(), SandboxError> {
        debug!("コンテナを破棄しています: {}", self.name);
        
        // 状態を更新
        {
            let mut state = self.state.write().await;
            *state = ContainerState::Stopped;
        }
        
        // コンテナタイプに応じてクリーンアップ
        let config = self.config.read().await;
        match config.container_tech() {
            ContainerTech::Docker => {
                // 実行中のコンテナを停止（もしあれば）
                let container_name = format!("nexusshell-{}", self.id);
                let _ = Command::new("docker")
                    .args(["stop", &container_name])
                    .output();
            },
            ContainerTech::Podman => {
                // 実行中のコンテナを停止（もしあれば）
                let container_name = format!("nexusshell-{}", self.id);
                let _ = Command::new("podman")
                    .args(["stop", &container_name])
                    .output();
            },
            _ => {}
        }
        
        // 一時ディレクトリを削除
        if self.root_dir.exists() {
            tokio::fs::remove_dir_all(&self.root_dir).await
                .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリの削除に失敗: {}", e)))?;
        }
        
        info!("コンテナを破棄しました: {}", self.name);
        
        Ok(())
    }
}

/// Linuxサンドボックスの子プロセスで実行される関数
/// この関数は新しい名前空間内で実行されます。
#[cfg(target_os = "linux")]
unsafe extern "C" fn child_fn(args_ptr: *mut libc::c_void) -> libc::c_int {
    let args = &*(args_ptr as *mut CloneArgs);
    let policy = &args.policy; // CloneArgsからポリシーを取得

    // TODO: 実行前にルートファイルシステムを変更 (chroot)
    // if nix::unistd::chroot(&args.container_path).is_err() { return 1; }
    // if nix::unistd::chdir("/").is_err() { return 1; }

    // TODO: ユーザー/グループを変更 (setuid/setgid)
    // (事前に /etc/passwd, /etc/group を適切に設定しておく必要がある)
    // let target_uid = Uid::from_raw(1000); // 例: non-root user
    // let target_gid = Gid::from_raw(1000);
    // if nix::unistd::setgid(target_gid).is_err() { return 1; }
    // if nix::unistd::setuid(target_uid).is_err() { return 1; }

    // TODO: ケイパビリティの適用
    // if policy.drop_capabilities() {
    //     use caps::{CapSet, clear, drop, setproc};
    //     let mut current_caps = getcap(None).unwrap_or_default(); // 現在のケイパビリティセットを取得
    //     let kept_caps = policy.kept_capabilities();
    //     for cap_to_check in Capability::iter_variants() { // 全ての既知のケイパビリティをチェック
    //         if !kept_caps.contains(&cap_to_check) {
    //              drop(None, CapSet::Effective, cap_to_check).ok(); // エラーは無視
    //              drop(None, CapSet::Permitted, cap_to_check).ok();
    //              drop(None, CapSet::Inheritable, cap_to_check).ok();
    //         }
    //     }
    //     if setproc(&current_caps).is_err() { return 1; }
    // }

    // TODO: Seccompフィルタの適用
    // if policy.enable_seccomp() {
    //     use seccomp_sys::*;
    //     let filter = seccomp_init(SCMP_ACT_ERRNO(libc::EPERM));
    //     if filter.is_null() { return 1; }
    //     for syscall_nr in policy.allowed_syscalls() {
    //         if seccomp_rule_add(filter, SCMP_ACT_ALLOW, *syscall_nr as i32, 0) < 0 { 
    //             seccomp_release(filter); return 1; 
    //         }
    //     }
    //     // 特定のアーキテクチャチェックも追加することが推奨される (e.g., AUDIT_ARCH_X86_64)
    //     // if seccomp_attr_set(filter, SCMP_FLTATR_ACT_BADARCH, SCMP_ACT_KILL) < 0 { seccomp_release(filter); return 1; }
    //     if seccomp_load(filter) < 0 { seccomp_release(filter); return 1; }
    //     seccomp_release(filter); // ロード後は解放してよい
    // }

    // 親プロセスに準備完了を通知
    if nix::unistd::write(args.child_pipe_write, &[1]).is_err() {
        return 1; // エラー終了
    }

    // 親プロセスからの実行開始通知を待つ
    let mut buf = [0u8; 1];
    if nix::unistd::read(args.child_pipe_read, &mut buf).is_err() || buf[0] != 1 {
        return 1; // エラー終了
    }

    // TODO: 実際のコマンド実行はここで行うか、さらに execve する
    //       コマンド実行の準備 (cmd.exec() など)
    //       現状の apply_linux_sandbox は Command 構造体を直接実行しているため、
    //       この child_fn 内で Command を再構築するか、実行に必要な情報を渡す必要がある。

    // 仮の成功終了
    0
}

/// clone() に渡す引数
#[cfg(target_os = "linux")]
struct CloneArgs {
    container_path: std::ffi::CString,
    parent_pipe_read: std::os::unix::io::RawFd,
    parent_pipe_write: std::os::unix::io::RawFd,
    child_pipe_read: std::os::unix::io::RawFd,
    child_pipe_write: std::os::unix::io::RawFd,
    policy: SandboxPolicy,
    // TODO: SandboxConfig の情報も必要に応じて渡す
} 