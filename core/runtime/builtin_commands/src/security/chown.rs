use std::collections::HashMap;
use std::fs;
use std::path::Path;
use anyhow::{Result, anyhow, Context};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// ファイル所有者変更コマンド
pub struct ChownCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl ChownCommand {
    /// 新しいChownCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "chown".to_string(),
            description: "ファイルやディレクトリの所有者を変更します".to_string(),
            usage: "chown [オプション] 所有者[:グループ] ファイル...".to_string(),
        }
    }
    
    /// オプションパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("chown")
            .about("ファイルやディレクトリの所有者を変更します")
            .arg(
                Arg::new("recursive")
                    .short('R')
                    .long("recursive")
                    .help("ディレクトリ内のファイルとサブディレクトリに再帰的に所有者を適用します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .help("実行内容を詳細に表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("no_dereference")
                    .short('h')
                    .long("no-dereference")
                    .help("シンボリックリンク自体の所有者を変更します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("owner")
                    .help("所有者（とオプションでグループ）。形式: user:group または user")
                    .required(true)
            )
            .arg(
                Arg::new("files")
                    .help("所有者を変更するファイルまたはディレクトリのリスト")
                    .required(true)
                    .action(ArgAction::Append)
            )
    }
    
    /// 所有者とグループをパース
    fn parse_owner(&self, owner_str: &str) -> Result<(Option<String>, Option<String>)> {
        if owner_str.is_empty() {
            return Err(anyhow!("所有者が指定されていません"));
        }
        
        // 所有者とグループをコロンで分割
        if let Some(pos) = owner_str.find(':') {
            let (user, group) = owner_str.split_at(pos);
            let group = &group[1..]; // コロンをスキップ
            
            let user_opt = if user.is_empty() { None } else { Some(user.to_string()) };
            let group_opt = if group.is_empty() { None } else { Some(group.to_string()) };
            
            // 少なくとも一方が指定されている必要がある
            if user_opt.is_none() && group_opt.is_none() {
                return Err(anyhow!("ユーザーまたはグループのいずれかを指定する必要があります"));
            }
            
            Ok((user_opt, group_opt))
        } else {
            // グループが指定されていない場合
            Ok((Some(owner_str.to_string()), None))
        }
    }
    
    /// ファイルの所有者を再帰的に変更
    fn chown_recursive(&self, path: &Path, user: &Option<String>, group: &Option<String>, 
                       no_dereference: bool, verbose: bool) -> Result<()> {
        // 現在のファイル/ディレクトリの所有者を変更
        self.chown_file(path, user, group, no_dereference, verbose)?;
        
        // ディレクトリの場合は再帰的に処理
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                self.chown_recursive(&path, user, group, no_dereference, verbose)?;
            }
        }
        
        Ok(())
    }
    
    /// 単一ファイルの所有者を変更
    fn chown_file(&self, path: &Path, user: &Option<String>, group: &Option<String>, 
                  no_dereference: bool, verbose: bool) -> Result<()> {
        // UNIXプラットフォームでのみ実装
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            use nix::unistd::{Uid, Gid, chown};
            use nix::Error as NixError;
            
            // ユーザーIDとグループIDを取得
            let uid = if let Some(user) = user {
                match nix::unistd::User::from_name(user)? {
                    Some(user_info) => Some(user_info.uid),
                    None => return Err(anyhow!("ユーザー '{}' が見つかりません", user)),
                }
            } else {
                None
            };
            
            let gid = if let Some(group) = group {
                match nix::unistd::Group::from_name(group)? {
                    Some(group_info) => Some(group_info.gid),
                    None => return Err(anyhow!("グループ '{}' が見つかりません", group)),
                }
            } else {
                None
            };
            
            // 所有者を変更
            let result = if no_dereference {
                chown(
                    path, 
                    uid.map(Uid::from_raw), 
                    gid.map(Gid::from_raw),
                )
            } else {
                let canonical_path = fs::canonicalize(path)?;
                chown(
                    &canonical_path, 
                    uid.map(Uid::from_raw), 
                    gid.map(Gid::from_raw),
                )
            };
            
            // エラー処理
            if let Err(e) = result {
                match e {
                    NixError::Sys(errno) => {
                        return Err(anyhow!("所有者の変更に失敗しました: {}", errno));
                    }
                    _ => {
                        return Err(anyhow!("所有者の変更に失敗しました: {}", e));
                    }
                }
            }
            
            if verbose {
                let user_str = user.as_deref().unwrap_or("-");
                let group_str = group.as_deref().unwrap_or("-");
                println!("所有者を {}:{} に変更: {}", 
                        user_str, group_str, path.display());
            }
            
            Ok(())
        }
        
        // Windowsなど他のプラットフォームでは警告表示
        #[cfg(not(unix))]
        {
            if verbose {
                println!("警告: {} はWindowsプラットフォームでは完全にはサポートされていません", self.name);
            }
            
            // Windows環境では未実装
            Err(anyhow!("このプラットフォームではchownコマンドはサポートされていません"))
        }
    }
}

impl BuiltinCommand for ChownCommand {
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
        
        // オプション取得
        let recursive = matches.get_flag("recursive");
        let verbose = matches.get_flag("verbose");
        let no_dereference = matches.get_flag("no_dereference");
        
        // 所有者情報を取得
        let owner_str = matches.get_one::<String>("owner")
            .ok_or_else(|| anyhow!("所有者が指定されていません"))?;
        
        // 所有者とグループをパース
        let (user, group) = self.parse_owner(owner_str)?;
        
        // ファイルリストを取得
        let files = matches.get_many::<String>("files")
            .ok_or_else(|| anyhow!("ファイルが指定されていません"))?
            .cloned()
            .collect::<Vec<_>>();
        
        // Windows環境ではサポートされていないことを警告
        #[cfg(not(unix))]
        {
            return Err(anyhow!("このプラットフォームではchownコマンドはサポートされていません"));
        }
        
        let mut result = String::new();
        
        // 各ファイルに対して所有者を変更
        #[cfg(unix)]
        for file in files {
            let path = Path::new(&file);
            
            if !path.exists() {
                result.push_str(&format!("エラー: ファイルが存在しません: {}\n", file));
                continue;
            }
            
            if recursive && path.is_dir() {
                // 再帰的に所有者を変更
                if let Err(e) = self.chown_recursive(path, &user, &group, no_dereference, verbose) {
                    result.push_str(&format!("エラー: {}: {}\n", file, e));
                }
            } else {
                // 単一ファイルの所有者を変更
                if let Err(e) = self.chown_file(path, &user, &group, no_dereference, verbose) {
                    result.push_str(&format!("エラー: {}: {}\n", file, e));
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
            "{}\n\n使用法: {}\n\nオプション:\n  -R, --recursive      ディレクトリ内のファイルとサブディレクトリに再帰的に所有者を適用\n  -v, --verbose        実行内容を詳細に表示\n  -h, --no-dereference  シンボリックリンク自体の所有者を変更\n\n引数:\n  owner              所有者（とオプションでグループ）。形式: user:group または user\n  files...           所有者を変更するファイルまたはディレクトリのリスト\n\n例:\n  chown user1 file.txt               file.txtの所有者をuser1に変更\n  chown user1:group1 file.txt        file.txtの所有者とグループを変更\n  chown -R user1:group1 ディレクトリ   ディレクトリ以下のファイルに対して再帰的に所有者とグループを変更\n  chown :group1 file.txt             file.txtのグループのみを変更\n\n注意:\n  このコマンドはUNIXシステムでのみ完全に機能します。\n",
            self.description,
            self.usage
        )
    }
} 