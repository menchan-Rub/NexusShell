use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use std::fs;
use std::path::Path;
use std::io;
use tracing::{debug, error, info};

/// ファイルやディレクトリをコピーするコマンド
pub struct CpCommand;

#[async_trait]
impl BuiltinCommand for CpCommand {
    fn name(&self) -> &'static str {
        "cp"
    }

    fn description(&self) -> &'static str {
        "ファイルやディレクトリをコピーする"
    }

    fn usage(&self) -> &'static str {
        "使用法: cp [-r] <ソース> <宛先>\n\
        \n\
        オプション:\n\
        -r, --recursive    ディレクトリを再帰的にコピーする"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 3 {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: 引数が不足しています\n{}", self.usage()).into_bytes()));
        }

        let mut args = context.args.iter().skip(1);
        let mut recursive = false;
        let mut source_path = None;
        let mut dest_path = None;

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") {
                match arg.as_str() {
                    "-r" | "--recursive" => recursive = true,
                    _ => {
                        return Ok(CommandResult::failure(1)
                            .with_stderr(format!("エラー: 不明なオプション: {}\n{}", arg, self.usage()).into_bytes()));
                    }
                }
            } else if source_path.is_none() {
                source_path = Some(arg);
            } else if dest_path.is_none() {
                dest_path = Some(arg);
            } else {
                // 追加の引数があれば警告
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: 余分な引数: {}\n{}", arg, self.usage()).into_bytes()));
            }
        }

        let source = match source_path {
            Some(path) => path,
            None => return Ok(CommandResult::failure(1)
                .with_stderr("エラー: ソースパスが指定されていません".into_bytes())),
        };

        let dest = match dest_path {
            Some(path) => path,
            None => return Ok(CommandResult::failure(1)
                .with_stderr("エラー: 宛先パスが指定されていません".into_bytes())),
        };

        // 現在のディレクトリからの相対パスを解決
        let source_path = context.current_dir.join(source);
        let dest_path = context.current_dir.join(dest);

        // ソースパスが存在するか確認
        if !source_path.exists() {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: ソースパス '{}' が存在しません", source).into_bytes()));
        }

        // コピー処理
        if source_path.is_dir() {
            if recursive {
                copy_dir_all(&source_path, &dest_path).context("ディレクトリのコピーに失敗しました")?;
            } else {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: '{}' はディレクトリです。ディレクトリをコピーするには -r オプションを使用してください", source).into_bytes()));
            }
        } else {
            // 宛先がディレクトリの場合、その中にファイルをコピー
            let final_dest = if dest_path.is_dir() {
                dest_path.join(source_path.file_name().unwrap())
            } else {
                dest_path
            };
            
            fs::copy(&source_path, &final_dest)
                .context(format!("ファイル '{}' から '{}' へのコピーに失敗しました", 
                    source_path.display(), final_dest.display()))?;
        }

        Ok(CommandResult::success())
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
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_cp_file() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースファイルを作成
        let source_path = temp_path.join("source.txt");
        let mut source_file = File::create(&source_path).unwrap();
        writeln!(source_file, "テストデータ").unwrap();

        // 宛先パス
        let dest_path = temp_path.join("dest.txt");

        // コマンドを実行
        let command = CpCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cp".to_string(), 
                source_path.file_name().unwrap().to_str().unwrap().to_string(),
                dest_path.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // 宛先ファイルが存在することを確認
        assert!(dest_path.exists());

        // 内容が同じことを確認
        let source_content = fs::read_to_string(&source_path).unwrap();
        let dest_content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(source_content, dest_content);
    }

    #[tokio::test]
    async fn test_cp_directory_recursive() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // ソースディレクトリ構造を作成
        let source_dir = temp_path.join("source_dir");
        fs::create_dir(&source_dir).unwrap();
        
        // ソースディレクトリにファイルを作成
        let source_file = source_dir.join("test.txt");
        let mut file = File::create(&source_file).unwrap();
        writeln!(file, "テストデータ").unwrap();

        // サブディレクトリを作成
        let source_subdir = source_dir.join("subdir");
        fs::create_dir(&source_subdir).unwrap();
        
        // サブディレクトリにファイルを作成
        let subdir_file = source_subdir.join("subtest.txt");
        let mut file = File::create(&subdir_file).unwrap();
        writeln!(file, "サブディレクトリのテストデータ").unwrap();

        // 宛先ディレクトリ
        let dest_dir = temp_path.join("dest_dir");

        // コマンドを実行
        let command = CpCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "cp".to_string(),
                "-r".to_string(),
                source_dir.file_name().unwrap().to_str().unwrap().to_string(),
                dest_dir.file_name().unwrap().to_str().unwrap().to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // 宛先ディレクトリが存在することを確認
        assert!(dest_dir.exists());

        // コピーされたファイルが存在することを確認
        let dest_file = dest_dir.join("test.txt");
        assert!(dest_file.exists());

        // コピーされたサブディレクトリとファイルが存在することを確認
        let dest_subdir = dest_dir.join("subdir");
        assert!(dest_subdir.exists());
        
        let dest_subdir_file = dest_subdir.join("subtest.txt");
        assert!(dest_subdir_file.exists());

        // 内容が同じことを確認
        let source_content = fs::read_to_string(&source_file).unwrap();
        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert_eq!(source_content, dest_content);

        let source_subdir_content = fs::read_to_string(&subdir_file).unwrap();
        let dest_subdir_content = fs::read_to_string(&dest_subdir_file).unwrap();
        assert_eq!(source_subdir_content, dest_subdir_content);
    }
} 