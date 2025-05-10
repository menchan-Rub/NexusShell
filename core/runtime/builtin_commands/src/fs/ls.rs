use crate::{BuiltinCommand, CommandContext, CommandResult};
use crate::fs::utils::{FileInfo, FileType, get_file_info, format_file_size, format_permissions, get_file_type_char, format_timestamp, get_username, get_groupname};
use anyhow::Result;
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, warn};

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
        "ディレクトリの内容をリスト表示します"
    }

    fn usage(&self) -> &'static str {
        "ls [オプション]... [ファイル]...\n\n\
        主なオプション:\n\
        -a, --all                隠しファイルを含む全てのエントリを表示\n\
        -A, --almost-all         '.'と'..'を除く全てのエントリを表示\n\
        -l                       詳細な情報を表示\n\
        -h, --human-readable     サイズを読みやすい形式で表示（例: 1K 234M 2G）\n\
        -R, --recursive          サブディレクトリを再帰的に一覧表示\n\
        -r, --reverse            ソート順を逆にする\n\
        -S                       ファイルサイズでソート\n\
        -t                       更新時刻でソート\n\
        -F, --classify           ディレクトリには'/'、実行可能ファイルには'*'などを付加\n\
        -1                       1行につき1エントリずつ表示\n\
        --color[=WHEN]           カラー表示を使用 (always/never/auto)"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // 引数を取得（最初の引数はコマンド名なので、それ以降を使用）
        let args = context.args.iter().skip(1).collect::<Vec<_>>();
        
        // オプションを解析
        let (options, targets) = parse_options(&args)?;
        
        // 表示対象のパスを決定
        let targets = if targets.is_empty() {
            // 引数がなければカレントディレクトリを使用
            vec![context.current_dir.clone()]
        } else {
            // 指定されたパスを現在のディレクトリからの相対パスとして解釈
            targets.iter().map(|t| {
                if Path::new(t).is_absolute() {
                    PathBuf::from(t)
                } else {
                    context.current_dir.join(t)
                }
            }).collect()
        };
        
        // 結果を格納するバッファ
        let mut output = Vec::new();
        
        // 複数のディレクトリが指定された場合はディレクトリ名も表示
        let show_directory_names = targets.len() > 1 || options.recursive;
        
        // 各ターゲットについて処理
        for (i, target) in targets.iter().enumerate() {
            // 複数のディレクトリがある場合は、2つ目以降の前に空行を挿入
            if i > 0 {
                output.push(b'\n');
            }
            
            // ディレクトリ名を表示
            if show_directory_names {
                let header = format!("{}:", target.display());
                output.extend_from_slice(header.as_bytes());
                output.push(b'\n');
            }
            
            // ファイルの一覧を取得して表示
            let result = list_directory(target, &options);
            match result {
                Ok(listing) => {
                    output.extend_from_slice(listing.as_bytes());
                }
                Err(err) => {
                    let error_message = format!("ls: {}: {}", target.display(), err);
                    error!("{}", error_message);
                    output.extend_from_slice(error_message.as_bytes());
                    output.push(b'\n');
                }
            }
            
            // 再帰的に表示する場合
            if options.recursive {
                match list_recursive(target, &options) {
                    Ok(recursive_listing) => {
                        output.extend_from_slice(recursive_listing.as_bytes());
                    }
                    Err(err) => {
                        let error_message = format!("ls: 再帰的なリスト表示中にエラーが発生しました: {}", err);
                        error!("{}", error_message);
                        output.extend_from_slice(error_message.as_bytes());
                        output.push(b'\n');
                    }
                }
            }
        }
        
        // 最後に改行を追加（出力が空でない場合）
        if !output.is_empty() && output.last() != Some(&b'\n') {
            output.push(b'\n');
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// コマンドライン引数からオプションとターゲットパスを解析
fn parse_options(args: &[&String]) -> Result<(LsOptions, Vec<String>)> {
    let mut options = LsOptions::default();
    let mut targets = Vec::new();
    
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        
        if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
            // 短いオプション（例: -la）
            for c in arg.chars().skip(1) {
                match c {
                    'a' => options.all = true,
                    'A' => options.almost_all = true,
                    'l' => options.long_format = true,
                    'h' => options.human_readable = true,
                    'R' => options.recursive = true,
                    'r' => options.reverse_sort = true,
                    'S' => options.sort_by_size = true,
                    't' => options.sort_by_time = true,
                    'i' => options.show_inode = true,
                    'F' => options.classify = true,
                    '1' => options.one_per_line = true,
                    _ => {
                        return Err(anyhow::anyhow!("ls: 不明なオプション -- '{}'", c));
                    }
                }
            }
        } else if arg.starts_with("--") {
            // 長いオプション（例: --all）
            match arg.as_str() {
                "--all" => options.all = true,
                "--almost-all" => options.almost_all = true,
                "--human-readable" => options.human_readable = true,
                "--recursive" => options.recursive = true,
                "--reverse" => options.reverse_sort = true,
                "--classify" => options.classify = true,
                _ if arg.starts_with("--color") => {
                    options.colorize = match arg.as_str() {
                        "--color" | "--color=always" => true,
                        "--color=never" => false,
                        "--color=auto" => atty::is(atty::Stream::Stdout),
                        _ => {
                            return Err(anyhow::anyhow!("ls: '--color' オプションの引数が不正です: '{}'", arg));
                        }
                    };
                }
                _ => {
                    return Err(anyhow::anyhow!("ls: 不明なオプションです: '{}'", arg));
                }
            }
        } else {
            // ターゲットパス
            targets.push(arg.clone());
        }
        
        i += 1;
    }
    
    // デフォルトで色付き表示を有効化（端末に出力している場合）
    if !options.colorize {
        options.colorize = atty::is(atty::Stream::Stdout);
    }
    
    Ok((options, targets))
}

