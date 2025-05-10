use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use byte_unit::{Byte, ByteUnit};
use std::fs;
use std::path::Path;
use std::process::Command;

/// ディスクの空き容量と使用状況を表示するコマンド
///
/// ファイルシステムごとの全体サイズ、使用量、空き容量、使用率、
/// マウントポイントを表示します。
///
/// # 使用例
///
/// ```bash
/// df                  # すべてのファイルシステムの情報を表示
/// df -h               # 人間が読みやすい形式で表示
/// df -i               # iノード情報を表示
/// df /home            # 特定のマウントポイントのみ表示
/// ```
pub struct DfCommand;

/// ファイルシステム情報を格納する構造体
struct FilesystemInfo {
    filesystem: String,
    size: u64,
    used: u64,
    available: u64,
    use_percent: f64,
    mounted_on: String,
}

#[async_trait]
impl BuiltinCommand for DfCommand {
    fn name(&self) -> &'static str {
        "df"
    }

    fn description(&self) -> &'static str {
        "ディスクの空き容量と使用状況を表示します"
    }

    fn usage(&self) -> &'static str {
        "df [オプション] [ファイル...]\n\n\
        オプション:\n\
        -h, --human-readable    サイズを人間が読みやすい形式（例：1K 234M 2G）で表示\n\
        -i, --inodes            ブロックの代わりにiノード情報を表示\n\
        -t, --type=TYPE         指定したタイプのファイルシステムのみを表示\n\
        -x, --exclude-type=TYPE 指定したタイプのファイルシステムを除外\n\
        --help                  このヘルプを表示して終了"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // オプション解析
        let human_readable = context.args.iter().any(|arg| *arg == "-h" || *arg == "--human-readable");
        let show_inodes = context.args.iter().any(|arg| *arg == "-i" || *arg == "--inodes");
        let show_help = context.args.iter().any(|arg| *arg == "--help");
        
        // タイプフィルタリングオプションの解析
        let mut include_types = Vec::new();
        let mut exclude_types = Vec::new();
        
        for i in 0..context.args.len() {
            let arg = &context.args[i];
            if arg.starts_with("-t=") || arg.starts_with("--type=") {
                let fs_type = arg.split('=').nth(1).unwrap_or("");
                if !fs_type.is_empty() {
                    include_types.push(fs_type.to_string());
                }
            } else if arg.starts_with("-x=") || arg.starts_with("--exclude-type=") {
                let fs_type = arg.split('=').nth(1).unwrap_or("");
                if !fs_type.is_empty() {
                    exclude_types.push(fs_type.to_string());
                }
            } else if arg == "-t" || arg == "--type" {
                if i + 1 < context.args.len() {
                    include_types.push(context.args[i + 1].clone());
                }
            } else if arg == "-x" || arg == "--exclude-type" {
                if i + 1 < context.args.len() {
                    exclude_types.push(context.args[i + 1].clone());
                }
            }
        }
        
        // ヘルプの表示
        if show_help {
            return Ok(CommandResult::success().with_stdout(self.usage().as_bytes().to_vec()));
        }
        
        // 指定されたパスを取得（なければすべてのファイルシステムを表示）
        let mut paths = Vec::new();
        for arg in &context.args {
            if !arg.starts_with('-') && arg != "-t" && arg != "--type" && arg != "-x" && arg != "--exclude-type" {
                paths.push(arg.clone());
            }
        }
        
        // ファイルシステム情報の取得
        let mut fs_info = if show_inodes {
            get_filesystem_inode_info(&paths, &include_types, &exclude_types)?
        } else {
            get_filesystem_info(&paths, &include_types, &exclude_types)?
        };
        
        // 表示フォーマットの作成
        let mut output = Vec::new();
        
        if show_inodes {
            // inode情報のヘッダー
            output.extend_from_slice(format!("{:<20} {:>10} {:>10} {:>10} {:>8} {:<20}\n", 
                "ファイルシステム", "Inodes", "使用済み", "空き", "使用率%", "マウント位置").as_bytes());
        } else {
            // 通常のヘッダー
            output.extend_from_slice(format!("{:<20} {:>10} {:>10} {:>10} {:>8} {:<20}\n", 
                "ファイルシステム", "容量", "使用済み", "空き", "使用率%", "マウント位置").as_bytes());
        }
        
        // 各ファイルシステムの情報を出力
        for info in &fs_info {
            let size_str = if human_readable {
                format_size_human_readable(info.size)
            } else {
                format!("{}", info.size / 1024) // KBで表示
            };
            
            let used_str = if human_readable {
                format_size_human_readable(info.used)
            } else {
                format!("{}", info.used / 1024) // KBで表示
            };
            
            let avail_str = if human_readable {
                format_size_human_readable(info.available)
            } else {
                format!("{}", info.available / 1024) // KBで表示
            };
            
            output.extend_from_slice(format!("{:<20} {:>10} {:>10} {:>10} {:>7.1}% {:<20}\n", 
                info.filesystem, size_str, used_str, avail_str, info.use_percent, info.mounted_on).as_bytes());
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// ファイルシステム情報を取得
fn get_filesystem_info(paths: &[String], include_types: &[String], exclude_types: &[String]) -> Result<Vec<FilesystemInfo>> {
    let mut result = Vec::new();
    
    #[cfg(target_os = "linux")]
    {
        // Linuxの場合はdfコマンドを利用
        let mut cmd = Command::new("df");
        cmd.arg("-k"); // 1KBブロック単位で表示
        
        // パスが指定されていれば追加
        for path in paths {
            cmd.arg(path);
        }
        
        let output = cmd.output().map_err(|e| anyhow!("dfコマンドの実行に失敗しました: {}", e))?;
        
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().skip(1).collect(); // ヘッダー行をスキップ
            
            for line in lines {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let filesystem = parts[0].to_string();
                    let size = parts[1].parse::<u64>().unwrap_or(0) * 1024; // KBをバイトに変換
                    let used = parts[2].parse::<u64>().unwrap_or(0) * 1024;
                    let available = parts[3].parse::<u64>().unwrap_or(0) * 1024;
                    let use_percent = parts[4].trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
                    let mounted_on = parts[5].to_string();
                    
                    // タイプフィルタリング
                    let fs_type = get_filesystem_type(&mounted_on);
                    if !include_types.is_empty() && !include_types.iter().any(|t| fs_type.contains(t)) {
                        continue;
                    }
                    if exclude_types.iter().any(|t| fs_type.contains(t)) {
                        continue;
                    }
                    
                    result.push(FilesystemInfo {
                        filesystem,
                        size,
                        used,
                        available,
                        use_percent,
                        mounted_on,
                    });
                }
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOSの場合もdfコマンドを利用
        let mut cmd = Command::new("df");
        cmd.arg("-k"); // 1KBブロック単位で表示
        
        // パスが指定されていれば追加
        for path in paths {
            cmd.arg(path);
        }
        
        let output = cmd.output().map_err(|e| anyhow!("dfコマンドの実行に失敗しました: {}", e))?;
        
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().skip(1).collect(); // ヘッダー行をスキップ
            
            for line in lines {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 9 {
                    let filesystem = parts[0].to_string();
                    let size = parts[1].parse::<u64>().unwrap_or(0) * 1024; // KBをバイトに変換
                    let used = parts[2].parse::<u64>().unwrap_or(0) * 1024;
                    let available = parts[3].parse::<u64>().unwrap_or(0) * 1024;
                    let capacity = parts[4].trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
                    let mounted_on = parts[8].to_string();
                    
                    // タイプフィルタリング
                    let fs_type = get_filesystem_type(&mounted_on);
                    if !include_types.is_empty() && !include_types.iter().any(|t| fs_type.contains(t)) {
                        continue;
                    }
                    if exclude_types.iter().any(|t| fs_type.contains(t)) {
                        continue;
                    }
                    
                    result.push(FilesystemInfo {
                        filesystem,
                        size,
                        used,
                        available,
                        use_percent: capacity,
                        mounted_on,
                    });
                }
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        // Windowsの場合はPowerShellを使用
        let output = Command::new("powershell")
            .args(&["-Command", "Get-Volume | Select-Object DriveLetter, FileSystemType, Size, SizeRemaining | Format-List"])
            .output()
            .map_err(|e| anyhow!("PowerShellコマンドの実行に失敗しました: {}", e))?;
            
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().collect();
            
            let mut drive_letter = String::new();
            let mut fs_type = String::new();
            let mut size: u64 = 0;
            let mut size_remaining: u64 = 0;
            
            for line in lines {
                if line.trim().is_empty() {
                    // 空行は項目の区切り
                    if !drive_letter.is_empty() && size > 0 {
                        let used = size.saturating_sub(size_remaining);
                        let use_percent = if size > 0 { 
                            (used as f64 / size as f64) * 100.0 
                        } else { 
                            0.0 
                        };
                        
                        // パスフィルタリング
                        let should_include = paths.is_empty() || 
                            paths.iter().any(|p| p.starts_with(&format!("{}:", drive_letter)));
                            
                        // タイプフィルタリング
                        if !include_types.is_empty() && !include_types.iter().any(|t| fs_type.contains(t)) {
                            continue;
                        }
                        if exclude_types.iter().any(|t| fs_type.contains(t)) {
                            continue;
                        }
                        
                        if should_include {
                            result.push(FilesystemInfo {
                                filesystem: format!("{}:", drive_letter),
                                size,
                                used,
                                available: size_remaining,
                                use_percent,
                                mounted_on: format!("{}:\\", drive_letter),
                            });
                        }
                    }
                    
                    drive_letter = String::new();
                    fs_type = String::new();
                    size = 0;
                    size_remaining = 0;
                    continue;
                }
                
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let key = parts[0].trim();
                    let value = parts[1..].join(":").trim().to_string();
                    
                    match key {
                        "DriveLetter" => drive_letter = value,
                        "FileSystemType" => fs_type = value,
                        "Size" => size = value.parse::<u64>().unwrap_or(0),
                        "SizeRemaining" => size_remaining = value.parse::<u64>().unwrap_or(0),
                        _ => {}
                    }
                }
            }
        }
    }
    
    // クロスプラットフォーム対応（上記の特定OS向けコードが実行されなかった場合）
    if result.is_empty() {
        // 各パスについて空き容量を取得
        let mut paths_to_check = paths.clone();
        if paths_to_check.is_empty() {
            // パスが指定されていない場合は、現在のディレクトリを使用
            paths_to_check.push(".".to_string());
        }
        
        for path_str in paths_to_check {
            let path = Path::new(&path_str);
            if let Ok(metadata) = fs::metadata(path) {
                if metadata.is_dir() {
                    if let Ok(available) = fs::available_space(path) {
                        if let Ok(total) = fs::total_space(path) {
                            let used = total.saturating_sub(available);
                            let use_percent = if total > 0 { 
                                (used as f64 / total as f64) * 100.0 
                            } else { 
                                0.0 
                            };
                            
                            result.push(FilesystemInfo {
                                filesystem: path_str.clone(),
                                size: total,
                                used,
                                available,
                                use_percent,
                                mounted_on: path_str,
                            });
                        }
                    }
                }
            }
        }
    }
    
    Ok(result)
}

