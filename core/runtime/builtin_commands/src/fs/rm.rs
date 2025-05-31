use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use std::fs;
use std::io;
use std::path::Path;
use tracing::{debug, error, info};

/// ファイルやディレクトリを削除するコマンド
pub struct RmCommand;

#[async_trait]
impl BuiltinCommand for RmCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    fn description(&self) -> &'static str {
        "ファイルやディレクトリを削除する"
    }

    fn usage(&self) -> &'static str {
        "使用法: rm [-f] [-r] [-i] <ファイル...>\n\
        \n\
        オプション:\n\
        -f, --force       確認なしで削除を強制する\n\
        -r, --recursive   ディレクトリとその内容を再帰的に削除する\n\
        -i, --interactive 削除前に確認を求める"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 2 {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: 引数が不足しています\n{}", self.usage()).into_bytes()));
        }

        let mut args = context.args.iter().skip(1);
        let mut force = false;
        let mut recursive = false;
        let mut interactive = false;
        let mut paths = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
                // -rf のような複合オプションをサポート
                for c in arg.chars().skip(1) {
                    match c {
                        'f' => force = true,
                        'r' => recursive = true,
                        'i' => interactive = true,
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else if arg == "--force" {
                force = true;
            } else if arg == "--recursive" {
                recursive = true;
            } else if arg == "--interactive" {
                interactive = true;
            } else {
                paths.push(arg);
            }
        }

        if paths.is_empty() {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: 削除するファイルが指定されていません\n{}", self.usage()).into_bytes()));
        }

        // force と interactive が両方指定された場合は force を優先
        if force && interactive {
            interactive = false;
        }

        let mut result = CommandResult::success();

        // 各パスを処理
        for path_str in paths {
            let path = context.current_dir.join(path_str);
            
            if !path.exists() {
                if !force {
                    let err_msg = format!("エラー: '{}' は存在しません\n", path_str);
                    result.stderr.extend_from_slice(err_msg.as_bytes());
                    result.exit_code = 1;
                }
                continue;
            }

            // ディレクトリの場合
            if path.is_dir() {
                if !recursive {
                    let err_msg = format!("エラー: '{}' はディレクトリです。ディレクトリを削除するには -r オプションを使用してください\n", path_str);
                    result.stderr.extend_from_slice(err_msg.as_bytes());
                    result.exit_code = 1;
                    continue;
                }

                // 対話モードの確認
                if interactive {
                    // ユーザーからの削除確認を求める
                    let prompt = format!("ディレクトリ '{}' を削除しますか？ (y/n): ", path_str);
                    let mut input = String::new();
                    
                    // 標準エラー出力に質問を出力（標準出力はリダイレクトされている可能性があるため）
                    std::io::stderr().write_all(prompt.as_bytes()).ok();
                    std::io::stderr().flush().ok();
                    
                    // 標準入力から回答を読み込む
                    match io::stdin().read_line(&mut input) {
                        Ok(_) => {
                            let answer = input.trim().to_lowercase();
                            if answer != "y" && answer != "yes" {
                                debug!("ユーザーが削除を拒否しました: {}", path_str);
                                continue; // 次のファイルへ
                            }
                        },
                        Err(_) => {
                            // 入力エラーの場合は安全のため削除しない
                            let err_msg = format!("エラー: ユーザー入力の読み取りに失敗しました。'{}' の削除をスキップします。\n", path_str);
                            result.stderr.extend_from_slice(err_msg.as_bytes());
                            continue; // 次のファイルへ
                        }
                    }
                }

                match remove_dir_all(&path) {
                    Ok(_) => {
                        debug!("ディレクトリを削除しました: {}", path.display());
                    },
                    Err(err) => {
                        if !force {
                            let err_msg = format!("エラー: '{}' の削除に失敗しました: {}\n", path_str, err);
                            result.stderr.extend_from_slice(err_msg.as_bytes());
                            result.exit_code = 1;
                        }
                    }
                }
            } else {
                // ファイルの場合
                if interactive {
                    // ユーザーからの削除確認を求める
                    let prompt = format!("ファイル '{}' を削除しますか？ (y/n): ", path_str);
                    let mut input = String::new();
                    
                    // 標準エラー出力に質問を出力
                    std::io::stderr().write_all(prompt.as_bytes()).ok();
                    std::io::stderr().flush().ok();
                    
                    // 標準入力から回答を読み込む
                    match io::stdin().read_line(&mut input) {
                        Ok(_) => {
                            let answer = input.trim().to_lowercase();
                            if answer != "y" && answer != "yes" {
                                debug!("ユーザーが削除を拒否しました: {}", path_str);
                                continue; // 次のファイルへ
                            }
                        },
                        Err(_) => {
                            // 入力エラーの場合は安全のため削除しない
                            let err_msg = format!("エラー: ユーザー入力の読み取りに失敗しました。'{}' の削除をスキップします。\n", path_str);
                            result.stderr.extend_from_slice(err_msg.as_bytes());
                            continue; // 次のファイルへ
                        }
                    }
                }

                match fs::remove_file(&path) {
                    Ok(_) => {
                        debug!("ファイルを削除しました: {}", path.display());
                    },
                    Err(err) => {
                        if !force {
                            let err_msg = format!("エラー: '{}' の削除に失敗しました: {}\n", path_str, err);
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

// ディレクトリを再帰的に削除する関数
fn remove_dir_all(path: &Path) -> io::Result<()> {
    // std::fs::remove_dir_all を使うこともできますが
    // カスタム実装によりエラーハンドリングを細かく制御できます
    
    if path.is_dir() {
        // ディレクトリ内のすべてのエントリを処理
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        
        // 空になったディレクトリを削除
        fs::remove_dir(path)
    } else {
        // ディレクトリではない場合はファイルとして削除
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_rm_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "テストデータ").unwrap();

        // ファイルが存在することを確認
        assert!(file_path.exists());

        // コマンドを実行
        let command = RmCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "rm".to_string(), 
                file_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ファイルが削除されたことを確認
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_rm_directory_fails_without_recursive() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ディレクトリを作成
        let dir_path = temp_path.join("test_dir");
        fs::create_dir(&dir_path).unwrap();

        // ディレクトリが存在することを確認
        assert!(dir_path.exists());

        // コマンドを実行（-r なし）
        let command = RmCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "rm".to_string(), 
                dir_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);

        // ディレクトリがまだ存在することを確認
        assert!(dir_path.exists());

        // エラーメッセージに「ディレクトリ」という単語が含まれていることを確認
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("ディレクトリ"));
    }

    #[tokio::test]
    async fn test_rm_directory_recursive() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ディレクトリ構造を作成
        let dir_path = temp_path.join("test_dir");
        fs::create_dir(&dir_path).unwrap();
        
        // ディレクトリ内にファイルを作成
        let file_path = dir_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "テストデータ").unwrap();

        // サブディレクトリを作成
        let subdir_path = dir_path.join("subdir");
        fs::create_dir(&subdir_path).unwrap();
        
        // サブディレクトリ内にファイルを作成
        let subdir_file_path = subdir_path.join("subtest.txt");
        let mut file = File::create(&subdir_file_path).unwrap();
        writeln!(file, "サブディレクトリのテストデータ").unwrap();

        // コマンドを実行（-r あり）
        let command = RmCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "rm".to_string(),
                "-r".to_string(),
                dir_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ディレクトリが削除されたことを確認
        assert!(!dir_path.exists());
    }

    #[tokio::test]
    async fn test_rm_nonexistent_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 存在しないファイルのパス
        let nonexistent_file = "nonexistent.txt";
        let nonexistent_path = temp_path.join(nonexistent_file);
        
        // 確認のためにファイルが存在しないことを確認
        assert!(!nonexistent_path.exists());

        // コマンドを実行
        let command = RmCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "rm".to_string(), 
                nonexistent_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);

        // エラーメッセージに「存在しません」という文言が含まれていることを確認
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("存在しません"));
    }

    #[tokio::test]
    async fn test_rm_force_nonexistent_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 存在しないファイルのパス
        let nonexistent_file = "nonexistent.txt";
        
        // コマンドを実行（-f あり）
        let command = RmCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "rm".to_string(),
                "-f".to_string(),
                nonexistent_file.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);  // -f オプションによりエラーにならない

        // エラーメッセージがないことを確認
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.is_empty());
    }

    #[tokio::test]
    async fn test_rm_multiple_files() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを複数作成
        let file_paths = vec!["file1.txt", "file2.txt", "file3.txt"];
        
        for filename in &file_paths {
            let file_path = temp_path.join(filename);
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "テストデータ: {}", filename).unwrap();
            assert!(file_path.exists());
        }

        // コマンドを実行
        let command = RmCommand;
        let mut args = vec!["rm".to_string()];
        args.extend(file_paths.iter().map(|s| s.to_string()));
        
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

        // すべてのファイルが削除されたことを確認
        for filename in file_paths {
            let file_path = temp_path.join(filename);
            assert!(!file_path.exists());
        }
    }
} 