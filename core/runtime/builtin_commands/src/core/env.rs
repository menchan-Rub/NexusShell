/*!
# envコマンド

環境変数を表示または変更します。

## 使用法

```
env [オプション] [名前=値 ...] [コマンド [引数 ...]]
```

## 説明

このコマンドは、現在の環境変数を表示したり、一時的に環境変数を変更してコマンドを実行したりします。
引数なしで実行すると、現在の環境変数の一覧を表示します。

## オプション

- `-i, --ignore-environment`: 既存の環境をクリアしてから実行
- `-u, --unset=名前`: 指定した環境変数を削除
- `-0, --null`: 各出力行を NULL で終了
- `-v, --verbose`: 詳細な情報を表示
- `-h, --help`: ヘルプを表示
*/

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use clap::{App, Arg, ArgMatches};
use tracing::{debug, info, warn, error};

use crate::{BuiltinCommand, CommandContext, CommandResult};

/// env組み込みコマンド
pub struct EnvCommand;

#[async_trait]
impl BuiltinCommand for EnvCommand {
    fn name(&self) -> &'static str {
        "env"
    }

    fn description(&self) -> &'static str {
        "環境変数を表示または変更"
    }

    fn usage(&self) -> &'static str {
        "env [オプション] [名前=値 ...] [コマンド [引数 ...]]"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let app = App::new("env")
            .about("環境変数を表示または変更")
            .arg(Arg::with_name("ignore-environment")
                .short('i')
                .long("ignore-environment")
                .help("既存の環境をクリアしてから実行"))
            .arg(Arg::with_name("unset")
                .short('u')
                .long("unset")
                .takes_value(true)
                .multiple(true)
                .help("指定した環境変数を削除"))
            .arg(Arg::with_name("null")
                .short('0')
                .long("null")
                .help("各出力行を NULL で終了"))
            .arg(Arg::with_name("verbose")
                .short('v')
                .long("verbose")
                .help("詳細な情報を表示"))
            .arg(Arg::with_name("NAME_VALUE")
                .multiple(true)
                .help("NAME=VALUE 形式の環境変数設定"));

        // 引数をパース
        let args_vec: Vec<&str> = context.args.iter().map(|s| s.as_str()).collect();
        let matches = match app.try_get_matches_from(args_vec) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: {}\n", e).into_bytes()));
            }
        };

        // オプションの解析
        let ignore_env = matches.is_present("ignore-environment");
        let null_terminate = matches.is_present("null");
        let verbose = matches.is_present("verbose");
        let unset_vars: Vec<&str> = match matches.values_of("unset") {
            Some(values) => values.collect(),
            None => vec![],
        };

        // 環境変数の準備
        let mut env_vars = if ignore_env {
            HashMap::new()
        } else {
            context.env_vars.clone()
        };

        // 削除する変数を処理
        for var in unset_vars {
            env_vars.remove(var);
            if verbose {
                debug!("環境変数を削除: {}", var);
            }
        }

        // 追加する環境変数の処理
        let mut remaining_args = Vec::new();
        if let Some(name_values) = matches.values_of("NAME_VALUE") {
            for arg in name_values {
                if let Some(pos) = arg.find('=') {
                    let (name, value) = arg.split_at(pos);
                    let value = &value[1..]; // '='をスキップ
                    
                    env_vars.insert(name.to_string(), value.to_string());
                    if verbose {
                        debug!("環境変数を設定: {}={}", name, value);
                    }
                } else {
                    // '='がない場合はコマンドとして扱う
                    remaining_args.push(arg.to_string());
                }
            }
        }

        // コマンド実行または環境変数表示
        if !remaining_args.is_empty() {
            // コマンドを実行
            let command_name = &remaining_args[0];
            let args = &remaining_args[1..];
            
            if verbose {
                debug!("コマンドを実行: {} (引数: {:?})", command_name, args);
            }
            
            // 外部コマンドを実行
            let status = Command::new(command_name)
                .args(args)
                .envs(&env_vars)
                .current_dir(&context.current_dir)
                .stdin(if context.stdin_connected { Stdio::inherit() } else { Stdio::null() })
                .stdout(if context.stdout_connected { Stdio::inherit() } else { Stdio::null() })
                .stderr(if context.stderr_connected { Stdio::inherit() } else { Stdio::null() })
                .status()
                .with_context(|| format!("コマンド実行に失敗: {}", command_name))?;
            
            let exit_code = status.code().unwrap_or(1);
            return Ok(CommandResult {
                exit_code,
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        } else {
            // 環境変数を表示
            let separator = if null_terminate { '\0' } else { '\n' };
            let mut output = String::new();
            
            let mut vars: Vec<(&String, &String)> = env_vars.iter().collect();
            vars.sort_by(|a, b| a.0.cmp(b.0));
            
            for (name, value) in vars {
                output.push_str(&format!("{}={}{}", name, value, separator));
            }
            
            return Ok(CommandResult::success()
                .with_stdout(output.into_bytes()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_env_display() {
        let mut env_vars = HashMap::new();
        env_vars.insert("TEST_VAR".to_string(), "test_value".to_string());
        env_vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        
        let context = CommandContext {
            current_dir: PathBuf::from("/tmp"),
            env_vars,
            args: vec!["env".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let command = EnvCommand;
        let result = command.execute(context).await.unwrap();
        
        assert_eq!(result.exit_code, 0);
        let output = String::from_utf8(result.stdout).unwrap();
        
        assert!(output.contains("TEST_VAR=test_value"));
        assert!(output.contains("PATH=/usr/bin:/bin"));
    }

    #[tokio::test]
    async fn test_env_with_null_terminator() {
        let mut env_vars = HashMap::new();
        env_vars.insert("TEST_VAR".to_string(), "test_value".to_string());
        
        let context = CommandContext {
            current_dir: PathBuf::from("/tmp"),
            env_vars,
            args: vec!["env".to_string(), "-0".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let command = EnvCommand;
        let result = command.execute(context).await.unwrap();
        
        assert_eq!(result.exit_code, 0);
        let output = result.stdout;
        
        // NULL終端を確認
        assert!(output.contains(&0));
    }
} 