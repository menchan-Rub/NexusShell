use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use std::fs;
use std::io;
use std::path::Path;
use tracing::{debug, error, info};

/// ファイルやディレクトリを移動するコマンド
pub struct MvCommand;

#[async_trait]
impl BuiltinCommand for MvCommand {
    fn name(&self) -> &'static str {
        "mv"
    }

    fn description(&self) -> &'static str {
        "ファイルやディレクトリを移動または名前変更する"
    }

    fn usage(&self) -> &'static str {
        "使用法: mv [-f] [-i] [-n] <ソース> <宛先>\n\
        \n\
        オプション:\n\
        -f, --force      宛先が既に存在する場合、確認なく上書きする\n\
        -i, --interactive 宛先が既に存在する場合、上書きの確認を求める\n\
        -n, --no-clobber  宛先が既に存在する場合、上書きしない"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 3 {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: 引数が不足しています\n{}", self.usage()).into_bytes()));
        }

        let mut args = context.args.iter().skip(1);
        let mut force = false;
        let mut interactive = false;
        let mut no_clobber = false;
        let mut sources = Vec::new();
        let mut dest = None;

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
                // -fi のような複合オプションをサポート
                for c in arg.chars().skip(1) {
                    match c {
                        'f' => force = true,
                        'i' => interactive = true,
                        'n' => no_clobber = true,
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else if arg == "--force" {
                force = true;
            } else if arg == "--interactive" {
                interactive = true;
            } else if arg == "--no-clobber" {
                no_clobber = true;
            } else {
                sources.push(arg);
            }
        }

        // ソースとデスティネーションの分離
        if sources.len() < 2 {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: ソースと宛先の両方を指定してください\n{}", self.usage()).into_bytes()));
        }

        dest = sources.pop();
        
        // 競合するオプションの処理
        if force && interactive {
            interactive = false;  // forceが優先
        }
        if force && no_clobber {
            no_clobber = false;  // forceが優先
        }
        if interactive && no_clobber {
            interactive = false;  // no_clobberが優先
        }

        let dest_path = context.current_dir.join(dest.unwrap());
        let dest_is_dir = dest_path.is_dir();
        
        let mut result = CommandResult::success();

        // ソースパスを処理
        for source in sources {
            let source_path = context.current_dir.join(source);
            
            // ソースが存在するか確認
            if !source_path.exists() {
                let err_msg = format!("エラー: '{}' は存在しません\n", source);
                result.stderr.extend_from_slice(err_msg.as_bytes());
                result.exit_code = 1;
                continue;
            }

            // 宛先パスを決定
            let final_dest = if dest_is_dir {
                // 宛先がディレクトリの場合、その中にファイルを移動
                dest_path.join(source_path.file_name().unwrap())
            } else {
                // 宛先がディレクトリでない場合、直接移動
                dest_path.clone()
            };

            // 宛先が既に存在するか確認
            if final_dest.exists() {
                if no_clobber {
                    debug!("宛先 '{}' が既に存在し、-n オプションが指定されているため、スキップします", final_dest.display());
                    continue;
                }
                
                if interactive {
                    print!("'{}' を上書きしますか？ (y/n): ", final_dest.display());
                    io::stdout().flush().unwrap();
                    let mut answer = String::new();
                    io::stdin().read_line(&mut answer).unwrap();
                    if !answer.trim().eq_ignore_ascii_case("y") {
                        return Ok(CommandResult::success().with_stdout(b"キャンセル\n".to_vec()));
                    }
                }
            }

            // ファイル/ディレクトリの移動
            match fs::rename(&source_path, &final_dest) {
                Ok(_) => {
                    debug!("'{}' を '{}' に移動しました", source, final_dest.display());
                },
                Err(err) => {
                    if err.kind() == io::ErrorKind::CrossesDevices {
                        // 異なるデバイス間の移動の場合、コピー＆削除を試みる
                        debug!("デバイスをまたいだ移動を試みます: コピー＆削除");
                        
                        if source_path.is_dir() {
                            if let Err(err) = copy_dir_all(&source_path, &final_dest)
                                .and_then(|_| fs::remove_dir_all(&source_path)) {
                                let err_msg = format!("エラー: '{}' から '{}' への移動に失敗しました: {}\n", 
                                    source, final_dest.display(), err);
                                result.stderr.extend_from_slice(err_msg.as_bytes());
                                result.exit_code = 1;
                            }
                        } else {
                            if let Err(err) = fs::copy(&source_path, &final_dest)
                                .and_then(|_| fs::remove_file(&source_path)) {
                                let err_msg = format!("エラー: '{}' から '{}' への移動に失敗しました: {}\n", 
                                    source, final_dest.display(), err);
                                result.stderr.extend_from_slice(err_msg.as_bytes());
                                result.exit_code = 1;
                            }
                        }
                    } else {
                        let err_msg = format!("エラー: '{}' から '{}' への移動に失敗しました: {}\n", 
                            source, final_dest.display(), err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        result.exit_code = 1;
                    }
                }
            }
        }

        Ok(result)
    }
}

