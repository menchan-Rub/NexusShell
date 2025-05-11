use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use tracing::{debug, error, info};

/// テキストファイルから特定の列や文字を抽出するコマンド
pub struct CutCommand;

#[async_trait]
impl BuiltinCommand for CutCommand {
    fn name(&self) -> &'static str {
        "cut"
    }

    fn description(&self) -> &'static str {
        "ファイルの各行から指定した部分を抽出する"
    }

    fn usage(&self) -> &'static str {
        "使用法: cut オプション [ファイル...]\n\
        \n\
        オプション:\n\
        -b, --bytes=LIST       指定したバイト位置を抽出\n\
        -c, --characters=LIST  指定した文字位置を抽出\n\
        -d, --delimiter=DELIM  フィールドの区切り文字を指定（デフォルトはタブ）\n\
        -f, --fields=LIST      指定したフィールドを抽出\n\
        --complement           指定したバイト、文字、フィールドを除いて抽出\n\
        -s, --only-delimited   区切り文字を含まない行を出力しない\n\
        \n\
        LISTは番号またはコンマで区切られた番号のリスト\n\
        例: 1,3,5 または 1-3,5-7\n\
        \n\
        バイト、文字、およびフィールドの番号は1から始まる。\n\
        -f と -d オプションを使用するか、-b または -c オプションを使用する必要がある。\n\
        ファイルが指定されない場合は標準入力から読み込み。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut bytes_list = None;
        let mut chars_list = None;
        let mut fields_list = None;
        let mut delimiter = '\t'; // デフォルトの区切り文字はタブ
        let mut complement = false;
        let mut only_delimited = false;
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg == "-b" || arg == "--bytes" {
                if let Some(list) = args.next() {
                    bytes_list = Some(list.clone());
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: -b オプションには値が必要です\n{}", self.usage()).into_bytes()));
                }
            } else if arg.starts_with("--bytes=") {
                bytes_list = Some(arg[8..].to_string());
            } else if arg.starts_with("-b") && arg.len() > 2 {
                bytes_list = Some(arg[2..].to_string());
            } else if arg == "-c" || arg == "--characters" {
                if let Some(list) = args.next() {
                    chars_list = Some(list.clone());
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: -c オプションには値が必要です\n{}", self.usage()).into_bytes()));
                }
            } else if arg.starts_with("--characters=") {
                chars_list = Some(arg[13..].to_string());
            } else if arg.starts_with("-c") && arg.len() > 2 {
                chars_list = Some(arg[2..].to_string());
            } else if arg == "-d" || arg == "--delimiter" {
                if let Some(delim) = args.next() {
                    if delim.len() != 1 {
                        return Ok(CommandResult::failure(1)
                            .with_stderr("エラー: 区切り文字は1文字でなければなりません\n".into_bytes()));
                    }
                    delimiter = delim.chars().next().unwrap();
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: -d オプションには値が必要です\n{}", self.usage()).into_bytes()));
                }
            } else if arg.starts_with("--delimiter=") {
                let delim = &arg[12..];
                if delim.len() != 1 {
                    return Ok(CommandResult::failure(1)
                        .with_stderr("エラー: 区切り文字は1文字でなければなりません\n".into_bytes()));
                }
                delimiter = delim.chars().next().unwrap();
            } else if arg.starts_with("-d") && arg.len() > 2 {
                let delim = &arg[2..];
                if delim.len() != 1 {
                    return Ok(CommandResult::failure(1)
                        .with_stderr("エラー: 区切り文字は1文字でなければなりません\n".into_bytes()));
                }
                delimiter = delim.chars().next().unwrap();
            } else if arg == "-f" || arg == "--fields" {
                if let Some(list) = args.next() {
                    fields_list = Some(list.clone());
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: -f オプションには値が必要です\n{}", self.usage()).into_bytes()));
                }
            } else if arg.starts_with("--fields=") {
                fields_list = Some(arg[9..].to_string());
            } else if arg.starts_with("-f") && arg.len() > 2 {
                fields_list = Some(arg[2..].to_string());
            } else if arg == "--complement" {
                complement = true;
            } else if arg == "-s" || arg == "--only-delimited" {
                only_delimited = true;
            } else if !arg.starts_with('-') {
                files.push(arg.clone());
            } else {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 不明なオプション: {}\n{}", arg, self.usage()).into_bytes()));
            }
        }

        // いずれかのリストオプションが指定されていることを確認
        if bytes_list.is_none() && chars_list.is_none() && fields_list.is_none() {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: 切り出すバイト、文字、またはフィールドを指定してください\n{}", self.usage()).into_bytes()));
        }

        // バイト/文字とフィールドの両方が指定されていないことを確認
        if (bytes_list.is_some() || chars_list.is_some()) && fields_list.is_some() {
            return Ok(CommandResult::failure(1)
                .with_stderr("エラー: バイト/文字とフィールドの両方を同時に指定することはできません\n".into_bytes()));
        }

        // インデックスリストの解析
        let positions = if let Some(list) = bytes_list.or(chars_list) {
            match parse_index_list(&list) {
                Ok(indices) => indices,
                Err(err) => {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: インデックスリストの形式が不正です: {}\n", err).into_bytes()));
                }
            }
        } else if let Some(list) = &fields_list {
            match parse_index_list(list) {
                Ok(indices) => indices,
                Err(err) => {
                    return Ok(CommandResult::failure(1)
                        .with_stderr(format!("エラー: フィールドリストの形式が不正です: {}\n", err).into_bytes()));
                }
            }
        } else {
            HashSet::new()
        };

        let mut result = CommandResult::success();

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            if !context.stdin_connected {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 標準入力が接続されていません\n".into_bytes()));
            }

            // 標準入力の処理（実際の実装では標準入力から読み込む）
            // ここではダミーの空文字列を使用
            let content = String::new();
            
            if bytes_list.is_some() {
                let processed = cut_bytes(&content, &positions, complement);
                result.stdout = processed.into_bytes();
            } else if chars_list.is_some() {
                let processed = cut_chars(&content, &positions, complement);
                result.stdout = processed.into_bytes();
            } else if fields_list.is_some() {
                let processed = cut_fields(&content, &positions, delimiter, only_delimited, complement);
                result.stdout = processed.into_bytes();
            }
        } else {
            // 各ファイルを処理
            for file_path in &files {
                let path = context.current_dir.join(file_path);
                
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        if bytes_list.is_some() {
                            let processed = cut_bytes(&content, &positions, complement);
                            result.stdout.extend_from_slice(processed.as_bytes());
                        } else if chars_list.is_some() {
                            let processed = cut_chars(&content, &positions, complement);
                            result.stdout.extend_from_slice(processed.as_bytes());
                        } else if fields_list.is_some() {
                            let processed = cut_fields(&content, &positions, delimiter, only_delimited, complement);
                            result.stdout.extend_from_slice(processed.as_bytes());
                        }
                    },
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

/// インデックスリストを解析する関数
/// 例: "1,3-5,7" -> {1, 3, 4, 5, 7}
fn parse_index_list(list: &str) -> Result<HashSet<usize>> {
    let mut indices = HashSet::new();
    
    for part in list.split(',') {
        if part.contains('-') {
            let range_parts: Vec<&str> = part.split('-').collect();
            
            if range_parts.len() != 2 {
                return Err(anyhow::anyhow!("不正な範囲指定: {}", part));
            }
            
            let start = range_parts[0].parse::<usize>()
                .map_err(|_| anyhow::anyhow!("数値ではない: {}", range_parts[0]))?;
            
            let end = range_parts[1].parse::<usize>()
                .map_err(|_| anyhow::anyhow!("数値ではない: {}", range_parts[1]))?;
            
            if start == 0 || end == 0 {
                return Err(anyhow::anyhow!("インデックスは1から始まる必要があります"));
            }
            
            if start > end {
                return Err(anyhow::anyhow!("範囲の開始が終了よりも大きい: {}-{}", start, end));
            }
            
            for i in start..=end {
                indices.insert(i);
            }
        } else {
            let index = part.parse::<usize>()
                .map_err(|_| anyhow::anyhow!("数値ではない: {}", part))?;
            
            if index == 0 {
                return Err(anyhow::anyhow!("インデックスは1から始まる必要があります"));
            }
            
            indices.insert(index);
        }
    }
    
    Ok(indices)
}

/// バイト単位で切り出す関数
fn cut_bytes(content: &str, positions: &HashSet<usize>, complement: bool) -> String {
    let mut result = String::new();
    
    for line in content.lines() {
        let bytes = line.as_bytes();
        let mut line_result = Vec::new();
        
        for (i, byte) in bytes.iter().enumerate() {
            let pos = i + 1; // 1-indexedに変換
            let should_include = positions.contains(&pos);
            
            if should_include != complement {
                line_result.push(*byte);
            }
        }
        
        // 切り出したバイト列を文字列に変換（無効なUTF-8の場合は置換）
        if let Ok(s) = String::from_utf8(line_result) {
            result.push_str(&s);
            result.push('\n');
        } else {
            // UTF-8に変換できないバイト列の場合は、置換字を使用
            let s = String::from_utf8_lossy(&line_result).into_owned();
            result.push_str(&s);
            result.push('\n');
        }
    }
    
    result
}

/// 文字単位で切り出す関数
fn cut_chars(content: &str, positions: &HashSet<usize>, complement: bool) -> String {
    let mut result = String::new();
    
    for line in content.lines() {
        let mut line_result = String::new();
        
        for (i, c) in line.chars().enumerate() {
            let pos = i + 1; // 1-indexedに変換
            let should_include = positions.contains(&pos);
            
            if should_include != complement {
                line_result.push(c);
            }
        }
        
        result.push_str(&line_result);
        result.push('\n');
    }
    
    result
}

/// フィールド単位で切り出す関数
fn cut_fields(
    content: &str, 
    positions: &HashSet<usize>, 
    delimiter: char, 
    only_delimited: bool,
    complement: bool,
) -> String {
    let mut result = String::new();
    
    for line in content.lines() {
        // 区切り文字が含まれているか確認
        if only_delimited && !line.contains(delimiter) {
            continue;
        }
        
        let fields: Vec<&str> = line.split(delimiter).collect();
        let mut line_result = Vec::new();
        
        for (i, field) in fields.iter().enumerate() {
            let pos = i + 1; // 1-indexedに変換
            let should_include = positions.contains(&pos);
            
            if should_include != complement {
                line_result.push(*field);
            }
        }
        
        result.push_str(&line_result.join(&delimiter.to_string()));
        result.push('\n');
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
    async fn test_cut_bytes() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Hello, World!\nTest 123\nABCDEF\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（バイト位置1-5を抽出）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-b".to_string(),
                "1-5".to_string(),
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
        assert!(output.contains("Hello"));
        assert!(output.contains("Test "));
        assert!(output.contains("ABCDE"));
    }

    #[tokio::test]
    async fn test_cut_chars() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成（マルチバイト文字を含む）
        let file_path = temp_path.join("test.txt");
        let content = "こんにちは世界\nテスト123\nABCDEF\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（文字位置1-3を抽出）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-c".to_string(),
                "1-3".to_string(),
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
        assert!(output.contains("こんに"));
        assert!(output.contains("テス"));
        assert!(output.contains("ABC"));
    }

    #[tokio::test]
    async fn test_cut_fields() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成（タブ区切り）
        let file_path = temp_path.join("test.txt");
        let content = "name\tage\tcity\nJohn\t30\tNew York\nAlice\t25\tLondon\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（フィールド1と3を抽出）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-f".to_string(),
                "1,3".to_string(),
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
        assert!(output.contains("name\tcity"));
        assert!(output.contains("John\tNew York"));
        assert!(output.contains("Alice\tLondon"));
    }

    #[tokio::test]
    async fn test_cut_custom_delimiter() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成（コンマ区切り）
        let file_path = temp_path.join("test.txt");
        let content = "name,age,city\nJohn,30,New York\nAlice,25,London\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（コンマ区切りでフィールド2を抽出）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-d".to_string(),
                ",".to_string(),
                "-f".to_string(),
                "2".to_string(),
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
        assert!(output.contains("age"));
        assert!(output.contains("30"));
        assert!(output.contains("25"));
    }

    #[tokio::test]
    async fn test_cut_complement() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Hello, World!\nTest 123\nABCDEF\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（バイト位置1-5以外を抽出）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-b".to_string(),
                "1-5".to_string(),
                "--complement".to_string(),
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
        assert!(output.contains(", World!"));
        assert!(output.contains("123"));
        assert!(output.contains("F"));
    }

    #[tokio::test]
    async fn test_cut_only_delimited() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成（区切り文字を含む行と含まない行）
        let file_path = temp_path.join("test.txt");
        let content = "name,age,city\nJohn,30,New York\nNo delimiter line\nAlice,25,London\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（区切り文字を含む行のみ処理）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-d".to_string(),
                ",".to_string(),
                "-f".to_string(),
                "1".to_string(),
                "-s".to_string(),
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
        assert!(output.contains("name"));
        assert!(output.contains("John"));
        assert!(output.contains("Alice"));
        assert!(!output.contains("No delimiter line"));
    }

    #[tokio::test]
    async fn test_cut_error_handling() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 不正なオプション指定（-bと-fの両方）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-b".to_string(),
                "1-3".to_string(),
                "-f".to_string(),
                "1".to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        // エラーメッセージが期待通りであることを確認
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("バイト/文字とフィールドの両方を同時に指定することはできません"));
    }

    #[tokio::test]
    async fn test_cut_invalid_index_list() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 不正なインデックスリスト（0を含む）
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-b".to_string(),
                "0,1,2".to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        // エラーメッセージが期待通りであることを確認
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("インデックスは1から始まる必要があります"));
    }

    #[tokio::test]
    async fn test_cut_missing_option_value() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // オプション値が不足している
        let command = CutCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cut".to_string(),
                "-b".to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        // エラーメッセージが期待通りであることを確認
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("-b オプションには値が必要です"));
    }
} 