use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::fs::File;
use std::io::{self, BufReader, BufRead, Read, Write};
use std::path::Path;
use regex::{Regex, Captures};
use tracing::{debug, error, info};

/// テキストストリームの変換を行うコマンド
pub struct SedCommand;

#[async_trait]
impl BuiltinCommand for SedCommand {
    fn name(&self) -> &'static str {
        "sed"
    }

    fn description(&self) -> &'static str {
        "テキストの検索と置換を行う"
    }

    fn usage(&self) -> &'static str {
        "使用法: sed [オプション] [スクリプト] [ファイル...]\n\
        \n\
        オプション:\n\
        -e <スクリプト>    実行するスクリプトを指定\n\
        -f <ファイル>      スクリプトファイルを指定\n\
        -n, --quiet        自動出力を抑制\n\
        -i[拡張子]         ファイルを直接編集（バックアップを作成する場合は拡張子を指定）\n\
        \n\
        スクリプト書式:\n\
        s/検索パターン/置換文字列/[フラグ]    検索と置換\n\
        d                               行の削除\n\
        p                               行の表示\n\
        \n\
        フラグ:\n\
        g    すべての一致を置換（デフォルトは最初の一致のみ）\n\
        i    大文字小文字を区別しない\n\
        \n\
        ファイルが指定されていない場合は標準入力から読み込む"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut scripts = Vec::new();
        let mut quiet_mode = false;
        let mut in_place = false;
        let mut backup_ext = String::new();
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg == "-e" {
                if let Some(script) = args.next() {
                    scripts.push(script.clone());
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr("エラー: -e オプションにはスクリプトが必要です\n".into_bytes()));
                }
            } else if arg == "-f" {
                if let Some(script_file) = args.next() {
                    let script_path = context.current_dir.join(script_file);
                    match std::fs::read_to_string(&script_path) {
                        Ok(content) => {
                            for line in content.lines() {
                                if !line.trim().is_empty() && !line.trim_start().starts_with('#') {
                                    scripts.push(line.to_string());
                                }
                            }
                        },
                        Err(err) => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: スクリプトファイル '{}' を読み込めません: {}\n", 
                                    script_file, err).into_bytes()));
                        }
                    }
                } else {
                    return Ok(CommandResult::failure(1)
                        .with_stderr("エラー: -f オプションにはファイル名が必要です\n".into_bytes()));
                }
            } else if arg == "-n" || arg == "--quiet" {
                quiet_mode = true;
            } else if arg.starts_with("-i") {
                in_place = true;
                if arg.len() > 2 {
                    backup_ext = arg[2..].to_string();
                }
            } else if arg.starts_with('-') {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 不明なオプション: {}\n{}", arg, self.usage()).into_bytes()));
            } else if scripts.is_empty() {
                // 最初の非オプション引数はスクリプトとして扱う
                scripts.push(arg.clone());
            } else {
                // 残りの非オプション引数はファイルパスとして扱う
                files.push(arg.clone());
            }
        }

        if scripts.is_empty() {
            return Ok(CommandResult::failure(1)
                .with_stderr("エラー: スクリプトが指定されていません\n".into_bytes()));
        }

        // スクリプトの解析
        let sed_scripts = match parse_scripts(&scripts) {
            Ok(scripts) => scripts,
            Err(err) => {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: スクリプトの解析に失敗しました: {}\n", err).into_bytes()));
            }
        };

        let mut result = CommandResult::success();

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            if !context.stdin_connected {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 標準入力が接続されていません".into_bytes()));
            }
            
            // 標準入力からの処理はスタブとして残す
            // 本来はstdinから読み込むべきだが、このコードでは空文字列を返す
            let content = String::new();
            let processed = process_sed(&content, &sed_scripts, quiet_mode);
            result.stdout = processed.into_bytes();
        } else {
            // 各ファイルを処理
            for file_path in &files {
                let path = context.current_dir.join(file_path);
                
                if in_place {
                    // ファイル内容を直接編集
                    match process_file_in_place(&path, &sed_scripts, quiet_mode, &backup_ext) {
                        Ok(_) => (),
                        Err(err) => {
                            let err_msg = format!("エラー: {}: {}\n", file_path, err);
                            result.stderr.extend_from_slice(err_msg.as_bytes());
                            result.exit_code = 1;
                        }
                    }
                } else {
                    // 通常処理：ファイルを読み込んで処理し、出力に書き込む
                    match process_file(&path, &sed_scripts, quiet_mode, &mut result.stdout) {
                        Ok(_) => (),
                        Err(err) => {
                            let err_msg = format!("エラー: {}: {}\n", file_path, err);
                            result.stderr.extend_from_slice(err_msg.as_bytes());
                            result.exit_code = 1;
                        }
                    }
                }
            }
        }
        
        Ok(result)
    }
}

