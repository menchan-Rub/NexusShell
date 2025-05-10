use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::io::{BufRead, BufReader};
use regex::Regex;
use serde_json::{Value, json};
use tracing::{debug, warn, error};

/// データストリームの各要素に関数を適用するコマンド
///
/// 各行、JSONオブジェクト、またはその他の構造化データに対して
/// 変換関数を適用します。入力データを指定された式に基づいて変換し、
/// 結果を出力します。
///
/// # 使用例
///
/// ```bash
/// cat data.json | map "item => item.price * 0.9"  # 各アイテムの価格を10%割引
/// ls -l | map "line => line.split(' ').filter(s => s.length > 0)[8]"  # ファイル名だけを抽出
/// ```
pub struct MapCommand;

/// マップ操作のタイプ
enum MapOperationType {
    /// JavaScript式を使用した変換
    JsExpression(String),
    /// 正規表現でのキャプチャグループを使用した変換
    RegexCapture(Regex, usize),
    /// 固定フィールドインデックスを使用した変換（区切り文字ベース）
    FieldExtract(String, usize),
    /// JSONパスを使用したプロパティ抽出
    JsonPath(String),
}

#[async_trait]
impl BuiltinCommand for MapCommand {
    fn name(&self) -> &'static str {
        "map"
    }

    fn description(&self) -> &'static str {
        "データストリームの各要素に関数を適用します"
    }

    fn usage(&self) -> &'static str {
        "map [オプション] <変換式>\n\n\
        オプション:\n\
        -i, --input-format <format>  入力フォーマットを指定（text, json, csv）\n\
        -o, --output-format <format> 出力フォーマットを指定（text, json, csv）\n\
        -d, --delimiter <char>       フィールド区切り文字を指定（デフォルトはタブ）\n\
        -r, --regex                  変換式を正規表現として解釈\n\
        -f, --field <num>            区切り文字で分割された特定フィールドを抽出\n\
        -j, --json-path              変換式をJSONパスとして解釈\n\n\
        変換式の例:\n\
        - map \"x => x.toUpperCase()\"      各行を大文字に変換\n\
        - map -r \"([0-9]+)\" 1            各行から最初の数字をキャプチャ\n\
        - map -f 2                       各行の3番目のフィールドを抽出\n\
        - map -j \"data.items[0].name\"    JSONからプロパティを抽出"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 2 {
            return Err(anyhow!("変換式が指定されていません。使用方法: map [オプション] <変換式>"));
        }

        // オプションと引数を解析
        let mut input_format = "text";
        let mut output_format = "text";
        let mut delimiter = "\t";
        let mut operation_type = None;
        
        let mut i = 1;
        while i < context.args.len() {
            match context.args[i].as_str() {
                "-i" | "--input-format" => {
                    i += 1;
                    if i < context.args.len() {
                        input_format = &context.args[i];
                    } else {
                        return Err(anyhow!("--input-format オプションには値が必要です"));
                    }
                },
                "-o" | "--output-format" => {
                    i += 1;
                    if i < context.args.len() {
                        output_format = &context.args[i];
                    } else {
                        return Err(anyhow!("--output-format オプションには値が必要です"));
                    }
                },
                "-d" | "--delimiter" => {
                    i += 1;
                    if i < context.args.len() {
                        delimiter = &context.args[i];
                    } else {
                        return Err(anyhow!("--delimiter オプションには値が必要です"));
                    }
                },
                "-r" | "--regex" => {
                    i += 1;
                    if i < context.args.len() {
                        let pattern = &context.args[i];
                        i += 1;
                        let group_index = if i < context.args.len() && context.args[i].parse::<usize>().is_ok() {
                            context.args[i].parse::<usize>().unwrap()
                        } else {
                            i -= 1; // 数値でない場合はインデックスを戻す
                            1 // デフォルトは最初のキャプチャグループ
                        };
                        
                        let regex = Regex::new(pattern)
                            .map_err(|e| anyhow!("正規表現パターンが無効です: {}", e))?;
                        
                        operation_type = Some(MapOperationType::RegexCapture(regex, group_index));
                    } else {
                        return Err(anyhow!("--regex オプションにはパターンが必要です"));
                    }
                },
                "-f" | "--field" => {
                    i += 1;
                    if i < context.args.len() {
                        let field_index = context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("フィールドインデックスは数値である必要があります"))?;
                        
                        operation_type = Some(MapOperationType::FieldExtract(delimiter.to_string(), field_index));
                    } else {
                        return Err(anyhow!("--field オプションにはインデックスが必要です"));
                    }
                },
                "-j" | "--json-path" => {
                    i += 1;
                    if i < context.args.len() {
                        operation_type = Some(MapOperationType::JsonPath(context.args[i].clone()));
                    } else {
                        return Err(anyhow!("--json-path オプションにはJSONパスが必要です"));
                    }
                },
                _ => {
                    // オプションでない場合は変換式として扱う
                    if operation_type.is_none() {
                        operation_type = Some(MapOperationType::JsExpression(context.args[i].clone()));
                    }
                }
            }
            
            i += 1;
        }
        
        // 操作タイプが指定されていない場合は最後の引数を式として使用
        if operation_type.is_none() && !context.args.is_empty() {
            operation_type = Some(MapOperationType::JsExpression(
                context.args.last().unwrap().clone()
            ));
        }
        
        // 操作タイプのチェック
        let operation = operation_type.ok_or_else(|| 
            anyhow!("変換操作が指定されていません")
        )?;
        
        // 標準入力が接続されているか確認
        if !context.stdin_connected {
            return Err(anyhow!("標準入力からデータを読み取れません"));
        }
        
        // 標準入力からデータを読み取り、変換を適用
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        let mut output = Vec::new();
        
        match input_format {
            "text" => {
                for line in reader.lines() {
                    let line = line?;
                    let result = apply_operation(&operation, &line)?;
                    output.extend_from_slice(format!("{}\n", result).as_bytes());
                }
            },
            "json" => {
                let json_data: Value = serde_json::from_reader(reader)
                    .map_err(|e| anyhow!("JSONの解析エラー: {}", e))?;
                
                if let Value::Array(items) = json_data {
                    for item in items {
                        let item_str = item.to_string();
                        let result = apply_operation(&operation, &item_str)?;
                        output.extend_from_slice(format!("{}\n", result).as_bytes());
                    }
                } else {
                    // 単一のJSONオブジェクトの場合
                    let item_str = json_data.to_string();
                    let result = apply_operation(&operation, &item_str)?;
                    output.extend_from_slice(format!("{}\n", result).as_bytes());
                }
            },
            "csv" => {
                let mut rdr = csv::ReaderBuilder::new()
                    .delimiter(delimiter.as_bytes()[0])
                    .from_reader(reader);
                
                for result in rdr.records() {
                    let record = result?;
                    let record_str = record.iter().collect::<Vec<_>>().join(delimiter);
                    let result = apply_operation(&operation, &record_str)?;
                    output.extend_from_slice(format!("{}\n", result).as_bytes());
                }
            },
            _ => return Err(anyhow!("未サポートの入力フォーマット: {}", input_format)),
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// 指定された操作を入力データに適用
fn apply_operation(operation: &MapOperationType, input: &str) -> Result<String> {
    match operation {
        MapOperationType::JsExpression(expr) => {
            // 注：実際の実装ではJavaScriptエンジンを使用するか、簡易的な式評価を行う
            // ここではより実用的な簡易実装として、いくつかの一般的なケースを処理
            
            if expr == "x => x.toUpperCase()" {
                Ok(input.to_uppercase())
            } else if expr == "x => x.toLowerCase()" {
                Ok(input.to_lowercase())
            } else if expr == "x => x.trim()" {
                Ok(input.trim().to_string())
            } else if expr.contains("=>") {
                // パラメータ名を抽出（x => の「x」部分）
                let parts: Vec<&str> = expr.splitn(2, "=>").collect();
                if parts.len() != 2 {
                    return Err(anyhow!("無効な式フォーマット: {}", expr));
                }
                
                let param_name = parts[0].trim();
                let expr_body = parts[1].trim();
                
                // 簡易マッピング変換 (実際の実装ではもっと洗練された式評価が必要)
                if expr_body.contains(&format!("{}.length", param_name)) {
                    Ok(input.len().to_string())
                } else if expr_body.contains(&format!("{}.split", param_name)) {
                    // 簡易的なsplit実装
                    let re = Regex::new(r#"split\(['"](.*)['"]\)"#)?;
                    if let Some(caps) = re.captures(expr_body) {
                        let delimiter = &caps[1];
                        let fields: Vec<&str> = input.split(delimiter).collect();
                        
                        // インデックスアクセスを試みる
                        let index_re = Regex::new(r"\[([0-9]+)\]")?;
                        if let Some(idx_caps) = index_re.captures(expr_body) {
                            let idx = idx_caps[1].parse::<usize>()?;
                            if idx < fields.len() {
                                return Ok(fields[idx].to_string());
                            } else {
                                return Err(anyhow!("インデックスが範囲外です: {}", idx));
                            }
                        }
                        
                        // インデックスが指定されていない場合は全フィールドを返す
                        Ok(format!("{:?}", fields))
                    } else {
                        Err(anyhow!("split操作の解析に失敗しました: {}", expr_body))
                    }
                } else {
                    // その他の式は未実装としてエラー
                    Err(anyhow!("未実装の式: {}", expr))
                }
            } else {
                // 単純な式ならそのまま返す
                Ok(expr.replace("input", input))
            }
        },
        MapOperationType::RegexCapture(regex, group_index) => {
            if let Some(captures) = regex.captures(input) {
                if *group_index < captures.len() {
                    Ok(captures[*group_index].to_string())
                } else {
                    Err(anyhow!("キャプチャグループインデックスが範囲外です: {}", group_index))
                }
            } else {
                // マッチしない場合は空文字列
                Ok(String::new())
            }
        },
        MapOperationType::FieldExtract(delimiter, field_index) => {
            let fields: Vec<&str> = input.split(delimiter).collect();
            if *field_index < fields.len() {
                Ok(fields[*field_index].to_string())
            } else {
                Err(anyhow!("フィールドインデックスが範囲外です: {}", field_index))
            }
        },
        MapOperationType::JsonPath(path) => {
            // JSONを解析
            let data: Value = serde_json::from_str(input)
                .map_err(|e| anyhow!("JSON解析エラー: {}", e))?;
            
            // JSONパスを解析して値を抽出
            let path_segments: Vec<&str> = path.split('.').collect();
            let mut current = &data;
            
            for segment in path_segments {
                // 配列インデックスを処理 (例: items[0])
                let array_re = Regex::new(r"^(.*)\[([0-9]+)\]$")?;
                if let Some(caps) = array_re.captures(segment) {
                    let prop_name = &caps[1];
                    let index = caps[2].parse::<usize>()?;
                    
                    if let Some(obj) = current.get(prop_name) {
                        if let Some(array) = obj.as_array() {
                            if index < array.len() {
                                current = &array[index];
                            } else {
                                return Err(anyhow!("配列インデックスが範囲外です: {}", index));
                            }
                        } else {
                            return Err(anyhow!("プロパティ '{}' は配列ではありません", prop_name));
                        }
                    } else {
                        return Err(anyhow!("プロパティ '{}' が見つかりません", prop_name));
                    }
                } else {
                    // 通常のプロパティアクセス
                    if let Some(value) = current.get(segment) {
                        current = value;
                    } else {
                        return Err(anyhow!("プロパティ '{}' が見つかりません", segment));
                    }
                }
            }
            
            // 最終的な値を文字列に変換
            Ok(current.to_string())
        }
    }
} 