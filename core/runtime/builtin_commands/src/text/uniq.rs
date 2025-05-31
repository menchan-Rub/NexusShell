use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use tracing::{debug, error, info};

/// テキストファイルの重複行を除去するコマンド
pub struct UniqCommand;

#[async_trait]
impl BuiltinCommand for UniqCommand {
    fn name(&self) -> &'static str {
        "uniq"
    }

    fn description(&self) -> &'static str {
        "重複する行を検出または除去する"
    }

    fn usage(&self) -> &'static str {
        "使用法: uniq [オプション] [入力ファイル [出力ファイル]]\n\
        \n\
        オプション:\n\
        -c, --count           行の出現回数を先頭に付ける\n\
        -d, --repeated        重複する行のみを出力\n\
        -u, --unique          重複しない行のみを出力\n\
        -i, --ignore-case     大文字小文字を区別せずに比較\n\
        -f, --skip-fields=N   比較時に最初のN個のフィールドをスキップ\n\
        -s, --skip-chars=N    比較時に最初のN文字をスキップ\n\
        \n\
        入力ファイルが指定されない場合は標準入力から読み込み、\n\
        出力ファイルが指定されない場合は標準出力に書き出す。\n\
        uniqは通常、ソートされた入力で動作することを想定している。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut show_counts = false;
        let mut only_duplicated = false;
        let mut only_unique = false;
        let mut ignore_case = false;
        let mut skip_fields = 0;
        let mut skip_chars = 0;
        let mut input_file = None;
        let mut output_file = None;

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg == "-c" || arg == "--count" {
                show_counts = true;
            } else if arg == "-d" || arg == "--repeated" {
                only_duplicated = true;
            } else if arg == "-u" || arg == "--unique" {
                only_unique = true;
            } else if arg == "-i" || arg == "--ignore-case" {
                ignore_case = true;
            } else if arg == "-f" || arg == "--skip-fields" {
                if let Some(value) = args.next() {
                    skip_fields = value.parse::<usize>().unwrap_or_else(|_| {
                        debug!("Invalid value for skip-fields: {}", value);
                        0
                    });
                }
            } else if arg.starts_with("--skip-fields=") {
                let value = &arg[14..];
                skip_fields = value.parse::<usize>().unwrap_or_else(|_| {
                    debug!("Invalid value for skip-fields: {}", value);
                    0
                });
            } else if arg == "-s" || arg == "--skip-chars" {
                if let Some(value) = args.next() {
                    skip_chars = value.parse::<usize>().unwrap_or_else(|_| {
                        debug!("Invalid value for skip-chars: {}", value);
                        0
                    });
                }
            } else if arg.starts_with("--skip-chars=") {
                let value = &arg[13..];
                skip_chars = value.parse::<usize>().unwrap_or_else(|_| {
                    debug!("Invalid value for skip-chars: {}", value);
                    0
                });
            } else if !arg.starts_with('-') {
                if input_file.is_none() {
                    input_file = Some(arg.clone());
                } else if output_file.is_none() {
                    output_file = Some(arg.clone());
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: 余分な引数 '{}'\n{}", arg, self.usage()).into_bytes()));
                }
            } else {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 不明なオプション: {}\n{}", arg, self.usage()).into_bytes()));
            }
        }

        // -d と -u が同時に指定された場合はエラー
        if only_duplicated && only_unique {
            return Ok(CommandResult::failure(1)
                .with_stderr("エラー: --repeated と --unique オプションは同時に使用できません\n".into_bytes()));
        }

        let mut result = CommandResult::success();

        // 入力ファイルの処理
        if let Some(file_path) = input_file {
            let path = context.current_dir.join(&file_path);
            if !path.exists() {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: ファイル '{}' が見つかりません\n", file_path).into_bytes()));
            }

            // 出力ファイルが指定されている場合
            if let Some(out_path) = output_file {
                let out_file_path = context.current_dir.join(&out_path);
                match File::create(&out_file_path) {
                    Ok(mut file) => {
                        match process_file(&path, &mut file, show_counts, only_duplicated, only_unique, 
                                          ignore_case, skip_fields, skip_chars) {
                            Ok(_) => (),
                            Err(err) => {
                                let err_msg = format!("エラー: {}: {}\n", file_path, err);
                                result.stderr.extend_from_slice(err_msg.as_bytes());
                                result.exit_code = 1;
                            }
                        }
                    },
                    Err(err) => {
                        let err_msg = format!("エラー: 出力ファイル '{}' を作成できません: {}\n", out_path, err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        result.exit_code = 1;
                    }
                }
            } else {
                // 出力ファイルが指定されていない場合は標準出力に
                match process_file(&path, &mut result.stdout, show_counts, only_duplicated, only_unique, 
                                  ignore_case, skip_fields, skip_chars) {
                    Ok(_) => (),
                    Err(err) => {
                        let err_msg = format!("エラー: {}: {}\n", file_path, err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        result.exit_code = 1;
                    }
                }
            }
        } else {
            // 標準入力から本物のデータを読み込む
            let stdin = io::stdin();
            let mut stdout = io::stdout();
            let mut prev_line = String::new();
            for line in stdin.lock().lines() {
                let line = line?;
                if line != prev_line {
                    writeln!(stdout, "{}", line)?;
                    prev_line = line;
                }
            }

            // 出力ファイルが指定されていない場合は標準出力に
            match process_string(&String::new(), &mut result.stdout, show_counts, only_duplicated, only_unique, 
                               ignore_case, skip_fields, skip_chars) {
                Ok(_) => (),
                Err(err) => {
                    let err_msg = format!("エラー: {}\n", err);
                    result.stderr.extend_from_slice(err_msg.as_bytes());
                    result.exit_code = 1;
                }
            }
        }
        
        Ok(result)
    }
}