// ディレクトリを再帰的にコピーする関数
fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(file_name);

        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_mv_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースファイルを作成
        let source_path = temp_path.join("source.txt");
        let content = "テストデータ\n";
        let mut file = File::create(&source_path).unwrap();
        write!(file, "{}", content).unwrap();

        // 宛先パス
        let dest_path = temp_path.join("dest.txt");

        // コマンドを実行
        let command = MvCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mv".to_string(),
                source_path.file_name().unwrap().to_str().unwrap().to_string(),
                dest_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ソースファイルが消えていることを確認
        assert!(!source_path.exists());

        // 宛先ファイルが存在することを確認
        assert!(dest_path.exists());

        // 内容が保持されていることを確認
        let dest_content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(dest_content, content);
    }

    #[tokio::test]
    async fn test_mv_to_directory() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースファイルを作成
        let source_path = temp_path.join("source.txt");
        let content = "テストデータ\n";
        let mut file = File::create(&source_path).unwrap();
        write!(file, "{}", content).unwrap();

        // 宛先ディレクトリを作成
        let dest_dir = temp_path.join("dest_dir");
        fs::create_dir(&dest_dir).unwrap();

        // コマンドを実行
        let command = MvCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mv".to_string(),
                source_path.file_name().unwrap().to_str().unwrap().to_string(),
                dest_dir.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ソースファイルが消えていることを確認
        assert!(!source_path.exists());

        // 宛先ディレクトリにファイルが移動していることを確認
        let new_path = dest_dir.join(source_path.file_name().unwrap());
        assert!(new_path.exists());

        // 内容が保持されていることを確認
        let dest_content = fs::read_to_string(&new_path).unwrap();
        assert_eq!(dest_content, content);
    }

    #[tokio::test]
    async fn test_mv_directory() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースディレクトリを作成
        let source_dir = temp_path.join("source_dir");
        fs::create_dir(&source_dir).unwrap();
        
        // ソースディレクトリ内にファイルを作成
        let file_in_source = source_dir.join("test.txt");
        let content = "テストデータ\n";
        let mut file = File::create(&file_in_source).unwrap();
        write!(file, "{}", content).unwrap();

        // 宛先パス
        let dest_dir = temp_path.join("dest_dir");

        // コマンドを実行
        let command = MvCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mv".to_string(),
                source_dir.file_name().unwrap().to_str().unwrap().to_string(),
                dest_dir.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ソースディレクトリが消えていることを確認
        assert!(!source_dir.exists());

        // 宛先ディレクトリが存在することを確認
        assert!(dest_dir.exists());

        // 宛先ディレクトリにファイルが移動していることを確認
        let file_in_dest = dest_dir.join("test.txt");
        assert!(file_in_dest.exists());

        // 内容が保持されていることを確認
        let dest_content = fs::read_to_string(&file_in_dest).unwrap();
        assert_eq!(dest_content, content);
    }

    #[tokio::test]
    async fn test_mv_multiple_sources_to_directory() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 複数のソースファイルを作成
        let source_files = vec!["file1.txt", "file2.txt", "file3.txt"];
        let mut source_paths = Vec::new();
        
        for filename in &source_files {
            let file_path = temp_path.join(filename);
            source_paths.push(file_path.clone());
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "テストデータ: {}", filename).unwrap();
            assert!(file_path.exists());
        }

        // 宛先ディレクトリを作成
        let dest_dir = temp_path.join("dest_dir");
        fs::create_dir(&dest_dir).unwrap();

        // コマンドを実行
        let command = MvCommand;
        let mut args = vec!["mv".to_string()];
        args.extend(source_files.iter().map(|s| s.to_string()));
        args.push(dest_dir.file_name().unwrap().to_str().unwrap().to_string());
        
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

        // すべてのソースファイルが消えていることを確認
        for path in &source_paths {
            assert!(!path.exists());
        }

        // 宛先ディレクトリに全ファイルが移動していることを確認
        for filename in source_files {
            let file_in_dest = dest_dir.join(filename);
            assert!(file_in_dest.exists());
        }
    }

    #[tokio::test]
    async fn test_mv_no_clobber() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースファイルを作成
        let source_path = temp_path.join("source.txt");
        let source_content = "ソースファイルのテストデータ\n";
        let mut file = File::create(&source_path).unwrap();
        write!(file, "{}", source_content).unwrap();

        // 宛先ファイルも作成（既に存在する状態）
        let dest_path = temp_path.join("dest.txt");
        let dest_content = "宛先ファイルのテストデータ\n";
        let mut file = File::create(&dest_path).unwrap();
        write!(file, "{}", dest_content).unwrap();

        // コマンドを実行（-n オプションあり）
        let command = MvCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mv".to_string(),
                "-n".to_string(),
                source_path.file_name().unwrap().to_str().unwrap().to_string(),
                dest_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ソースファイルはまだ存在する（移動されなかった）
        assert!(source_path.exists());

        // 宛先ファイルは上書きされていない
        let final_content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(final_content, dest_content);
    }

    #[tokio::test]
    async fn test_mv_force() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースファイルを作成
        let source_path = temp_path.join("source.txt");
        let source_content = "ソースファイルのテストデータ\n";
        let mut file = File::create(&source_path).unwrap();
        write!(file, "{}", source_content).unwrap();

        // 宛先ファイルも作成（既に存在する状態）
        let dest_path = temp_path.join("dest.txt");
        let dest_content = "宛先ファイルのテストデータ\n";
        let mut file = File::create(&dest_path).unwrap();
        write!(file, "{}", dest_content).unwrap();

        // コマンドを実行（-f オプションあり）
        let command = MvCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mv".to_string(),
                "-f".to_string(),
                source_path.file_name().unwrap().to_str().unwrap().to_string(),
                dest_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ソースファイルは消えている（移動された）
        assert!(!source_path.exists());

        // 宛先ファイルは上書きされている
        let final_content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(final_content, source_content);
    }
} 