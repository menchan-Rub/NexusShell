/**
 * パイプラインコマンドモジュール
 * 
 * パイプライン内でのコマンド実行を管理します。
 */

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::{Duration, Instant};
use std::sync::Arc;

use tokio::process::{Command as TokioCommand, Child};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::RwLock;
use anyhow::{Result, anyhow, Context};
use tracing::{debug, error, info, warn};

use super::error::PipelineError;
use super::PipelineData;

/// コマンド引数
#[derive(Debug, Clone)]
pub struct CommandArgs {
    /// 引数値のリスト
    pub values: Vec<String>,
    /// フラグとオプションのマップ
    pub flags: HashMap<String, Option<String>>,
    /// 環境変数のマップ
    pub env: HashMap<String, String>,
}

impl CommandArgs {
    /// 新しい空のコマンド引数を作成
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            flags: HashMap::new(),
            env: HashMap::new(),
        }
    }
    
    /// 位置引数を追加
    pub fn add_arg(&mut self, value: impl Into<String>) -> &mut Self {
        self.values.push(value.into());
        self
    }
    
    /// フラグを追加（値なし）
    pub fn add_flag(&mut self, name: impl Into<String>) -> &mut Self {
        self.flags.insert(name.into(), None);
        self
    }
    
    /// オプションを追加（値あり）
    pub fn add_option(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.flags.insert(name.into(), Some(value.into()));
        self
    }
    
    /// 環境変数を追加
    pub fn add_env(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.env.insert(name.into(), value.into());
        self
    }
    
    /// 引数をコマンドライン配列に変換
    pub fn to_args_array(&self) -> Vec<String> {
        let mut args = self.values.clone();
        
        for (name, value) in &self.flags {
            if name.starts_with("--") {
                match value {
                    Some(val) => args.push(format!("{}={}", name, val)),
                    None => args.push(name.clone()),
                }
            } else if name.starts_with('-') {
                args.push(name.clone());
                if let Some(val) = value {
                    args.push(val.clone());
                }
            } else {
                match value {
                    Some(val) => {
                        args.push(format!("-{}", name));
                        args.push(val.clone());
                    },
                    None => args.push(format!("-{}", name)),
                }
            }
        }
        
        args
    }
}

impl Default for CommandArgs {
    fn default() -> Self {
        Self::new()
    }
}

/// コマンド実行結果
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// 成功したかどうか
    pub success: bool,
    /// 終了コード
    pub exit_code: Option<i32>,
    /// 標準出力
    pub stdout: Option<Vec<u8>>,
    /// 標準エラー出力
    pub stderr: Option<Vec<u8>>,
    /// 実行時間
    pub execution_time: Duration,
}

impl CommandResult {
    /// 成功結果を作成
    pub fn success(stdout: Vec<u8>, execution_time: Duration) -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: None,
            execution_time,
        }
    }
    
    /// 失敗結果を作成
    pub fn failure(exit_code: i32, stderr: Vec<u8>, execution_time: Duration) -> Self {
        Self {
            success: false,
            exit_code: Some(exit_code),
            stdout: None,
            stderr: Some(stderr),
            execution_time,
        }
    }
    
    /// エラー結果を作成
    pub fn error(error_message: impl Into<String>, execution_time: Duration) -> Self {
        Self {
            success: false,
            exit_code: Some(1),
            stdout: None,
            stderr: Some(error_message.into().into_bytes()),
            execution_time,
        }
    }
    
    /// 標準出力を文字列として取得
    pub fn stdout_string(&self) -> Option<String> {
        self.stdout.as_ref().and_then(|output| String::from_utf8(output.clone()).ok())
    }
    
    /// 標準エラー出力を文字列として取得
    pub fn stderr_string(&self) -> Option<String> {
        self.stderr.as_ref().and_then(|output| String::from_utf8(output.clone()).ok())
    }
    
    /// エラーメッセージを取得（標準エラー出力または終了コード）
    pub fn error_message(&self) -> String {
        if let Some(stderr) = self.stderr_string() {
            stderr
        } else if let Some(code) = self.exit_code {
            format!("コマンドが終了コード {} で終了しました", code)
        } else {
            "不明なエラー".to_string()
        }
    }
}

