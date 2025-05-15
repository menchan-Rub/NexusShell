use std::collections::HashMap;
use std::path::Path;
use std::fs;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command, ArgMatches};

use crate::BuiltinCommand;

/// ファイルテスト、文字列比較、数値比較等を行うtestコマンド
pub struct TestCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl TestCommand {
    /// 新しいTestCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "test".to_string(),
            description: "条件式の評価を行います".to_string(),
            usage: "test 式 または [ 式 ]".to_string(),
        }
    }
    
    /// コマンドパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("test")
            .about("条件式の評価を行います")
            .arg(
                Arg::new("expressions")
                    .help("評価する条件式")
                    .action(ArgAction::Append)
            )
    }
    
    /// 条件式を評価
    fn evaluate_expression(&self, args: &[String]) -> Result<bool> {
        if args.is_empty() {
            return Ok(false); // 引数がなければfalse
        }
        
        // 引数が1つの場合は、それが空でなければtrue
        if args.len() == 1 {
            return Ok(!args[0].is_empty());
        }
        
        // 後置の ']' があれば削除
        let mut filtered_args = args.to_vec();
        if filtered_args.last().map_or(false, |s| s == "]") {
            filtered_args.pop();
        }
        
        if filtered_args.is_empty() {
            return Ok(false);
        }
        
        // 単項演算子の処理
        match filtered_args[0].as_str() {
            "-n" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-n' には1つの引数が必要です"));
                }
                return Ok(!filtered_args[1].is_empty());
            },
            "-z" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-z' には1つの引数が必要です"));
                }
                return Ok(filtered_args[1].is_empty());
            },
            "-d" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-d' には1つの引数が必要です"));
                }
                return Ok(Path::new(&filtered_args[1]).is_dir());
            },
            "-e" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-e' には1つの引数が必要です"));
                }
                return Ok(Path::new(&filtered_args[1]).exists());
            },
            "-f" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-f' には1つの引数が必要です"));
                }
                return Ok(Path::new(&filtered_args[1]).is_file());
            },
            "-s" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-s' には1つの引数が必要です"));
                }
                return Ok(match fs::metadata(&filtered_args[1]) {
                    Ok(metadata) => metadata.len() > 0,
                    Err(_) => false,
                });
            },
            "-r" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-r' には1つの引数が必要です"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    return Ok(match fs::metadata(&filtered_args[1]) {
                        Ok(metadata) => (metadata.permissions().mode() & 0o444) != 0,
                        Err(_) => false,
                    });
                }
                #[cfg(not(unix))]
                {
                    // Windows環境では簡易的にファイルの存在チェックのみ
                    return Ok(Path::new(&filtered_args[1]).exists());
                }
            },
            "-w" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-w' には1つの引数が必要です"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    return Ok(match fs::metadata(&filtered_args[1]) {
                        Ok(metadata) => (metadata.permissions().mode() & 0o222) != 0,
                        Err(_) => false,
                    });
                }
                #[cfg(not(unix))]
                {
                    // Windows環境では簡易的に読み取り専用チェック
                    return Ok(match fs::metadata(&filtered_args[1]) {
                        Ok(metadata) => !metadata.permissions().readonly(),
                        Err(_) => false,
                    });
                }
            },
            "-x" => {
                if filtered_args.len() != 2 {
                    return Err(anyhow!("'-x' には1つの引数が必要です"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    return Ok(match fs::metadata(&filtered_args[1]) {
                        Ok(metadata) => (metadata.permissions().mode() & 0o111) != 0,
                        Err(_) => false,
                    });
                }
                #[cfg(not(unix))]
                {
                    // Windows環境では拡張子をチェック
                    let path = Path::new(&filtered_args[1]);
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        return Ok(ext_str == "exe" || ext_str == "bat" || ext_str == "cmd");
                    }
                    return Ok(false);
                }
            },
            "!" => {
                if filtered_args.len() < 2 {
                    return Err(anyhow!("'!' 演算子には引数が必要です"));
                }
                return Ok(!self.evaluate_expression(&filtered_args[1..])?)
            },
            _ => {}
        }
        
        // 二項演算子の処理
        if filtered_args.len() == 3 {
            let left = &filtered_args[0];
            let op = &filtered_args[1];
            let right = &filtered_args[2];
            
            match op.as_str() {
                "=" => return Ok(left == right),
                "!=" => return Ok(left != right),
                "-eq" => {
                    // 数値として比較
                    if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right.parse::<i64>()) {
                        return Ok(a == b);
                    }
                    return Err(anyhow!("'-eq' には数値が必要です"));
                },
                "-ne" => {
                    if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right.parse::<i64>()) {
                        return Ok(a != b);
                    }
                    return Err(anyhow!("'-ne' には数値が必要です"));
                },
                "-gt" => {
                    if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right.parse::<i64>()) {
                        return Ok(a > b);
                    }
                    return Err(anyhow!("'-gt' には数値が必要です"));
                },
                "-ge" => {
                    if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right.parse::<i64>()) {
                        return Ok(a >= b);
                    }
                    return Err(anyhow!("'-ge' には数値が必要です"));
                },
                "-lt" => {
                    if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right.parse::<i64>()) {
                        return Ok(a < b);
                    }
                    return Err(anyhow!("'-lt' には数値が必要です"));
                },
                "-le" => {
                    if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right.parse::<i64>()) {
                        return Ok(a <= b);
                    }
                    return Err(anyhow!("'-le' には数値が必要です"));
                },
                _ => return Err(anyhow!("無効な演算子: {}", op)),
            }
        }
        
        // AND と OR の処理
        for i in 0..filtered_args.len() {
            if filtered_args[i] == "-a" && i > 0 && i < filtered_args.len() - 1 {
                let left = self.evaluate_expression(&filtered_args[0..i])?;
                let right = self.evaluate_expression(&filtered_args[i+1..])?;
                return Ok(left && right);
            } else if filtered_args[i] == "-o" && i > 0 && i < filtered_args.len() - 1 {
                let left = self.evaluate_expression(&filtered_args[0..i])?;
                let right = self.evaluate_expression(&filtered_args[i+1..])?;
                return Ok(left || right);
            }
        }
        
        // ここに来たら解析できなかった
        Err(anyhow!("無効な条件式: {:?}", filtered_args))
    }
}

