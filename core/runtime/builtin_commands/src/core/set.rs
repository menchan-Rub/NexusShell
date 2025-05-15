use std::collections::HashMap;
use std::env;
use std::fmt;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// シェル変数の設定を行うコマンド
pub struct SetCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl SetCommand {
    /// 新しいSetCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "set".to_string(),
            description: "シェル変数の設定と表示を行います".to_string(),
            usage: "set [オプション] [変数[=値] ...]".to_string(),
        }
    }
    
    /// オプションパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("set")
            .about("シェル変数の設定と表示を行います")
            .arg(
                Arg::new("allexport")
                    .short('a')
                    .long("allexport")
                    .help("作成または変更されたすべての変数を自動的にエクスポートします")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("braceexpand")
                    .short('B')
                    .long("braceexpand")
                    .help("ブレース展開を有効にします")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("errexit")
                    .short('e')
                    .long("errexit")
                    .help("コマンドが0以外の終了ステータスを返した場合に終了します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("nounset")
                    .short('u')
                    .long("nounset")
                    .help("未定義の変数を参照した場合にエラーとします")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .help("入力行を表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("xtrace")
                    .short('x')
                    .long("xtrace")
                    .help("コマンドとその引数を実行前に表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("variables")
                    .action(ArgAction::Append)
                    .help("設定する変数とその値")
            )
    }
    
    /// 環境変数を設定
    fn set_variable(&self, var_spec: &str) -> Result<()> {
        // 変数名と値を分割
        if let Some(pos) = var_spec.find('=') {
            let (name, value) = var_spec.split_at(pos);
            let value = &value[1..]; // '='を取り除く
            
            // 環境変数を設定
            env::set_var(name, value);
            Ok(())
        } else {
            // '='がない場合は変数の設定なしとみなす
            Ok(())
        }
    }
    
    /// 現在の環境変数を表示
    fn display_variables(&self) -> Result<String> {
        let mut result = String::new();
        let vars: HashMap<_, _> = env::vars().collect();
        
        // 変数名をソートして表示
        let mut names: Vec<_> = vars.keys().collect();
        names.sort();
        
        for name in names {
            if let Some(value) = vars.get(name) {
                result.push_str(&format!("{}=\"{}\"\n", name, value));
            }
        }
        
        Ok(result)
    }
}

impl BuiltinCommand for SetCommand {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn usage(&self) -> &str {
        &self.usage
    }
    
    fn execute(&self, args: Vec<String>, _env: &mut HashMap<String, String>) -> Result<String> {
        // 引数解析
        let matches = match self.build_parser().try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("引数解析エラー: {}", e)),
        };
        
        // オプション処理
        if matches.get_flag("allexport") {
            println!("注意: allexport オプションはこの実装では動作が異なります");
        }
        
        if matches.get_flag("errexit") {
            println!("注意: errexit オプションはこの実装では動作が異なります");
        }
        
        // 引数処理
        if let Some(vars) = matches.get_many::<String>("variables") {
            for var_spec in vars {
                self.set_variable(var_spec)?;
            }
            Ok("".to_string())
        } else {
            // 引数なしの場合は現在の環境変数を表示
            self.display_variables()
        }
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -a, --allexport    作成または変更されたすべての変数をエクスポート\n  -e, --errexit     コマンドが非ゼロ終了ステータスで終了した場合にシェルを終了\n  -u, --nounset     未設定の変数を参照した場合にエラーとする\n  -v, --verbose     入力行を表示\n  -x, --xtrace      コマンドと引数を表示\n",
            self.description,
            self.usage
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_set_variable() {
        let cmd = SetCommand::new();
        
        // テスト用の変数を設定
        cmd.set_variable("TEST_VAR=test_value").unwrap();
        
        // 環境変数が設定されたことを確認
        assert_eq!(env::var("TEST_VAR").unwrap(), "test_value");
    }
    
    #[test]
    fn test_display_variables() {
        let cmd = SetCommand::new();
        
        // テスト用の変数を設定
        env::set_var("TEST_DISPLAY_VAR", "display_value");
        
        // 環境変数の表示結果に設定した変数が含まれることを確認
        let result = cmd.display_variables().unwrap();
        assert!(result.contains("TEST_DISPLAY_VAR=\"display_value\""));
    }
} 