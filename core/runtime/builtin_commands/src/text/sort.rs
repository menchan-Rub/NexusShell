use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, Context as AnyhowContext};
use async_trait::async_trait;
use std::fs::File;
use std::io::{self, BufReader, BufRead, Read};
use std::path::Path;
use std::cmp::Ordering;
use tracing::{debug, error, info};

/// テキストファイルの行をソートするコマンド
pub struct SortCommand;

#[async_trait]
impl BuiltinCommand for SortCommand {
    fn name(&self) -> &'static str {
        "sort"
    }

    fn description(&self) -> &'static str {
        "テキストファイルの行をソートする"
    }

    fn usage(&self) -> &'static str {
        "使用法: sort [-r] [-n] [-f] [-u] [-k <フィールド>] [ファイル...]\n\
        \n\
        オプション:\n\
        -r          逆順（降順）でソートする\n\
        -n          数値としてソートする\n\
        -f          大文字小文字を区別せずにソートする\n\
        -u          重複行を削除する\n\
        -k <N>      N番目のフィールド（列）でソートする（1始まり）\n\
        \n\
        ファイルが指定されていない場合は標準入力から読み込む"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let mut args = context.args.iter().skip(1);
        let mut reverse = false;
        let mut numeric_sort = false;
        let mut ignore_case = false;
        let mut unique = false;
        let mut key_field = 0; // 0はフィールド指定なし
        let mut files = Vec::new();

        // 引数の解析
        while let Some(arg) = args.next() {
            if arg.starts_with("-") && arg.len() > 1 && !arg.starts_with("--") {
                // -rnfu のような複合オプション対応
                for c in arg.chars().skip(1) {
                    match c {
                        'r' => reverse = true,
                        'n' => numeric_sort = true,
                        'f' => ignore_case = true,
                        'u' => unique = true,
                        'k' => {
                            if let Some(field_arg) = args.next() {
                                match field_arg.parse::<usize>() {
                                    Ok(n) if n > 0 => key_field = n,
                                    _ => {
                                        return Ok(CommandResult::failure(1)
                                            .with_stderr(format!("エラー: -k の後には正の整数を指定してください\n").into_bytes()));
                                    }
                                }
                            } else {
                                return Ok(CommandResult::failure(1)
                                    .with_stderr(format!("エラー: -k の後にはフィールド番号が必要です\n").into_bytes()));
                            }
                        },
                        _ => {
                            return Ok(CommandResult::failure(1)
                                .with_stderr(format!("エラー: 不明なオプション: -{}\n{}", c, self.usage()).into_bytes()));
                        }
                    }
                }
            } else {
                files.push(arg.clone());
            }
        }

        let mut result = CommandResult::success();
        let mut lines = Vec::new();

        // ファイルが指定されていない場合は標準入力から読み込む
        if files.is_empty() {
            if !context.stdin_connected {
                return Ok(CommandResult::failure(1)
                    .with_stderr("エラー: 標準入力が接続されていません".into_bytes()));
            }
            
            // 標準入力の処理はスタブとして残す
            // 本来は stdin から読み込むべきだが、このコードでは空文字列を返す
            let content = String::new();
            collect_lines(&content, &mut lines);
        } else {
            // 各ファイルを処理
            for file_path in &files {
                let path = context.current_dir.join(file_path);
                
                match read_file_lines(&path, &mut lines) {
                    Ok(_) => (),
                    Err(err) => {
                        let err_msg = format!("エラー: {}: {}\n", file_path, err);
                        result.stderr.extend_from_slice(err_msg.as_bytes());
                        result.exit_code = 1;
                    }
                }
            }
        }

        // ソート処理
        sort_lines(&mut lines, reverse, numeric_sort, ignore_case, unique, key_field);

        // 結果を出力バッファに書き込み
        let mut output = Vec::new();
        for line in lines {
            output.extend_from_slice(line.as_bytes());
            output.push(b'\n');
        }

        result.stdout = output;
        Ok(result)
    }
}

// ファイルから行を読み込む関数
fn read_file_lines(path: &Path, lines: &mut Vec<String>) -> io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    for line_result in reader.lines() {
        let line = line_result?;
        lines.push(line);
    }
    
    Ok(())
}

