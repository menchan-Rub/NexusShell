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
            // 高度なJavaScript式エンジンを使用して式を評価
            // 複数のバックエンドから最適なものを選択
            evaluate_js_expression(expr, input)
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

/// JavaScriptエンジンを使った式の評価
/// 複数のJavaScriptエバリュエーターエンジンをサポート
fn evaluate_js_expression(expr: &str, param: &str) -> Result<String> {
    // 1. 最速: 単純な式は特殊な高速評価器で処理
    if let Ok(result) = fast_expression_eval(expr, param) {
        debug!("高速評価器で式を処理: {}", expr);
        return Ok(result);
    }
    // 2. Rhai（安全なサンドボックス）
    #[cfg(feature = "js-rhai")]
    {
        match evaluate_with_rhai(expr, param) {
            Ok(result) => {
                debug!("Rhaiエンジンで式を処理: {}", expr);
                return Ok(result);
            },
            Err(e) => debug!("Rhaiエンジン失敗: {}", e),
        }
    }
    // 3. QuickJS（高速・型安全）
    #[cfg(feature = "js-quickjs")]
    {
        match evaluate_with_quickjs(expr, param) {
            Ok(result) => {
                debug!("QuickJSエンジンで式を処理: {}", expr);
                return Ok(result);
            },
            Err(e) => debug!("QuickJSエンジン失敗: {}", e),
        }
    }
    // 4. Deno/V8（フル互換・サンドボックス）
    #[cfg(feature = "js-deno")]
    {
        match evaluate_with_deno(expr, param) {
            Ok(result) => {
                debug!("Denoエンジンで式を処理: {}", expr);
                return Ok(result);
            },
            Err(e) => debug!("Denoエンジン失敗: {}", e),
        }
    }
    // 5. フォールバック: カスタム式パーサー
    match evaluate_with_custom_parser(expr, param) {
        Ok(result) => {
            debug!("カスタムパーサーで式を処理: {}", expr);
            Ok(result)
        },
        Err(e) => {
            // 全てのエンジンが失敗した場合
            error!("全バックエンドで式評価失敗: {}", e);
            Err(anyhow!("JavaScript式『{}』の評価に失敗: {}", expr, e))
        }
    }
}

/// 簡易パターンマッチングに基づく高速な式評価
/// 一般的なパターンのみをサポート
fn fast_expression_eval(expr: &str, param: &str) -> Result<String> {
    let expr = expr.trim();
    
    // 直接参照 (param)
    if expr == "param" {
        return Ok(param.to_string());
    }
    
    // 文字列長 (param.length)
    if expr == "param.length" {
        return Ok(param.len().to_string());
    }
    
    // 大文字小文字変換
    if expr == "param.toUpperCase()" {
        return Ok(param.to_uppercase());
    }
    
    if expr == "param.toLowerCase()" {
        return Ok(param.to_lowercase());
    }
    
    // トリム操作
    if expr == "param.trim()" {
        return Ok(param.trim().to_string());
    }
    
    if expr == "param.trimStart()" || expr == "param.trimLeft()" {
        return Ok(param.trim_start().to_string());
    }
    
    if expr == "param.trimEnd()" || expr == "param.trimRight()" {
        return Ok(param.trim_end().to_string());
    }
    
    // 部分文字列抽出
    let substring_re = Regex::new(r"param\.substring\((\d+)(?:,\s*(\d+))?\)")?;
    if let Some(caps) = substring_re.captures(expr) {
        let start = caps[1].parse::<usize>()?;
        let end = caps.get(2).map_or(param.len(), |m| m.as_str().parse::<usize>().unwrap_or(param.len()));
        
        if start <= end && end <= param.len() {
            let safe_start = std::cmp::min(start, param.len());
            let safe_end = std::cmp::min(end, param.len());
            return Ok(param[safe_start..safe_end].to_string());
        }
    }
    
    // 文字列置換
    let replace_re = Regex::new(r"param\.replace\(['\"](/(?:[^/\\]|\\.)*/)['\"](, ['\"](.*)['\"]\))")?;
    if let Some(caps) = replace_re.captures(expr) {
        let pattern = &caps[1]; // '/pattern/'形式
        let replacement = &caps[3];
        
        // パターンから正規表現を作成
        let regex_str = if pattern.starts_with('/') && pattern.ends_with('/') {
            &pattern[1..pattern.len() - 1]
        } else {
            pattern
        };
        let re = regex::Regex::new(regex_str)?;
        
        // グローバル置換フラグがあるかチェック
        if expr.contains("/g") {
            return Ok(re.replace_all(param, replacement).to_string());
        } else {
            return Ok(re.replace(param, replacement).to_string());
        }
    }
    
    // 数値変換と操作
    if expr == "parseInt(param)" || expr == "Number(param)" {
        return Ok(param.parse::<i64>().map_or(
            "NaN".to_string(), 
            |n| n.to_string()
        ));
    }
    
    if expr == "parseFloat(param)" {
        return Ok(param.parse::<f64>().map_or(
            "NaN".to_string(), 
            |n| n.to_string()
        ));
    }
    
    // 数値計算式 - param + 数値
    let num_op_re = Regex::new(r"Number\(param\) (\+|\-|\*|/) (\d+(?:\.\d+)?)")?;
    if let Some(caps) = num_op_re.captures(expr) {
        let op = &caps[1];
        let num = caps[2].parse::<f64>()?;
        
        if let Ok(param_num) = param.parse::<f64>() {
            let result = match op {
                "+" => param_num + num,
                "-" => param_num - num,
                "*" => param_num * num,
                "/" => param_num / num,
                _ => return Err(anyhow!("未サポートの演算子: {}", op)),
            };
            return Ok(result.to_string());
        }
    }
    
    // JSON解析
    if expr == "JSON.parse(param)" {
        // 有効なJSONかチェック
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(param) {
            return Ok(json.to_string());
        }
    }
    
    // Split操作と配列アクセス
    let split_re = Regex::new(r"param\.split\(['\"]([^'\"]*)['\"](?:\)|\)\[(\d+)\])")?;
    if let Some(caps) = split_re.captures(expr) {
        let delimiter = &caps[1];
        let parts: Vec<&str> = param.split(delimiter).collect();
        
        if let Some(idx) = caps.get(2) {
            let idx = idx.as_str().parse::<usize>()?;
            if idx < parts.len() {
                return Ok(parts[idx].to_string());
            } else {
                return Ok("undefined".to_string());
            }
        } else {
            return Ok(format!("[{}]", parts.iter()
                .map(|p| format!("\"{}\"", p))
                .collect::<Vec<_>>()
                .join(", ")));
        }
    }
    
    // サポートしていない式の場合はエラー
    Err(anyhow!("高速評価器でサポートされていない式: {}", expr))
}

