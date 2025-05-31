use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};
use filetime;
use chrono::{NaiveDateTime, NaiveDate, NaiveTime, Local, TimeZone, Utc};

/// ファイルを作成したりタイムスタンプを更新するコマンド
pub struct TouchCommand;

#[async_trait]
impl BuiltinCommand for TouchCommand {
    fn name(&self) -> &'static str {
        "touch"
    }

    fn description(&self) -> &'static str {
        "ファイルを作成したりタイムスタンプを更新する"
    }

    fn usage(&self) -> &'static str {
        "使用法: touch [-a] [-m] [-c] [-r REF_FILE] [-t TIMESTAMP] <ファイル...>\n\
        \n\
        オプション:\n\
        -a          アクセス時間のみ変更\n\
        -m          修正時間のみ変更\n\
        -c          ファイルが存在しない場合は作成しない\n\
        -r REF_FILE 指定したファイルと同じタイムスタンプを使用\n\
        -t TIMESTAMP [[CC]YY]MMDDhhmm[.ss] 形式でタイムスタンプを指定"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 2 {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: 引数が不足しています\n{}", self.usage()).into_bytes()));
        }

        let mut args = context.args.iter().skip(1);
        let mut access_time = true;
        let mut modification_time = true;
        let mut no_create = false;
        let mut reference_file = None;
        let mut timestamp = None;
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
                for c in arg.chars().skip(1) {
                    match c {
                        'a' => {
                            access_time = true;
                            modification_time = false;
                        },
                        'm' => {
                            access_time = false;
                            modification_time = true;
                        },
                        'c' => no_create = true,
                        'r' => {
                            reference_file = args.next().map(|s| s.to_string());
                            if reference_file.is_none() {
                                return Ok(CommandResult::failure(1)
                                    .with_stderr("エラー: -r オプションには参照ファイルが必要です".into_bytes()));
                            }
                        },
                        't' => {
                            timestamp = args.next().map(|s| s.to_string());
                            if timestamp.is_none() {
                                return Ok(CommandResult::failure(1)
                                    .with_stderr("エラー: -t オプションにはタイムスタンプが必要です".into_bytes()));
                            }
                        },
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else if arg == "--access" {
                access_time = true;
                modification_time = false;
            } else if arg == "--modification" {
                access_time = false;
                modification_time = true;
            } else if arg == "--no-create" {
                no_create = true;
            } else if arg == "--reference" {
                reference_file = args.next().map(|s| s.to_string());
                if reference_file.is_none() {
                    return Ok(CommandResult::failure(1)
                        .with_stderr("エラー: --reference オプションには参照ファイルが必要です".into_bytes()));
                }
            } else if arg == "--timestamp" {
                timestamp = args.next().map(|s| s.to_string());
                if timestamp.is_none() {
                    return Ok(CommandResult::failure(1)
                        .with_stderr("エラー: --timestamp オプションにはタイムスタンプが必要です".into_bytes()));
                }
            } else {
                files.push(arg.to_string());
            }
        }

        if files.is_empty() {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: ファイルが指定されていません\n{}", self.usage()).into_bytes()));
        }

        // 参照ファイルまたは指定タイムスタンプから基準となる atime, mtime を決定
        let base_times: Option<(SystemTime, SystemTime)> = if let Some(ref_file_name) = reference_file {
            let ref_path = context.current_dir.join(&ref_file_name); // ref_file_name を借用
            if !ref_path.exists() {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 参照ファイル '{}' が存在しません", ref_file_name).into_bytes()));
            }
            let metadata = fs::metadata(&ref_path).with_context(|| format!("参照ファイル '{}' のメタデータ取得失敗", ref_path.display()))?;
            Some((
                metadata.accessed().with_context(|| format!("参照ファイル '{}' のアクセス時刻取得失敗", ref_path.display()))?,
                metadata.modified().with_context(|| format!("参照ファイル '{}' の修正時刻取得失敗", ref_path.display()))?,
            ))
        } else if let Some(ts_str) = timestamp {
            match parse_timestamp(&ts_str) {
                Ok(parsed_time) => Some((parsed_time, parsed_time)),
                Err(e) => {
                    let error_message = format!("エラー: -t のタイムスタンプ解析失敗 '{}': {}\n", ts_str, e);
                    let mut result = CommandResult::failure(1);
                    result.stderr.extend_from_slice(error_message.as_bytes());
                    return Ok(result);
                }
            }
        } else {
            let now = SystemTime::now();
            Some((now, now))
        };

        let mut result = CommandResult::success();

        let mut overall_success = true;

        for file_name in files {
            let file_path = context.current_dir.join(&file_name);
            
            if !file_path.exists() {
                if no_create {
                    debug!("ファイル '{}' が存在せず、-c オプションが指定されているためスキップします", file_name);
                    continue;
                }
                match File::create(&file_path) {
                    Ok(_) => debug!("ファイル '{}' を作成しました", file_name),
                    Err(err) => {
                        let err_msg = format!("エラー: ファイル '{}' の作成に失敗: {}\n", file_name, err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        overall_success = false;
                        continue;
                    }
                }
            }

            if let Some((base_atime, base_mtime)) = base_times {
                let current_meta = fs::metadata(&file_path).ok(); // 既存のタイムスタンプ取得用

                let atime_to_set: Option<SystemTime> = if access_time {
                    Some(base_atime)
                } else if modification_time { // -mのみ指定で-aなしの場合、atimeは変更しない
                    current_meta.as_ref().and_then(|m| m.accessed().ok())
                } else { // -aも-mも指定されていない場合 (フラグなしtouch) は両方更新
                     Some(base_atime)
                };

                let mtime_to_set: Option<SystemTime> = if modification_time {
                    Some(base_mtime)
                } else if access_time { // -aのみ指定で-mなしの場合、mtimeは変更しない
                    current_meta.as_ref().and_then(|m| m.modified().ok())
                } else { // フラグなしtouch
                    Some(base_mtime)
                };
                
                // access_time と modification_time が両方trueのデフォルトケースも上記でカバーされる
                // (両方Some(base_atime/mtime)になる)

                let mut action_failed = false;
                if let Some(atime_val) = atime_to_set {
                    if let Err(e) = filetime::set_file_atime(&file_path, system_time_to_file_time(atime_val)) {
                        let err_msg = format!("エラー: ファイル '{}' のアクセス時刻更新に失敗: {}\n", file_name, e);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        action_failed = true;
                    } else {
                        debug!("ファイル '{}' のアクセス時刻を更新 ({:?})", file_name, atime_val);
                    }
                }

                if !action_failed { // アクセス時刻更新が成功した場合のみ修正時刻更新へ
                    if let Some(mtime_val) = mtime_to_set {
                        if let Err(e) = filetime::set_file_mtime(&file_path, system_time_to_file_time(mtime_val)) {
                            let err_msg = format!("エラー: ファイル '{}' の修正時刻更新に失敗: {}\n", file_name, e);
                            result.stderr.extend_from_slice(err_msg.as_bytes());
                            action_failed = true;
                        } else {
                            debug!("ファイル '{}' の修正時刻を更新 ({:?})", file_name, mtime_val);
                        }
                    }
                }
                if action_failed {
                    overall_success = false;
                }
            }
        }
        
        if !overall_success {
            result.exit_code = 1;
        }

        Ok(result)
    }
}

