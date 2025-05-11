use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use std::fs;
use std::path::Path;
use tracing::{debug, error, info};

/// ディレクトリを作成するコマンド
pub struct MkdirCommand;

#[async_trait]
impl BuiltinCommand for MkdirCommand {
    fn name(&self) -> &'static str {
        "mkdir"
    }

    fn description(&self) -> &'static str {
        "ディレクトリを作成する"
    }

    fn usage(&self) -> &'static str {
        "使用法: mkdir [-p] <ディレクトリ名> [<ディレクトリ名>...]\n\
        \n\
        オプション:\n\
        -p, --parents    必要に応じて親ディレクトリも作成する"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 2 {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: ディレクトリ名が指定されていません\n{}", self.usage()).into_bytes()));
        }

        let mut args = context.args.iter().skip(1);
        let mut create_parents = false;
        let mut directories = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") {
                match arg.as_str() {
                    "-p" | "--parents" => create_parents = true,
                    _ => {
                        return Ok(CommandResult::failure(1)
                            .with_stderr(format!("エラー: 不明なオプション: {}\n{}", arg, self.usage()).into_bytes()));
                    }
                }
            } else {
                directories.push(arg);
            }
        }

        if directories.is_empty() {
            return Ok(CommandResult::failure(1)
                .with_stderr(format!("エラー: ディレクトリ名が指定されていません\n{}", self.usage()).into_bytes()));
        }

        // 各ディレクトリを作成
        for dir in directories {
            let dir_path = context.current_dir.join(dir);
            
            let result = if create_parents {
                fs::create_dir_all(&dir_path)
            } else {
                fs::create_dir(&dir_path)
            };

            if let Err(err) = result {
                return Ok(CommandResult::failure(1)
                    .with_stderr(format!("エラー: ディレクトリ '{}' の作成に失敗しました: {}", dir, err).into_bytes()));
            }

            debug!("ディレクトリを作成しました: {}", dir_path.display());
        }

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_mkdir_single_directory() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 新しいディレクトリ名
        let new_dir_name = "test_dir";
        let new_dir_path = temp_path.join(new_dir_name);

        // コマンドを実行
        let command = MkdirCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mkdir".to_string(),
                new_dir_name.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // ディレクトリが存在することを確認
        assert!(new_dir_path.exists());
        assert!(new_dir_path.is_dir());
    }

    #[tokio::test]
    async fn test_mkdir_multiple_directories() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 新しいディレクトリ名
        let dir_names = vec!["dir1", "dir2", "dir3"];
        
        // コマンドを実行
        let command = MkdirCommand;
        let mut args = vec!["mkdir".to_string()];
        args.extend(dir_names.iter().map(|s| s.to_string()));
        
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

        // 各ディレクトリが存在することを確認
        for dir_name in dir_names {
            let dir_path = temp_path.join(dir_name);
            assert!(dir_path.exists());
            assert!(dir_path.is_dir());
        }
    }

    #[tokio::test]
    async fn test_mkdir_with_parents() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 深いパス構造
        let deep_path = "parent/child/grandchild";
        
        // コマンドを実行
        let command = MkdirCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mkdir".to_string(),
                "-p".to_string(),
                deep_path.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);

        // 深いパスが作成されたことを確認
        let full_path = temp_path.join(deep_path);
        assert!(full_path.exists());
        assert!(full_path.is_dir());
        
        // 中間ディレクトリも作成されたことを確認
        let parent_path = temp_path.join("parent");
        let child_path = parent_path.join("child");
        
        assert!(parent_path.exists());
        assert!(parent_path.is_dir());
        assert!(child_path.exists());
        assert!(child_path.is_dir());
    }

    #[tokio::test]
    async fn test_mkdir_without_parents_fails() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 存在しない親ディレクトリを含むパス
        let invalid_path = "nonexistent/directory";
        
        // コマンドを実行
        let command = MkdirCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "mkdir".to_string(),
                invalid_path.to_string(),
            ],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };

        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        // エラーメッセージに「失敗しました」が含まれていることを確認
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("失敗しました"));
        
        // ディレクトリが作成されていないことを確認
        let path = temp_path.join(invalid_path);
        assert!(!path.exists());
    }
} 