#[cfg(feature = "js-rhai")]
fn evaluate_with_rhai(expr: &str, param: &str) -> Result<String> {
    use rhai::{Engine, Scope, AST};
    
    // Rhaiエンジンの初期化
    let mut engine = Engine::new();
    
    // 文字列操作関数の登録
    engine.register_fn("to_uppercase", |s: &str| s.to_uppercase());
    engine.register_fn("to_lowercase", |s: &str| s.to_lowercase());
    engine.register_fn("trim", |s: &str| s.trim().to_string());
    engine.register_fn("substring", |s: &str, start: i64, end: i64| {
        if start < 0 || end < 0 || start > end {
            return "".to_string();
        }
        let start = start as usize;
        let end = end as usize;
        if start >= s.len() {
            return "".to_string();
        }
        let end = std::cmp::min(end, s.len());
        s[start..end].to_string()
    });
    
    // 式を前処理
    let modified_expr = expr.replace("param", "input");
    
    // スクリプト実行のためのスコープとASTを作成
    let mut scope = Scope::new();
    scope.push("input", param.to_string());
    
    // スクリプト実行
    let ast = engine.compile(&format!("result = {}", modified_expr))?;
    engine.run_ast_with_scope(&mut scope, &ast)?;
    
    // 結果を取得
    let result: String = scope.get_value("result")?;
    Ok(result)
}

#[cfg(feature = "js-quickjs")]
fn evaluate_with_quickjs(expr: &str, param: &str) -> Result<String> {
    use quick_js::{Context, JsValue};
    
    // QuickJSコンテキストを作成
    let context = Context::new()?;
    
    // パラメータをグローバル変数として設定
    context.eval("const param = null;")?;
    context.set_global("param", param)?;
    
    // 式を評価
    let result = context.eval(expr)?;
    
    // JsValueを文字列に変換
    match result {
        JsValue::String(s) => Ok(s),
        JsValue::Int(i) => Ok(i.to_string()),
        JsValue::Float(f) => Ok(f.to_string()),
        JsValue::Bool(b) => Ok(b.to_string()),
        JsValue::Null => Ok("null".to_string()),
        JsValue::Undefined => Ok("undefined".to_string()),
        JsValue::Object(_) => Ok("[Object]".to_string()),
        JsValue::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(|val| {
                match val {
                    JsValue::String(s) => format!("\"{}\"", s),
                    JsValue::Int(i) => i.to_string(),
                    JsValue::Float(f) => f.to_string(),
                    JsValue::Bool(b) => b.to_string(),
                    JsValue::Null => "null".to_string(),
                    JsValue::Undefined => "undefined".to_string(),
                    JsValue::Object(_) => "[Object]".to_string(),
                    JsValue::Array(_) => "[Array]".to_string(),
                    JsValue::Function(_) => "[Function]".to_string(),
                }
            }).collect();
            Ok(format!("[{}]", elements.join(", ")))
        },
        JsValue::Function(_) => Ok("[Function]".to_string()),
    }
}