/// iノード情報を取得（Linuxのみ対応）
fn get_filesystem_inode_info(paths: &[String], include_types: &[String], exclude_types: &[String]) -> Result<Vec<FilesystemInfo>> {
    let mut result = Vec::new();
    
    #[cfg(target_os = "linux")]
    {
        // Linuxの場合はdf -iコマンドを利用
        let mut cmd = Command::new("df");
        cmd.arg("-i"); // iノード情報を表示
        
        // パスが指定されていれば追加
        for path in paths {
            cmd.arg(path);
        }
        
        let output = cmd.output().map_err(|e| anyhow!("df -iコマンドの実行に失敗しました: {}", e))?;
        
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().skip(1).collect(); // ヘッダー行をスキップ
            
            for line in lines {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let filesystem = parts[0].to_string();
                    let inodes = parts[1].parse::<u64>().unwrap_or(0);
                    let used = parts[2].parse::<u64>().unwrap_or(0);
                    let available = parts[3].parse::<u64>().unwrap_or(0);
                    let use_percent = parts[4].trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
                    let mounted_on = parts[5].to_string();
                    
                    // タイプフィルタリング
                    let fs_type = get_filesystem_type(&mounted_on);
                    if !include_types.is_empty() && !include_types.iter().any(|t| fs_type.contains(t)) {
                        continue;
                    }
                    if exclude_types.iter().any(|t| fs_type.contains(t)) {
                        continue;
                    }
                    
                    result.push(FilesystemInfo {
                        filesystem,
                        size: inodes,
                        used,
                        available,
                        use_percent,
                        mounted_on,
                    });
                }
            }
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // 非Linux環境では代替手段としてファイルシステム情報を表示し、
        // 注意メッセージを出力
        let mut fs_info = get_filesystem_info(paths, include_types, exclude_types)?;
        for info in &mut fs_info {
            info.size = 0;
            info.used = 0;
            info.available = 0;
            info.use_percent = 0.0;
        }
        result = fs_info;
        
        // 警告メッセージをコンソールに出力
        eprintln!("iノード情報はこのプラットフォームではサポートされていません。");
    }
    
    Ok(result)
}

/// ファイルシステムのタイプを取得
fn get_filesystem_type(path: &str) -> String {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("df")
            .args(&["-T", path])
            .output();
            
        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = output_str.lines().skip(1).collect(); // ヘッダー行をスキップ
                if let Some(line) = lines.first() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        return parts[1].to_string();
                    }
                }
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("df")
            .args(&["-T", path])
            .output();
            
        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = output_str.lines().skip(1).collect(); // ヘッダー行をスキップ
                if let Some(line) = lines.first() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        return parts[1].to_string();
                    }
                }
            }
        }
    }
    
    // デフォルト値または失敗した場合
    "unknown".to_string()
}

/// サイズを人間が読みやすい形式にフォーマット
fn format_size_human_readable(size: u64) -> String {
    let byte = Byte::from_bytes(size as u128);
    
    if size >= 1024 * 1024 * 1024 {
        format!("{:.1}G", byte.get_adjusted_unit(ByteUnit::GB))
    } else if size >= 1024 * 1024 {
        format!("{:.1}M", byte.get_adjusted_unit(ByteUnit::MB))
    } else if size >= 1024 {
        format!("{:.1}K", byte.get_adjusted_unit(ByteUnit::KB))
    } else {
        format!("{}B", size)
    }
} 