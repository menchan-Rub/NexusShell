use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow, Context};
use clap::{Arg, ArgAction, Command};
use crate::parser::NexusParser;
use crate::executor::NexusExecutor;
use crate::CommandResult;

use crate::BuiltinCommand;

/// スクリプトファイル実行コマンド
pub struct SourceCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl SourceCommand {
    /// 新しいSourceCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "source".to_string(),
            description: "指定されたファイルを現在のシェル環境で実行します".to_string(),
            usage: "source ファイル [引数 ...]".to_string(),
        }
    }
    
    /// オプションパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("source")
            .about("指定されたファイルを現在のシェル環境で実行します")
            .arg(
                Arg::new("file")
                    .help("実行するファイル")
                    .required(true)
            )
            .arg(
                Arg::new("args")
                    .help("スクリプトに渡す引数")
                    .num_args(0..)
            )
    }
    
    /// ファイルパスを解決する
    fn resolve_path(&self, file_path: &str, env: &HashMap<String, String>) -> Result<PathBuf> {
        // ファイルパスが絶対パスの場合はそのまま返す
        let path = Path::new(file_path);
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        
        // 相対パスの場合、カレントディレクトリからの相対パスとして解決
        let current_dir = env::current_dir().context("カレントディレクトリの取得に失敗しました")?;
        let full_path = current_dir.join(path);
        
        if full_path.exists() {
            return Ok(full_path);
        }
        
        // 環境変数PATHからの検索
        if let Some(path_var) = env.get("PATH") {
            for dir in path_var.split(':') {
                let test_path = Path::new(dir).join(file_path);
                if test_path.exists() {
                    return Ok(test_path);
                }
            }
        }
        
        Err(anyhow!("ファイル '{}' が見つかりません", file_path))
    }
    
    /// スクリプトファイルを実行
    async fn execute_script(&self, file_path: &Path, args: Vec<&str>, env: &mut HashMap<String, String>, context: &CommandContext) -> Result<String> {
        if !file_path.exists() {
            return Err(anyhow!("ファイル '{}' が見つかりません", file_path.display()));
        }
        let file = File::open(file_path)
            .with_context(|| format!("ファイル '{}' を開けませんでした", file_path.display()))?;
        let reader = BufReader::new(file);
        let parser = NexusParser::new();
        let executor = NexusExecutor::new();
        let mut output = String::new();
        env.insert("0".to_string(), file_path.to_string_lossy().to_string());
        for (i, arg) in args.iter().enumerate() {
            env.insert((i + 1).to_string(), arg.to_string());
        }
        env.insert("#".to_string(), args.len().to_string());
        for (line_num, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    return Err(anyhow!("source: {}行目の読み込みに失敗: {}", line_num + 1, e));
                }
            };
            let ast = parser.parse_line(&line).map_err(|e| anyhow!("source: {}行目の構文エラー: {}", line_num + 1, e))?;
            let exec_result = executor.execute_ast(&ast, context).await.map_err(|e| anyhow!("source: {}行目の実行エラー: {}", line_num + 1, e))?;
            output.push_str(&exec_result.stdout_as_string());
            // 環境変数の反映（例: export等）
            for (k, v) in exec_result.env_changes.iter() {
                env.insert(k.clone(), v.clone());
            }
        }
        Ok(output)
    }
}

impl BuiltinCommand for SourceCommand {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn usage(&self) -> &str {
        &self.usage
    }
    
    fn execute(&self, args: Vec<String>, env: &mut HashMap<String, String>) -> Result<String> {
        // 引数解析
        let matches = match self.build_parser().try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("引数解析エラー: {}", e)),
        };
        
        // ファイルパスを取得
        let file_path = matches.get_one::<String>("file")
            .ok_or_else(|| anyhow!("ファイルパスが指定されていません"))?;
        
        // 引数を取得
        let script_args: Vec<&str> = matches.get_many::<String>("args")
            .map(|args| args.map(|s| s.as_str()).collect())
            .unwrap_or_default();
        
        // ファイルパスを解決
        let resolved_path = self.resolve_path(file_path, env)?;
        
        // スクリプトを実行
        self.execute_script(&resolved_path, script_args, env, &CommandContext::default())
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\n引数:\n  ファイル    実行するスクリプトファイル\n  引数       スクリプトに渡す引数\n",
            self.description,
            self.usage
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
    
    #[test]
    fn test_resolve_path() {
        let cmd = SourceCommand::new();
        let mut env = HashMap::new();
        
        // テスト用の一時ディレクトリを作成
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_script.sh");
        
        {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(b"# テストスクリプト\necho 'hello world'\n").unwrap();
        }
        
        // 絶対パスのテスト
        let absolute_path = file_path.to_str().unwrap();
        let result = cmd.resolve_path(absolute_path, &env).unwrap();
        assert_eq!(result, file_path);
        
        // 相対パスのテスト（この場合、テンポラリディレクトリに移動する必要がある）
        let current_dir = env::current_dir().unwrap();
        env::set_current_dir(dir.path()).unwrap();
        let result = cmd.resolve_path("test_script.sh", &env).unwrap();
        assert_eq!(result, file_path);
        
        // 元のディレクトリに戻す
        env::set_current_dir(current_dir).unwrap();
    }
    
    #[test]
    fn test_execute_script() {
        let cmd = SourceCommand::new();
        let mut env = HashMap::new();
        
        // テスト用の一時ディレクトリを作成
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_script.sh");
        
        {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(b"# テストスクリプト\nTEST_VAR=hello\necho $TEST_VAR world\n").unwrap();
        }
        
        // スクリプトを実行
        let result = cmd.execute_script(&file_path, vec!["arg1", "arg2"], &mut env, &CommandContext::default()).unwrap();
        
        // スクリプトの実行結果を検証
        assert!(result.contains("実行: echo $TEST_VAR world"));
        
        // 環境変数が設定されていることを確認
        assert_eq!(env.get("TEST_VAR"), Some(&"hello".to_string()));
        
        // 引数が設定されていることを確認
        assert_eq!(env.get("0"), Some(&file_path.to_string_lossy().to_string()));
        assert_eq!(env.get("1"), Some(&"arg1".to_string()));
        assert_eq!(env.get("2"), Some(&"arg2".to_string()));
        assert_eq!(env.get("#"), Some(&"2".to_string()));
    }
} 