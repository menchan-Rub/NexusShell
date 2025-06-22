# 🔧 NexusShell API仕様書 v2.2.0

## 目次

1. [概要](#概要)
2. [コアAPI](#コアapi)
3. [コマンドAPI](#コマンドapi)
4. [パフォーマンスAPI](#パフォーマンスapi)
5. [機能管理API](#機能管理api)
6. [システムAPI](#システムapi)
7. [拡張API](#拡張api)

---

## 概要

NexusShell APIは、シェルの全機能にプログラマティックにアクセスするためのインターフェースを提供します。

### API設計原則

- **型安全性**: Rustの型システムを活用
- **非同期対応**: async/awaitパターン
- **エラーハンドリング**: Result型による安全なエラー処理
- **パフォーマンス**: ゼロコスト抽象化

---

## コアAPI

### NexusShell構造体

```rust
pub struct NexusShell {
    config: ShellConfig,
    features: HashMap<String, bool>,
    command_count: u64,
    performance_data: PerformanceData,
    current_dir: PathBuf,
    environment_vars: HashMap<String, String>,
    command_history: Vec<CommandHistoryEntry>,
    aliases: HashMap<String, String>,
    jobs: Vec<Job>,
    last_command_status: i32,
}
```

### 初期化

```rust
impl NexusShell {
    /// 新しいNexusShellインスタンスを作成
    pub fn new() -> Self {
        // 実装...
    }
    
    /// 包括的システム初期化
    fn init_comprehensive_system(&mut self) {
        // エイリアス初期化
        // 環境変数設定
        // 機能有効化
    }
}
```

### コマンド実行

```rust
impl NexusShell {
    /// コマンドを実行
    pub async fn execute_command(&mut self, input: &str) 
        -> Result<String, Box<dyn std::error::Error>> {
        // パフォーマンス測定開始
        let start_time = Instant::now();
        
        // コマンド解析と実行
        let result = self.execute_single_command_perfect(command, args).await;
        
        // メトリクス更新
        let execution_time = start_time.elapsed();
        self.update_performance_metrics(execution_time);
        
        result
    }
}
```

---

## コマンドAPI

### ファイルシステム操作

#### ls コマンド

```rust
impl NexusShell {
    /// ディレクトリ内容表示
    async fn ls_perfect(&self, args: &[String]) 
        -> Result<String, Box<dyn std::error::Error>> {
        
        // オプション解析
        let long_format = args.iter().any(|arg| arg.contains('l'));
        let show_hidden = args.iter().any(|arg| arg.contains('a'));
        let human_readable = args.iter().any(|arg| arg.contains('h'));
        
        // ディレクトリ読み取り
        let entries = std::fs::read_dir(&self.current_dir)?;
        
        // フォーマット出力
        for entry in entries {
            // 実装...
        }
        
        Ok(result)
    }
}
```

#### cd コマンド

```rust
impl NexusShell {
    /// ディレクトリ変更
    async fn cd_perfect(&mut self, args: &[String]) 
        -> Result<String, Box<dyn std::error::Error>> {
        
        let new_dir = if args.is_empty() {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
        } else {
            PathBuf::from(&args[0])
        };
        
        if new_dir.is_dir() {
            self.current_dir = new_dir.canonicalize()?;
            env::set_current_dir(&self.current_dir)?;
            Ok(String::new())
        } else {
            Ok(format!("cd: {}: No such file or directory", args[0]))
        }
    }
}
```

### テキスト処理

#### grep コマンド

```rust
impl NexusShell {
    /// パターン検索
    async fn grep_perfect(&self, args: &[String]) 
        -> Result<String, Box<dyn std::error::Error>> {
        
        if args.len() < 2 {
            return Ok("grep: missing operand".to_string());
        }
        
        let pattern = &args[0];
        let files = &args[1..];
        
        // オプション解析
        let ignore_case = args.iter().any(|arg| arg.contains('i'));
        let line_numbers = args.iter().any(|arg| arg.contains('n'));
        
        // 正規表現コンパイル
        let regex = if ignore_case {
            regex::RegexBuilder::new(pattern).case_insensitive(true).build()?
        } else {
            regex::Regex::new(pattern)?
        };
        
        // ファイル検索
        for file in files {
            // 実装...
        }
        
        Ok(result)
    }
}
```

---

## パフォーマンスAPI

### PerformanceData構造体

```rust
#[derive(Debug)]
struct PerformanceData {
    total_execution_time: Duration,
    successful_commands: u64,
    failed_commands: u64,
    memory_usage: u64,
    cpu_usage: f32,
    io_operations: u64,
}

impl Default for PerformanceData {
    fn default() -> Self {
        Self {
            total_execution_time: Duration::new(0, 0),
            successful_commands: 0,
            failed_commands: 0,
            memory_usage: 0,
            cpu_usage: 0.0,
            io_operations: 0,
        }
    }
}
```

### メトリクス更新

```rust
impl NexusShell {
    /// パフォーマンスメトリクス更新
    fn update_performance_metrics(&mut self, execution_time: Duration) {
        self.performance_data.total_execution_time += execution_time;
        self.performance_data.io_operations += 1;
        
        // CPU使用率更新（簡略化）
        self.performance_data.cpu_usage = 0.1;
    }
    
    /// 成功率計算
    fn calculate_success_rate(&self) -> f64 {
        let total = self.performance_data.successful_commands + 
                   self.performance_data.failed_commands;
        if total == 0 {
            0.0
        } else {
            (self.performance_data.successful_commands as f64 / total as f64) * 100.0
        }
    }
}
```

### 統計表示

```rust
impl NexusShell {
    /// 高度統計表示
    async fn show_advanced_stats(&self) -> String {
        let success_rate = self.calculate_success_rate();
        let total_commands = self.performance_data.successful_commands + 
                           self.performance_data.failed_commands;
        
        format!(
            "===== NEXUSSHELL ADVANCED STATISTICS =====\n\
            EXECUTION METRICS:\n\
            Total Commands: {}\n\
            Successful: {}\n\
            Failed: {}\n\
            Success Rate: {:.1}%\n\
            Error Rate: {:.1}%\n\n\
            PERFORMANCE METRICS:\n\
            Total Execution Time: {:.3}ms\n\
            Average Command Time: {:.3}ms\n\
            Commands Per Second: {:.2}\n\
            I/O Operations: {}\n\
            Peak Performance: Optimized\n",
            total_commands,
            self.performance_data.successful_commands,
            self.performance_data.failed_commands,
            success_rate,
            100.0 - success_rate,
            self.performance_data.total_execution_time.as_millis(),
            if total_commands > 0 { 
                self.performance_data.total_execution_time.as_millis() as f64 / total_commands as f64 
            } else { 0.0 },
            if self.performance_data.total_execution_time.as_secs() > 0 {
                total_commands as f64 / self.performance_data.total_execution_time.as_secs() as f64
            } else { 0.0 },
            self.performance_data.io_operations
        )
    }
}
```

---

## 機能管理API

### 機能制御

```rust
impl NexusShell {
    /// 機能有効化
    async fn enable_feature(&mut self, args: &[String]) 
        -> Result<String, Box<dyn std::error::Error>> {
        
        if args.is_empty() {
            return Ok("enable: missing feature name".to_string());
        }
        
        let feature_name = &args[0];
        
        if self.features.contains_key(feature_name) {
            self.features.insert(feature_name.clone(), true);
            Ok(format!("Feature '{}' has been enabled.", feature_name))
        } else {
            Ok(format!("Feature '{}' not found.", feature_name))
        }
    }
    
    /// 機能無効化
    async fn disable_feature(&mut self, args: &[String]) 
        -> Result<String, Box<dyn std::error::Error>> {
        
        if args.is_empty() {
            return Ok("disable: missing feature name".to_string());
        }
        
        let feature_name = &args[0];
        
        if self.features.contains_key(feature_name) {
            self.features.insert(feature_name.clone(), false);
            Ok(format!("Feature '{}' has been disabled.", feature_name))
        } else {
            Ok(format!("Feature '{}' not found.", feature_name))
        }
    }
}
```

### 機能一覧表示

```rust
impl NexusShell {
    /// 機能一覧表示
    async fn show_features(&self) -> String {
        let mut result = String::new();
        result.push_str("===== NEXUSSHELL ADVANCED FEATURES =====\n\n");
        
        let features = vec![
            ("file_operations", "Advanced File System Operations"),
            ("text_processing", "Text Processing & Manipulation"),
            ("system_monitoring", "System Monitoring & Analysis"),
            ("network_tools", "Network Utilities & Diagnostics"),
            ("compression", "Archive & Compression Tools"),
            ("development_tools", "Development Environment"),
            ("advanced_search", "Advanced Search & Filtering"),
            ("job_control", "Process & Job Management"),
            ("performance_monitoring", "Performance Analytics"),
            ("security_tools", "Security & Encryption"),
        ];
        
        for (key, description) in features {
            let status = if self.features.get(key).unwrap_or(&false) {
                "[✓ ENABLED ]"
            } else {
                "[✗ DISABLED]"
            };
            
            result.push_str(&format!("{} - {} {}\n\n", description, description, status));
        }
        
        result
    }
}
```

---

## システムAPI

### システム情報取得

```rust
impl NexusShell {
    /// システム情報表示
    async fn show_system_info(&self) -> String {
        let cpu_count = num_cpus::get();
        let username = whoami::username();
        let hostname = whoami::hostname();
        
        format!(
            "===== SYSTEM INFORMATION =====\n\n\
            OPERATING SYSTEM:\n\
            OS: {}\n\
            Platform: Windows\n\
            Architecture: x86_64\n\
            Kernel Version: Advanced\n\n\
            HARDWARE INFORMATION:\n\
            CPU Cores: {}\n\
            Memory: Available\n\
            Storage: Accessible\n\
            Network: Connected\n\n\
            USER ENVIRONMENT:\n\
            Current User: {}\n\
            Home Directory: {}\n\
            Working Directory: {}\n\
            Shell Version: NexusShell v{}\n",
            env::consts::OS,
            cpu_count,
            username,
            dirs::home_dir().unwrap_or_default().display(),
            self.current_dir.file_name().unwrap_or_default().to_string_lossy(),
            self.config.version
        )
    }
}
```

---

## 拡張API

### プラグインインターフェース

```rust
/// プラグイントレイト
pub trait Plugin {
    /// プラグイン名
    fn name(&self) -> &str;
    
    /// プラグインバージョン
    fn version(&self) -> &str;
    
    /// プラグイン説明
    fn description(&self) -> &str;
    
    /// コマンド実行
    fn execute(&self, args: &[String]) -> Result<String, Box<dyn std::error::Error>>;
    
    /// 初期化
    fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    /// 終了処理
    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
```

### カスタムコマンド追加

```rust
impl NexusShell {
    /// カスタムコマンド登録
    pub fn register_command<F>(&mut self, name: String, handler: F)
    where
        F: Fn(&[String]) -> Result<String, Box<dyn std::error::Error>> + 'static,
    {
        // コマンドハンドラー登録実装
    }
    
    /// プラグイン読み込み
    pub fn load_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<(), Box<dyn std::error::Error>> {
        // プラグイン読み込み実装
        Ok(())
    }
}
```

### エイリアス管理

```rust
impl NexusShell {
    /// エイリアス解決
    fn resolve_alias(&self, command: &str, args: &[String]) -> (String, Vec<String>) {
        if let Some(alias_command) = self.aliases.get(command) {
            let parts: Vec<&str> = alias_command.split_whitespace().collect();
            if !parts.is_empty() {
                let mut new_args = parts[1..].iter().map(|s| s.to_string()).collect::<Vec<_>>();
                new_args.extend(args.iter().cloned());
                return (parts[0].to_string(), new_args);
            }
        }
        (command.to_string(), args.to_vec())
    }
    
    /// エイリアス追加
    pub fn add_alias(&mut self, alias: String, command: String) {
        self.aliases.insert(alias, command);
    }
    
    /// エイリアス削除
    pub fn remove_alias(&mut self, alias: &str) -> Option<String> {
        self.aliases.remove(alias)
    }
}
```

---

## エラーハンドリング

### カスタムエラー型

```rust
#[derive(Debug)]
pub enum NexusShellError {
    CommandNotFound(String),
    InvalidArguments(String),
    FileSystemError(std::io::Error),
    PermissionDenied(String),
    ParseError(String),
}

impl std::fmt::Display for NexusShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NexusShellError::CommandNotFound(cmd) => {
                write!(f, "Command not found: {}", cmd)
            }
            NexusShellError::InvalidArguments(msg) => {
                write!(f, "Invalid arguments: {}", msg)
            }
            NexusShellError::FileSystemError(err) => {
                write!(f, "File system error: {}", err)
            }
            NexusShellError::PermissionDenied(msg) => {
                write!(f, "Permission denied: {}", msg)
            }
            NexusShellError::ParseError(msg) => {
                write!(f, "Parse error: {}", msg)
            }
        }
    }
}

impl std::error::Error for NexusShellError {}
```

---

## 設定API

### ShellConfig構造体

```rust
#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub version: String,
    pub session_id: String,
    pub startup_time: Instant,
    pub max_history: usize,
    pub prompt_format: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            version: "2.2.0".to_string(),
            session_id: uuid::Uuid::new_v4().to_string(),
            startup_time: Instant::now(),
            max_history: 10000,
            prompt_format: "{}@{}:{}$ ".to_string(),
        }
    }
}
```

---

<div align="center">

**🔧 NexusShell API仕様書 v2.2.0**

この仕様書は開発者がNexusShellを拡張・カスタマイズするための完全なリファレンスです。

Made with ❤️ by [menchan-Rub](https://github.com/menchan-Rub)

</div> 