/// ファイルを処理する関数
fn process_file<W: Write>(
    path: &Path, 
    output: &mut W,
    show_counts: bool,
    only_duplicated: bool,
    only_unique: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
) -> io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines()
        .collect::<Result<Vec<_>, _>>()?;
    
    process_lines(&lines, output, show_counts, only_duplicated, only_unique, 
                 ignore_case, skip_fields, skip_chars)
}

/// 文字列を処理する関数
fn process_string<W: Write>(
    input: &str,
    output: &mut W,
    show_counts: bool,
    only_duplicated: bool,
    only_unique: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
) -> io::Result<()> {
    let lines: Vec<String> = input.lines().map(String::from).collect();
    
    process_lines(&lines, output, show_counts, only_duplicated, only_unique, 
                 ignore_case, skip_fields, skip_chars)
}

/// 行を処理する関数
fn process_lines<W: Write>(
    lines: &[String],
    output: &mut W,
    show_counts: bool,
    only_duplicated: bool,
    only_unique: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
) -> io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    
    let mut line_counts: HashMap<String, usize> = HashMap::new();
    let mut processed_lines: Vec<String> = Vec::new();
    let mut last_key = String::new();
    
    for line in lines {
        let key = get_comparison_key(line, ignore_case, skip_fields, skip_chars);
        
        if key != last_key || processed_lines.is_empty() {
            processed_lines.push(line.clone());
            line_counts.insert(key.clone(), 1);
            last_key = key;
        } else {
            // 直前の行と同じキーを持つ場合、カウントを更新
            if let Some(count) = line_counts.get_mut(&key) {
                *count += 1;
            }
        }
    }
    
    // 出力
    for (i, line) in processed_lines.iter().enumerate() {
        let key = get_comparison_key(line, ignore_case, skip_fields, skip_chars);
        let count = *line_counts.get(&key).unwrap_or(&0);
        
        // フィルタリング条件に基づいて出力
        let should_output = if only_duplicated {
            count > 1
        } else if only_unique {
            count == 1
        } else {
            true
        };
        
        if should_output {
            if show_counts {
                writeln!(output, "{:7} {}", count, line)?;
            } else {
                writeln!(output, "{}", line)?;
            }
        }
    }
    
    Ok(())
}

