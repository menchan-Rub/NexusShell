use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::fs::File;
use std::io::{self, BufReader, BufRead};
use std::path::Path;
use tracing::{debug, error, info};
use regex::Regex;

/// テキストファイル内でパターンを検索するコマンド
pub struct GrepCommand;

#[async_trait]
impl BuiltinCommand for GrepCommand {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "テキストファイル内でパターンを検索する"
    }

    fn usage(&self) -> &'static str {
        "使用法: grep [-i] [-v] [-n] [-c] <パターン> [ファイル...]\n\
        \n\
        オプション:\n\
        -i          大文字小文字を区別しない\n\
        -v          パターンに一致しない行を表示する\n\
        -n          一致した行の行番号を表示する\n\
        -c          一致した行数のみを表示する\n\
        \n\
        ファイルが指定されていない場合は標準入力から読み込む"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut case_insensitive = false;
        let mut invert_match = false;
        let mut line_number = false;
        let mut count_only = false;
        let mut pattern = None;
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
                // -ivnc のような複合オプション対応
                for c in arg.chars().skip(1) {
                    match c {
                        'i' => case_insensitive = true,
                        'v' => invert_match = true,
                        'n' => line_number = true,
                        'c' => count_only = true,
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else if pattern.is_none() {
                pattern = Some(arg.clone());
            } else {
                files.push(arg.clone());
            }
        }

        // パターンが指定されているか確認
        let pattern = match pattern {
            Some(p) => p,
            None => {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 検索パターンが指定されていません\n".into_bytes()));
            }
        };

        // 正規表現を準備
        let regex_pattern = if case_insensitive {
            format!("(?i){}", pattern)
        } else {
            pattern.clone()
        };

        let regex = match Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(e) => {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 無効な正規表現: {}\n", e).into_bytes()));
            }
        };

        let mut result = CommandResult::success();
        let mut output = Vec::new();

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            if !context.stdin_connected {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 標準入力が接続されていません".into_bytes()));
            }
            
            // 標準入力の処理はスタブとして残す
            // 本来は stdin から読み込むべきだが、このコードでは空文字列を返す
            let content = String::new();
            process_content(&content, &regex, invert_match, line_number, count_only, &mut output);
        } else {
            // 各ファイルを処理
            for file_path in &files {
                let path = context.current_dir.join(file_path);
                
                match process_file(&path, &regex, invert_match, line_number, count_only, &mut output, files.len() > 1, file_path) {
                    Ok(_) => (),
                    Err(err) => {
                        let err_msg = format!("エラー: {}: {}\n", file_path, err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        result.exit_code = 1;
                    }
                }
            }
        }

        result.stdout = output;
        Ok(result)
    }
}

// ファイルを処理する関数
fn process_file(
    path: &Path, 
    regex: &Regex,
    invert_match: bool,
    line_number: bool,
    count_only: bool,
    output: &mut Vec<u8>,
    print_filename: bool,
    filename: &str,
) -> io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    if count_only {
        // 一致する行数をカウントするモード
        let mut count = 0;
        for line in reader.lines() {
            let line = line?;
            let matches = regex.is_match(&line);
            if matches != invert_match {
                count += 1;
            }
        }
        
        if print_filename {
            output.extend_from_slice(format!("{}:{}\n", filename, count).as_bytes());
        } else {
            output.extend_from_slice(format!("{}\n", count).as_bytes());
        }
    } else {
        // 一致する行を表示するモード
        for (i, line_result) in reader.lines().enumerate() {
            let line = line_result?;
            let matches = regex.is_match(&line);
            
            if matches != invert_match {
                let line_num = i + 1;
                
                if print_filename {
                    if line_number {
                        output.extend_from_slice(format!("{}:{}:{}\n", filename, line_num, line).as_bytes());
                    } else {
                        output.extend_from_slice(format!("{}:{}\n", filename, line).as_bytes());
                    }
                } else {
                    if line_number {
                        output.extend_from_slice(format!("{}:{}\n", line_num, line).as_bytes());
                    } else {
                        output.extend_from_slice(format!("{}\n", line).as_bytes());
                    }
                }
            }
        }
    }
    
    Ok(())
}

