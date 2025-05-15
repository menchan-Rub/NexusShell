use super::error::SandboxError;
use super::config::{SandboxConfig, ContainerTech};
use super::policy::SandboxPolicy;
use super::ExecutionResult;

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
        
        // ここでは実装を簡略化していますが、実際にはLXCコンテナの作成とセットアップが必要
        
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
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: {}", e)))?;
        
        tokio::fs::create_dir_all(root.join("lib")).await
            .map_err(|e| SandboxError::FileSystemError(format!("ディレクトリ作成に失敗: {}", e)))?;
        
        // 基本的なコマンドをコピー
        // 実際の実装ではもっと多くのファイルとライブラリが必要
        
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
        
        let start_time = Instant::now();
        
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
        
        let execution_time = start_time.elapsed();
        
        // システム情報を取得
        let mut system = System::new_all();
        system.refresh_all();
        
        // 結果を返す
        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes: 0, // 実際のプロセスはすでに終了しているため、正確な計測は難しい
            cpu_usage_percent: 0.0,
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
        // Linux名前空間の分離（実際の実装ではclone()を使用）
        
        // SECCOMPフィルターの設定
        #[cfg(feature = "seccomp")]
        if policy.enable_seccomp() {
            let allowed_syscalls = policy.allowed_syscalls();
            
            // 事前フォークの安全対策として、unsafe内での実装を回避
            cmd.before_exec(move || {
                // seccompフィルターを初期化
                let ctx = unsafe { seccomp_init(SCMP_ACT_ERRNO) };
                if ctx.is_null() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "seccompの初期化に失敗しました"
                    ));
                }
                
                // 許可されたシステムコールを追加
                for syscall in allowed_syscalls {
                    let res = unsafe {
                        seccomp_rule_add(
                            ctx,
                            SCMP_ACT_ALLOW,
                            syscall as i32,
                            0,
                        )
                    };
                    
                    if res != 0 {
                        unsafe { seccomp_release(ctx) };
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("seccompルールの追加に失敗: syscall={}", syscall)
                        ));
                    }
                }
                
                // seccompフィルターを適用
                let res = unsafe { seccomp_load(ctx) };
                unsafe { seccomp_release(ctx) };
                
                if res != 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "seccompフィルターの適用に失敗しました"
                    ));
                }
                
                Ok(())
            });
        }
        
        // ケイパビリティの制限
        if policy.drop_capabilities() {
            cmd.before_exec(move || {
                // すべてのケイパビリティをドロップ
                caps::clear(None, CapSet::Effective)
                    .map_err(|e| std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("ケイパビリティのクリアに失敗: {}", e)
                    ))?;
                
                caps::clear(None, CapSet::Permitted)
                    .map_err(|e| std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("ケイパビリティのクリアに失敗: {}", e)
                    ))?;
                
                caps::clear(None, CapSet::Inheritable)
                    .map_err(|e| std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("ケイパビリティのクリアに失敗: {}", e)
                    ))?;
                
                Ok(())
            });
        }
        
        Ok(())
    }

    /// Dockerでコマンドを実行します
    async fn execute_docker(
        &self,
        command: &str,
        config: &SandboxConfig,
        policy: &SandboxPolicy,
    ) -> Result<ExecutionResult, SandboxError> {
        debug!("Dockerでコマンドを実行します: {}", command);
        
        let start_time = Instant::now();
        
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
        
        let execution_time = start_time.elapsed();
        
        // 結果を返す
        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes: 0, // Dockerの場合は別途statsコマンドなどで取得する必要がある
            cpu_usage_percent: 0.0,
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
        
        let start_time = Instant::now();
        
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
        
        let execution_time = start_time.elapsed();
        
        // 結果を返す
        let result = ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            execution_time_ms: execution_time.as_millis() as u64,
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
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
        debug!("LXCでコマンドを実行します: {}", command);
        
        // 実際のLXC実装は本番環境に応じて実装する必要があります
        // ここでは簡略化されたモックを返します
        
        Err(SandboxError::CommandExecutionFailed("LXC実行はまだ実装されていません".to_string()))
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
            
            let start_time = Instant::now();
            
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
            
            let execution_time = start_time.elapsed();
            
            // 結果を返す
            let result = ExecutionResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                execution_time_ms: execution_time.as_millis() as u64,
                memory_usage_bytes: 0,
                cpu_usage_percent: 0.0,
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
        
        // 実際のWasm実装は本番環境に応じて実装する必要があります
        // ここでは簡略化されたモックを返します
        
        Err(SandboxError::CommandExecutionFailed("WebAssembly実行はまだ実装されていません".to_string()))
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