use std::time::Duration;

/// リモート接続設定
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// 接続タイムアウト
    connect_timeout: Duration,
    /// コマンド実行タイムアウト
    command_timeout: Duration,
    /// TCP キープアライブ間隔
    tcp_keepalive: Option<Duration>,
    /// 接続再試行回数
    retry_count: usize,
    /// 接続再試行間隔
    retry_interval: Duration,
    /// デフォルトポート
    default_port: u16,
    /// 環境変数の転送
    forward_env_vars: bool,
    /// X11転送
    forward_x11: bool,
    /// agent転送
    forward_agent: bool,
    /// TCP転送
    tcp_forwarding: bool,
    /// コマンド実行にPTYを割り当てる
    allocate_pty: bool,
    /// 圧縮を有効にするかどうか
    compression: bool,
}

impl RemoteConfig {
    /// 新しいリモート設定を作成します
    pub fn new() -> Self {
        Self::default()
    }

    /// 接続タイムアウトを取得します
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// 接続タイムアウトを設定します
    pub fn set_connect_timeout(&mut self, timeout: Duration) {
        self.connect_timeout = timeout;
    }

    /// コマンド実行タイムアウトを取得します
    pub fn command_timeout(&self) -> Duration {
        self.command_timeout
    }

    /// コマンド実行タイムアウトを設定します
    pub fn set_command_timeout(&mut self, timeout: Duration) {
        self.command_timeout = timeout;
    }

    /// TCPキープアライブ間隔を取得します
    pub fn tcp_keepalive(&self) -> Option<Duration> {
        self.tcp_keepalive
    }

    /// TCPキープアライブ間隔を設定します
    pub fn set_tcp_keepalive(&mut self, interval: Option<Duration>) {
        self.tcp_keepalive = interval;
    }

    /// 接続再試行回数を取得します
    pub fn retry_count(&self) -> usize {
        self.retry_count
    }

    /// 接続再試行回数を設定します
    pub fn set_retry_count(&mut self, count: usize) {
        self.retry_count = count;
    }

    /// 接続再試行間隔を取得します
    pub fn retry_interval(&self) -> Duration {
        self.retry_interval
    }

    /// 接続再試行間隔を設定します
    pub fn set_retry_interval(&mut self, interval: Duration) {
        self.retry_interval = interval;
    }

    /// デフォルトポートを取得します
    pub fn default_port(&self) -> u16 {
        self.default_port
    }

    /// デフォルトポートを設定します
    pub fn set_default_port(&mut self, port: u16) {
        self.default_port = port;
    }

    /// 環境変数の転送が有効かどうかを取得します
    pub fn forward_env_vars(&self) -> bool {
        self.forward_env_vars
    }

    /// 環境変数の転送の有効/無効を設定します
    pub fn set_forward_env_vars(&mut self, forward: bool) {
        self.forward_env_vars = forward;
    }

    /// X11転送が有効かどうかを取得します
    pub fn forward_x11(&self) -> bool {
        self.forward_x11
    }

    /// X11転送の有効/無効を設定します
    pub fn set_forward_x11(&mut self, forward: bool) {
        self.forward_x11 = forward;
    }

    /// agent転送が有効かどうかを取得します
    pub fn forward_agent(&self) -> bool {
        self.forward_agent
    }

    /// agent転送の有効/無効を設定します
    pub fn set_forward_agent(&mut self, forward: bool) {
        self.forward_agent = forward;
    }

    /// TCP転送が有効かどうかを取得します
    pub fn tcp_forwarding(&self) -> bool {
        self.tcp_forwarding
    }

    /// TCP転送の有効/無効を設定します
    pub fn set_tcp_forwarding(&mut self, forwarding: bool) {
        self.tcp_forwarding = forwarding;
    }

    /// PTY割り当てが有効かどうかを取得します
    pub fn allocate_pty(&self) -> bool {
        self.allocate_pty
    }

    /// PTY割り当ての有効/無効を設定します
    pub fn set_allocate_pty(&mut self, allocate: bool) {
        self.allocate_pty = allocate;
    }

    /// 圧縮が有効かどうかを取得します
    pub fn compression(&self) -> bool {
        self.compression
    }

    /// 圧縮の有効/無効を設定します
    pub fn set_compression(&mut self, compression: bool) {
        self.compression = compression;
    }
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            command_timeout: Duration::from_secs(60),
            tcp_keepalive: Some(Duration::from_secs(15)),
            retry_count: 3,
            retry_interval: Duration::from_secs(5),
            default_port: 22,
            forward_env_vars: true,
            forward_x11: false,
            forward_agent: false,
            tcp_forwarding: false,
            allocate_pty: true,
            compression: true,
        }
    }
} 