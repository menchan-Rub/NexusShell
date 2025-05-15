use std::collections::HashMap;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// ユーザー切り替えコマンド
pub struct SuCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl SuCommand {
    /// 新しいSuCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "su".to_string(),
            description: "ユーザー ID を切り替えます".to_string(),
            usage: "su [オプション] [ユーザー名]".to_string(),
        }
    }
    
    /// コマンドパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("su")
            .about("ユーザー ID を切り替えます")
            .arg(
                Arg::new("login")
                    .short('l')
                    .long("login")
                    .help("ログインシェルとして起動します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("command")
                    .short('c')
                    .long("command")
                    .help("指定したコマンドを実行します")
                    .action(ArgAction::Set)
            )
            .arg(
                Arg::new("shell")
                    .short('s')
                    .long("shell")
                    .help("指定したシェルを使用します")
                    .action(ArgAction::Set)
            )
            .arg(
                Arg::new("user")
                    .help("切り替えるユーザー名（デフォルトはroot）")
            )
    }
    
    /// 現在のユーザー名を取得
    fn get_current_user(&self) -> String {
        #[cfg(unix)]
        {
            use std::env;
            match env::var("USER") {
                Ok(user) => user,
                Err(_) => {
                    match env::var("LOGNAME") {
                        Ok(logname) => logname,
                        Err(_) => "unknown".to_string(),
                    }
                }
            }
        }
        
        #[cfg(windows)]
        {
            use std::env;
            match env::var("USERNAME") {
                Ok(username) => username,
                Err(_) => "unknown".to_string(),
            }
        }
    }
}

impl BuiltinCommand for SuCommand {
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
        
        // 現在のユーザーを取得
        let current_user = self.get_current_user();
        
        // 切り替え先ユーザーを取得（デフォルトはroot）
        let target_user = matches.get_one::<String>("user")
            .map(|s| s.as_str())
            .unwrap_or("root");
        
        // ログインモード
        let login_mode = matches.get_flag("login");
        
        // シェル
        let shell = matches.get_one::<String>("shell").cloned();
        
        // 実行コマンド
        let command = matches.get_one::<String>("command").cloned();
        
        // ユーザーが存在するか確認する（実際の実装ではシステムチェック）
        let is_user_exists = vec!["root", "admin", "guest", "user"].contains(&target_user);
        
        if !is_user_exists {
            return Err(anyhow!("ユーザー {} が存在しません", target_user));
        }
        
        // ユーザー切り替えには管理者権限が必要
        if current_user != "root" && current_user != "admin" {
            // 実際の実装ではパスワード認証を行う
            return Err(anyhow!("権限がありません。管理者権限が必要です。"));
        }
        
        // 環境変数を設定
        env.insert("SUDO_USER".to_string(), current_user);
        env.insert("USER".to_string(), target_user.to_string());
        env.insert("HOME".to_string(), format!("/home/{}", target_user));
        
        if login_mode {
            env.insert("LOGIN_SHELL".to_string(), "true".to_string());
        }
        
        if let Some(sh) = shell {
            env.insert("SHELL".to_string(), sh);
        }
        
        // 実行結果
        if let Some(cmd) = command {
            // 実際の実装ではコマンドを実行
            Ok(format!("ユーザー {} としてコマンド {} を実行しました", target_user, cmd))
        } else {
            Ok(format!("ユーザー {} に切り替えました", target_user))
        }
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -l, --login     ログインシェルとして起動します\n  -c, --command=COMMAND  指定したコマンドを実行します\n  -s, --shell=SHELL     指定したシェルを使用します\n\n引数:\n  user                切り替えるユーザー名（デフォルトはroot）\n\n例:\n  su                  rootユーザーに切り替え\n  su admin            adminユーザーに切り替え\n  su -c 'ls -la' admin  adminユーザーとしてコマンドを実行\n",
            self.description,
            self.usage
        )
    }
} 