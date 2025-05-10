use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::io::{BufRead, BufReader};
use std::collections::HashMap;
use tracing::{debug, warn, error};

/// データの統計分析を行うコマンド
///
/// テキストデータや数値データに対して各種統計処理を行います。
/// 平均、中央値、標準偏差などの基本統計量や、さらに高度な分析も可能です。
///
/// # 使用例
///
/// ```bash
/// cat numbers.txt | stats             # 基本統計量を表示
/// cat data.csv | stats --column 3     # 特定の列の統計を表示
/// cat logs.txt | stats --histogram    # ヒストグラムを表示
/// ```
pub struct StatsCommand;

#[async_trait]
impl BuiltinCommand for StatsCommand {
    fn name(&self) -> &'static str {
        "stats"
    }

    fn description(&self) -> &'static str {
        "データの統計分析を行います"
    }

    fn usage(&self) -> &'static str {
        "stats [オプション]\n\n\
        オプション:\n\
        --delimiter <CHAR>      フィールド区切り文字を指定（デフォルトはスペース）\n\
        --column <NUM>          分析する列を指定（1から始まる）\n\
        --header               入力の最初の行をヘッダーとして扱う\n\
        --basic                基本統計量のみ表示（デフォルト）\n\
        --full                 詳細な統計情報を表示\n\
        --histogram            ヒストグラムを表示\n\
        --quartiles            四分位数を表示\n\
        --percentile <NUM>     指定したパーセンタイル値を表示\n\
        --mode                 最頻値を表示\n\
        --format <FORMAT>      出力フォーマットを指定（text, csv, json）\n\
        --sort <FIELD>         特定のフィールドでソートして表示"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // オプションの解析
        let mut delimiter = ' ';
        let mut column = 0; // 0は全体を意味する
        let mut has_header = false;
        let mut show_histogram = false;
        let mut show_quartiles = false;
        let mut show_mode = false;
        let mut percentile = None;
        let mut output_format = "text";
        
        let mut i = 1;
        while i < context.args.len() {
            match context.args[i].as_str() {
                "--delimiter" => {
                    i += 1;
                    if i < context.args.len() {
                        let delim_str = &context.args[i];
                        if delim_str.len() != 1 {
                            return Err(anyhow!("区切り文字は1文字だけ指定してください"));
                        }
                        delimiter = delim_str.chars().next().unwrap();
                    } else {
                        return Err(anyhow!("--delimiter オプションには値が必要です"));
                    }
                },
                "--column" => {
                    i += 1;
                    if i < context.args.len() {
                        column = context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("列番号は数値である必要があります"))?;
                        
                        if column == 0 {
                            return Err(anyhow!("列番号は1以上である必要があります"));
                        }
                        
                        // 内部的には0-indexedで扱う
                        column -= 1;
                    } else {
                        return Err(anyhow!("--column オプションには値が必要です"));
                    }
                },
                "--header" => {
                    has_header = true;
                },
                "--histogram" => {
                    show_histogram = true;
                },
                "--quartiles" => {
                    show_quartiles = true;
                },
                "--mode" => {
                    show_mode = true;
                },
                "--percentile" => {
                    i += 1;
                    if i < context.args.len() {
                        let p = context.args[i].parse::<f64>()
                            .map_err(|_| anyhow!("パーセンタイルは数値である必要があります"))?;
                        
                        if p < 0.0 || p > 100.0 {
                            return Err(anyhow!("パーセンタイルは0から100の間である必要があります"));
                        }
                        
                        percentile = Some(p);
                    } else {
                        return Err(anyhow!("--percentile オプションには値が必要です"));
                    }
                },
                "--format" => {
                    i += 1;
                    if i < context.args.len() {
                        let format = context.args[i].as_str();
                        match format {
                            "text" | "csv" | "json" => {
                                output_format = format;
                            },
                            _ => return Err(anyhow!("未サポートの出力フォーマット: {}", format)),
                        }
                    } else {
                        return Err(anyhow!("--format オプションには値が必要です"));
                    }
                },
                _ => {
                    return Err(anyhow!("不明なオプション: {}", context.args[i]));
                }
            }
            
            i += 1;
        }
        
        // 標準入力からデータを読み取り
        if !context.stdin_connected {
            return Err(anyhow!("標準入力からデータを読み取れません"));
        }
        
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        let mut lines = reader.lines();
        
        // ヘッダー行の処理
        let header_names = if has_header {
            if let Some(header_line) = lines.next() {
                let header_line = header_line?;
                header_line.split(delimiter).map(|s| s.trim().to_string()).collect::<Vec<_>>()
            } else {
                return Err(anyhow!("ヘッダー行がありません"));
            }
        } else {
            Vec::new()
        };
        
        // データの読み取り
        let mut values = Vec::new();
        let mut categorical_data = HashMap::new();
        let mut total_rows = 0;
        let mut non_numeric_count = 0;
        
        for line_result in lines {
            let line = line_result?;
            if line.trim().is_empty() {
                continue;
            }
            
            total_rows += 1;
            let fields: Vec<&str> = line.split(delimiter).map(|s| s.trim()).collect();
            
            // 特定の列を抽出
            let value_str = if column < fields.len() {
                fields[column]
            } else {
                warn!("指定された列のインデックスが範囲外です: {}", column + 1);
                continue;
            };
            
            // カテゴリカルデータの集計（頻度分布用）
            *categorical_data.entry(value_str.to_string()).or_insert(0) += 1;
            
            // 数値に変換
            match value_str.parse::<f64>() {
                Ok(num) => {
                    values.push(num);
                },
                Err(_) => {
                    non_numeric_count += 1;
                }
            }
        }
        
        // 分析結果の出力
        let mut output = Vec::new();
        
        if values.is_empty() {
            return Err(anyhow!("分析可能な数値データがありません"));
        }
        
        // 基本統計量の計算
        let stats = calculate_basic_statistics(&values);
        
        // 表示するヘッダー名を決定
        let column_name = if !header_names.is_empty() && column < header_names.len() {
            &header_names[column]
        } else {
            "値"
        };
        
        // 出力フォーマットに基づいて結果を整形
        match output_format {
            "text" => {
                // 基本統計量のテキスト表示
                output.extend_from_slice(format!("列: {}\n", column_name).as_bytes());
                output.extend_from_slice(format!("データ件数: {}\n", values.len()).as_bytes());
                output.extend_from_slice(format!("非数値データ: {}\n", non_numeric_count).as_bytes());
                output.extend_from_slice(format!("最小値: {:.6}\n", stats.min).as_bytes());
                output.extend_from_slice(format!("最大値: {:.6}\n", stats.max).as_bytes());
                output.extend_from_slice(format!("合計: {:.6}\n", stats.sum).as_bytes());
                output.extend_from_slice(format!("平均: {:.6}\n", stats.mean).as_bytes());
                output.extend_from_slice(format!("標準偏差: {:.6}\n", stats.std_dev).as_bytes());
                
                // 四分位数を表示
                if show_quartiles {
                    output.extend_from_slice(format!("\n四分位数:\n").as_bytes());
                    output.extend_from_slice(format!("第1四分位数 (25%): {:.6}\n", stats.q1).as_bytes());
                    output.extend_from_slice(format!("中央値 (50%): {:.6}\n", stats.median).as_bytes());
                    output.extend_from_slice(format!("第3四分位数 (75%): {:.6}\n", stats.q3).as_bytes());
                    output.extend_from_slice(format!("四分位範囲 (IQR): {:.6}\n", stats.q3 - stats.q1).as_bytes());
                }
                
                // 指定したパーセンタイル値を表示
                if let Some(p) = percentile {
                    let p_value = calculate_percentile(&mut values.clone(), p);
                    output.extend_from_slice(format!("\n第{}パーセンタイル: {:.6}\n", p, p_value).as_bytes());
                }
                
                // 最頻値を表示
                if show_mode {
                    output.extend_from_slice(format!("\n最頻値:\n").as_bytes());
                    let modes = find_modes(&categorical_data);
                    for (value, count) in modes {
                        output.extend_from_slice(format!("  {} ({}回)\n", value, count).as_bytes());
                    }
                }
                
                // ヒストグラムを表示
                if show_histogram {
                    output.extend_from_slice(format!("\nヒストグラム:\n").as_bytes());
                    let histogram = create_histogram(&values, 10, stats.min, stats.max);
                    for (bin_start, bin_end, count) in histogram {
                        let bar = "#".repeat(((count as f64) / (values.len() as f64) * 50.0) as usize);
                        output.extend_from_slice(
                            format!("{:.2} - {:.2} [{:4}]: {}\n", 
                                bin_start, bin_end, count, bar).as_bytes()
                        );
                    }
                }
            },
            "csv" => {
                // CSVフォーマット出力
                output.extend_from_slice(b"metric,value\n");
                output.extend_from_slice(format!("count,{}\n", values.len()).as_bytes());
                output.extend_from_slice(format!("min,{}\n", stats.min).as_bytes());
                output.extend_from_slice(format!("max,{}\n", stats.max).as_bytes());
                output.extend_from_slice(format!("sum,{}\n", stats.sum).as_bytes());
                output.extend_from_slice(format!("mean,{}\n", stats.mean).as_bytes());
                output.extend_from_slice(format!("stddev,{}\n", stats.std_dev).as_bytes());
                output.extend_from_slice(format!("median,{}\n", stats.median).as_bytes());
                output.extend_from_slice(format!("q1,{}\n", stats.q1).as_bytes());
                output.extend_from_slice(format!("q3,{}\n", stats.q3).as_bytes());
            },
            "json" => {
                // JSONフォーマット出力
                let json = format!("{{
                    \"column\": \"{}\",
                    \"count\": {},
                    \"non_numeric\": {},
                    \"min\": {},
                    \"max\": {},
                    \"sum\": {},
                    \"mean\": {},
                    \"stddev\": {},
                    \"median\": {},
                    \"q1\": {},
                    \"q3\": {}
                }}", 
                column_name, values.len(), non_numeric_count, 
                stats.min, stats.max, stats.sum, stats.mean, stats.std_dev,
                stats.median, stats.q1, stats.q3);
                
                output.extend_from_slice(json.as_bytes());
            },
            _ => {
                return Err(anyhow!("未サポートの出力フォーマット: {}", output_format));
            }
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// 基本統計量を格納する構造体
struct BasicStatistics {
    min: f64,
    max: f64,
    sum: f64,
    mean: f64,
    std_dev: f64,
    median: f64,
    q1: f64,
    q3: f64,
}