// 文字列から行を収集する関数（標準入力用）
fn collect_lines(content: &str, lines: &mut Vec<String>) {
    for line in content.lines() {
        lines.push(line.to_string());
    }
}

// 行のソート関数
fn sort_lines(
    lines: &mut Vec<String>,
    reverse: bool,
    numeric_sort: bool,
    ignore_case: bool,
    unique: bool,
    key_field: usize
) {
    lines.sort_by(|a, b| {
        let a_key = extract_sort_key(a, key_field);
        let b_key = extract_sort_key(b, key_field);

        let ordering = if numeric_sort {
            compare_numeric(&a_key, &b_key)
        } else if ignore_case {
            a_key.to_lowercase().cmp(&b_key.to_lowercase())
        } else {
            a_key.cmp(&b_key)
        };

        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });

    // 重複行を削除（uniqueオプションが指定されている場合）
    if unique {
        lines.dedup();
    }
}

// ソート用のキー（フィールド）を抽出する関数
fn extract_sort_key(line: &str, key_field: usize) -> String {
    if key_field == 0 {
        // フィールド指定なしの場合は行全体を返す
        return line.to_string();
    }

    // スペースで区切ってフィールドを取得
    let fields: Vec<&str> = line.split_whitespace().collect();
    
    // 指定されたフィールドが存在する場合はそれを返す、なければ空文字列
    if key_field <= fields.len() {
        fields[key_field - 1].to_string()
    } else {
        "".to_string()
    }
}

// 数値としての比較を行う関数
fn compare_numeric(a: &str, b: &str) -> Ordering {
    let a_num = a.parse::<f64>().unwrap_or(f64::NAN);
    let b_num = b.parse::<f64>().unwrap_or(f64::NAN);

    if a_num.is_nan() && b_num.is_nan() {
        // 両方数値に変換できない場合は文字列として比較
        a.cmp(b)
    } else if a_num.is_nan() {
        // aが数値でない場合は大きいとみなす
        Ordering::Greater
    } else if b_num.is_nan() {
        // bが数値でない場合は小さいとみなす
        Ordering::Less
    } else {
        // 両方数値の場合は数値として比較
        a_num.partial_cmp(&b_num).unwrap_or(Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sort_basic() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "banana\napple\ncherry\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
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
        assert_eq!(output, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn test_sort_reverse() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "banana\napple\ncherry\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（逆順）
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
                "-r".to_string(),
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
        assert_eq!(output, "cherry\nbanana\napple\n");
    }

    #[tokio::test]
    async fn test_sort_numeric() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "10\n2\n1\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（数値ソート）
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
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
        assert_eq!(output, "1\n2\n10\n");
    }

    #[tokio::test]
    async fn test_sort_ignore_case() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "Banana\napple\nCherry\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（大文字小文字区別なし）
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
                "-f".to_string(),
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
        assert_eq!(output, "apple\nBanana\nCherry\n");
    }

    #[tokio::test]
    async fn test_sort_unique() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "banana\napple\nbanana\ncherry\napple\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（重複削除）
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
                "-u".to_string(),
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
        assert_eq!(output, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn test_sort_key_field() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "2 banana\n1 apple\n3 cherry\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（フィールド指定）
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
                "-k".to_string(),
                "1".to_string(),
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
        assert_eq!(output, "1 apple\n2 banana\n3 cherry\n");
    }

    #[tokio::test]
    async fn test_sort_numeric_key_field() {
        // 一時ディレクトリを作成
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // テスト用ファイルを作成
        let file_path = temp_path.join("test.txt");
        let content = "banana 10\napple 2\ncherry 1\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // コマンドを実行（フィールド指定と数値ソート）
        let command = SortCommand;
        let context = CommandContext {
            current_dir: temp_path.to_path_buf(),
            env_vars: std::collections::HashMap::new(),
            args: vec![
                "sort".to_string(),
                "-n".to_string(),
                "-k".to_string(),
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
        assert_eq!(output, "cherry 1\napple 2\nbanana 10\n");
    }
} 