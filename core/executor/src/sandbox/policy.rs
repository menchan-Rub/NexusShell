use std::collections::HashSet;
use std::path::PathBuf;

/// サンドボックスセキュリティポリシー
/// サンドボックス内で許可される操作を定義します
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// ファイルシステムアクセスを許可するかどうか
    allow_fs_access: bool,
    /// ネットワークアクセスを許可するかどうか
    allow_network: bool,
    /// 外部プロセスの実行を許可するかどうか
    allow_process_execution: bool,
    /// システムコール制限を有効にするかどうか (Linux seccomp)
    enable_seccomp: bool,
    /// 特権の降格を有効にするかどうか (capabilities)
    drop_capabilities: bool,
    /// 読み取りアクセスが許可されるパス
    allowed_read_paths: HashSet<PathBuf>,
    /// 書き込みアクセスが許可されるパス
    allowed_write_paths: HashSet<PathBuf>,
    /// 実行が許可されるパス
    allowed_exec_paths: HashSet<PathBuf>,
    /// 許可されるシステムコール（seccompで使用）
    allowed_syscalls: HashSet<u32>,
    /// 拒否されるシステムコール（seccompで使用）
    denied_syscalls: HashSet<u32>,
    /// 環境変数へのアクセスを許可するかどうか
    allow_env_access: bool,
    /// タイムゾーン情報へのアクセスを許可するかどうか
    allow_timezone_access: bool,
}

impl SandboxPolicy {
    /// 新しいセキュリティポリシーを作成します
    pub fn new() -> Self {
        Self {
            allow_fs_access: false,
            allow_network: false,
            allow_process_execution: false,
            enable_seccomp: true,
            drop_capabilities: true,
            allowed_read_paths: HashSet::new(),
            allowed_write_paths: HashSet::new(),
            allowed_exec_paths: HashSet::new(),
            allowed_syscalls: Self::default_allowed_syscalls(),
            denied_syscalls: Self::default_denied_syscalls(),
            allow_env_access: false,
            allow_timezone_access: false,
        }
    }

    /// 基本的なセキュリティポリシーを作成します
    pub fn basic() -> Self {
        let mut policy = Self::new();
        policy.allow_fs_access = true;
        policy.allow_process_execution = true;
        policy.allow_env_access = true;
        
        // 基本的な読み取りパスを許可
        policy.allowed_read_paths.insert(PathBuf::from("/usr/bin"));
        policy.allowed_read_paths.insert(PathBuf::from("/usr/lib"));
        policy.allowed_read_paths.insert(PathBuf::from("/lib"));
        policy.allowed_read_paths.insert(PathBuf::from("/etc"));
        
        // 基本的な書き込みパスを許可
        policy.allowed_write_paths.insert(PathBuf::from("/tmp"));
        
        // 基本的な実行パスを許可
        policy.allowed_exec_paths.insert(PathBuf::from("/usr/bin"));
        policy.allowed_exec_paths.insert(PathBuf::from("/bin"));
        
        policy
    }

    /// リラックスしたセキュリティポリシーを作成します
    pub fn relaxed() -> Self {
        let mut policy = Self::basic();
        policy.allow_network = true;
        policy.enable_seccomp = false;
        policy.drop_capabilities = false;
        policy.allow_timezone_access = true;
        
        policy
    }

    /// ファイルシステムアクセスを許可するかどうかを取得します
    pub fn allow_fs_access(&self) -> bool {
        self.allow_fs_access
    }

    /// ファイルシステムアクセスを許可するかどうかを設定します
    pub fn set_allow_fs_access(&mut self, allow: bool) {
        self.allow_fs_access = allow;
    }

    /// ネットワークアクセスを許可するかどうかを取得します
    pub fn allow_network(&self) -> bool {
        self.allow_network
    }

    /// ネットワークアクセスを許可するかどうかを設定します
    pub fn set_allow_network(&mut self, allow: bool) {
        self.allow_network = allow;
    }

    /// 外部プロセスの実行を許可するかどうかを取得します
    pub fn allow_process_execution(&self) -> bool {
        self.allow_process_execution
    }

    /// 外部プロセスの実行を許可するかどうかを設定します
    pub fn set_allow_process_execution(&mut self, allow: bool) {
        self.allow_process_execution = allow;
    }

    /// seccompを有効にするかどうかを取得します
    pub fn enable_seccomp(&self) -> bool {
        self.enable_seccomp
    }

    /// seccompを有効にするかどうかを設定します
    pub fn set_enable_seccomp(&mut self, enable: bool) {
        self.enable_seccomp = enable;
    }

    /// ケイパビリティをドロップするかどうかを取得します
    pub fn drop_capabilities(&self) -> bool {
        self.drop_capabilities
    }

    /// ケイパビリティをドロップするかどうかを設定します
    pub fn set_drop_capabilities(&mut self, drop: bool) {
        self.drop_capabilities = drop;
    }

    /// 読み取りアクセスが許可されるパスを取得します
    pub fn allowed_read_paths(&self) -> &HashSet<PathBuf> {
        &self.allowed_read_paths
    }

    /// 読み取りアクセスが許可されるパスを追加します
    pub fn add_allowed_read_path(&mut self, path: PathBuf) {
        self.allowed_read_paths.insert(path);
    }