/// ディレクトリの内容を一覧表示
fn list_directory(dir_path: &Path, options: &LsOptions) -> Result<String> {
    // ディレクトリかどうかをチェック
    if dir_path.is_dir() {
        // ディレクトリ内のエントリを取得
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // 隠しファイルの処理
            if file_name.starts_with(".") {
                if file_name == "." || file_name == ".." {
                    if !options.all {
                        continue;
                    }
                } else if !options.all && !options.almost_all {
                    continue;
                }
            }
            
            // ファイル情報を取得
            match get_file_info(&path) {
                Ok(info) => entries.push(info),
                Err(err) => {
                    warn!("ファイル情報の取得に失敗しました: {}: {}", path.display(), err);
                    // エラーが出ても処理を続行
                }
            }
        }
        
        // エントリをソート
        sort_entries(&mut entries, options);
        
        // 結果を整形
        format_directory_listing(&entries, options)
    } else if dir_path.exists() {
        // 単一のファイルの場合
        match get_file_info(dir_path) {
            Ok(info) => {
                let entries = vec![info];
                format_directory_listing(&entries, options)
            }
            Err(err) => Err(anyhow::anyhow!("ファイル情報の取得に失敗しました: {}", err))
        }
    } else {
        Err(anyhow::anyhow!("そのようなファイルやディレクトリはありません"))
    }
}

