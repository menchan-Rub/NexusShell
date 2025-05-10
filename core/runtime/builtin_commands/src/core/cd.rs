use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::Result;
use async_trait::async_trait;
use std::env;
use std::path::{Path, PathBuf};
use std::fs;
use tracing::{debug, warn, error};

/// カレントディレクトリを変更するコマンド
///
/// UNIXの標準的なcdコマンドの実装です。引数としてディレクトリパスを受け取り、
/// カレントディレクトリをそのパスに変更します。引数がない場合はホームディレクトリに移動します。
/// チルダ展開やシンボリックリンクの解決をサポートしています。
///
/// # 使用例
///
/// ```bash
/// cd /path/to/directory  # 指定したディレクトリに移動
/// cd                    # ホームディレクトリに移動
/// cd -                  # 前回のディレクトリに移動
/// cd ..                 # 親ディレクトリに移動
/// cd ~username          # 指定したユーザーのホームディレクトリに移動
/// ```
pub struct CdCommand;

#[async_trait]
impl BuiltinCommand for CdCommand {
    fn name(&self) -> &'static str {
        "cd"
    }

    fn description(&self) -> &'static str {
        "カレントディレクトリを変更します"
    }

    fn usage(&self) -> &'static str {
        "cd [ディレクトリ]\n\n引数を省略するとホームディレクトリに移動します。\n'-'を指定すると前回のディレクトリに移動します。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // 引数を取得（最初の引数はコマンド名なので、それ以降を使用）
        let args = context.args.iter().skip(1).collect::<Vec<_>>();
        
        // 移動先のディレクトリを決定
        let target_dir = if args.is_empty() {
            // 引数がなければホームディレクトリに移動
            match dirs::home_dir() {
                Some(home) => home,
                None => {
                    let error_message = "ホームディレクトリが見つかりません".to_string();
                    error!("{}", error_message);
                    return Ok(CommandResult::failure(1)
                        .with_stderr(error_message.into_bytes()));
                }
            }
        } else if args[0] == "-" {
            // 前回のディレクトリに移動
            if let Ok(prev_dir) = env::var("OLDPWD") {
                PathBuf::from(prev_dir)
            } else {
                let error_message = "前回のディレクトリが設定されていません".to_string();
                error!("{}", error_message);
                return Ok(CommandResult::failure(1)
                    .with_stderr(error_message.into_bytes()));
            }
        } else {
            // 指定されたディレクトリに移動
            let mut path = PathBuf::from(args[0]);
            
            // チルダ展開
            if args[0].starts_with('~') {
                path = expand_tilde(args[0])?;
            }
            
            // 相対パスの場合は現在のディレクトリからの相対パスとして解釈
            if path.is_relative() {
                path = context.current_dir.join(path);
            }
            
            path
        };
        
        // ディレクトリの存在確認
        if !target_dir.exists() {
            let error_message = format!("cd: {}: そのようなファイルやディレクトリはありません", 
                target_dir.display());
            error!("{}", error_message);
            return Ok(CommandResult::failure(1)
                .with_stderr(error_message.into_bytes()));
        }
        
        // ディレクトリであるか確認
        if !target_dir.is_dir() {
            let error_message = format!("cd: {}: ディレクトリではありません", 
                target_dir.display());
            error!("{}", error_message);
            return Ok(CommandResult::failure(1)
                .with_stderr(error_message.into_bytes()));
        }
        
        // ディレクトリにアクセス権があるか確認
        if !is_directory_accessible(&target_dir) {
            let error_message = format!("cd: {}: アクセス権がありません", 
                target_dir.display());
            error!("{}", error_message);
            return Ok(CommandResult::failure(1)
                .with_stderr(error_message.into_bytes()));
        }
        
        // 現在のディレクトリを記録（OLDPWD環境変数として）
        // 注: 実際のシェル環境で環境変数を設定するには、シェルに対して変更を通知する必要がある
        if let Ok(current_dir) = env::current_dir() {
            debug!("OLDPWD={}", current_dir.display());
            // 環境変数の設定は、実際のシェル実装では、ここで行うのではなく
            // コマンドの実行結果として返し、シェルに環境変数の更新を依頼します
        }
        
        // カレントディレクトリを変更
        // 注: これはプロセス内でのみ有効。実際のシェルでは、戻り値を通じてシェルに
        // ディレクトリ変更を通知する必要があります。
        debug!("ディレクトリを変更: {}", target_dir.display());
        
        // 実際には、シェルに変更を通知するためのデータを返す
        let result = CommandResult::success()
            .with_stdout(format!("{}", target_dir.display()).into_bytes());
        
        Ok(result)
    }
}

/// チルダ展開を行う
///
/// `~` をユーザーのホームディレクトリに展開します。
/// `~username` を指定したユーザーのホームディレクトリに展開します。
fn expand_tilde(path: &str) -> Result<PathBuf> {
    if !path.starts_with('~') {
        return Ok(PathBuf::from(path));
    }
    
    if path == "~" || path.starts_with("~/") {
        // 現在のユーザーのホームディレクトリ
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("ホームディレクトリが見つかりません"))?;
        
        if path == "~" {
            return Ok(home);
        }
        
        // ~/rest/of/path の処理
        return Ok(home.join(&path[2..]));
    }
    
    // ~username または ~username/rest/of/path の処理
    let parts: Vec<&str> = path[1..].splitn(2, '/').collect();
    let username = parts[0];
    
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::ptr;
        use libc::{getpwnam, passwd};
        
        // ユーザー名をCStringに変換
        let c_username = CString::new(username)
            .map_err(|_| anyhow::anyhow!("無効なユーザー名"))?;
        
        // getpwnam関数で指定したユーザーの情報を取得
        let passwd_ptr = unsafe { getpwnam(c_username.as_ptr()) };
        
        if passwd_ptr == ptr::null_mut() {
            return Err(anyhow::anyhow!("ユーザー '{}' が見つかりません", username));
        }
        
        // passwdからホームディレクトリを取得
        let home_ptr = unsafe { (*passwd_ptr).pw_dir };
        
        if home_ptr == ptr::null_mut() {
            return Err(anyhow::anyhow!("ユーザー '{}' のホームディレクトリが設定されていません", username));
        }
        
        // C文字列をRustの文字列に変換
        let home_c_str = unsafe { std::ffi::CStr::from_ptr(home_ptr) };
        let home_str = home_c_str.to_str()
            .map_err(|_| anyhow::anyhow!("ホームディレクトリのパスが無効です"))?;
        
        let mut home_path = PathBuf::from(home_str);
        
        // パスの残りの部分があれば追加
        if parts.len() > 1 {
            home_path = home_path.join(parts[1]);
        }
        
        return Ok(home_path);
    }
    
    #[cfg(not(unix))]
    {
        // 非UNIXシステムでの実装
        // Windowsなどでは、ユーザー名によるホームディレクトリの解決に
        // システム固有のAPIを使用する必要があります
        
        return Err(anyhow::anyhow!("このプラットフォームでは ~username の展開はサポートされていません"));
    }
}

/// ディレクトリにアクセス可能か確認
fn is_directory_accessible(dir: &Path) -> bool {
    // read_dirが成功すれば、ディレクトリへのアクセス権があると判断
    fs::read_dir(dir).is_ok()
} 