/// 基本統計量を計算
fn calculate_basic_statistics(values: &[f64]) -> BasicStatistics {
    if values.is_empty() {
        return BasicStatistics {
            min: 0.0,
            max: 0.0,
            sum: 0.0,
            mean: 0.0,
            std_dev: 0.0,
            median: 0.0,
            q1: 0.0,
            q3: 0.0,
        };
    }
    
    // 最小値と最大値
    let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    
    // 合計と平均
    let sum: f64 = values.iter().sum();
    let mean = sum / values.len() as f64;
    
    // 標準偏差
    let variance = values.iter()
        .map(|&x| (x - mean).powi(2))
        .sum::<f64>() / values.len() as f64;
    let std_dev = variance.sqrt();
    
    // ソートしたデータをコピー（中央値や四分位数の計算用）
    let mut sorted_values = values.to_vec();
    sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    // 中央値
    let median = calculate_percentile(&mut sorted_values.clone(), 50.0);
    
    // 四分位数
    let q1 = calculate_percentile(&mut sorted_values.clone(), 25.0);
    let q3 = calculate_percentile(&mut sorted_values.clone(), 75.0);
    
    BasicStatistics {
        min,
        max,
        sum,
        mean,
        std_dev,
        median,
        q1,
        q3,
    }
}

