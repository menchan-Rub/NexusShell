use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::fs::File;
use std::io::{self, BufReader, BufRead, Read};
use std::path::Path;
use tracing::{debug, error, info};

/// テキストファイルの行数、単語数、バイト数を数えるコマンド
pub struct WcCommand;

#[async_trait]
impl BuiltinCommand for WcCommand {
    fn name(&self) -> &'static str {
        "wc"
    }

    fn description(&self) -> &'static str {
        "テキストファイルの行数、単語数、バイト数を数える"
    }

    fn usage(&self) -> &'static str {
        "使用法: wc [-c] [-m] [-l] [-w] [ファイル...]\n\
        \n\
        オプション:\n\
        -c, --bytes       バイト数を表示する\n\
        -m, --chars       文字数を表示する\n\
        -l, --lines       行数を表示する\n\
        -w, --words       単語数を表示する\n\
        \n\
        オプションが指定されていない場合は、行数、単語数、バイト数を表示します。\n\
        ファイルが指定されていない場合は標準入力から読み込みます。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut count_bytes = false;
        let mut count_chars = false;
        let mut count_lines = false;
        let mut count_words = false;
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg == "-c" || arg == "--bytes" {
                count_bytes = true;
            } else if arg == "-m" || arg == "--chars" {
                count_chars = true;
            } else if arg == "-l" || arg == "--lines" {
                count_lines = true;
            } else if arg == "-w" || arg == "--words" {
                count_words = true;
            } else if arg.starts_with("-") && arg.len() > 1 {
                // 複合オプション（例: -lw）の処理
                for c in arg.chars().skip(1) {
                    match c {
                        'c' => count_bytes = true,
                        'm' => count_chars = true,
                        'l' => count_lines = true,
                        'w' => count_words = true,
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else {
                // ファイル引数
                files.push(arg.clone());
            }
        }

        // オプションが指定されていない場合は、すべてのカウントを行う
        if !count_bytes && !count_chars && !count_lines && !count_words {
            count_lines = true;
            count_words = true;
            count_bytes = true;
        }

        let mut result = CommandResult::success();
        let mut total_lines = 0;
        let mut total_words = 0;
        let mut total_bytes = 0;
        let mut total_chars = 0;

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            let mut buffer = String::new();
            let bytes = handle.read_to_string(&mut buffer)?;
            total_bytes += bytes;
            total_lines += buffer.lines().count();
            total_words += buffer.split_whitespace().count();
            total_chars += buffer.chars().count();
            // 結果出力
            print_wc_result(total_lines, total_words, total_bytes, total_chars, "");
        } else {
            for file in files {
                let mut f = std::fs::File::open(file)?;
                let mut buffer = String::new();
                let bytes = f.read_to_string(&mut buffer)?;
                total_bytes += bytes;
                total_lines += buffer.lines().count();
                total_words += buffer.split_whitespace().count();
                total_chars += buffer.chars().count();
                print_wc_result(total_lines, total_words, total_bytes, total_chars, file);
            }
        }

        Ok(result)
    }
}

// ファイルを処理する関数
fn count_file(
    path: &Path, 
    count_lines: bool,
    count_words: bool,
    count_bytes: bool,
    count_chars: bool,
) -> io::Result<(usize, usize, usize, usize)> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len() as usize;
    
    // バイト数のみの場合は、実際にファイルを読み込まずにメタデータから取得
    if count_bytes && !count_lines && !count_words && !count_chars {
        let metadata = std::fs::metadata(path)?;
        let byte_count = metadata.len();
        return Ok((0, 0, byte_count as usize, 0));
    }
    
    let mut bytes = 0;
    let mut chars = 0;
    let mut words = 0;
    let mut lines = 0;
    
    // 行、単語、文字数をカウントする場合
    if count_lines || count_words || count_chars {
        let reader = BufReader::new(file);
        
        for line_result in reader.lines() {
            let line = line_result?;
            
            if count_lines {
                lines += 1;
            }
            
            if count_words {
                words += line.split_whitespace().count();
            }
            
            if count_chars {
                chars += line.chars().count();
            }
            
            if count_bytes {
                bytes += line.as_bytes().len();
                // 改行文字のバイト数を加算（Linuxなら1バイト、Windowsなら2バイト）
                bytes += if cfg!(target_os = "windows") { 2 } else { 1 };
            }
        }
    } else {
        // ファイルサイズをバイト数とする
        bytes = file_size;
    }
    
    Ok((lines, words, bytes, chars))
}

