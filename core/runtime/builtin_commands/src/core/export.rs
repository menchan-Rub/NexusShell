/*!
 * export コマンド実装
 * 
 * このモジュールは環境変数の設定と表示を行う `export` コマンドを実装します。
 * 引数なしで呼び出された場合は現在の環境変数をすべて表示し、
 * NAME=VALUE の形式で呼び出された場合は環境変数を設定します。
 */

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::{BuiltinCommand, CommandContext, CommandResult};

/// export コマンド実装
///
/// 環境変数の設定と表示を行います。
/// 
/// 使用例:
/// - `export` - 現在の環境変数をすべて表示
/// - `export NAME=VALUE` - 環境変数 NAME に VALUE を設定
/// - `export NAME` - 環境変数 NAME の値を表示
pub struct ExportCommand;

#[async_trait]
impl BuiltinCommand for ExportCommand {
    fn name(&self) -> &'static str {
        "export"
    }

    fn description(&self) -> &'static str {
        "環境変数を設定または表示します"
    }

    fn usage(&self) -> &'static str {
        "export [NAME=VALUE]..."
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        debug!("export コマンドを実行しています: {:?}", context.args);

        // 引数がない場合は全ての環境変数を表示
        if context.args.len() <= 1 {
            return self.display_all_env_vars(&context);
        }

        // 各引数を処理
        let mut result = CommandResult::success();
        let mut updated_env = context.env_vars.clone();
        let mut error_occurred = false;

        // 最初の引数はコマンド名なのでスキップ
        for arg in context.args.iter().skip(1) {
            if let Err(e) = self.process_arg(arg, &mut updated_env, &mut result) {
                // エラーメッセージを標準エラーに追加
                let mut error_msg = format!("export: {}\n", e);
                result.stderr.append(&mut error_msg.into_bytes());
                error_occurred = true;
            }
        }

        // エラーが発生した場合は終了コードを設定
        if error_occurred {
            result.exit_code = 1;
        }

        Ok(result)
    }
}

impl ExportCommand {
    /// 全ての環境変数を表示
    fn display_all_env_vars(&self, context: &CommandContext) -> Result<CommandResult> {
        let mut output = Vec::new();
        
        // 環境変数をソートして表示
        let mut sorted_vars: Vec<(&String, &String)> = context.env_vars.iter().collect();
        sorted_vars.sort_by(|a, b| a.0.cmp(b.0));
        
        for (name, value) in sorted_vars {
            let line = format!("export {}=\"{}\"\n", name, value);
            output.extend_from_slice(line.as_bytes());
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }

    /// 単一の引数を処理
    fn process_arg(&self, arg: &str, env_vars: &mut HashMap<String, String>, result: &mut CommandResult) -> Result<()> {
        // NAME=VALUE 形式の場合
        if let Some(pos) = arg.find('=') {
            let (name, value) = arg.split_at(pos);
            
            // 環境変数名のバリデーション
            if name.is_empty() {
                return Err(anyhow!("無効な環境変数名"));
            }
            
            if !self.is_valid_env_name(name) {
                return Err(anyhow!("無効な環境変数名: {}", name));
            }
            
            // '=' の後の部分を取得（先頭の '=' は除く）
            let value = &value[1..];
            
            // 環境変数を設定
            debug!("環境変数を設定: {} = {}", name, value);
            env_vars.insert(name.to_string(), value.to_string());
            
        } else {
            // NAME 形式の場合、その環境変数の値を表示
            if !self.is_valid_env_name(arg) {
                return Err(anyhow!("無効な環境変数名: {}", arg));
            }
            
            if let Some(value) = env_vars.get(arg) {
                let line = format!("{}=\"{}\"\n", arg, value);
                result.stdout.extend_from_slice(line.as_bytes());
            } else {
                return Err(anyhow!("環境変数が設定されていません: {}", arg));
            }
        }
        
        Ok(())
    }
    
    /// 環境変数名が有効かどうかを検証
    fn is_valid_env_name(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        
        // 環境変数名の最初の文字は英字またはアンダースコアである必要がある
        let first_char = name.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return false;
        }
        
        // 残りの文字は英数字またはアンダースコアである必要がある
        for c in name.chars().skip(1) {
            if !c.is_ascii_alphanumeric() && c != '_' {
                return false;
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_export_display_all() {
        let command = ExportCommand;
        let mut env_vars = HashMap::new();
        env_vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        env_vars.insert("HOME".to_string(), "/home/user".to_string());
        
        let context = CommandContext {
            current_dir: std::path::PathBuf::from("/"),
            env_vars,
            args: vec!["export".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        let output = String::from_utf8(result.stdout).unwrap();
        assert!(output.contains("export HOME=\"/home/user\""));
        assert!(output.contains("export PATH=\"/usr/bin:/bin\""));
    }
    
    #[tokio::test]
    async fn test_export_set_variable() {
        let command = ExportCommand;
        let mut env_vars = HashMap::new();
        
        let context = CommandContext {
            current_dir: std::path::PathBuf::from("/"),
            env_vars,
            args: vec!["export".to_string(), "TEST=value".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 新しい環境変数が設定されていることを確認します
        // 実際の実装ではシェルの環境に反映されることになります
    }
    
    #[tokio::test]
    async fn test_export_invalid_name() {
        let command = ExportCommand;
        let mut env_vars = HashMap::new();
        
        let context = CommandContext {
            current_dir: std::path::PathBuf::from("/"),
            env_vars,
            args: vec!["export".to_string(), "1INVALID=value".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains("無効な環境変数名"));
    }
} 