/// 比較用のキーを生成する関数
fn get_comparison_key(line: &str, ignore_case: bool, skip_fields: usize, skip_chars: usize) -> String {
    let mut result = line.to_string();
    
    // フィールドをスキップ
    if skip_fields > 0 {
        let fields: Vec<&str> = result.split_whitespace().collect();
        if fields.len() > skip_fields {
            result = fields[skip_fields..].join(" ");
        } else {
            result = String::new();
        }
    }
    
    // 文字をスキップ
    if skip_chars > 0 && result.len() > skip_chars {
        result = result[skip_chars..].to_string();
    } else if skip_chars > 0 {
        result = String::new();
    }
    
    // 大文字小文字を区別しない場合
    if ignore_case {
        result = result.to_lowercase();
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_uniq_basic() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "line1\nline1\nline2\nline3\nline3\nline3\nline4\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（基本的な重複除去）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        let expected = "line1\nline2\nline3\nline4\n";
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn test_uniq_count() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "line1\nline1\nline2\nline3\nline3\nline3\nline4\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（出現回数を表示）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "-c".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        // 出力例: "      2 line1\n      1 line2\n      3 line3\n      1 line4\n"
        assert!(output.trim().contains("2 line1"));
        assert!(output.trim().contains("1 line2"));
        assert!(output.trim().contains("3 line3"));
        assert!(output.trim().contains("1 line4"));
    }

    #[tokio::test]
    async fn test_uniq_duplicated() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "line1\nline1\nline2\nline3\nline3\nline3\nline4\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（重複行のみを表示）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "-d".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("line1"));
        assert!(output.contains("line3"));
        assert!(!output.contains("line2"));
        assert!(!output.contains("line4"));
    }

    #[tokio::test]
    async fn test_uniq_unique() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "line1\nline1\nline2\nline3\nline3\nline3\nline4\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（一意な行のみを表示）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "-u".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(!output.contains("line1"));
        assert!(output.contains("line2"));
        assert!(!output.contains("line3"));
        assert!(output.contains("line4"));
    }

    #[tokio::test]
    async fn test_uniq_ignore_case() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line1\nline1\nLINE2\nline2\nline3\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（大文字小文字を区別しない）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "-i".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 3); // "Line1", "LINE2", "line3"
    }

    #[tokio::test]
    async fn test_uniq_skip_fields() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "1 apple fruit\n2 apple fruit\n3 banana fruit\n4 apple fruit\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（最初のフィールドをスキップ）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "-f".to_string(),
                "1".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2); // "1 apple fruit", "3 banana fruit"
    }

    #[tokio::test]
    async fn test_uniq_skip_chars() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "AAApple\nBBBpple\nCCCrange\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（最初の3文字をスキップ）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "-s".to_string(),
                "3".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2); // "AAApple", "CCCrange"
    }

    #[tokio::test]
    async fn test_uniq_output_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用入力ファイルを作成
        let input_path = temp_path.join("input.txt");
        let output_path = temp_path.join("output.txt");
        let content = "line1\nline1\nline2\nline3\nline3\nline3\nline4\n";
        let mut file = File::create(&input_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（出力ファイルを指定）
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                input_path.file_name().unwrap().to_str().unwrap().to_string(),
                output_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力ファイルが期待通りであることを確認
        let output_content = std::fs::read_to_string(&output_path).unwrap();
        let expected = "line1\nline2\nline3\nline4\n";
        assert_eq!(output_content, expected);
    }

    #[tokio::test]
    async fn test_uniq_error_handling() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 存在しないファイルを指定
        let command = UniqCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "uniq".to_string(),
                "nonexistent_file.txt".to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        // エラーメッセージが期待通りであることを確認
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("見つかりません"));
    }
} 