/// パイプラインコマンド
#[derive(Debug)]
pub struct Command {
    /// コマンド名
    name: String,
    /// コマンドパス
    path: Option<PathBuf>,
    /// コマンド引数
    args: CommandArgs,
    /// 作業ディレクトリ
    working_dir: Option<PathBuf>,
    /// タイムアウト（秒）
    timeout: Option<u64>,
    /// プロセスの子プロセス
    child: RwLock<Option<Child>>,
}

impl Command {
    /// 新しいコマンドを作成
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
            args: CommandArgs::new(),
            working_dir: None,
            timeout: None,
            child: RwLock::new(None),
        }
    }
    
    /// コマンドパスを設定
    pub fn with_path(mut self, path: impl AsRef<Path>) -> Self {
        self.path = Some(path.as_ref().to_path_buf());
        self
    }
    
    /// コマンド引数を設定
    pub fn with_args(mut self, args: CommandArgs) -> Self {
        self.args = args;
        self
    }
    
    /// 作業ディレクトリを設定
    pub fn with_working_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.working_dir = Some(dir.as_ref().to_path_buf());
        self
    }
    
    /// タイムアウトを設定
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout = Some(seconds);
        self
    }
    
    /// コマンド名を取得
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// コマンドパスを取得
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
    
    /// コマンド引数を取得
    pub fn args(&self) -> &CommandArgs {
        &self.args
    }
    
    /// コマンドを実行
    pub async fn execute(&self) -> Result<CommandResult, PipelineError> {
        debug!("コマンド実行開始: {}", self.name);
        
        let start_time = Instant::now();
        
        // コマンドプロセスを構築
        let mut command = self.build_command();
        
        // コマンドを実行
        let mut child = command.spawn().map_err(|e| {
            PipelineError::ExecutionFailed(format!("コマンド '{}' の起動に失敗: {}", self.name, e))
        })?;
        
        // 子プロセスを保存
        {
            let mut child_lock = self.child.write().await;
            *child_lock = Some(child);
        }
        
        // 出力を取得
        let output = match self.timeout {
            Some(secs) => {
                // タイムアウト付きで待機
                match tokio::time::timeout(Duration::from_secs(secs), async {
                    let mut child_lock = self.child.write().await;
                    if let Some(child) = child_lock.as_mut() {
                        child.wait_with_output().await
                    } else {
                        Err(std::io::Error::new(std::io::ErrorKind::Other, "子プロセスがありません"))
                    }
                }).await {
                    Ok(result) => result,
                    Err(_) => {
                        // タイムアウト - プロセスを強制終了
                        self.kill().await?;
                        
                        let execution_time = start_time.elapsed();
                        return Ok(CommandResult::error(
                            format!("コマンド '{}' がタイムアウトしました ({} 秒)", self.name, secs),
                            execution_time
                        ));
                    }
                }
            },
            None => {
                // タイムアウトなしで待機
                let mut child_lock = self.child.write().await;
                if let Some(child) = child_lock.as_mut() {
                    child.wait_with_output().await
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::Other, "子プロセスがありません"))
                }
            }
        }.map_err(|e| {
            PipelineError::ExecutionFailed(format!("コマンド '{}' の実行中にエラー: {}", self.name, e))
        })?;
        
        // 実行時間を計算
        let execution_time = start_time.elapsed();
        
        // 子プロセスをクリア
        {
            let mut child_lock = self.child.write().await;
            *child_lock = None;
        }
        
        // 出力を解析して結果を返す
        let result = if output.status.success() {
            CommandResult::success(output.stdout, execution_time)
        } else {
            let exit_code = output.status.code().unwrap_or(1);
            CommandResult::failure(exit_code, output.stderr, execution_time)
        };
        
        debug!("コマンド実行完了: {}, 成功: {}, 時間: {:?}",
               self.name, result.success, execution_time);
        
        Ok(result)
    }
    
    /// コマンドを強制終了
    pub async fn kill(&self) -> Result<(), PipelineError> {
        let mut child_lock = self.child.write().await;
        
        if let Some(child) = child_lock.as_mut() {
            debug!("コマンド {} を強制終了します", self.name);
            
            // プロセスを強制終了
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(id) = child.id() {
                    // SIGTERMを送信
                    unsafe {
                        // libcの代わりにsignal-hookを使用
                        // 2は SIGTERM に相当
                        let _ = nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(id as i32),
                            nix::sys::signal::Signal::SIGTERM
                        );
                    }
                }
            }
            
            #[cfg(windows)]
            {
                child.kill().map_err(|e| {
                    PipelineError::ExecutionFailed(
                        format!("コマンド '{}' の強制終了に失敗: {}", self.name, e)
                    )
                })?;
            }
            
            // 子プロセス参照をクリア
            *child_lock = None;
        }
        
        Ok(())
    }
    
    /// Tokioのコマンドプロセスを構築
    fn build_command(&self) -> TokioCommand {
        let mut command = if let Some(path) = &self.path {
            TokioCommand::new(path)
        } else {
            TokioCommand::new(&self.name)
        };
        
        // 引数を追加
        for arg in self.args.to_args_array() {
            command.arg(arg);
        }
        
        // 環境変数を設定
        for (key, value) in &self.args.env {
            command.env(key, value);
        }
        
        // 作業ディレクトリを設定
        if let Some(dir) = &self.working_dir {
            command.current_dir(dir);
        }
        
        command
    }
    
    /// パイプラインデータからコマンドを実行
    pub async fn execute_with_data(&self, input: PipelineData) -> Result<PipelineData, PipelineError> {
        // 入力データを変換
        let input_bytes = match input {
            PipelineData::Bytes(bytes) => Some(bytes),
            PipelineData::Text(text) => Some(text.into_bytes()),
            PipelineData::Json(json) => Some(serde_json::to_vec(&json).map_err(|e| {
                PipelineError::DataError(format!("JSONの変換に失敗: {}", e))
            })?),
            PipelineData::Empty => None,
            _ => return Err(PipelineError::DataError(
                format!("サポートされていない入力データ型: {:?}", input)
            )),
        };
        
        // コマンドプロセスを構築
        let mut command = self.build_command();
        
        // 入力がある場合は標準入力として渡す
        if let Some(input_data) = input_bytes {
            command.stdin(std::process::Stdio::piped());
        }
        
        // 標準出力と標準エラー出力をキャプチャ
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        
        // コマンドを実行
        let mut child = command.spawn().map_err(|e| {
            PipelineError::ExecutionFailed(format!("コマンド '{}' の起動に失敗: {}", self.name, e))
        })?;
        
        // 入力データがある場合は標準入力に書き込み
        if let Some(input_data) = input_bytes {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(&input_data).await.map_err(|e| {
                    PipelineError::ExecutionFailed(
                        format!("コマンド '{}' への入力書き込みに失敗: {}", self.name, e)
                    )
                })?;
            }
        }
        
        // 出力を待機
        let output = child.wait_with_output().await.map_err(|e| {
            PipelineError::ExecutionFailed(format!("コマンド '{}' の実行中にエラー: {}", self.name, e))
        })?;
        
        // 出力を処理
        if output.status.success() {
            Ok(PipelineData::Bytes(output.stdout))
        } else {
            Err(PipelineError::ExecutionFailed(
                format!("コマンド '{}' が失敗しました (終了コード: {:?}): {}",
                        self.name,
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr))
            ))
        }
    }
}

impl Clone for Command {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            path: self.path.clone(),
            args: self.args.clone(),
            working_dir: self.working_dir.clone(),
            timeout: self.timeout,
            child: RwLock::new(None),
        }
    }
} 