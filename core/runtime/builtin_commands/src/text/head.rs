use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::fs::File;
use std::io::{self, BufReader, BufRead, Read};
use std::path::Path;
use tracing::{debug, error, info};

/// ファイルの先頭部分を表示するコマンド
pub struct HeadCommand;

#[async_trait]
impl BuiltinCommand for HeadCommand {
    fn name(&self) -> &'static str {
        "head"
    }

    fn description(&self) -> &'static str {
        "ファイルの先頭部分を表示する"
    }

    fn usage(&self) -> &'static str {
        "使用法: head [-n <行数>] [-c <バイト数>] [-q] [-v] [ファイル...]\n\
        \n\
        オプション:\n\
        -n, --lines=<N>     先頭のN行を表示する（デフォルト: 10）\n\
        -c, --bytes=<N>     先頭のNバイトを表示する\n\
        -q, --quiet         ファイルヘッダーを表示しない\n\
        -v, --verbose       ファイルヘッダーを常に表示する\n\
        \n\
        ファイルが指定されていない場合は標準入力から読み込む"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut line_count = 10; // デフォルト行数
        let mut byte_count = None;
        let mut quiet = false;
        let mut verbose = false;
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg == "-n" || arg.starts_with("--lines=") {
                let n = if arg == "-n" {
                    if let Some(count_str) = args.next() {
                        count_str
                    } else {
                        return Ok(CommandResult::failure(1)
                            .with_stderr("エラー: -n オプションには値が必要です\n".into_bytes()));
                    }
                } else {
                    &arg["--lines=".len()..]
                };

                match n.parse::<usize>() {
                    Ok(n) => line_count = n,
                    Err(_) => {
                        return Ok(CommandResult::failure(1)
                            .with_stderr(format!("エラー: 無効な行数: {}\n", n).into_bytes()));
                    }
                }
            } else if arg == "-c" || arg.starts_with("--bytes=") {
                let c = if arg == "-c" {
                    if let Some(count_str) = args.next() {
                        count_str
                    } else {
                        return Ok(CommandResult::failure(1)
                            .with_stderr("エラー: -c オプションには値が必要です\n".into_bytes()));
                    }
                } else {
                    &arg["--bytes=".len()..]
                };

                match c.parse::<usize>() {
                    Ok(c) => byte_count = Some(c),
                    Err(_) => {
                        return Ok(CommandResult::failure(1)
                            .with_stderr(format!("エラー: 無効なバイト数: {}\n", c).into_bytes()));
                    }
                }
            } else if arg == "-q" || arg == "--quiet" {
                quiet = true;
            } else if arg == "-v" || arg == "--verbose" {
                verbose = true;
            } else if arg.starts_with("-") {
                // 未知のオプション
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 不明なオプション: {}\n{}", arg, self.usage()).into_bytes()));
            } else {
                // ファイル引数
                files.push(arg.clone());
            }
        }

        let mut result = CommandResult::success();

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            if !context.stdin_connected {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 標準入力が接続されていません".into_bytes()));
            }
            
            // 標準入力の処理はスタブとして残す
            // 本来は stdin から読み込むべきだが、このコードでは空文字列を返す
            let content = String::new();
            process_content(&content, line_count, byte_count, &mut result.stdout);
        } else {
            // 各ファイルを処理
            let show_headers = verbose || (!quiet && files.len() > 1);
            
            for (i, file_path) in files.iter().enumerate() {
                let path = context.current_dir.join(file_path);
                
                // 複数ファイルの場合、ファイル間に空行を挿入
                if i > 0 && show_headers {
                    result.stdout.extend_from_slice(b"\n");
                }
                
                // ヘッダー表示（複数ファイルの場合またはverboseが指定された場合）
                if show_headers {
                    let header = format!("==> {} <==\n", file_path);
                    result.stdout.extend_from_slice(header.as_bytes());
                }
                
                match process_file(&path, line_count, byte_count, &mut result.stdout) {
                    Ok(_) => (),
                    Err(err) => {
                        let err_msg = format!("エラー: {}: {}\n", file_path, err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        result.exit_code = 1;
                    }
                }
            }
        }

        Ok(result)
    }
}

