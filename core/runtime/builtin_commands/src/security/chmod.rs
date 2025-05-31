use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::os::unix::fs::PermissionsExt;
use anyhow::{Result, anyhow, Context};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// ファイル権限変更コマンド
pub struct ChmodCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl ChmodCommand {
    /// 新しいChmodCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "chmod".to_string(),
            description: "ファイルやディレクトリの権限を変更します".to_string(),
            usage: "chmod [オプション] モード ファイル...".to_string(),
        }
    }
    
    /// オプションパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("chmod")
            .about("ファイルやディレクトリの権限を変更します")
            .arg(
                Arg::new("recursive")
                    .short('R')
                    .long("recursive")
                    .help("ディレクトリ内のファイルとサブディレクトリに再帰的に権限を適用します")
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
                Arg::new("mode")
                    .help("設定する権限モード（数字表記または記号表記）")
                    .required(true)
            )
            .arg(
                Arg::new("files")
                    .help("権限を変更するファイルまたはディレクトリのリスト")
                    .required(true)
                    .action(ArgAction::Append)
            )
    }
    
    /// モードをパース
    fn parse_mode(&self, mode_str: &str) -> Result<u32> {
        // 数字表記 (例: 755)
        if mode_str.chars().all(|c| c.is_digit(8)) {
            // 8進数として解析
            match u32::from_str_radix(mode_str, 8) {
                Ok(mode) => {
                    // モードは 0-0777 の範囲に制限
                    if mode <= 0o777 {
                        return Ok(mode);
                    } else {
                        return Err(anyhow!("権限モードが範囲外です: {}", mode_str));
                    }
                }
                Err(_) => {
                    return Err(anyhow!("権限モードの解析に失敗しました: {}", mode_str));
                }
            }
        }
        
        // 記号表記 (例: u+x,g-w)
        // この実装は簡易版で、u+x,g-w などの形式をサポート
        let mut base_mode = if Path::new(mode_str).exists() {
            // stat コマンドでファイルの現在のパーミッションを取得
            let metadata = fs::metadata(mode_str)?;
            metadata.permissions().mode() & 0o777
        } else {
            // 既定値
            0o644
        };
        
        for part in mode_str.split(',') {
            let mut chars = part.chars();
            
            // 対象ユーザー種別を取得 (u: ユーザー, g: グループ, o: その他, a: 全て)
            let target = chars.next().ok_or_else(|| anyhow!("不正なモード形式: {}", part))?;
            
            // 操作を取得 (+: 追加, -: 削除, =: 設定)
            let operation = chars.next().ok_or_else(|| anyhow!("不正なモード形式: {}", part))?;
            
            // 権限を取得 (r: 読込, w: 書込, x: 実行)
            let permissions: String = chars.collect();
            
            // 各権限に対応するビットマスク
            let mut mask = 0;
            for perm in permissions.chars() {
                match perm {
                    'r' => mask |= 0o4,
                    'w' => mask |= 0o2,
                    'x' => mask |= 0o1,
                    _ => return Err(anyhow!("不正な権限文字: {}", perm)),
                }
            }
            
            // ターゲット別のビットシフト量
            let shift = match target {
                'u' => 6, // ユーザー権限 (8進数の3桁目)
                'g' => 3, // グループ権限 (8進数の2桁目)
                'o' => 0, // その他の権限 (8進数の1桁目)
                'a' => {
                    // 全てのユーザー
                    // ユーザー権限を更新
                    match operation {
                        '+' => base_mode |= mask << 6,
                        '-' => base_mode &= !(mask << 6),
                        '=' => {
                            base_mode &= !(0o7 << 6);
                            base_mode |= mask << 6;
                        },
                        _ => return Err(anyhow!("不正な操作: {}", operation)),
                    }
                    
                    // グループ権限を更新
                    match operation {
                        '+' => base_mode |= mask << 3,
                        '-' => base_mode &= !(mask << 3),
                        '=' => {
                            base_mode &= !(0o7 << 3);
                            base_mode |= mask << 3;
                        },
                        _ => return Err(anyhow!("不正な操作: {}", operation)),
                    }
                    
                    // その他の権限を更新
                    match operation {
                        '+' => base_mode |= mask,
                        '-' => base_mode &= !mask,
                        '=' => {
                            base_mode &= !0o7;
                            base_mode |= mask;
                        },
                        _ => return Err(anyhow!("不正な操作: {}", operation)),
                    }
                    
                    continue;
                },
                _ => return Err(anyhow!("不正なターゲット: {}", target)),
            };
            
            // 権限を更新
            match operation {
                '+' => base_mode |= mask << shift,
                '-' => base_mode &= !(mask << shift),
                '=' => {
                    base_mode &= !(0o7 << shift);
                    base_mode |= mask << shift;
                },
                _ => return Err(anyhow!("不正な操作: {}", operation)),
            }
        }
        
        Ok(base_mode)
    }
    
    /// ファイルの権限を再帰的に変更
    fn chmod_recursive(&self, path: &Path, mode: u32, verbose: bool) -> Result<()> {
        // 現在のファイル/ディレクトリの権限を変更
        self.chmod_file(path, mode, verbose)?;
        
        // ディレクトリの場合は再帰的に処理
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                self.chmod_recursive(&path, mode, verbose)?;
            }
        }
        
        Ok(())
    }
    
    /// 単一ファイルの権限を変更
    fn chmod_file(&self, path: &Path, mode: u32, verbose: bool) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(mode);
            std::fs::set_permissions(path, perms)?;
            if verbose {
                println!("{} のパーミッションを {:o} に変更しました", path.display(), mode);
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            use std::process::Command;
            let output = Command::new("icacls")
                .arg(path)
                .arg("/grant")
                .arg(format!("Everyone:F"))
                .output()?;
            if !output.status.success() {
                return Err(anyhow::anyhow!("icaclsコマンド失敗: {}", String::from_utf8_lossy(&output.stderr)));
            }
            if verbose {
                println!("{} の権限を変更しました (Windows)", path.display());
            }
            Ok(())
        }
    }
}

