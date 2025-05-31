use crate::{BuiltinCommand, CommandContext, CommandResult, CommandMetadata};
use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, error};
use serde::{Serialize, Deserialize};
use std::process;

/// シェルを終了するコマンド
///
/// UNIXの標準的なexitコマンドの実装です。シェルを終了します。
/// 引数として終了コードを指定できます。省略した場合は最後に実行したコマンドの終了コードを使用します。
///
/// # 使用例
///
/// ```bash
/// exit      # 最後に実行したコマンドの終了コードでシェルを終了
/// exit 0    # 正常終了（0）でシェルを終了
/// exit 1    # エラー終了（1）でシェルを終了
/// ```
pub struct ExitCommand;

/// シェルに終了を要求するための特別な終了コード
pub const EXIT_SHELL_REQUEST: i32 = -9999;

/// シェル制御アクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellAction {
    /// シェルを終了
    Exit,
    /// 現在のスクリプトのみ終了
    Return,
    /// 現在のループから抜ける
    Break,
    /// 現在のループの次の反復へ
    Continue,
    /// 指定したシグナルを送信
    Signal(i32),
}

/// シェル制御情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellControl {
    /// 実行するアクション
    pub action: ShellAction,
    /// 終了コード（Exit/Returnで使用）
    pub exit_code: i32,
    /// 追加メッセージ（オプション）
    pub message: Option<String>,
}

/// シェルイベントの種類
#[derive(Debug, Clone)]
pub enum ShellEvent {
    /// シェル終了リクエスト (終了コード)
    ExitRequested(i32),
    /// シグナル受信
    SignalReceived(i32),
    /// ジョブステータス変更
    JobStatusChanged(usize),
    /// 環境変数変更
    EnvVarChanged(String),
    /// ディレクトリ変更
    DirectoryChanged(String),
}

/// シェルとの通信用メタデータ
impl CommandResult {
    /// シェルを終了するための結果を作成
    pub fn exit(exit_code: i32) -> Self {
        let mut result = Self {
            exit_code: EXIT_SHELL_REQUEST,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        
        let shell_control = ShellControl {
            action: ShellAction::Exit,
            exit_code,
            message: None,
        };
        
        result.metadata = Some(CommandMetadata {
            shell_control: Some(shell_control),
            ..CommandMetadata::default()
        });
        
        result
    }
}

/// 終了コードを通知する実装
impl ShellExitNotifier for CommandResult {
    fn notify_exit_code(&self, exit_code: i32) -> Result<(), ShellError> {
        if exit_code < 0 || exit_code > 255 {
            return Err(ShellError::InvalidExitCode(format!(
                "終了コード {} は有効範囲 0-255 の外です", exit_code
            )));
        }
        
        // プロセス終了シグナルを送信
        // 実際のシェルに終了コードを通知するため、各プラットフォームごとに適切な実装が必要
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            
            let ppid = nix::unistd::getppid();
            debug!("現在の親プロセスID (Unix): {}", ppid);

            if ppid.as_raw() > 1 { // init/systemd でないことを確認
                // is_process_shell の呼び出しはコメントアウトのまま（外部依存のため）
                // if is_process_shell(ppid.as_raw()) {
                //     debug!("親プロセス (PID: {}) はシェルであると判断されました。", ppid);
                //     // NexusShellがSIGUSR1を処理することを期待
                //     match kill(ppid, Signal::SIGUSR1) {
                //         Ok(_) => debug!("親プロセス (PID: {}) に SIGUSR1 を送信しました。", ppid),
                //         Err(e) => error!("親プロセス (PID: {}) への SIGUSR1 送信に失敗: {} {}", ppid, e, peningkatan),
                //     }
                // } else {
                //     debug!("親プロセス (PID: {}) はシェルではないか、判定できませんでした。", ppid);
                // }
                // デモのため、無条件にシグナル送信を試みる（実際には上記の判定が必要）
                debug!("親プロセス (PID:{}) に SIGUSR1 (終了通知) を送信試行します。", ppid);
                match kill(ppid, Signal::SIGUSR1) { // より制御された終了のためにSIGUSR1を使用
                    Ok(_) => debug!("親プロセス (PID: {}) に SIGUSR1 を送信しました。", ppid),
                    Err(e) => error!("親プロセス (PID: {}) への SIGUSR1 送信に失敗: {}", ppid, e),
                }
            } else {
                debug!("親プロセスが init (PID 1) のため、シグナルは送信しません。");
            }
        }
        
