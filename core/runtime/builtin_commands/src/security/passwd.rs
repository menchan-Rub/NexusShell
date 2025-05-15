use std::collections::HashMap;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// パスワード変更コマンド
pub struct PasswdCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl PasswdCommand {
    /// 新しいPasswdCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "passwd".to_string(),
            description: "ユーザーのパスワードを変更します".to_string(),
            usage: "passwd [オプション] [ユーザー名]".to_string(),
        }
    }
    
    /// コマンドパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("passwd")
            .about("ユーザーのパスワードを変更します")
            .arg(
                Arg::new("lock")
                    .short('l')
                    .long("lock")
                    .help("指定したユーザーのアカウントをロックします")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("unlock")
                    .short('u')
                    .long("unlock")
                    .help("指定したユーザーのアカウントをアンロックします")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("delete")
                    .short('d')
                    .long("delete")
                    .help("指定したユーザーのパスワードを削除します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("user")
                    .help("パスワードを変更するユーザー名")
            )
    }
    
    /// 現在のユーザー名を取得
    fn get_current_user(&self, env: &HashMap<String, String>) -> String {
        if let Some(user) = env.get("USER") {
            return user.clone();
        }
        
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

impl BuiltinCommand for PasswdCommand {
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
        let current_user = self.get_current_user(env);
        
        // 対象ユーザーを取得（指定がなければ現在のユーザー）
        let target_user = matches.get_one::<String>("user")
            .map(|s| s.clone())
            .unwrap_or_else(|| current_user.clone());
        
        // オプションを取得
        let lock = matches.get_flag("lock");
        let unlock = matches.get_flag("unlock");
        let delete = matches.get_flag("delete");
        
        // ユーザーが存在するか確認する（実際の実装ではシステムチェック）
        let is_user_exists = vec!["root", "admin", "guest", "user"].contains(&target_user.as_str());
        
        if !is_user_exists {
            return Err(anyhow!("ユーザー {} が存在しません", target_user));
        }
        
        // 他ユーザーのパスワードを変更するには管理者権限が必要
        if target_user != current_user && current_user != "root" && current_user != "admin" {
            return Err(anyhow!("権限がありません。他ユーザーのパスワードを変更するには管理者権限が必要です。"));
        }
        
        // アクションを実行
        if lock {
            // アカウントをロック
            env.insert(format!("USER_{}_LOCKED", target_user.to_uppercase()), "true".to_string());
            Ok(format!("ユーザー {} のアカウントをロックしました", target_user))
        } else if unlock {
            // アカウントをアンロック
            env.remove(&format!("USER_{}_LOCKED", target_user.to_uppercase()));
            Ok(format!("ユーザー {} のアカウントをアンロックしました", target_user))
        } else if delete {
            // パスワードを削除
            env.remove(&format!("USER_{}_PASSWORD", target_user.to_uppercase()));
            Ok(format!("ユーザー {} のパスワードを削除しました", target_user))
        } else {
            // パスワード変更（実際の実装ではインタラクティブに入力）
            
            // モック実装：パスワードが安全かチェック
            let mock_password = "new_secure_password123";
            
            if mock_password.len() < 8 {
                return Err(anyhow!("パスワードが短すぎます。8文字以上必要です。"));
            }
            
            if !mock_password.chars().any(|c| c.is_ascii_digit()) {
                return Err(anyhow!("パスワードには数字を含める必要があります。"));
            }
            
            if !mock_password.chars().any(|c| c.is_ascii_lowercase()) || 
               !mock_password.chars().any(|c| c.is_ascii_uppercase()) {
                return Err(anyhow!("パスワードには大文字と小文字の両方を含める必要があります。"));
            }
            
            // パスワードを設定
            env.insert(format!("USER_{}_PASSWORD", target_user.to_uppercase()), 
                       format!("HASH:{}", mock_password));
            
            Ok(format!("ユーザー {} のパスワードを変更しました", target_user))
        }
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -l, --lock      指定したユーザーのアカウントをロックします\n  -u, --unlock    指定したユーザーのアカウントをアンロックします\n  -d, --delete    指定したユーザーのパスワードを削除します\n\n引数:\n  user           パスワードを変更するユーザー名（指定しない場合は現在のユーザー）\n\n例:\n  passwd         現在のユーザーのパスワードを変更\n  passwd admin   adminユーザーのパスワードを変更\n  passwd -l guest  guestユーザーのアカウントをロック\n",
            self.description,
            self.usage
        )
    }
} 