impl BuiltinCommand for ChmodCommand {
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
        
        // モードを取得
        let mode_str = matches.get_one::<String>("mode")
            .ok_or_else(|| anyhow!("モードが指定されていません"))?;
        
        // モードをパース
        let mode = self.parse_mode(mode_str)?;
        
        // ファイルリストを取得
        let files = matches.get_many::<String>("files")
            .ok_or_else(|| anyhow!("ファイルが指定されていません"))?
            .cloned()
            .collect::<Vec<_>>();
        
        let mut result = String::new();
        
        // 各ファイルに対して権限を変更
        for file in files {
            let path = Path::new(&file);
            
            if !path.exists() {
                result.push_str(&format!("エラー: ファイルが存在しません: {}\n", file));
                continue;
            }
            
            if recursive && path.is_dir() {
                // 再帰的に権限を変更
                if let Err(e) = self.chmod_recursive(path, mode, verbose) {
                    result.push_str(&format!("エラー: {}: {}\n", file, e));
                }
            } else {
                // 単一ファイルの権限を変更
                if let Err(e) = self.chmod_file(path, mode, verbose) {
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
            "{}\n\n使用法: {}\n\nオプション:\n  -R, --recursive    ディレクトリ内のファイルとサブディレクトリに再帰的に権限を適用\n  -v, --verbose      実行内容を詳細に表示\n\n引数:\n  mode              設定する権限モード（数字表記または記号表記）\n  files...          権限を変更するファイルまたはディレクトリのリスト\n\n例:\n  chmod 755 script.sh            script.shに実行権限を付与\n  chmod -R u+w,g-w,o-w ディレクトリ  ディレクトリ以下のファイルに対して再帰的に権限を変更\n",
            self.description,
            self.usage
        )
    }
} 