/// ディレクトリ内のファイル一覧をソート
fn sort_entries(entries: &mut Vec<FileInfo>, options: &LsOptions) {
    if options.sort_by_size {
        // サイズでソート
        entries.sort_by(|a, b| {
            if options.reverse_sort {
                a.size.cmp(&b.size)
            } else {
                b.size.cmp(&a.size)
            }
        });
    } else if options.sort_by_time {
        // 更新時刻でソート
        entries.sort_by(|a, b| {
            if options.reverse_sort {
                a.modified.cmp(&b.modified)
            } else {
                b.modified.cmp(&a.modified)
            }
        });
    } else {
        // デフォルトは名前でソート
        entries.sort_by(|a, b| {
            let ordering = a.name.to_lowercase().cmp(&b.name.to_lowercase());
            if options.reverse_sort {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
}

/// ファイル一覧を整形して出力形式に変換
fn format_directory_listing(entries: &[FileInfo], options: &LsOptions) -> Result<String> {
    if entries.is_empty() {
        return Ok(String::new());
    }
    
    if options.long_format {
        // 詳細表示形式
        format_long_listing(entries, options)
    } else if options.one_per_line {
        // 1行に1エントリ
        let mut result = String::new();
        for entry in entries {
            let file_name = get_colorized_filename(entry, options);
            result.push_str(&file_name);
            result.push('\n');
        }
        Ok(result)
    } else {
        // 複数列表示（端末幅に合わせる）
        format_columns_listing(entries, options)
    }
}

/// 詳細表示形式でファイル一覧を整形
fn format_long_listing(entries: &[FileInfo], options: &LsOptions) -> Result<String> {
    let mut result = String::new();
    
    // カラム幅を計算
    let max_links = entries.iter().map(|e| e.links.to_string().len()).max().unwrap_or(1);
    let max_user = entries.iter().map(|e| get_username(e.uid).len()).max().unwrap_or(1);
    let max_group = entries.iter().map(|e| get_groupname(e.gid).len()).max().unwrap_or(1);
    let max_size = entries.iter().map(|e| {
        if options.human_readable {
            format_file_size(e.size).len()
        } else {
            e.size.to_string().len()
        }
    }).max().unwrap_or(1);
    
    // 各エントリについて詳細情報を表示
    for entry in entries {
        // ファイルタイプとパーミッション
        let type_char = get_file_type_char(entry.file_type);
        let permissions = format_permissions(&entry.permissions);
        result.push(type_char);
        result.push_str(&permissions);
        result.push(' ');
        
        // ハードリンク数
        result.push_str(&format!("{:>width$} ", entry.links, width = max_links));
        
        // 所有者とグループ
        let owner = get_username(entry.uid);
        let group = get_groupname(entry.gid);
        result.push_str(&format!("{:<width$} ", owner, width = max_user));
        result.push_str(&format!("{:<width$} ", group, width = max_group));
        
        // ファイルサイズ
        if options.human_readable {
            let size = format_file_size(entry.size);
            result.push_str(&format!("{:>width$} ", size, width = max_size));
        } else {
            result.push_str(&format!("{:>width$} ", entry.size, width = max_size));
        }
        
        // 更新日時
        let time = format_timestamp(entry.modified);
        result.push_str(&format!("{} ", time));
        
        // ファイル名（色付き）
        let file_name = get_colorized_filename(entry, options);
        result.push_str(&file_name);
        
        // シンボリックリンクの場合はリンク先を表示
        if entry.file_type == FileType::SymbolicLink {
            if let Ok(target) = fs::read_link(&entry.path) {
                result.push_str(" -> ");
                result.push_str(&target.to_string_lossy());
            }
        }
        
        result.push('\n');
    }
    
    Ok(result)
}

/// 複数列表示形式でファイル一覧を整形
fn format_columns_listing(entries: &[FileInfo], options: &LsOptions) -> Result<String> {
    // 端末の幅を取得（利用できない場合は80列と仮定）
    let term_width = match term_size::dimensions() {
        Some((width, _)) => width,
        None => 80,
    };
    
    // ファイル名の最大長を計算（色コードを除く）
    let max_name_len = entries.iter()
        .map(|e| {
            let mut len = e.name.len();
            if options.classify {
                if e.file_type == FileType::Directory {
                    len += 1; // '/'を追加
                } else if e.file_type == FileType::SymbolicLink {
                    len += 1; // '@'を追加
                } else if e.permissions.user.execute ||
                          e.permissions.group.execute ||
                          e.permissions.other.execute {
                    len += 1; // '*'を追加
                }
            }
            len
        })
        .max()
        .unwrap_or(0);
    
    // カラム間のスペース（最低2文字）
    let column_spacing = 2;
    
    // カラム数を計算
    let column_width = max_name_len + column_spacing;
    let num_columns = if column_width > 0 {
        std::cmp::max(1, term_width / column_width)
    } else {
        1
    };
    
    // 行数を計算
    let num_rows = (entries.len() + num_columns - 1) / num_columns;
    
    let mut result = String::new();
    
    // 行ごとに処理
    for row in 0..num_rows {
        for col in 0..num_columns {
            let index = row + col * num_rows;
            if index < entries.len() {
                let entry = &entries[index];
                let file_name = get_colorized_filename(entry, options);
                
                // 名前を表示
                result.push_str(&file_name);
                
                // 最後の列でなければ余白を追加
                if col < num_columns - 1 && index + num_rows < entries.len() {
                    let visible_length = entry.name.len(); // 色コードを除いた表示幅
                    let padding = column_width - visible_length;
                    result.push_str(&" ".repeat(padding));
                }
            }
        }
        result.push('\n');
    }
    
    Ok(result)
}

/// ファイル名を色付きで取得（オプションに応じて）
fn get_colorized_filename(entry: &FileInfo, options: &LsOptions) -> String {
    let mut name = entry.name.clone();
    
    // オプションに応じてファイル名に種類を示す記号を追加
    if options.classify {
        if entry.file_type == FileType::Directory {
            name.push('/');
        } else if entry.file_type == FileType::SymbolicLink {
            name.push('@');
        } else if entry.permissions.user.execute ||
                  entry.permissions.group.execute ||
                  entry.permissions.other.execute {
            name.push('*');
        }
    }
    
    // 色付き表示が有効な場合
    if options.colorize {
        // ANSIエスケープシーケンスで色を指定
        match entry.file_type {
            FileType::Directory => format!("\x1b[1;34m{}\x1b[0m", name),  // 青色（太字）
            FileType::SymbolicLink => format!("\x1b[1;36m{}\x1b[0m", name), // シアン（太字）
            FileType::Regular if entry.permissions.user.execute ||
                            entry.permissions.group.execute ||
                            entry.permissions.other.execute => 
                format!("\x1b[1;32m{}\x1b[0m", name), // 緑色（太字）
            _ => name, // 通常のファイルは色なし
        }
    } else {
        name
    }
}

/// ディレクトリを再帰的に表示
fn list_recursive(dir_path: &Path, options: &LsOptions) -> Result<String> {
    let mut result = String::new();
    let mut dirs_to_process = Vec::new();
    
    // ディレクトリ内のエントリを取得
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // "."や".."は処理しない
            if file_name != "." && file_name != ".." {
                // 隠しディレクトリの処理
                if file_name.starts_with(".") {
                    if options.all || options.almost_all {
                        dirs_to_process.push(path);
                    }
                } else {
                    dirs_to_process.push(path);
                }
            }
        }
    }
    
    // サブディレクトリを処理
    for dir in dirs_to_process {
        result.push('\n');
        let header = format!("\n{}:", dir.display());
        result.push_str(&header);
        
        // サブディレクトリの内容を表示
        match list_directory(&dir, options) {
            Ok(listing) => {
                result.push('\n');
                result.push_str(&listing);
            }
            Err(err) => {
                let error_message = format!("\nls: {}: {}", dir.display(), err);
                result.push_str(&error_message);
            }
        }
        
        // さらに再帰
        match list_recursive(&dir, options) {
            Ok(recursive_listing) => {
                result.push_str(&recursive_listing);
            }
            Err(err) => {
                let error_message = format!("\nls: 再帰的なリスト表示中にエラーが発生しました: {}", err);
                result.push_str(&error_message);
            }
        }
    }
    
    Ok(result)
} 