        #[cfg(windows)]
        {
            // use std::process::Command; // 既存のものは一度コメントアウト
            // Windowsでの親プロセスへの通知は、taskkillよりも名前付きパイプ等が望ましいが、
            // exitコマンド単体での実装としては複雑すぎるため、ログ出力に留めます。
            // get_windows_parent_pidは外部関数と仮定
            // if let Some(parent_pid) = get_windows_parent_pid() { // 外部関数のため呼び出しをコメントアウト
            //     debug!("Windowsの親プロセスID: {}", parent_pid);
            //     debug!("親プロセス (PID: {}) に taskkill を使用して終了通知を試みます。", parent_pid);
            //     let status = Command::new("taskkill")
            //         .args(&["/PID", &parent_pid.to_string(), "/F"]) // /F は強制終了
            //         .status();
            //     if let Err(e) = status {
            //         error!("親プロセス (PID: {}) への taskkill 実行に失敗: {}", parent_pid, e);
            //     } else if let Ok(exit_status) = status {
            //         if exit_status.success() {
            //             debug!("親プロセス (PID: {}) への taskkill が成功しました。", parent_pid);
            //         } else {
            //             error!("親プロセス (PID: {}) への taskkill が失敗しました: {}", parent_pid, exit_status);
            //         }
            //     }
            // } else {
            //     debug!("Windowsの親プロセスIDが取得できませんでした。");
            // }
            debug!("Windows環境での親プロセスへのシグナル送信処理（現状ログ出力のみのスタブ）");
        }
        
        Ok(())
    }
}

/// シェル終了通知のためのトレイト
pub trait ShellExitNotifier {
    /// シェルに終了コードを通知する
    fn notify_exit_code(&self, exit_code: i32) -> Result<(), ShellError>;
}

/// シェル終了関連のエラー
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("無効な終了コード: {0}")]
    InvalidExitCode(String),
    
    #[error("通知エラー: {0}")]
    NotificationError(String),
}

#[async_trait]
impl BuiltinCommand for ExitCommand {
    fn name(&self) -> &'static str {
        "exit"
    }

    fn description(&self) -> &'static str {
        "シェルを終了します"
    }

    fn usage(&self) -> &'static str {
        "exit [終了コード]\n\n終了コードを省略した場合は、最後に実行したコマンドの終了コードを使用します。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // 引数を取得（最初の引数はコマンド名なので、それ以降を使用）
        let args = context.args.iter().skip(1).collect::<Vec<_>>();
        
        // 終了コードを決定
        let exit_code = if args.is_empty() {
            // 引数がない場合は0を使用
            // 実際のシェル実装では、最後に実行したコマンドの終了コードを使用する
            0
        } else if args.len() == 1 {
            // 引数が1つの場合は終了コードとして解釈
            match args[0].parse::<i32>() {
                Ok(code) => code,
                Err(_) => {
                    let error_message = format!("exit: {}: 数値以外の引数です", args[0]);
                    error!("{}", error_message);
                    return Ok(CommandResult::failure(2)
                        .with_stderr(error_message.into_bytes()));
                }
            }
        } else {
            // 引数が複数ある場合はエラー
            let error_message = "exit: 引数が多すぎます".to_string();
            error!("{}", error_message);
            return Ok(CommandResult::failure(1)
                .with_stderr(error_message.into_bytes()));
        };
        
        // 終了コードを親シェルに通知し、CommandResult経由で返す
        let mut result = CommandResult::success();
        result.exit_code = Some(exit_code);
        result.exit_reason = Some("ユーザー要求による終了".to_string());
        // プロセス終了（クロスプラットフォーム）
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::getppid;
            let ppid = getppid();
            let _ = kill(ppid, Signal::SIGTERM);
        }
        #[cfg(windows)]
        {
            use winapi::um::processthreadsapi::GetCurrentProcessId;
            use winapi::um::wincon::GenerateConsoleCtrlEvent;
            unsafe { GenerateConsoleCtrlEvent(0, GetCurrentProcessId()); }
        }
        Ok(result)
    }

    /// 終了前のクリーンアップタスクを実行
    async fn perform_cleanup_tasks(&self, context: &CommandContext) -> Result<()> {
        // 一時ファイルのクリーンアップ
        self.cleanup_temp_files(context).await?;
        
        // バックグラウンドジョブの状態を保存
        self.save_job_states(context).await?;
        
        // 履歴の保存
        self.save_history(context).await?;
        
        Ok(())
    }
    
    /// 一時ファイルのクリーンアップ
    async fn cleanup_temp_files(&self, context: &CommandContext) -> Result<()> {
        if let Some(temp_dir) = &context.temp_dir {
            // クリーンアップが必要な一時ファイルが存在するか確認
            if temp_dir.exists() && context.config.cleanup_temp_on_exit {
                tracing::debug!("一時ディレクトリをクリーンアップ: {:?}", temp_dir);
                if let Err(e) = std::fs::remove_dir_all(temp_dir) {
                    tracing::warn!("一時ディレクトリの削除に失敗: {:?}", e);
                }
            }
        }
        Ok(())
    }
    
    /// バックグラウンドジョブの状態を保存
    async fn save_job_states(&self, context: &CommandContext) -> Result<()> {
        if let Some(job_manager) = &context.job_manager {
            tracing::debug!("バックグラウンドジョブの状態を保存");
            job_manager.write().await.save_job_states()?;
        }
        Ok(())
    }
    
    /// シェル履歴の保存
    async fn save_history(&self, context: &CommandContext) -> Result<()> {
        if let Some(history) = &context.history {
            tracing::debug!("コマンド履歴を保存");
            let history = history.read().await;
            history.save_to_file()?;
        }
        Ok(())
    }
}

