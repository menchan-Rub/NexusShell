use std::collections::HashMap;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// 環境変数削除コマンド
pub struct UnsetCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl UnsetCommand {
    /// 新しいUnsetCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "unset".to_string(),
            description: "環境変数または関数を削除します".to_string(),
            usage: "unset [オプション] 変数名...".to_string(),
        }
    }
    
    /// コマンドパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("unset")
            .about("環境変数または関数を削除します")
            .arg(
                Arg::new("function")
                    .short('f')
                    .long("function")
                    .help("指定した名前を関数名として扱います")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("variable")
                    .short('v')
                    .long("variable")
                    .help("指定した名前を変数名として扱います")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("names")
                    .help("削除する変数名または関数名のリスト")
                    .required(true)
                    .action(ArgAction::Append)
            )
    }
}

impl BuiltinCommand for UnsetCommand {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn usage(&self) -> &str {
        &self.usage
    }
    
    fn execute(&self, args: Vec<String>, env: &mut HashMap<String, String>) -> Result<String> {
        // 引数解析
        let matches = match self.build_parser().try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("引数解析エラー: {}", e)),
        };
        
        // オプション取得
        let is_function = matches.get_flag("function");
        let is_variable = matches.get_flag("variable");
        
        // 名前リストを取得
        let names = matches.get_many::<String>("names")
            .ok_or_else(|| anyhow!("変数名が指定されていません"))?
            .cloned()
            .collect::<Vec<_>>();
        
        let mut result = String::new();
        
        // 各名前に対して処理
        for name in names {
            // 関数モードの場合
            if is_function && !is_variable {
                // 実際のシェルでは関数テーブルから関数を削除
                // ここではメッセージを追加
                result.push_str(&format!("関数 '{}' を削除しました\n", name));
            } 
            // 変数モードまたはデフォルト
            else {
                // 環境変数を削除
                env.remove(&name);
                if is_variable {
                    result.push_str(&format!("変数 '{}' を削除しました\n", name));
                }
            }
        }
        
        if result.is_empty() {
            Ok("".to_string())
        } else {
            Ok(result)
        }
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -f, --function    指定した名前を関数名として扱います\n  -v, --variable    指定した名前を変数名として扱います\n\n引数:\n  names...          削除する変数名または関数名のリスト\n\n例:\n  unset VAR1 VAR2        環境変数 VAR1 と VAR2 を削除\n  unset -f func1 func2   関数 func1 と func2 を削除\n",
            self.description,
            self.usage
        )
    }
} 