#[cfg(feature = "js-deno")]
fn evaluate_with_deno(expr: &str, param: &str) -> Result<String> {
    use deno_core::{JsRuntime, RuntimeOptions, serde_json};
    use deno_core::op;
    use deno_core::extension;
    
    // Deno/V8ランタイムオプションの設定
    let options = RuntimeOptions {
        extensions: vec![
            extension!(
                "nexus_runtime",
                ops = [op_print]
            )
        ],
        ..Default::default()
    };
    
    // デバッグ用出力操作
    #[op]
    fn op_print(msg: String) -> Result<(), deno_core::error::AnyError> {
        debug!("JS: {}", msg);
        Ok(())
    }
    
    // ランタイムの作成と初期化
    let mut runtime = JsRuntime::new(options);
    
    // パラメータと共通ユーティリティを設定するブートストラップコード
    let init_code = format!(
        r#"
        // グローバル変数の設定
        const param = {};
        
        // 便利なユーティリティ関数
        function escapeRegExp(string) {{
            return string.replace(/[.*+?^${{}}()|[\]\\]/g, '\\$&');
        }}
        
        // 出力用関数
        function print(msg) {{
            Deno.core.ops.op_print(String(msg));
        }}
        "#,
        serde_json::to_string(param)?,
    );
    
    // ブートストラップコードを実行
    runtime.execute_script("<bootstrap>", &init_code)?;
    
    // 式の評価
    let result_val = runtime.execute_script("<expr>", expr)?;
    let result = runtime.resolve_value(result_val)?;
    
    // 結果をJSON文字列に変換して返す
    let json_result = runtime.serde_value_to_str(result)?;
    
    // JSON文字列からクォーテーションを取り除く処理
    let clean_result = if json_result.starts_with('"') && json_result.ends_with('"') && json_result.len() > 1 {
        let inner = &json_result[1..json_result.len() - 1];
        // JSONエスケープを元に戻す
        inner.replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t")
    } else {
        json_result
    };
    
    Ok(clean_result)
}

/// 独自の簡易パーサーによる式評価
/// より複雑な式のサポートを提供
fn evaluate_with_custom_parser(expr: &str, param: &str) -> Result<String> {
    // 式を字句解析してトークンに分割
    let tokens = tokenize_expression(expr)?;
    
    // 操作子と操作対象を識別
    if tokens.is_empty() {
        return Ok(param.to_string());
    }
    
    // 入れ子の関数呼び出しをサポート
    let (result, _) = evaluate_tokens(&tokens, param, 0)?;
    Ok(result)
}

/// 式をトークンに分解
fn tokenize_expression(expr: &str) -> Result<Vec<String>> {
    let mut tokens = vec![];
    let mut current = String::new();
    let mut in_string = false;
    let mut string_delim = ' ';
    let mut depth = 0;
    
    for c in expr.chars() {
        match c {
            '\'' | '"' if !in_string => {
                in_string = true;
                string_delim = c;
                current.push(c);
            }
            c if c == string_delim && in_string => {
                in_string = false;
                current.push(c);
            }
            '(' if !in_string => {
                depth += 1;
                current.push(c);
            }
            ')' if !in_string => {
                depth -= 1;
                current.push(c);
                if depth == 0 && !current.is_empty() {
                    tokens.push(current);
                    current = String::new();
                }
            }
            '.' if !in_string && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(current);
                    current = String::new();
                }
                tokens.push(".".to_string());
            }
            _ => {
                current.push(c);
            }
        }
    }
    
    if !current.is_empty() {
        tokens.push(current);
    }
    
    Ok(tokens)
}

/// トークン列を評価
fn evaluate_tokens(tokens: &[String], param: &str, start_idx: usize) -> Result<(String, usize)> {
    if start_idx >= tokens.len() {
        return Ok((param.to_string(), start_idx));
    }
    
    let mut result = param.to_string();
    let mut idx = start_idx;
    
    while idx < tokens.len() {
        let token = &tokens[idx];
        
        match token.as_str() {
            "." => {
                // メソッド呼び出しまたはプロパティアクセス
                idx += 1;
                if idx >= tokens.len() {
                    return Err(anyhow!("構文エラー: ドットの後にトークンがありません"));
                }
                
                let method_or_prop = &tokens[idx];
                
                // メソッド呼び出し
                if method_or_prop.ends_with('(') && idx + 1 < tokens.len() && tokens[idx + 1].starts_with(')') {
                    let method_name = &method_or_prop[..method_or_prop.len() - 1];
                    
                    // サポートされているメソッドの処理
                    result = match method_name {
                        "toUpperCase" => result.to_uppercase(),
                        "toLowerCase" => result.to_lowercase(),
                        "trim" => result.trim().to_string(),
                        "trimStart" | "trimLeft" => result.trim_start().to_string(),
                        "trimEnd" | "trimRight" => result.trim_end().to_string(),
                        "toString" => result,
                        _ => return Err(anyhow!("未サポートのメソッド: {}", method_name)),
                    };
                    
                    idx += 2; // メソッド名と閉じ括弧をスキップ
                } else {
                    // プロパティアクセス
                    match method_or_prop.as_str() {
                        "length" => {
                            result = result.len().to_string();
                            idx += 1;
                        }
                        _ => return Err(anyhow!("未サポートのプロパティ: {}", method_or_prop)),
                    }
                }
            }
            _ => {
                // その他のトークンは無視
                idx += 1;
            }
        }
    }
    
    Ok((result, idx))
} 