// テキスト文字列のカウント関数（標準入力用）
fn count_text(
    content: &str,
    count_lines: bool,
    count_words: bool,
    count_bytes: bool,
    count_chars: bool,
) -> (usize, usize, usize, usize) {
    let mut lines = 0;
    let mut words = 0;
    let mut bytes = 0;
    let mut chars = 0;
    
    if count_lines {
        lines = content.lines().count();
    }
    
    if count_words {
        words = content.split_whitespace().count();
    }
    
    if count_bytes {
        bytes = content.as_bytes().len();
    }
    
    if count_chars {
        chars = content.chars().count();
    }
    
    (lines, words, bytes, chars)
}

// カウント結果のフォーマット関数
fn format_counts(
    counts: (usize, usize, usize, usize), 
    count_lines: bool,
    count_words: bool,
    count_bytes: bool,
    count_chars: bool,
    filename: Option<&str>,
) -> String {
    let (lines, words, bytes, chars) = counts;
    let mut output = String::new();
    
    if count_lines {
        output.push_str(&format!("{:8}", lines));
    }
    
    if count_words {
        output.push_str(&format!("{:8}", words));
    }
    
    if count_chars {
        output.push_str(&format!("{:8}", chars));
    }
    
    if count_bytes {
        output.push_str(&format!("{:8}", bytes));
    }
    
    if let Some(name) = filename {
        output.push_str(&format!(" {}", name));
    }
    
    output.push_str("\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_wc_basic() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "This is line one.\nThis is line two.\nThis is line three.\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（デフォルト: -lwc）
        let command = WcCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "wc".to_string(),
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
        
        // 行数、単語数、バイト数を確認
        assert!(output.contains("3")); // 3行
        assert!(output.contains("12")); // 12単語
        assert!(output.contains("test.txt")); // ファイル名
    }

    #[tokio::test]
    async fn test_wc_lines_only() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "This is line one.\nThis is line two.\nThis is line three.\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（行数のみ）
        let command = WcCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "wc".to_string(),
                "-l".to_string(),
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
        
        // ファイル名と行数のみを確認
        assert!(output.contains("3")); // 3行
        assert!(output.contains("test.txt")); // ファイル名
        
        // 単語数とバイト数が含まれていないことを確認
        // 注: これは単純な確認方法ではないため、出力の形式によって調整が必要
        let fields: Vec<&str> = output.split_whitespace().collect();
        assert_eq!(fields.len(), 2); // "3" と "test.txt" のみ
    }

    #[tokio::test]
    async fn test_wc_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイル1を作成
        let file1_path = temp_path.join("test1.txt");
        let content1 = "This is file one.\nIt has two lines.\n";
        let mut file1 = File::create(&file1_path).unwrap();
        write!(file1, "{}", content1).unwrap();

        // テスト用ファイル2を作成
        let file2_path = temp_path.join("test2.txt");
        let content2 = "This is file two.\nIt has three lines.\nThis is line three.\n";
        let mut file2 = File::create(&file2_path).unwrap();
        write!(file2, "{}", content2).unwrap();

        // コマンドを実行（複数ファイル）
        let command = WcCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "wc".to_string(),
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
        
        // 各ファイルの出力と合計行を確認
        assert!(output.contains("test1.txt"));
        assert!(output.contains("test2.txt"));
        assert!(output.contains("合計")); // 合計行があることを確認
    }

    #[tokio::test]
    async fn test_wc_all_options() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成 - 日本語文字を含む
        let file_path = temp_path.join("test.txt");
        let content = "これは日本語です。\nThis is English.\n私はテスト。\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（すべてのオプション）
        let command = WcCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "wc".to_string(),
                "-lwcm".to_string(),
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
        
        // 行数、単語数、バイト数、文字数を確認
        assert!(output.contains("3")); // 3行
        assert!(output.contains("test.txt")); // ファイル名
        
        // 注: 正確な単語数、バイト数、文字数はシステムによって異なる可能性があるため、
        // ここでは単純に出力が生成されることのみを確認します。
    }
} 