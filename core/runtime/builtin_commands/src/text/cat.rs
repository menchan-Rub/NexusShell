use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::fs::File;
use std::io::{self, Read, BufReader, BufRead};
use std::path::Path;
use tracing::{debug, error, info};
use tokio::io::AsyncReadExt;

/// ファイルの内容を表示するコマンド
pub struct CatCommand;

#[async_trait]
impl BuiltinCommand for CatCommand {
    fn name(&self) -> &'static str {
        "cat"
    }

    fn description(&self) -> &'static str {
        "ファイルの内容を表示する"
    }

    fn usage(&self) -> &'static str {
        "使用法: cat [-n] [-b] [-A] [ファイル...]\n\
        \n\
        オプション:\n\
        -n          すべての行に行番号を表示する\n\
        -b          空白行以外に行番号を表示する\n\
        -A          非表示文字を表示する (^G, $など)\n\
        \n\
        ファイルが指定されていない場合は標準入力から読み込む"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut show_all = false;
        let mut number_lines = false;
        let mut number_nonblank = false;
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
                // -nA のような複合オプション対応
                for c in arg.chars().skip(1) {
                    match c {
                        'A' => show_all = true,
                        'n' => number_lines = true,
                        'b' => number_nonblank = true,
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else if arg == "--show-all" {
                show_all = true;
            } else if arg == "--number" {
                number_lines = true;
            } else if arg == "--number-nonblank" {
                number_nonblank = true;
            } else {
                files.push(arg);
            }
        }

        // number_lines と number_nonblank の両方が指定された場合は number_nonblank を優先
        if number_lines && number_nonblank {
            number_lines = false;
        }

        let mut result = CommandResult::success();
        let mut output = Vec::new();

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            if !context.stdin_connected {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 標準入力が接続されていません".into_bytes()));
            }
            // 標準入力から全データを読み込む
            let mut stdin = tokio::io::stdin();
            let mut content = String::new();
            stdin.read_to_string(&mut content).await.unwrap_or(0);
            process_content(&content, show_all, number_lines, number_nonblank, &mut output);
        } else {
            // 各ファイルを処理
            let mut line_number = 1;
            
            for file_path in files {
                let path = context.current_dir.join(file_path);
                
                match process_file(&path, show_all, number_lines, number_nonblank, &mut output, &mut line_number) {
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
    show_all: bool, 
    number_lines: bool, 
    number_nonblank: bool,
    output: &mut Vec<u8>,
    line_number: &mut usize,
) -> io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    for line in reader.lines() {
        let line = line?;
        
        if number_lines {
            output.extend_from_slice(format!("{:6}\t", line_number).as_bytes());
            *line_number += 1;
        } else if number_nonblank && !line.trim().is_empty() {
            output.extend_from_slice(format!("{:6}\t", line_number).as_bytes());
            *line_number += 1;
        }
        
        if show_all {
            // 制御文字をわかりやすく表示
            for c in line.chars() {
                if c < ' ' && c != '\t' {
                    output.extend_from_slice(format!("^{}", (c as u8 + 64) as char).as_bytes());
                } else if c == '\t' {
                    output.extend_from_slice(b"\\t");
                } else if c == 127 {
                    output.extend_from_slice(b"^?");
                } else {
                    output.push(c as u8);
                }
            }
            output.extend_from_slice(b"$\n");
        } else {
            output.extend_from_slice(line.as_bytes());
            output.push(b'\n');
        }
    }
    
    Ok(())
}

// 文字列コンテンツを処理する関数（標準入力用）
fn process_content(
    content: &str, 
    show_all: bool, 
    number_lines: bool, 
    number_nonblank: bool,
    output: &mut Vec<u8>,
) {
    let mut line_number = 1;
    
    for line in content.lines() {
        if number_lines {
            output.extend_from_slice(format!("{:6}\t", line_number).as_bytes());
            line_number += 1;
        } else if number_nonblank && !line.trim().is_empty() {
            output.extend_from_slice(format!("{:6}\t", line_number).as_bytes());
            line_number += 1;
        }
        
        if show_all {
            // 制御文字をわかりやすく表示
            for c in line.chars() {
                if c < ' ' && c != '\t' {
                    output.extend_from_slice(format!("^{}", (c as u8 + 64) as char).as_bytes());
                } else if c == '\t' {
                    output.extend_from_slice(b"\\t");
                } else if c == 127 {
                    output.extend_from_slice(b"^?");
                } else {
                    output.push(c as u8);
                }
            }
            output.extend_from_slice(b"$\n");
        } else {
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
    async fn test_cat_single_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nLine 2\nLine 3\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行
        let command = CatCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cat".to_string(),
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
        assert_eq!(output, content);
    }

    #[tokio::test]
    async fn test_cat_with_line_numbers() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nLine 2\nLine 3\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行
        let command = CatCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cat".to_string(),
                "-n".to_string(),
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
        let expected = "     1\tLine 1\n     2\tLine 2\n     3\tLine 3\n";
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn test_cat_with_show_all() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成（制御文字を含む）
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nLine\twith\ttabs\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行
        let command = CatCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cat".to_string(),
                "-A".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // 出力に制御文字の表示が含まれていることを確認
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("\\t"));
        assert!(output.contains("$"));
    }

    #[tokio::test]
    async fn test_cat_nonexistent_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 存在しないファイルを指定
        let nonexistent_file = "nonexistent.txt";

        // コマンドを実行
        let command = CatCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cat".to_string(),
                nonexistent_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);

        // エラーメッセージに「エラー」が含まれていることを確認
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("エラー"));
    }

    #[tokio::test]
    async fn test_cat_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを複数作成
        let file1_path = temp_path.join("file1.txt");
        let file2_path = temp_path.join("file2.txt");
        
        let content1 = "File 1, Line 1\nFile 1, Line 2\n";
        let content2 = "File 2, Line 1\nFile 2, Line 2\n";
        
        let mut file1 = File::create(&file1_path).unwrap();
        write!(file1, "{}", content1).unwrap();
        
        let mut file2 = File::create(&file2_path).unwrap();
        write!(file2, "{}", content2).unwrap();

        // コマンドを実行
        let command = CatCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cat".to_string(),
                file1_path.file_name().unwrap().to_str().unwrap().to_string(),
                file2_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // 出力が期待通りであることを確認（両方のファイルの内容が含まれる）
        let output = String::from_utf8_lossy(&result.stdout);
        let expected = format!("{}{}", content1, content2);
        assert_eq!(output, expected);
    }
} 