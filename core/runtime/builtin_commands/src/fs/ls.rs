use crate::{BuiltinCommand, CommandContext, CommandResult};
use crate::fs::utils::{FileInfo, FileType, get_file_info, format_file_size, format_permissions, get_file_type_char, format_timestamp, get_username, get_groupname};
use anyhow::Result;
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, warn};
use chrono::{DateTime, Local};
use clap::{App, Arg, ArgMatches};
use colored::Colorize;
use std::os::unix::fs::PermissionsExt;
use std::time::SystemTime;
use std::io;

/// オプションフラグを表す構造体
#[derive(Debug, Default)]
struct LsOptions {
    /// 全てのファイルを表示（隠しファイルを含む）
    all: bool,
    /// '.'と'..'を除くほぼ全てのファイルを表示
    almost_all: bool,
    /// リスト形式で表示
    long_format: bool,
    /// サイズを人間が読みやすい形式で表示
    human_readable: bool,
    /// ディレクトリ自体ではなく、その内容をリスト
    list_directory_contents: bool,
    /// ファイルサイズでソート
    sort_by_size: bool,
    /// 更新時刻でソート
    sort_by_time: bool,
    /// 逆順でソート
    reverse_sort: bool,
    /// 再帰的にディレクトリを表示
    recursive: bool,
    /// 色付きで表示
    colorize: bool,
    /// iノード番号を表示
    show_inode: bool,
    /// ディレクトリの末尾にスラッシュを表示
    classify: bool,
    /// 1列に1エントリずつ表示
    one_per_line: bool,
}

/// ディレクトリの内容をリスト表示するコマンド
///
/// UNIXの標準的なlsコマンドの実装です。指定されたディレクトリの内容を表示します。
/// オプションにより、詳細情報の表示、隠しファイルの表示、並び順の変更などが可能です。
///
/// # 使用例
///
/// ```bash
/// ls                  # カレントディレクトリの内容を表示
/// ls -l               # 詳細情報付きで表示
/// ls -la              # 隠しファイルを含むすべてのファイルを詳細表示
/// ls -lh              # 人間が読みやすいサイズ形式で詳細表示
/// ls -R               # 再帰的にディレクトリの内容を表示
/// ls /path/to/dir     # 指定したディレクトリの内容を表示
/// ```
pub struct LsCommand;

#[async_trait]
impl BuiltinCommand for LsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    fn description(&self) -> &'static str {
        "ディレクトリの内容を表示します"
    }

    fn usage(&self) -> &'static str {
        "ls [オプション]... [ファイル]..."
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // コマンドライン引数をパース
        let matches = App::new("ls")
            .about("ディレクトリの内容を表示します")
            .arg(
                Arg::with_name("all")
                    .short('a')
                    .long("all")
                    .help(".(ドット)で始まるエントリも表示します")
            )
            .arg(
                Arg::with_name("long")
                    .short('l')
                    .help("詳細情報を表示します")
            )
            .arg(
                Arg::with_name("human-readable")
                    .short('h')
                    .help("ファイルサイズを読みやすい形式で表示します（例：1K 234M 2G）")
            )
            .arg(
                Arg::with_name("size")
                    .short('S')
                    .help("ファイルサイズでソートします")
            )
            .arg(
                Arg::with_name("time")
                    .short('t')
                    .help("更新日時でソートします")
            )
            .arg(
                Arg::with_name("reverse")
                    .short('r')
                    .help("逆順でソートします")
            )
            .arg(
                Arg::with_name("recursive")
                    .short('R')
                    .help("サブディレクトリを再帰的に表示します")
            )
            .arg(
                Arg::with_name("directory")
                    .short('d')
                    .help("ディレクトリの内容ではなくディレクトリ自体を表示します")
            )
            .arg(
                Arg::with_name("no-dereference")
                    .short('P')
                    .help("シンボリックリンクをたどりません")
            )
            .arg(
                Arg::with_name("FILES")
                    .help("表示するファイルやディレクトリ")
                    .multiple(true)
            )
            .get_matches_from(context.args);

        // オプションの解析
        let options = LsOptions {
            all: matches.is_present("all"),
            long_format: matches.is_present("long"),
            human_readable: matches.is_present("human-readable"),
            sort_by_size: matches.is_present("size"),
            sort_by_time: matches.is_present("time"),
            reverse_sort: matches.is_present("reverse"),
            recursive: matches.is_present("recursive"),
            colorize: true,  // デフォルトで色付き表示
            show_inode: false,
            classify: false,
            one_per_line: false,
            almost_all: false,
            list_directory_contents: false,
        };

        // 表示対象のパスを取得（指定がなければカレントディレクトリ）
        let paths: Vec<&str> = matches.values_of("FILES")
            .map(|vals| vals.collect())
            .unwrap_or_else(|| vec!["."].into_iter().collect());

        // ls実行
        let result = self.execute_ls(&paths, &options).await?;
        
        Ok(CommandResult::success(result.into_bytes()))
    }
}