/// 指定したパーセンタイル値を計算
fn calculate_percentile(sorted_values: &mut Vec<f64>, percentile: f64) -> f64 {
    sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = (percentile / 100.0 * (sorted_values.len() - 1) as f64) as usize;
    let fraction = (percentile / 100.0 * (sorted_values.len() - 1) as f64) - idx as f64;
    
    if idx + 1 < sorted_values.len() {
        sorted_values[idx] + fraction * (sorted_values[idx + 1] - sorted_values[idx])
    } else {
        sorted_values[idx]
    }
}

/// 最頻値を見つける
fn find_modes(data: &HashMap<String, usize>) -> Vec<(String, usize)> {
    if data.is_empty() {
        return Vec::new();
    }
    
    let max_count = data.values().cloned().max().unwrap_or(0);
    
    // 最大頻度を持つ全ての要素を収集
    data.iter()
        .filter(|(_, &count)| count == max_count)
        .map(|(value, &count)| (value.clone(), count))
        .collect()
}

/// ヒストグラムを作成
fn create_histogram(values: &[f64], bin_count: usize, min: f64, max: f64) -> Vec<(f64, f64, usize)> {
    if values.is_empty() || min >= max || bin_count == 0 {
        return Vec::new();
    }
    
    let range = max - min;
    let bin_width = range / bin_count as f64;
    
    // ビンを初期化
    let mut bins = vec![0; bin_count];
    
    // 各値をビンに振り分け
    for &value in values {
        if value == max {
            // 最大値は最後のビンに入れる
            bins[bin_count - 1] += 1;
        } else {
            let bin_index = ((value - min) / bin_width) as usize;
            if bin_index < bin_count {
                bins[bin_index] += 1;
            }
        }
    }
    
    // 結果をフォーマット
    bins.iter()
        .enumerate()
        .map(|(i, &count)| {
            let bin_start = min + i as f64 * bin_width;
            let bin_end = bin_start + bin_width;
            (bin_start, bin_end, count)
        })
        .collect()
} 