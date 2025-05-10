use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use tracing::{debug, error};

/// 現在の作業ディレクトリを表示するコマンド
///
/// UNIXの標準的なpwdコマンドの実装です。現在の作業ディレクトリのパスを表示します。
/// `-P` オプションを指定すると、シンボリックリンクを解決した物理パスを表示します。
/// デフォルトでは論理パス（環境変数PWDの値）を表示します。
///
/// # 使用例
///
/// ```bash
/// pwd      # 現在の作業ディレクトリを表示
/// pwd -P   # シンボリックリンクを解決した物理パスを表示
/// pwd -L   # 論理パス（環境変数PWDの値）を表示（デフォルト）
/// ```
pub struct PwdCommand;

#[async_trait]
impl BuiltinCommand for PwdCommand {
    fn name(&self) -> &'static str {
        "pwd"
    }

    fn description(&self) -> &'static str {
        "現在の作業ディレクトリを表示します"
    }

    fn usage(&self) -> &'static str {
        "pwd [-LP]\n\n-L オプションで論理パス（デフォルト）、-P オプションで物理パスを表示します。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // 引数を取得（最初の引数はコマンド名なので、それ以降を使用）
        let args = context.args.iter().skip(1).collect::<Vec<_>>();
        
        // オプションを解析
        let mut physical_path = false;
        for arg in &args {
            match arg.as_str() {
                "-P" => physical_path = true,
                "-L" => physical_path = false,
                _ if arg.starts_with('-') => {
                    let error_message = format!("pwd: 不明なオプション: {}", arg);
                    error!("{}", error_message);
                    return Ok(CommandResult::failure(1)
                        .with_stderr(error_message.into_bytes()));
                }
                _ => {
                    let error_message = "pwd: 引数が多すぎます".to_string();
                    error!("{}", error_message);
                    return Ok(CommandResult::failure(1)
                        .with_stderr(error_message.into_bytes()));
                }
            }
        }
        
        // 現在のディレクトリを取得
        let current_dir = if physical_path {
            // 物理パスを取得（シンボリックリンクを解決）
            get_physical_path(&context.current_dir)
        } else {
            // 論理パス（環境変数PWDの値）またはカレントディレクトリを取得
            context.current_dir.clone()
        };
        
        // パスを文字列に変換
        let path_string = match current_dir.to_str() {
            Some(s) => s.to_string(),
            None => {
                let error_message = "パスにUTF-8でない文字が含まれています".to_string();
                error!("{}", error_message);
                return Ok(CommandResult::failure(1)
                    .with_stderr(error_message.into_bytes()));
            }
        };
        
        debug!("現在のディレクトリ: {}", path_string);
        
        // 結果を返す（改行付き）
        let mut output = path_string.into_bytes();
        output.push(b'\n');
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// 物理パスを取得（シンボリックリンクを解決）
fn get_physical_path(path: &PathBuf) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(canonical_path) => canonical_path,
        Err(_) => path.clone(),
    }
} 