// 文字列コンテンツを処理する関数（標準入力用）
fn process_content(
    content: &str, 
    regex: &Regex,
    invert_match: bool,
    line_number: bool,
    count_only: bool,
    output: &mut Vec<u8>,
) {
    if count_only {
        // 一致する行数をカウントするモード
        let mut count = 0;
        for line in content.lines() {
            let matches = regex.is_match(line);
            if matches != invert_match {
                count += 1;
            }
        }
        
        output.extend_from_slice(format!("{}\n", count).as_bytes());
    } else {
        // 一致する行を表示するモード
        for (i, line) in content.lines().enumerate() {
            let matches = regex.is_match(line);
            
            if matches != invert_match {
                let line_num = i + 1;
                
                if line_number {
                    output.extend_from_slice(format!("{}:{}\n", line_num, line).as_bytes());
                } else {
                    output.extend_from_slice(format!("{}\n", line).as_bytes());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_grep_basic_match() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nLine 2\nLine 3\nTest line\nFinal line\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行
        let command = GrepCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "grep".to_string(),
                "Line".to_string(),
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
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
        assert!(!output.contains("Test line"));
        assert!(!output.contains("Final line"));
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "HELLO\nWorld\nhello\nworld\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（大文字小文字を区別しない）
        let command = GrepCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "grep".to_string(),
                "-i".to_string(),
                "hello".to_string(),
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
        assert!(output.contains("HELLO"));
        assert!(output.contains("hello"));
        assert!(!output.contains("World"));
        assert!(!output.contains("world"));
    }

    #[tokio::test]
    async fn test_grep_line_number() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nTest\nLine 3\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（行番号表示）
        let command = GrepCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "grep".to_string(),
                "-n".to_string(),
                "Line".to_string(),
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
        assert!(output.contains("1:Line 1"));
        assert!(output.contains("3:Line 3"));
        assert!(!output.contains("2:Test"));
    }

    #[tokio::test]
    async fn test_grep_count_only() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nTest\nLine 3\nTest line\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（カウントのみ）
        let command = GrepCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "grep".to_string(),
                "-c".to_string(),
                "Line".to_string(),
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
        assert_eq!(output.trim(), "3");
    }

    #[tokio::test]
    async fn test_grep_invert_match() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nTest\nLine 3\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（反転マッチ）
        let command = GrepCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "grep".to_string(),
                "-v".to_string(),
                "Line".to_string(),
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
        assert!(output.contains("Test"));
        assert!(!output.contains("Line 1"));
        assert!(!output.contains("Line 3"));
    }

    #[tokio::test]
    async fn test_grep_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイル1を作成
        let file1_path = temp_path.join("test1.txt");
        let content1 = "File1 Line 1\nFile1 Test\n";
        let mut file1 = File::create(&file1_path).unwrap();
        write!(file1, "{}", content1).unwrap();

        // テスト用ファイル2を作成
        let file2_path = temp_path.join("test2.txt");
        let content2 = "File2 Test\nFile2 Line 2\n";
        let mut file2 = File::create(&file2_path).unwrap();
        write!(file2, "{}", content2).unwrap();

        // コマンドを実行（複数ファイル）
        let command = GrepCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "grep".to_string(),
                "Line".to_string(),
                file1_path.file_name().unwrap().to_str().unwrap().to_string(),
                file2_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // 出力が期待通りであることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("test1.txt:File1 Line 1"));
        assert!(output.contains("test2.txt:File2 Line 2"));
        assert!(!output.contains("File1 Test"));
        assert!(!output.contains("File2 Test"));
    }
} 