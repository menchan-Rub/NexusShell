use std::time::Duration;
use std::path::PathBuf;

/// サンドボックス設定
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// CPUの制限（コア数）
    cpu_limit: f64,
    /// メモリの制限（バイト）
    memory_limit: u64,
    /// ディスク容量の制限（バイト）
    disk_limit: u64,
    /// 実行時間の制限
    time_limit: Duration,
    /// プロセス数の制限
    process_limit: u32,
    /// ファイルディスクリプタの制限
    fd_limit: u32,
    /// ネットワークバンド幅制限（バイト/秒）
    network_bandwidth: Option<u64>,
    /// 一時ディレクトリ
    temp_dir: PathBuf,
    /// コンテナ技術
    container_tech: ContainerTech,
    /// コンテナイメージ
    container_image: String,
    /// 共有ディレクトリ
    shared_dirs: Vec<(PathBuf, PathBuf)>,
    /// 環境変数
    env_vars: Vec<(String, String)>,
    /// ルートで実行
    run_as_root: bool,
    /// コマンド実行タイムアウト
    command_timeout: Duration,
}

impl SandboxConfig {
    /// 新しいサンドボックス設定を作成します
    pub fn new() -> Self {
        Self::default()
    }

    /// CPU制限を取得します
    pub fn cpu_limit(&self) -> f64 {
        self.cpu_limit
    }

    /// CPU制限を設定します
    pub fn set_cpu_limit(&mut self, limit: f64) {
        self.cpu_limit = limit;
    }

    /// メモリ制限を取得します
    pub fn memory_limit(&self) -> u64 {
        self.memory_limit
    }

    /// メモリ制限を設定します
    pub fn set_memory_limit(&mut self, limit: u64) {
        self.memory_limit = limit;
    }

    /// ディスク容量制限を取得します
    pub fn disk_limit(&self) -> u64 {
        self.disk_limit
    }

    /// ディスク容量制限を設定します
    pub fn set_disk_limit(&mut self, limit: u64) {
        self.disk_limit = limit;
    }

    /// 実行時間制限を取得します
    pub fn time_limit(&self) -> Duration {
        self.time_limit
    }

    /// 実行時間制限を設定します
    pub fn set_time_limit(&mut self, limit: Duration) {
        self.time_limit = limit;
    }

    /// プロセス数制限を取得します
    pub fn process_limit(&self) -> u32 {
        self.process_limit
    }

    /// プロセス数制限を設定します
    pub fn set_process_limit(&mut self, limit: u32) {
        self.process_limit = limit;
    }

    /// ファイルディスクリプタ制限を取得します
    pub fn fd_limit(&self) -> u32 {
        self.fd_limit
    }

    /// ファイルディスクリプタ制限を設定します
    pub fn set_fd_limit(&mut self, limit: u32) {
        self.fd_limit = limit;
    }

    /// ネットワークバンド幅制限を取得します
    pub fn network_bandwidth(&self) -> Option<u64> {
        self.network_bandwidth
    }

    /// ネットワークバンド幅制限を設定します
    pub fn set_network_bandwidth(&mut self, limit: Option<u64>) {
        self.network_bandwidth = limit;
    }

    /// 一時ディレクトリを取得します
    pub fn temp_dir(&self) -> &PathBuf {
        &self.temp_dir
    }

    /// 一時ディレクトリを設定します
    pub fn set_temp_dir(&mut self, dir: PathBuf) {
        self.temp_dir = dir;
    }

    /// コンテナ技術を取得します
    pub fn container_tech(&self) -> ContainerTech {
        self.container_tech
    }

    /// コンテナ技術を設定します
    pub fn set_container_tech(&mut self, tech: ContainerTech) {
        self.container_tech = tech;
    }

    /// コンテナイメージを取得します
    pub fn container_image(&self) -> &str {
        &self.container_image
    }

    /// コンテナイメージを設定します
    pub fn set_container_image(&mut self, image: &str) {
        self.container_image = image.to_string();
    }

    /// 共有ディレクトリを取得します
    pub fn shared_dirs(&self) -> &[(PathBuf, PathBuf)] {
        &self.shared_dirs
    }

    /// 共有ディレクトリを追加します
    pub fn add_shared_dir(&mut self, host_path: PathBuf, container_path: PathBuf) {
        self.shared_dirs.push((host_path, container_path));
    }

    /// 共有ディレクトリをクリアします
    pub fn clear_shared_dirs(&mut self) {
        self.shared_dirs.clear();
    }

    /// 環境変数を取得します
    pub fn env_vars(&self) -> &[(String, String)] {
        &self.env_vars
    }

    /// 環境変数を追加します
    pub fn add_env_var(&mut self, key: &str, value: &str) {
        self.env_vars.push((key.to_string(), value.to_string()));
    }

    /// 環境変数をクリアします
    pub fn clear_env_vars(&mut self) {
        self.env_vars.clear();
    }

    /// ルートで実行するかどうかを取得します
    pub fn run_as_root(&self) -> bool {
        self.run_as_root
    }

    /// ルートで実行するかどうかを設定します
    pub fn set_run_as_root(&mut self, root: bool) {
        self.run_as_root = root;
    }

    /// コマンド実行タイムアウトを取得します
    pub fn command_timeout(&self) -> Duration {
        self.command_timeout
    }

    /// コマンド実行タイムアウトを設定します
    pub fn set_command_timeout(&mut self, timeout: Duration) {
        self.command_timeout = timeout;
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            cpu_limit: 1.0,
            memory_limit: 512 * 1024 * 1024, // 512MB
            disk_limit: 1024 * 1024 * 1024,  // 1GB
            time_limit: Duration::from_secs(60),
            process_limit: 10,
            fd_limit: 64,
            network_bandwidth: None,
            temp_dir: std::env::temp_dir().join("nexusshell_sandbox"),
            container_tech: ContainerTech::Native,
            container_image: "alpine:latest".to_string(),
            shared_dirs: Vec::new(),
            env_vars: Vec::new(),
            run_as_root: false,
            command_timeout: Duration::from_secs(30),
        }
    }
}

/// コンテナ技術
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerTech {
    /// ネイティブ（OS制限のみ）
    Native,
    /// Docker
    Docker,
    /// Podman
    Podman,
    /// LXC/LXD
    Lxc,
    /// chroot
    Chroot,
    /// WebAssembly
    Wasm,
} 