impl LsCommand {
    /// ls コマンドを実行
    async fn execute_ls(&self, paths: &[&str], options: &LsOptions) -> Result<String> {
        let mut result = String::new();
        let show_headers = paths.len() > 1 || options.recursive;
        
        for (i, path) in paths.iter().enumerate() {
            if i > 0 {
                result.push_str("\n");
            }
            
            if show_headers {
                result.push_str(&format!("{}:\n", path));
            }
            
            let path_obj = Path::new(path);
            if options.list_directory_contents || !path_obj.is_dir() {
                // 単一ファイルまたはディレクトリ自体を表示
                if let Ok(metadata) = fs::metadata(path_obj) {
                    let entry = path_obj.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string());

                    let formatted = if options.long_format {
                        self.format_entry_long(path_obj, &metadata, &entry, options)?
                    } else {
                        self.format_entry(path_obj, &metadata, &entry, options)?
                    };
                    
                    result.push_str(&formatted);
                    result.push_str("\n");
                } else {
                    result.push_str(&format!("ls: {}: そのようなファイルやディレクトリはありません\n", path));
                }
            } else {
                // ディレクトリの内容を表示
                match self.list_directory(path_obj, options) {
                    Ok(content) => {
                        result.push_str(&content);
                        
                        // 再帰的に表示
                        if options.recursive {
                            if let Ok(entries) = fs::read_dir(path_obj) {
                                for entry_result in entries {
                                    if let Ok(entry) = entry_result {
                                        let entry_path = entry.path();
                                        if entry_path.is_dir() {
                                            let entry_name = entry.file_name();
                                            let name = entry_name.to_string_lossy();
                                            
                                            // 隠しディレクトリは無視（-aオプションあり時は表示）
                                            if options.all || !name.starts_with('.') {
                                                let sub_path = path_obj.join(&entry_name);
                                                let sub_path_str = sub_path.to_string_lossy();
                                                
                                                result.push_str("\n\n");
                                                result.push_str(&format!("{}:\n", sub_path_str));
                                                
                                                if let Ok(sub_content) = self.list_directory(&sub_path, options) {
                                                    result.push_str(&sub_content);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        result.push_str(&format!("ls: {}: {}\n", path, e));
                    }
                }
            }
        }
        
        Ok(result)
    }
    
    /// ディレクトリの内容を一覧表示
    fn list_directory(&self, dir_path: &Path, options: &LsOptions) -> Result<String> {
        // ディレクトリを読み込む
        let mut entries: Vec<DirEntry> = fs::read_dir(dir_path)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy();
                options.all || !name.starts_with('.')
            })
            .collect();
        
        // エントリをソート
        self.sort_entries(&mut entries, options)?;
        
        let mut result = String::new();
        
        if options.long_format {
            // 長いリスト形式で表示
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if let Ok(metadata) = entry.metadata() {
                    let line = self.format_entry_long(&path, &metadata, &name, options)?;
                    result.push_str(&line);
                    result.push_str("\n");
                }
            }
        } else {
            // 通常形式で表示
            let mut lines = Vec::new();
            let mut current_line = String::new();
            let terminal_width = self.get_terminal_width();
            let max_name_len = entries.iter()
                .map(|e| e.file_name().to_string_lossy().len())
                .max()
                .unwrap_or(0);
            let column_width = (max_name_len + 2).min(40);
            let columns = (terminal_width / column_width).max(1);
            
            for (i, entry) in entries.iter().enumerate() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if let Ok(metadata) = entry.metadata() {
                    let formatted = self.format_entry(&path, &metadata, &name, options)?;
                    
                    if i > 0 && i % columns == 0 {
                        lines.push(current_line);
                        current_line = String::new();
                    }
                    
                    let padding = column_width.saturating_sub(name.len());
                    current_line.push_str(&formatted);
                    current_line.push_str(&" ".repeat(padding));
                }
            }
            
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            
            result.push_str(&lines.join("\n"));
        }
        
        Ok(result)
    }
    
    /// エントリを指定された順序でソート
    fn sort_entries(&self, entries: &mut Vec<DirEntry>, options: &LsOptions) -> Result<()> {
    if options.sort_by_size {
        // サイズでソート
        entries.sort_by(|a, b| {
                let size_a = a.metadata().map(|m| m.len()).unwrap_or(0);
                let size_b = b.metadata().map(|m| m.len()).unwrap_or(0);
            if options.reverse_sort {
                    size_a.cmp(&size_b)
            } else {
                    size_b.cmp(&size_a)
            }
        });
    } else if options.sort_by_time {
            // 更新日時でソート
        entries.sort_by(|a, b| {
                let time_a = a.metadata().and_then(|m| m.modified()).ok();
                let time_b = b.metadata().and_then(|m| m.modified()).ok();
                let ord = match (time_a, time_b) {
                    (Some(t_a), Some(t_b)) => t_b.cmp(&t_a),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                };
            if options.reverse_sort {
                    ord.reverse()
            } else {
                    ord
            }
        });
    } else {
            // 名前でソート（デフォルト）
        entries.sort_by(|a, b| {
                let name_a = a.file_name().to_string_lossy();
                let name_b = b.file_name().to_string_lossy();
                let ord = name_a.cmp(&name_b);
            if options.reverse_sort {
                    ord.reverse()
            } else {
                    ord
                }
            });
        }
        
        Ok(())
    }
    
    /// 通常形式でエントリをフォーマット
    fn format_entry(&self, path: &Path, metadata: &Metadata, name: &str, options: &LsOptions) -> Result<String> {
        if !options.colorize {
            return Ok(name.to_string());
        }
        
        // ファイルタイプに基づいて色付け
        if metadata.is_dir() {
            Ok(name.blue().bold().to_string())
        } else if metadata.is_symlink() {
            Ok(name.cyan().to_string())
        } else if metadata.permissions().mode() & 0o111 != 0 {
            // 実行可能ファイル
            Ok(name.green().to_string())
        } else {
            Ok(name.to_string())
        }
    }
    
    /// 長いリスト形式でエントリをフォーマット
    fn format_entry_long(&self, path: &Path, metadata: &Metadata, name: &str, options: &LsOptions) -> Result<String> {
        // パーミッション
        let mode = self.format_mode(metadata.permissions().mode());
        
        // ハードリンク数（Unixのみ）
        #[cfg(unix)]
        let nlink = metadata.nlink().unwrap_or(1);
        #[cfg(not(unix))]
        let nlink = 1;
        
        // 所有者名
        #[cfg(unix)]
        let owner = self.get_user_name(metadata.uid().unwrap_or(0));
        #[cfg(not(unix))]
        let owner = String::from("user");
        
        // グループ名
        #[cfg(unix)]
        let group = self.get_group_name(metadata.gid().unwrap_or(0));
        #[cfg(not(unix))]
        let group = String::from("group");
        
        // ファイルサイズ
        let size = if options.human_readable {
            self.format_size_human_readable(metadata.len())
        } else {
            metadata.len().to_string()
        };
        
        // 更新日時
        let modified = metadata.modified().unwrap_or_else(|_| SystemTime::now());
        let datetime: DateTime<Local> = modified.into();
        let date = datetime.format("%b %d %H:%M").to_string();
        
        // カラー表示の名前
        let colored_name = if options.colorize {
            if metadata.is_dir() {
                name.blue().bold().to_string()
            } else if metadata.is_symlink() {
                name.cyan().to_string()
            } else if metadata.permissions().mode() & 0o111 != 0 {
                name.green().to_string()
            } else {
                name.to_string()
            }
        } else {
            name.to_string()
        };
        
        // シンボリックリンクの場合はリンク先を表示
        let display_name = if metadata.is_symlink() && !options.no_dereference {
            if let Ok(target) = fs::read_link(path) {
                format!("{} -> {}", colored_name, target.to_string_lossy())
            } else {
                colored_name
            }
        } else {
            colored_name
        };
        
        Ok(format!(
            "{} {:>2} {:8} {:8} {:>8} {} {}",
            mode, nlink, owner, group, size, date, display_name
        ))
    }
    
    /// ファイルモードを文字列表現に変換
    fn format_mode(&self, mode: u32) -> String {
        let file_type = match mode & 0o170000 {
            0o040000 => 'd',  // ディレクトリ
            0o120000 => 'l',  // シンボリックリンク
            0o100000 => '-',  // 通常ファイル
            0o060000 => 'b',  // ブロックデバイス
            0o020000 => 'c',  // キャラクタデバイス
            0o010000 => 'p',  // 名前付きパイプ
            0o140000 => 's',  // ソケット
            _ => '?',         // 不明
        };
        
        let user_r = if mode & 0o400 != 0 { 'r' } else { '-' };
        let user_w = if mode & 0o200 != 0 { 'w' } else { '-' };
        let user_x = match mode & 0o4100 {
            0o4100 => 's',  // setuid + 実行可能
            0o4000 => 'S',  // setuid
            0o0100 => 'x',  // 実行可能
            _ => '-',       // 実行不可
        };
        
        let group_r = if mode & 0o040 != 0 { 'r' } else { '-' };
        let group_w = if mode & 0o020 != 0 { 'w' } else { '-' };
        let group_x = match mode & 0o2010 {
            0o2010 => 's',  // setgid + 実行可能
            0o2000 => 'S',  // setgid
            0o0010 => 'x',  // 実行可能
            _ => '-',       // 実行不可
        };
        
        let other_r = if mode & 0o004 != 0 { 'r' } else { '-' };
        let other_w = if mode & 0o002 != 0 { 'w' } else { '-' };
        let other_x = match mode & 0o1001 {
            0o1001 => 't',  // sticky bit + 実行可能
            0o1000 => 'T',  // sticky bit
            0o0001 => 'x',  // 実行可能
            _ => '-',       // 実行不可
        };
        
        format!(
            "{}{}{}{}{}{}{}{}{}{}",
            file_type, 
            user_r, user_w, user_x,
            group_r, group_w, group_x,
            other_r, other_w, other_x
        )
    }
    
    /// UID からユーザー名を取得
    #[cfg(unix)]
    fn get_user_name(&self, uid: u32) -> String {
        use std::ffi::CStr;
        use std::mem;
        use libc::{passwd, getpwuid, uid_t};
        
        unsafe {
            let pw = getpwuid(uid as uid_t);
            if !pw.is_null() {
                let passwd: &passwd = &*pw;
                if !passwd.pw_name.is_null() {
                    let c_str = CStr::from_ptr(passwd.pw_name);
                    if let Ok(name) = c_str.to_str() {
                        return name.to_string();
                    }
                }
            }
        }
        
        uid.to_string()
    }
    
    /// GID からグループ名を取得
    #[cfg(unix)]
    fn get_group_name(&self, gid: u32) -> String {
        use std::ffi::CStr;
        use std::mem;
        use libc::{group, getgrgid, gid_t};
        
        unsafe {
            let gr = getgrgid(gid as gid_t);
            if !gr.is_null() {
                let group: &group = &*gr;
                if !group.gr_name.is_null() {
                    let c_str = CStr::from_ptr(group.gr_name);
                    if let Ok(name) = c_str.to_str() {
                        return name.to_string();
                    }
                }
            }
        }
        
        gid.to_string()
    }
    
    /// ファイルサイズを人間が読みやすい形式でフォーマット
    fn format_size_human_readable(&self, size: u64) -> String {
        const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
        
        if size == 0 {
            return "0".to_string();
        }
        
        let mut size_f = size as f64;
        let mut unit_index = 0;
        
        while size_f >= 1024.0 && unit_index < UNITS.len() - 1 {
            size_f /= 1024.0;
            unit_index += 1;
        }
        
        if size_f < 10.0 && unit_index > 0 {
            format!("{:.1}{}", size_f, UNITS[unit_index])
        } else {
            format!("{:.0}{}", size_f, UNITS[unit_index])
        }
    }
    
    /// ターミナルの幅を取得
    fn get_terminal_width(&self) -> usize {
        #[cfg(unix)]
        {
            use std::mem;
            use libc::{ioctl, winsize, TIOCGWINSZ, STDOUT_FILENO};
            
            unsafe {
                let mut ws: winsize = mem::zeroed();
                if ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut ws) == 0 {
                    return ws.ws_col as usize;
                }
            }
        }
        
        // デフォルト値またはエラー時
        80
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::symlink;
    
    #[test]
    fn test_ls_empty_dir() {
        let dir = tempdir().unwrap();
        let ls = LsCommand;
        let options = LsOptions::default();
        
        let result = ls.list_directory(dir.path(), &options).unwrap();
        assert_eq!(result, "");
    }
    
    #[test]
    fn test_ls_with_files() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let _file = File::create(&file_path).unwrap();
        
        let ls = LsCommand;
        let options = LsOptions::default();
        
        let result = ls.list_directory(dir.path(), &options).unwrap();
        assert!(result.contains("test.txt"));
    }
    
    #[test]
    fn test_ls_hidden_files() {
        let dir = tempdir().unwrap();
        let visible_path = dir.path().join("visible.txt");
        let hidden_path = dir.path().join(".hidden.txt");
        let _visible = File::create(&visible_path).unwrap();
        let _hidden = File::create(&hidden_path).unwrap();
        
        let ls = LsCommand;
        let options = LsOptions::default();
        
        // 通常モード（隠しファイルを表示しない）
        let result = ls.list_directory(dir.path(), &options).unwrap();
        assert!(result.contains("visible.txt"));
        assert!(!result.contains(".hidden.txt"));
        
        // 隠しファイルを表示するモード
        let options_all = LsOptions {
            all: true,
            ..LsOptions::default()
        };
        let result_all = ls.list_directory(dir.path(), &options_all).unwrap();
        assert!(result_all.contains("visible.txt"));
        assert!(result_all.contains(".hidden.txt"));
    }
    
    #[test]
    fn test_format_mode() {
        let ls = LsCommand;
        
        // 標準的なファイルパーミッション
        assert_eq!(ls.format_mode(0o100644), "-rw-r--r--");
        
        // 実行可能ファイル
        assert_eq!(ls.format_mode(0o100755), "-rwxr-xr-x");
        
        // ディレクトリ
        assert_eq!(ls.format_mode(0o040755), "drwxr-xr-x");
        
        // シンボリックリンク
        assert_eq!(ls.format_mode(0o120777), "lrwxrwxrwx");
        
        // setuid, setgid, sticky bit
        assert_eq!(ls.format_mode(0o104755), "-rwsr-xr-x");
        assert_eq!(ls.format_mode(0o102755), "-rwxr-sr-x");
        assert_eq!(ls.format_mode(0o101755), "-rwxr-xr-t");
    }
    
    #[test]
    fn test_format_size_human_readable() {
        let ls = LsCommand;
        
        assert_eq!(ls.format_size_human_readable(0), "0");
        assert_eq!(ls.format_size_human_readable(1023), "1023B");
        assert_eq!(ls.format_size_human_readable(1024), "1K");
        assert_eq!(ls.format_size_human_readable(1536), "1.5K");
        assert_eq!(ls.format_size_human_readable(1048576), "1M");
        assert_eq!(ls.format_size_human_readable(1073741824), "1G");
    }
} 