    /// 書き込みアクセスが許可されるパスを取得します
    pub fn allowed_write_paths(&self) -> &HashSet<PathBuf> {
        &self.allowed_write_paths
    }

    /// 書き込みアクセスが許可されるパスを追加します
    pub fn add_allowed_write_path(&mut self, path: PathBuf) {
        self.allowed_write_paths.insert(path);
    }

    /// 実行が許可されるパスを取得します
    pub fn allowed_exec_paths(&self) -> &HashSet<PathBuf> {
        &self.allowed_exec_paths
    }

    /// 実行が許可されるパスを追加します
    pub fn add_allowed_exec_path(&mut self, path: PathBuf) {
        self.allowed_exec_paths.insert(path);
    }

    /// 許可されるシステムコールを取得します
    pub fn allowed_syscalls(&self) -> &HashSet<u32> {
        &self.allowed_syscalls
    }

    /// 許可されるシステムコールを追加します
    pub fn add_allowed_syscall(&mut self, syscall: u32) {
        self.allowed_syscalls.insert(syscall);
        self.denied_syscalls.remove(&syscall);
    }

    /// 拒否されるシステムコールを取得します
    pub fn denied_syscalls(&self) -> &HashSet<u32> {
        &self.denied_syscalls
    }

    /// 拒否されるシステムコールを追加します
    pub fn add_denied_syscall(&mut self, syscall: u32) {
        self.denied_syscalls.insert(syscall);
        self.allowed_syscalls.remove(&syscall);
    }

    /// 環境変数へのアクセスを許可するかどうかを取得します
    pub fn allow_env_access(&self) -> bool {
        self.allow_env_access
    }

    /// 環境変数へのアクセスを許可するかどうかを設定します
    pub fn set_allow_env_access(&mut self, allow: bool) {
        self.allow_env_access = allow;
    }

    /// タイムゾーン情報へのアクセスを許可するかどうかを取得します
    pub fn allow_timezone_access(&self) -> bool {
        self.allow_timezone_access
    }

    /// タイムゾーン情報へのアクセスを許可するかどうかを設定します
    pub fn set_allow_timezone_access(&mut self, allow: bool) {
        self.allow_timezone_access = allow;
    }

    /// デフォルトで許可されるシステムコールを取得します
    fn default_allowed_syscalls() -> HashSet<u32> {
        let syscalls = [
            // 基本的なファイルシステム関連のシステムコール
            0,    // read
            1,    // write
            2,    // open
            3,    // close
            4,    // stat
            5,    // fstat
            8,    // lseek
            9,    // mmap
            10,   // mprotect
            11,   // munmap
            12,   // brk
            16,   // ioctl (制限付き)
            21,   // access
            59,   // execve
            63,   // uname
            89,   // readlink
            97,   // getrlimit
            100,  // times
            158,  // arch_prctl
            186,  // gettid
            202,  // futex
            218,  // set_tid_address
            228,  // clock_gettime
            231,  // exit_group
            257,  // openat
            262,  // newfstatat
            
            // メモリ管理
            13,   // rt_sigaction
            14,   // rt_sigprocmask
            
            // 時間関連
            35,   // nanosleep
            228,  // clock_gettime
            
            // プロセス関連
            56,   // clone
            57,   // fork
            58,   // vfork
            60,   // exit
            61,   // wait4
            62,   // kill (自身のみ)
            
            // スレッド関連
            186,  // gettid
            56,   // clone (スレッド作成用)
            
            // ソケット関連（ネットワークが許可される場合）
            41,   // socket
            42,   // connect
            43,   // accept
            44,   // sendto
            45,   // recvfrom
            46,   // sendmsg
            47,   // recvmsg
            48,   // shutdown
            49,   // bind
            50,   // listen
            51,   // getsockname
            52,   // getpeername
            53,   // socketpair
            54,   // setsockopt
            55,   // getsockopt
            
            // その他の安全なシステムコール
            157,  // prctl (制限付き)
            302,  // prlimit64
        ].iter().cloned().collect();
        
        syscalls
    }

    /// デフォルトで拒否されるシステムコールを取得します
    fn default_denied_syscalls() -> HashSet<u32> {
        let syscalls = [
            // カーネルモジュール関連
            175,  // init_module
            176,  // delete_module
            
            // リブート関連
            169,  // reboot
            
            // デバイス関連
            85,   // creat
            
            // マウント関連
            165,  // mount
            166,  // umount2
            
            // ネットワーク監視関連
            101,  // ptrace
            
            // ユーザー/グループID関連（特権昇格に利用される可能性）
            105,  // setuid
            106,  // setgid
            113,  // setreuid
            114,  // setregid
            117,  // setresuid
            118,  // setresgid
            
            // システム管理関連
            153,  // chroot
            
            // プロセス/スレッド管理関連（危険な操作）
            130,  // kill (他のプロセス)
            131,  // tgkill
            142,  // sched_setparam
            144,  // sched_setscheduler
            
            // 時間関連（システム時刻変更）
            227,  // clock_settime
        ].iter().cloned().collect();
        
        syscalls
    }
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::new()
    }
} 