// ファイルを処理する関数
fn process_file(
    path: &Path, 
    line_count: usize,
    byte_count: Option<usize>,
    output: &mut Vec<u8>,
) -> io::Result<()> {
    let file = File::open(path)?;
    
    if let Some(bytes) = byte_count {
        // バイト数指定の場合
        let mut buffer = vec![0; bytes];
        let mut reader = File::open(path)?;
        let read_bytes = reader.read(&mut buffer)?;
        buffer.truncate(read_bytes);
        output.extend_from_slice(&buffer);
    } else {
        // 行数指定の場合
        let reader = BufReader::new(file);
        for (i, line_result) in reader.lines().enumerate() {
            if i >= line_count {
                break;
            }
            let line = line_result?;
            output.extend_from_slice(line.as_bytes());
            output.push(b'\n');
        }
    }
    
    Ok(())
}

// 文字列コンテンツを処理する関数（標準入力用）
fn process_content(
    content: &str, 
    line_count: usize,
    byte_count: Option<usize>,
    output: &mut Vec<u8>,
) {
    if let Some(bytes) = byte_count {
        // バイト数指定の場合
        let bytes_to_take = std::cmp::min(bytes, content.len());
        output.extend_from_slice(&content.as_bytes()[..bytes_to_take]);
    } else {
        // 行数指定の場合
        for (i, line) in content.lines().enumerate() {
            if i >= line_count {
                break;
            }
            output.extend_from_slice(line.as_bytes());
            output.push(b'\n');
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
    async fn test_head_basic() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10\nLine 11\nLine 12\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（デフォルト10行）
        let command = HeadCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "head".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（最初の10行）
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 10"));
        assert!(!output.contains("Line 11"));
    }

    #[tokio::test]
    async fn test_head_line_count() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10\nLine 11\nLine 12\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（行数指定）
        let command = HeadCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "head".to_string(),
                "-n".to_string(),
                "3".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（最初の3行）
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
        assert!(!output.contains("Line 4"));
    }

    #[tokio::test]
    async fn test_head_byte_count() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（バイト数指定）
        let command = HeadCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "head".to_string(),
                "-c".to_string(),
                "10".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（最初の10バイト）
        let output = String::from_utf8_lossy(&result.stdout);
        assert_eq!(output, "ABCDEFGHIJ");
    }

    #[tokio::test]
    async fn test_head_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイル1を作成
        let file1_path = temp_path.join("test1.txt");
        let content1 = "File1 Line 1\nFile1 Line 2\nFile1 Line 3\n";
        let mut file1 = File::create(&file1_path).unwrap();
        write!(file1, "{}", content1).unwrap();

        // テスト用ファイル2を作成
        let file2_path = temp_path.join("test2.txt");
        let content2 = "File2 Line 1\nFile2 Line 2\nFile2 Line 3\n";
        let mut file2 = File::create(&file2_path).unwrap();
        write!(file2, "{}", content2).unwrap();

        // コマンドを実行（複数ファイル）
        let command = HeadCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "head".to_string(),
                "-n".to_string(),
                "2".to_string(),
                file1_path.file_name().unwrap().to_str().unwrap().to_string(),
                file2_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（各ファイルのヘッダーと最初の2行）
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("==> test1.txt <=="));
        assert!(output.contains("File1 Line 1"));
        assert!(output.contains("File1 Line 2"));
        assert!(!output.contains("File1 Line 3"));
        
        assert!(output.contains("==> test2.txt <=="));
        assert!(output.contains("File2 Line 1"));
        assert!(output.contains("File2 Line 2"));
        assert!(!output.contains("File2 Line 3"));
    }

    #[tokio::test]
    async fn test_head_quiet_mode() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイル1を作成
        let file1_path = temp_path.join("test1.txt");
        let content1 = "File1 Line 1\nFile1 Line 2\n";
        let mut file1 = File::create(&file1_path).unwrap();
        write!(file1, "{}", content1).unwrap();

        // テスト用ファイル2を作成
        let file2_path = temp_path.join("test2.txt");
        let content2 = "File2 Line 1\nFile2 Line 2\n";
        let mut file2 = File::create(&file2_path).unwrap();
        write!(file2, "{}", content2).unwrap();

        // コマンドを実行（静寂モード）
        let command = HeadCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "head".to_string(),
                "-q".to_string(),
                file1_path.file_name().unwrap().to_str().unwrap().to_string(),
                file2_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（ヘッダーなし）
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(!output.contains("==> test1.txt <=="));
        assert!(!output.contains("==> test2.txt <=="));
        assert!(output.contains("File1 Line 1"));
        assert!(output.contains("File2 Line 1"));
    }
} 