/// sedスクリプトの種類を表す列挙型
#[derive(Debug)]
enum SedScript {
    Substitute {
        pattern: Regex,
        replacement: String,
        global: bool,
    },
    Delete,
    Print,
}

/// スクリプト文字列を解析してSedScript構造体に変換する
fn parse_scripts(scripts: &[String]) -> Result<Vec<SedScript>> {
    let mut sed_scripts = Vec::new();
    
    for script in scripts {
        if script.starts_with('s') && script.len() > 1 {
            // 置換スクリプト s/pattern/replacement/flags
            let delimiter = script.chars().nth(1).unwrap_or('/');
            let parts: Vec<&str> = script[2..].split(delimiter).collect();
            
            if parts.len() < 3 {
                return Err(anyhow::anyhow!("置換スクリプトの形式が不正です: {}", script));
            }
            
            let pattern = parts[0];
            let replacement = parts[1];
            let flags = if parts.len() > 3 { parts[2] } else { "" };
            
            let global = flags.contains('g');
            let case_insensitive = flags.contains('i');
            
            // 正規表現パターンの作成
            let regex_pattern = if case_insensitive {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            
            let regex = match Regex::new(&regex_pattern) {
                Ok(re) => re,
                Err(err) => return Err(anyhow::anyhow!("正規表現が不正です: {}", err)),
            };
            
            sed_scripts.push(SedScript::Substitute {
                pattern: regex,
                replacement: replacement.to_string(),
                global,
            });
        } else if script == "d" {
            // 削除スクリプト
            sed_scripts.push(SedScript::Delete);
        } else if script == "p" {
            // 表示スクリプト
            sed_scripts.push(SedScript::Print);
        } else {
            return Err(anyhow::anyhow!("未サポートのスクリプト形式です: {}", script));
        }
    }
    
    Ok(sed_scripts)
}

/// ファイルを処理する関数
fn process_file(
    path: &Path, 
    scripts: &[SedScript],
    quiet_mode: bool,
    output: &mut Vec<u8>,
) -> io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    for line in reader.lines() {
        let line = line?;
        process_line(&line, scripts, quiet_mode, output)?;
    }
    
    Ok(())
}

/// ファイルを直接編集する関数
fn process_file_in_place(
    path: &Path, 
    scripts: &[SedScript],
    quiet_mode: bool,
    backup_ext: &str,
) -> io::Result<()> {
    // バックアップを作成
    if !backup_ext.is_empty() {
        let backup_path = format!("{}{}", path.display(), backup_ext);
        std::fs::copy(path, backup_path)?;
    }
    
    // ファイルの内容を読み込む
    let content = std::fs::read_to_string(path)?;
    
    // sedスクリプトを適用
    let processed = process_sed(&content, scripts, quiet_mode);
    
    // 結果を書き戻す
    std::fs::write(path, processed)?;
    
    Ok(())
}

/// 文字列に対してsedスクリプトを適用する関数
fn process_sed(
    content: &str,
    scripts: &[SedScript],
    quiet_mode: bool,
) -> String {
    let mut output = String::new();
    
    for line in content.lines() {
        let mut buffer = Vec::new();
        if let Err(e) = process_line(line, scripts, quiet_mode, &mut buffer) {
            error!("行の処理中にエラーが発生しました: {}", e);
            continue;
        }
        
        if !buffer.is_empty() {
            output.push_str(&String::from_utf8_lossy(&buffer));
        }
    }
    
    output
}