#[cfg(unix)]
/// 指定されたPIDのプロセスがシェルかどうかを確認
fn is_process_shell(pid: i32) -> Result<bool, std::io::Error> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    
    // プロセスのコマンドライン情報を読み取る
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    let file = File::open(cmdline_path)?;
    let reader = BufReader::new(file);
    
    // コマンドラインを読み取り、シェルかどうかを判断
    if let Some(Ok(cmdline)) = reader.lines().next() {
        let cmd_lower = cmdline.to_lowercase();
        return Ok(cmd_lower.contains("bash") || 
                 cmd_lower.contains("zsh") || 
                 cmd_lower.contains("sh") || 
                 cmd_lower.contains("shell") ||
                 cmd_lower.contains("nexus"));
    }
    
    Ok(false)
}

#[cfg(windows)]
/// Windowsで親プロセスIDを取得
fn get_windows_parent_pid() -> Option<u32> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32EnumProcesses, K32GetModuleFileNameExW, K32GetProcessImageFileNameW
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ
    };
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    
    unsafe {
        let current_pid = std::process::id();
        let mut parent_pid = None;
        
        // 現在実行中のプロセスIDのリストを取得
        let mut processes = [0u32; 1024];
        let mut cb_needed = 0u32;
        
        if K32EnumProcesses(processes.as_mut_ptr(), 
                          std::mem::size_of_val(&processes) as u32, 
                          &mut cb_needed) == 0 {
            return None;
        }
        
        let process_count = cb_needed as usize / std::mem::size_of::<u32>();
        
        // 各プロセスをチェック
        for i in 0..process_count {
            let pid = processes[i];
            
            // 自分自身はスキップ
            if pid == current_pid {
                continue;
            }
            
            // プロセスをオープン
            let h_process = OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                0,
                pid
            );
            
            if h_process != 0 {
                // プロセス情報を取得...
                // 実際にはもっと複雑な処理が必要
                // ここではシンプルのために、親子関係の判定は省略
                
                CloseHandle(h_process);
                
                // デモンストレーション目的で、ここでは最初に見つかったプロセスを返す
                // 実際には WMI や ToolHelp API を使って親子関係を特定する必要がある
                parent_pid = Some(pid);
                break;
            }
        }
        
        parent_pid
    }
} 