impl BuiltinCommand for TestCommand {
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
        // 引数処理
        let command_name = if !args.is_empty() { args[0].clone() } else { "test".to_string() };
        
        // `[` コマンドとして呼び出された場合は特別処理
        let is_bracket_form = command_name == "[";
        let mut expressions = if is_bracket_form {
            // 最初と最後の括弧を除去
            args[1..].to_vec() 
        } else {
            // 通常の test コマンド
            if args.len() > 1 {
                args[1..].to_vec()
            } else {
                Vec::new()
            }
        };
        
        // 括弧形式の場合、最後の引数が ']' であることを確認
        if is_bracket_form && (expressions.is_empty() || expressions.last().unwrap() != "]") {
            return Err(anyhow!("']' が閉じられていません"));
        }
        
        // 条件式を評価
        let result = match self.evaluate_expression(&expressions) {
            Ok(val) => val,
            Err(e) => {
                // エラーが発生した場合はメッセージを表示して戻り値をfalseにする
                eprintln!("{}: {}", command_name, e);
                false
            }
        };
        
        // 終了コードを環境変数に設定
        env.insert("?".to_string(), if result { "0" } else { "1" }.to_string());
        
        // コマンドは通常何も出力しない
        Ok("".to_string())
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {} または [ 式 ]\n\n主な条件式:\n  文字列テスト:\n    -n 文字列    文字列の長さが0より大きければtrue\n    -z 文字列    文字列の長さが0ならtrue\n    文字列1 = 文字列2   文字列が等しければtrue\n    文字列1 != 文字列2  文字列が等しくなければtrue\n\n  ファイルテスト:\n    -d ファイル   ファイルがディレクトリならtrue\n    -e ファイル   ファイルが存在すればtrue\n    -f ファイル   ファイルが通常ファイルならtrue\n    -r ファイル   ファイルが読み取り可能ならtrue\n    -s ファイル   ファイルが存在しサイズが0より大きければtrue\n    -w ファイル   ファイルが書き込み可能ならtrue\n    -x ファイル   ファイルが実行可能ならtrue\n\n  数値比較:\n    n1 -eq n2    n1 = n2 ならtrue\n    n1 -ne n2    n1 ≠ n2 ならtrue\n    n1 -gt n2    n1 > n2 ならtrue\n    n1 -ge n2    n1 ≥ n2 ならtrue\n    n1 -lt n2    n1 < n2 ならtrue\n    n1 -le n2    n1 ≤ n2 ならtrue\n\n  論理演算子:\n    ! 式        式の否定\n    式1 -a 式2   式1かつ式2（AND）\n    式1 -o 式2   式1または式2（OR）\n\n例:\n  test -d /usr/bin              /usr/binがディレクトリかチェック\n  [ -f \"$HOME/.bashrc\" ]        .bashrcが通常ファイルかチェック\n  [ $count -eq 0 -o $count -gt 10 ]  countが0または10より大きいかチェック\n",
            self.description,
            self.usage
        )
    }
} 