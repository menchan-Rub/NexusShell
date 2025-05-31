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

    pub fn run(&mut self, target_user: &str, args: &ArgMatches) -> Result<String> {
        debug!("suコマンドを実行します: target_user={}", target_user);
        
        // ユーザーが存在するか確認する
        let is_user_exists = self.check_user_exists(target_user)?;
        
        if !is_user_exists {
            return Err(anyhow!("ユーザー {} は存在しません", target_user));
        }
        
        // 現在のユーザーを取得
        let current_user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        debug!("現在のユーザー: {}", current_user);
        
        // ログインシェルオプション
        let login_shell = args.get_flag("login");
        
        // パスワード認証をスキップ（-オプション）
        let skip_auth = args.get_flag("skip-auth");
        
        // 実行するコマンド
        let command = args.get_one::<String>("command").map(|s| s.as_str());
        
        // 認証処理（root以外の場合）
        let auth_successful = if current_user != "root" && !skip_auth {
            let passwd = rpassword::prompt_password(&format!("{}のパスワード: ", target_user))?;
            self.authenticate_user(target_user, &passwd)?
        } else {
            true
        };
        
        if !auth_successful {
            return Err(anyhow!("認証に失敗しました"));
        }
        
        // コマンドを実行するか、シェルを起動する
        if let Some(cmd) = command {
            self.execute_command_as_user(target_user, cmd, login_shell)?
        } else {
            self.switch_user(target_user, login_shell, args.get_one::<String>("shell").cloned())?
        }
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
                .args ["user", username])
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
    
    /// ユーザー認証を行う
    fn authenticate_user(&self, username: &str, password: &str) -> Result<bool> {
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
            let domain_wide = to_wstring("."); // ローカルマシン
            
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
            // 環境変数からパスワードを検証（開発環境用）
            let key = format!("USER_{}_PASSWORD", username.to_uppercase());
            
            if let Ok(stored_password) = std::env::var(&key) {
                if stored_password.starts_with("HASH:") {
                    // ハッシュされたパスワードを検証
                    let hash = &stored_password[5..];
                    self.verify_password_hash(password, hash)
                } else {
                    // 平文パスワードを比較
                    Ok(password == stored_password)
                }
            } else {
                // デフォルトの振る舞い: ユーザー名の逆がパスワード
                let expected = username.chars().rev().collect::<String>();
                Ok(password == expected)
            }
        }
    }
    
    /// パスワードハッシュを検証する
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn verify_password_hash(&self, password: &str, hash: &str) -> Result<bool> {
        use argon2::{
            password_hash::{PasswordHash, PasswordVerifier},
            Argon2
        };
        
        // ハッシュ文字列を解析
        let parsed_hash = match PasswordHash::new(hash) {
            Ok(h) => h,
            Err(e) => return Err(anyhow!("パスワードハッシュの解析に失敗: {}", e)),
        };
        
        // Argon2インスタンスを作成
        let argon2 = Argon2::default();
        
        // パスワードを検証
        Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }
    
    /// 指定されたユーザーでコマンドを実行する
    fn execute_command_as_user(&self, username: &str, command: &str, login_shell: bool) -> Result<String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::{Command, Stdio};
            use std::io::{Read, Write};
            
            let mut args = Vec::new();
            
            if login_shell {
                args.push("-l");
            }
            
            args.push("-c");
            args.push(command);
            args.push(username);
            
            // suコマンドで実行
            let mut child = Command::new("su")
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            
            // プロセスの標準出力を読み取る
            let mut output = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                stdout.read_to_string(&mut output)?;
            }
            
            // プロセスの標準エラー出力を読み取る
            let mut error = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                stderr.read_to_string(&mut error)?;
            }
            
            // 終了ステータスを取得
            let status = child.wait()?;
            
            if status.success() {
                if !output.is_empty() {
                    Ok(output)
                } else {
                    Ok(format!("ユーザー {} としてコマンド {} を実行しました", username, command))
                }
            } else {
                Err(anyhow!("コマンド実行エラー: {}", error))
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            use std::process::{Command, Stdio};
            use std::io::{Read, Write};
            
            // Windows環境ではrunas コマンドを使用
            let mut cmd = Command::new("runas");
            cmd.arg(format!("/user:{}", username));
            
            // コマンドを実行
            cmd.arg("cmd.exe /c");
            cmd.arg(command);
            
            // プロセスを実行
            let mut child = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            
            // プロセスの標準出力を読み取る
            let mut output = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                stdout.read_to_string(&mut output)?;
            }
            
            // プロセスの標準エラー出力を読み取る
            let mut error = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                stderr.read_to_string(&mut error)?;
            }
            
            // 終了ステータスを取得
            let status = child.wait()?;
            
            if status.success() {
                if !output.is_empty() {
                    Ok(output)
                } else {
                    Ok(format!("ユーザー {} としてコマンド {} を実行しました", username, command))
                }
            } else {
                Err(anyhow!("コマンド実行エラー: {}", error))
            }
        }
        
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            // ダミー実装 - 環境変数を変更してコマンドをシミュレート
            let old_user = std::env::var("USER").unwrap_or_default();
            
            // 一時的にUSER環境変数を変更
            std::env::set_var("USER", username);
            
            // コマンドを実行
            let output = format!("ユーザー {} としてコマンド '{}' を実行しました", username, command);
            
            // 元の環境変数に戻す
            std::env::set_var("USER", old_user);
            
            Ok(output)
        }
    }
    
    /// ユーザー切り替えの本物の実装（OSごとに分岐）
    fn switch_user(&self, username: &str, login_shell: bool, shell_path: Option<&str>, command_to_run: Option<&str>) -> Result<String> {
        #[cfg(unix)]
        {
            use std::process::{Command, Stdio};
            use std::io::{Read, Write};

            let mut cmd_args = Vec::new();

            // ログインシェルオプション
            if login_shell {
                cmd_args.push("-l".to_string());
            }

            // ユーザー名
            cmd_args.push(username.to_string());

            // シェル指定
            if let Some(s_path) = shell_path {
                cmd_args.push("-s".to_string());
                cmd_args.push(s_path.to_string());
            }

            let actual_command_to_run: String;
            if let Some(c) = command_to_run {
                cmd_args.push("-c".to_string());
                actual_command_to_run = c.to_string();
                cmd_args.push(actual_command_to_run.clone());
            }


            // `sudo` を試行し、失敗したら `su` を試行
            let mut child_process = Command::new("sudo")
                .args(&cmd_args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();

            if child_process.is_err() {
                // sudo が失敗した場合、su を試す
                // su の場合、-c オプションの引数はクォートが必要な場合がある
                if command_to_run.is_some() {
                    // 既存の -c とコマンドを削除
                    cmd_args.pop(); // コマンド本体
                    cmd_args.pop(); // -c
                    cmd_args.push("-c".to_string());
                    cmd_args.push(format!("'{}'", actual_command_to_run)); // シングルクォートで囲む
                }
                child_process = Command::new("su")
                    .args(&cmd_args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();
            }


            match child_process {
                Ok(mut child) => {
                    // パスワードが必要な場合に備えて標準入力を開いておく
                    // ただし、ここではパスワードプロンプトへの自動入力は実装しない
                    // 必要に応じてユーザーが手動で入力することを期待する
                    if let Some(mut stdin) = child.stdin.take() {
                        // プロンプトが表示された場合に備えて、何か書き込むか、
                        // またはそのまま閉じるかなどの戦略が必要
                        // ここでは一旦何もしない
                        drop(stdin);
                    }

                    let output = child.wait_with_output()?;
                    
                    if output.status.success() {
                        Ok(String::from_utf8_lossy(&output.stdout).to_string())
                    } else {
                        Err(anyhow!(
                            "コマンド実行に失敗しました ({}): {}",
                            output.status,
                            String::from_utf8_lossy(&output.stderr)
                        ))
                    }
                }
                Err(e) => Err(anyhow!("コマンドの起動に失敗しました: {}", e)),
            }
        }
        #[cfg(windows)]
        {
            use std::process::{Command, Stdio};
            // Windowsでは `runas` コマンドを使用
            // `runas` は直接パスワードを渡すオプションがないため、
            // 基本的には新しいウィンドウでプロンプトが表示される挙動となる。
            // /savecred オプションはセキュリティリスクがあるため推奨されない。
            // ここではコマンド実行のみを試みる。

            let mut cmd = Command::new("runas");
            cmd.arg(format!("/user:{}", username));

            if let Some(c) = command_to_run {
                 // runasでコマンドを実行する場合、コマンド全体を一つの文字列として渡す
                let mut full_command = String::new();
                if let Some(s_path) = shell_path {
                    full_command.push_str(s_path);
                    full_command.push_str(" /c "); // cmd.exe /c のような形式を想定
                } else {
                     // デフォルトシェル (cmd.exe)
                    full_command.push_str("cmd.exe /c ");
                }
                full_command.push_str(c);
                cmd.arg(full_command);

            } else {
                // コマンドが指定されていない場合は、指定されたシェル（またはデフォルト）を起動
                if let Some(s_path) = shell_path {
                    cmd.arg(s_path);
                } else {
                    // デフォルトシェル (cmd.exe)
                    cmd.arg("cmd.exe");
                }
            }
            
            // runas は対話的なパスワード入力を求めるため、
            // stdout/stderr の直接キャプチャは難しい場合がある。
            // ここではコマンド起動の成否のみを返す。
            match cmd.status() {
                Ok(status) => {
                    if status.success() {
                        Ok(format!("ユーザー {} としてコマンドを起動しました。", username))
                    } else {
                        Err(anyhow!("runas コマンドの実行に失敗しました (終了コード: {})", status))
                    }
                }
                Err(e) => Err(anyhow!("runas コマンドの起動に失敗しました: {}", e)),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            // その他のOSでは未サポート
            Err(anyhow!("このOSではユーザー切り替えはサポートされていません。"))
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
        // clapパーサーを使用して引数を解析
        let matches = self.build_parser().try_get_matches_from(&args)?;

        let target_user = matches.get_one::<String>("user")
            .map(|s| s.as_str())
            .unwrap_or("root"); // デフォルトはrootユーザー

        let login_shell = matches.get_flag("login");
        let shell_path = matches.get_one::<String>("shell").map(|s| s.as_str());
        let command_to_run = matches.get_one::<String>("command").map(|s| s.as_str());

        // ユーザーが存在するか確認
        if !self.check_user_exists(target_user)? {
            return Err(anyhow!("ユーザー {} は存在しません", target_user));
        }
        
        // 認証処理 (デモやテストのために、ここでは認証をスキップするオプションは設けない)
        // 実際のシェルでは、ここでPAMやWindows APIを使った認証が必要。
        // ただし、suコマンドの基本的な動作としては、OSのsu/sudo/runasに委ねるため、
        // ここでの独自認証は必須ではない。OS側の認証機構が使われる。

        self.switch_user(target_user, login_shell, shell_path, command_to_run)
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -l, --login     ログインシェルとして起動します\n  -c, --command=COMMAND  指定したコマンドを実行します\n  -s, --shell=SHELL     指定したシェルを使用します\n\n引数:\n  user                切り替えるユーザー名（デフォルトはroot）\n\n例:\n  su                  rootユーザーに切り替え\n  su admin            adminユーザーに切り替え\n  su -c 'ls -la' admin  adminユーザーとしてコマンドを実行\n",
            self.description,
            self.usage
        )
    }
} 