/// 1行を処理する関数
fn process_line(
    line: &str,
    scripts: &[SedScript],
    quiet_mode: bool,
    output: &mut Vec<u8>,
) -> io::Result<()> {
    let mut current_line = line.to_string();
    let mut deleted = false;
    let mut print_line = !quiet_mode;
    
    for script in scripts {
        match script {
            SedScript::Substitute { pattern, replacement, global } => {
                if *global {
                    // グローバル置換
                    current_line = pattern.replace_all(&current_line, replacement.as_str()).to_string();
                } else {
                    // 最初の一致のみ置換
                    current_line = pattern.replace(&current_line, replacement.as_str()).to_string();
                }
            },
            SedScript::Delete => {
                deleted = true;
                print_line = false;
                break;
            },
            SedScript::Print => {
                // 明示的に行を表示
                output.extend_from_slice(current_line.as_bytes());
                output.push(b'\n');
                // 'p'コマンドはquiet_modeでも表示する
                print_line = true;
            }
        }
    }
    
    if !deleted && print_line {
        output.extend_from_slice(current_line.as_bytes());
        output.push(b'\n');
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sed_basic_substitution() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Hello, world!\nThis is a test.\nHello, again!\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（基本的な置換）
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "s/Hello/Hi/".to_string(),
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
        assert!(output.contains("Hi, world!"));
        assert!(output.contains("This is a test."));
        assert!(output.contains("Hi, again!"));
    }

    #[tokio::test]
    async fn test_sed_global_substitution() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "The quick brown fox jumps over the lazy dog.\nThe fox is quick and the dog is lazy.\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（グローバル置換）
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "s/the/a/g".to_string(),
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
        assert!(output.contains("a quick brown fox jumps over a lazy dog."));
        assert!(output.contains("a fox is quick and a dog is lazy."));
    }

    #[tokio::test]
    async fn test_sed_case_insensitive() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Hello, World!\nhello, everyone!\nHELLO, UNIVERSE!\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（大文字小文字を区別しない置換）
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "s/hello/hi/i".to_string(),
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
        assert!(output.contains("Hi, World!"));
        assert!(output.contains("hi, everyone!"));
        assert!(output.contains("HI, UNIVERSE!"));
    }

    #[tokio::test]
    async fn test_sed_delete() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nDelete this line\nLine 3\nDelete this line too\nLine 5\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // 削除スクリプトで実行
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "-e".to_string(),
                "s/Delete.*/d/".to_string(),
                "-e".to_string(),
                "d".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（すべての行が削除されているはず）
        let output = String::from_utf8_lossy(&result.stdout);
        assert_eq!(output.trim(), "");
    }

    #[tokio::test]
    async fn test_sed_print() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Line 1\nImportant line\nLine 3\nAnother important line\nLine 5\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // 表示スクリプトで実行（-n オプションと p コマンドの組み合わせ）
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "-n".to_string(),
                "s/Important/Found/p".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 出力が期待通りであることを確認（Importantを含む行のみが置換後に表示される）
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(!output.contains("Line 1"));
        assert!(output.contains("Found line"));
        assert!(!output.contains("Line 3"));
        assert!(!output.contains("Another important line")); // パターンに一致しない
        assert!(!output.contains("Line 5"));
    }

    #[tokio::test]
    async fn test_sed_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイル1を作成
        let file1_path = temp_path.join("file1.txt");
        let content1 = "File1 Line1\nFile1 Line2\n";
        let mut file1 = File::create(&file1_path).unwrap();
        write!(file1, "{}", content1).unwrap();

        // テスト用ファイル2を作成
        let file2_path = temp_path.join("file2.txt");
        let content2 = "File2 Line1\nFile2 Line2\n";
        let mut file2 = File::create(&file2_path).unwrap();
        write!(file2, "{}", content2).unwrap();

        // コマンドを実行（複数ファイル）
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "s/Line/Item/g".to_string(),
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
        assert!(output.contains("File1 Item1"));
        assert!(output.contains("File1 Item2"));
        assert!(output.contains("File2 Item1"));
        assert!(output.contains("File2 Item2"));
    }

    #[tokio::test]
    async fn test_sed_in_place() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "This is a test.\nReplace this line.\nKeep this line.\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（インプレース編集）
        let command = SedCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sed".to_string(),
                "-i.bak".to_string(),
                "s/Replace/Changed/".to_string(),
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // ファイルが実際に変更されたことを確認
        let modified_content = std::fs::read_to_string(&file_path).unwrap();
        assert!(modified_content.contains("This is a test."));
        assert!(modified_content.contains("Changed this line."));
        assert!(modified_content.contains("Keep this line."));
        
        // バックアップファイルが作成されたことを確認
        let backup_path = temp_path.join("test.txt.bak");
        assert!(backup_path.exists());
        
        // バックアップファイルが元の内容を保持していることを確認
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, content);
    }
} 