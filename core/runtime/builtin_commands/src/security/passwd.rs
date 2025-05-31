use std::collections::HashMap;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};
use rpassword::read_password;
use std::io::{self, Write};

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
        
        // ユーザーが存在するか確認する
        let is_user_exists = self.check_user_exists(&target_user)?;
        
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
            self.lock_account(&target_user)?;
            Ok(format!("ユーザー {} のアカウントをロックしました", target_user))
        } else if unlock {
            // アカウントをアンロック
            self.unlock_account(&target_user)?;
            Ok(format!("ユーザー {} のアカウントをアンロックしました", target_user))
        } else if delete {
            // パスワードを削除
            self.delete_password(&target_user)?;
            Ok(format!("ユーザー {} のパスワードを削除しました", target_user))
        } else {
            // パスワード変更（インタラクティブ入力）
            self.change_password_interactive(&current_user, &target_user, env)
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

/// アカウントをロックする
fn lock_account(&self, username: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // Linux/Unixでは passwd -l コマンドを使用
        let output = Command::new("passwd")
            .args(["-l", username])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("アカウントロックに失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        
        // Windowsではnet user /active:no コマンドを使用
        let output = Command::new("net")
            .args(["user", username, "/active:no"])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("アカウントロックに失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // その他のプラットフォームではシミュレーション
        debug!("アカウントロック操作をシミュレーション: {}", username);
        Ok(())
    }
}

/// アカウントをアンロックする
fn unlock_account(&self, username: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // Linux/Unixでは passwd -u コマンドを使用
        let output = Command::new("passwd")
            .args(["-u", username])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("アカウントアンロックに失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        
        // Windowsではnet user /active:yes コマンドを使用
        let output = Command::new("net")
            .args(["user", username, "/active:yes"])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("アカウントアンロックに失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // その他のプラットフォームではシミュレーション
        debug!("アカウントアンロック操作をシミュレーション: {}", username);
        Ok(())
    }
}

/// パスワードを削除する
fn delete_password(&self, username: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // Linux/Unixでは passwd -d コマンドを使用
        let output = Command::new("passwd")
            .args(["-d", username])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("パスワード削除に失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        
        // Windowsでは空パスワードを設定
        let output = Command::new("net")
            .args(["user", username, "\"\""])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("パスワード削除に失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // その他のプラットフォームではシミュレーション
        debug!("パスワード削除操作をシミュレーション: {}", username);
        Ok(())
    }
}

/// インタラクティブにパスワードを変更する
fn change_password_interactive(&self, current_user: &str, target_user: &str, env: &mut HashMap<String, String>) -> Result<String> {
    // 現在のパスワードを確認（rootユーザーや新規作成ユーザー以外）
    if target_user == current_user && current_user != "root" {
        let current_password = rpassword::prompt_password("現在のパスワード: ")?;
        
        if !self.verify_password(target_user, &current_password)? {
            return Err(anyhow!("現在のパスワードが正しくありません。"));
        }
    }
    
    // 新しいパスワードを入力
    let new_password = rpassword::prompt_password("新しいパスワード: ")?;
            
            // パスワード強度をチェック
            if new_password.len() < 8 {
                return Err(anyhow!("パスワードが短すぎます。8文字以上必要です。"));
            }
            
            if !new_password.chars().any(|c| c.is_ascii_digit()) {
                return Err(anyhow!("パスワードには数字を含める必要があります。"));
            }
            
            if !new_password.chars().any(|c| c.is_ascii_lowercase()) || 
               !new_password.chars().any(|c| c.is_ascii_uppercase()) {
                return Err(anyhow!("パスワードには大文字と小文字の両方を含める必要があります。"));
            }
            
            // 特殊文字の存在確認
            let special_chars = "!@#$%^&*()_+-=[]{}|;:,.<>?/";
            if !new_password.chars().any(|c| special_chars.contains(c)) {
                return Err(anyhow!("パスワードには特殊文字を含める必要があります。"));
            }
            
    // 確認のためのパスワード再入力
    let confirm_password = rpassword::prompt_password("新しいパスワード(確認): ")?;
    
    // パスワードの一致を確認
    if new_password != confirm_password {
        return Err(anyhow!("パスワードが一致しません。"));
    }
    
    // パスワードを変更
    self.change_password(target_user, &new_password)?;
            
            Ok(format!("ユーザー {} のパスワードを変更しました", target_user))
        }

/// ユーザーが存在するか確認する
fn check_user_exists(&self, username: &str) -> Result<bool> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // Linux/Unixの場合はgetentコマンドを使用
        let output = Command::new("getent")
            .args(["passwd", username])
            .output()?;
            
        Ok(output.status.success())
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        
        // Windowsの場合はnet userコマンドを使用
        let output = Command::new("net")
            .args(["user", username])
            .output()?;
            
        Ok(output.status.success())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // その他のプラットフォームでは対応する方法を実装
        // 一般的なユーザー名のリストをチェック
        let common_users = vec!["root", "admin", "guest", "user"];
        Ok(common_users.contains(&username))
    }
}

/// パスワードを検証する
fn verify_password(&self, username: &str, password: &str) -> Result<bool> {
    #[cfg(target_os = "linux")]
    {
        use pam::Authenticator;
        
        // PAM認証を使用
        let mut authenticator = Authenticator::with_password("system-auth")
            .map_err(|e| anyhow!("PAM初期化エラー: {}", e))?;
            
        authenticator.get_handler().set_credentials(username, password);
        
        match authenticator.authenticate() {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        use winapi::um::winbase::LogonUserW;
        use winapi::um::winnt::{LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT};
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        
        // UTF-16文字列に変換
        fn to_wstring(s: &str) -> Vec<u16> {
            OsStr::new(s).encode_wide().chain(Some(0)).collect()
        }
        
        let username_wide = to_wstring(username);
        let password_wide = to_wstring(password);
        let domain_wide = to_wstring(".");  // ローカルマシン
        
        let mut token_handle = std::ptr::null_mut();
        
        // LogonUserでパスワードを検証
        let result = unsafe {
            LogonUserW(
                username_wide.as_ptr(),
                domain_wide.as_ptr(),
                password_wide.as_ptr(),
                LOGON32_LOGON_INTERACTIVE,
                LOGON32_PROVIDER_DEFAULT,
                &mut token_handle
            )
        };
        
        if result != 0 {
            // ハンドルを閉じる
            unsafe {
                winapi::um::handleapi::CloseHandle(token_handle);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // システム依存コードがない環境では環境変数のハッシュを検証
        let key = format!("USER_{}_PASSWORD", username.to_uppercase());
        
        if let Some(stored_hash) = env.get(&key) {
            if stored_hash.starts_with("HASH:") {
                // ハッシュを抽出して検証
                let hash = &stored_hash[5..];
                self.verify_password_hash(password, hash)
            } else {
                // 暗号化されていない場合は直接比較
                Ok(password == stored_hash)
            }
        } else {
            // パスワードが設定されていない
            Ok(false)
        }
    }
}

/// パスワードを変更する
fn change_password(&self, username: &str, new_password: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        use std::io::Write;
        
        // 現在の実行ユーザーがroot権限を持っているか確認
        let is_root = std::env::var("USER").unwrap_or_default() == "root" || 
                      std::process::id() == 0;
        
        if is_root {
            // chpasswdコマンドを使用
            let mut child = Command::new("chpasswd")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
                
            if let Some(mut stdin) = child.stdin.take() {
                // ユーザー名:パスワード形式
                stdin.write_all(format!("{}:{}\n", username, new_password).as_bytes())?;
            }
            
            let output = child.wait_with_output()?;
            
            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("パスワード変更に失敗: {}", error));
            }
        } else {
            // 一般ユーザーの場合はpasswdコマンドを使用
            let mut child = Command::new("passwd")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
                
            if let Some(mut stdin) = child.stdin.take() {
                // 新しいパスワードを2回入力（プロンプトに応じて）
                stdin.write_all(format!("{}\n{}\n", new_password, new_password).as_bytes())?;
            }
            
            let output = child.wait_with_output()?;
            
            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("パスワード変更に失敗: {}", error));
            }
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        
        // Windowsの場合はnet userコマンドを使用
        let output = Command::new("net")
            .args(["user", username, new_password])
            .output()?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("パスワード変更に失敗: {}", error));
        }
        
        Ok(())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // システム依存コードがない環境では環境変数にハッシュを保存
        // ハッシュ化
        let hashed_password = self.hash_password(new_password)?;
        let key = format!("USER_{}_PASSWORD", username.to_uppercase());
        
        env.insert(key, format!("HASH:{}", hashed_password));
        
        Ok(())
    }
} 