/// ファイルのタイムスタンプを完全同期（WSL/BSD/ネットワークFSも考慮）
fn synchronize_timestamps_cross_platform(src_path: &Path, dest_path: &Path) -> Result<()> {
    use filetime::{FileTime, set_file_times};
    use std::fs;
    #[cfg(target_os = "windows")]
    use std::os::windows::fs::MetadataExt;
    #[cfg(target_os = "unix")]
    use std::os::unix::fs::MetadataExt;

    // 参照ファイルのタイムスタンプを取得
    let src_metadata = fs::metadata(src_path)
        .map_err(|e| anyhow!("参照ファイルのメタデータ取得に失敗: {}", e))?;

    // アクセス時間と変更時間を取得
    let src_atime = FileTime::from_last_access_time(&src_metadata);
    let src_mtime = FileTime::from_last_modification_time(&src_metadata);

    // ネットワークFSやWSL環境では一部APIが失敗する場合があるためリトライ
    let mut last_err = None;
    for _ in 0..3 {
        match set_file_times(dest_path, src_atime, src_mtime) {
            Ok(_) => {
                debug!("タイムスタンプを完全同期: {} -> {}", src_path.display(), dest_path.display());
                return Ok(());
            },
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    Err(anyhow!("タイムスタンプの完全同期に失敗: {}", last_err.unwrap()))
}

/// SystemTime を FileTime に変換
fn system_time_to_file_time(time: SystemTime) -> filetime::FileTime {
    use std::time::{Duration, UNIX_EPOCH};
    
    // SystemTime を UNIX エポックからの経過秒数に変換
    let duration = time.duration_since(UNIX_EPOCH)
                       .unwrap_or(Duration::from_secs(0));
    
    // FileTime に変換（秒とナノ秒に分解）
    let seconds = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos();
    
    filetime::FileTime::from_unix_time(seconds, nanos)
}

/// 指定されたタイムスタンプでファイルを更新
fn update_file_timestamp(path: &Path, time: SystemTime) -> Result<()> {
    use filetime::set_file_times;
    
    // SystemTime を FileTime に変換
    let file_time = system_time_to_file_time(time);
    
    // アクセス時間と変更時間を設定
    set_file_times(path, file_time, file_time)
        .map_err(|e| anyhow!("タイムスタンプの設定に失敗: {}", e))?;
    
    debug!("タイムスタンプを更新しました: {}", path.display());
    Ok(())
}

/// ファイルの参照タイムスタンプを設定
fn set_reference_timestamp(target_path: &Path, reference_path: &Path) -> Result<()> {
    // 参照ファイルと対象ファイルのパスが同じ場合は何もしない
    if target_path == reference_path {
        return Ok(());
    }
    
    // 参照ファイルが存在しない場合はエラー
    if !reference_path.exists() {
        return Err(anyhow!("参照ファイルが存在しません: {}", reference_path.display()));
    }
    
    // 対象ファイルが存在しない場合はエラー
    if !target_path.exists() {
        return Err(anyhow!("対象ファイルが存在しません: {}", target_path.display()));
    }
    
    // タイムスタンプを同期
    synchronize_timestamps_cross_platform(reference_path, target_path)?;
    
    Ok(())
}

fn parse_timestamp(timestamp_str: &str) -> Result<SystemTime> {
    let (datetime_part, sec_part) = if let Some(pos) = timestamp_str.rfind('.') {
        let (main, sec_maybe) = timestamp_str.split_at(pos);
        (main, Some(&sec_maybe[1..]))
    } else {
        (timestamp_str, None)
    };

    let len = datetime_part.len();
    // MMDDhhmm (8), YYMMDDhhmm (10), CCYYMMDDhhmm (12)
    if ![8, 10, 12].contains(&len) {
        return Err(anyhow!("タイムスタンプ形式が無効 (len={}). 正しい形式: [[CC]YY]MMDDhhmm[.ss]", len));
    }

    let mut year_str = "";
    let mut month_str = "";
    let mut day_str = "";
    let mut hour_str = "";
    let mut minute_str = "";

    let current_year_full = Local::now().year();
    let mut year: i32 = current_year_full;

    if len == 12 { // CCYYMMDDhhmm
        year_str = &datetime_part[0..4];
        month_str = &datetime_part[4..6];
        day_str = &datetime_part[6..8];
        hour_str = &datetime_part[8..10];
        minute_str = &datetime_part[10..12];
        year = year_str.parse().context("年の解析失敗(CCYY)")?;
    } else if len == 10 { // YYMMDDhhmm
        year_str = &datetime_part[0..2];
        month_str = &datetime_part[2..4];
        day_str = &datetime_part[4..6];
        hour_str = &datetime_part[6..8];
        minute_str = &datetime_part[8..10];
        let short_year: i32 = year_str.parse().context("年の解析失敗(YY)")?;
        year = if short_year >= 69 { 1900 + short_year } else { 2000 + short_year };
    } else { // MMDDhhmm
        month_str = &datetime_part[0..2];
        day_str = &datetime_part[2..4];
        hour_str = &datetime_part[4..6];
        minute_str = &datetime_part[6..8];
        // year は current_year_full のまま
    }
    
    let month: u32 = month_str.parse().context("月の解析失敗")?;
    let day: u32 = day_str.parse().context("日の解析失敗")?;
    let hour: u32 = hour_str.parse().context("時の解析失敗")?;
    let minute: u32 = minute_str.parse().context("分の解析失敗")?;
    let mut second: u32 = 0;

    if let Some(s_str) = sec_part {
        if !s_str.is_empty() { // . の後に空文字列が来ることがあり得る
             second = s_str.parse().context("秒の解析失敗")?;
             if second > 59 { 
                 return Err(anyhow!("秒の値が無効です (0-59)"));
             }
        }
    }

    let naive_dt = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_opt(hour, minute, second))
        .ok_or_else(|| anyhow!("日付または時刻の値が無効です ({}-{}-{} {}:{}:{})", year, month, day, hour, minute, second))?;

    match Local.from_local_datetime(&naive_dt).single() {
        Some(dt) => Ok(dt.into()),
        None => Err(anyhow!("ローカル日時の曖昧性のため SystemTime に変換できません"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_touch_create_new_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 新しいファイル名
        let new_file = "new_file.txt";
        let file_path = temp_path.join(new_file);
        
        // ファイルが存在しないことを確認
        assert!(!file_path.exists());

        // コマンドを実行
        let command = TouchCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "touch".to_string(),
                new_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ファイルが作成されたことを確認
        assert!(file_path.exists());
        
        // ファイルが空であることを確認
        let metadata = fs::metadata(&file_path).unwrap();
        assert_eq!(metadata.len(), 0);
    }

    #[tokio::test]
    async fn test_touch_update_existing_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 既存ファイルを作成
        let existing_file = "existing_file.txt";
        let file_path = temp_path.join(existing_file);
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "テストデータ").unwrap();
        
        // 少し待ってタイムスタンプの差を確実にする
        thread::sleep(Duration::from_millis(100));
        
        // 最初のタイムスタンプを記録
        let initial_metadata = fs::metadata(&file_path).unwrap();
        let initial_modified = initial_metadata.modified().unwrap();
        
        // さらに少し待つ
        thread::sleep(Duration::from_millis(1000));

        // コマンドを実行
        let command = TouchCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "touch".to_string(),
                existing_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ファイルが存在することを確認
        assert!(file_path.exists());
        
        // タイムスタンプが更新されたことを確認
        let new_metadata = fs::metadata(&file_path).unwrap();
        let new_modified = new_metadata.modified().unwrap();
        
        // タイムスタンプが更新されていることを確認
        // 注: このテストはシステムのクロック解像度に依存します
        assert!(new_modified > initial_modified);
        
        // ファイルの内容が変更されていないことを確認
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "テストデータ\n");
    }

    #[tokio::test]
    async fn test_touch_no_create() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 存在しないファイル名
        let nonexistent_file = "nonexistent.txt";
        let file_path = temp_path.join(nonexistent_file);
        
        // ファイルが存在しないことを確認
        assert!(!file_path.exists());

        // コマンドを実行 (-c オプションあり)
        let command = TouchCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "touch".to_string(),
                "-c".to_string(),
                nonexistent_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // -c オプションによりファイルが作成されていないことを確認
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_touch_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 複数のファイル名
        let file_names = vec!["file1.txt", "file2.txt", "file3.txt"];
        let mut file_paths = Vec::new();
        
        for name in &file_names {
            file_paths.push(temp_path.join(name));
        }
        
        // ファイルが存在しないことを確認
        for path in &file_paths {
            assert!(!path.exists());
        }

        // コマンドを実行
        let command = TouchCommand;
        let mut args = vec!["touch".to_string()];
        args.extend(file_names.iter().map(|s| s.to_string()));
        
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args,
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // すべてのファイルが作成されたことを確認
        for path in &file_paths {
            assert!(path.exists());
        }
    }

    #[tokio::test]
    async fn test_touch_reference_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 参照ファイルを作成
        let reference_file = "reference.txt";
        let ref_path = temp_path.join(reference_file);
        let mut file = File::create(&ref_path).unwrap();
        writeln!(file, "参照データ").unwrap();
        
        // 少し待つ
        thread::sleep(Duration::from_millis(100));
        
        // 参照ファイルのタイムスタンプを記録
        let ref_metadata = fs::metadata(&ref_path).unwrap();
        let ref_modified = ref_metadata.modified().unwrap();
        let ref_accessed = ref_metadata.accessed().unwrap();
        
        // ターゲットファイルを作成
        let target_file = "target.txt";
        let target_path = temp_path.join(target_file);
        let mut file = File::create(&target_path).unwrap();
        writeln!(file, "ターゲットデータ").unwrap();
        
        // 少し待ってタイムスタンプの差を確実にする
        thread::sleep(Duration::from_millis(1000));
        
        // 最初のターゲットファイルのタイムスタンプを記録
        let initial_target_metadata = fs::metadata(&target_path).unwrap();
        let initial_target_modified = initial_target_metadata.modified().unwrap();
        
        // タイムスタンプが異なることを確認
        assert!(initial_target_modified > ref_modified);

        // コマンドを実行
        let command = TouchCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "touch".to_string(),
                "-r".to_string(),
                reference_file.to_string(),
                target_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        
        // タイムスタンプが同期されたことを確認
        let updated_target_metadata = fs::metadata(&target_path).unwrap();
        let updated_target_modified = updated_target_metadata.modified().unwrap();
        let updated_target_accessed = updated_target_metadata.accessed().unwrap();
        
        // タイムスタンプを比較（ミリ秒レベルで一致）
        let modified_diff = updated_target_modified
            .duration_since(ref_modified)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_millis();
        
        let accessed_diff = updated_target_accessed
            .duration_since(ref_accessed)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_millis();
        
        // 誤差は小さいはず（システムによって若干の違いがある可能性）
        assert!(modified_diff < 10, "変更時刻が同期されていません");
        assert!(accessed_diff < 10, "アクセス時刻が同期されていません");
    }
} 