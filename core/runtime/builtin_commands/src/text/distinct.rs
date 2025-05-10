use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::io::{BufRead, BufReader};
use std::collections::{HashSet, HashMap};
use tracing::{debug, warn, error};

/// データから重複を削除するコマンド
///
/// 入力データから重複する行または値を削除します。
/// キーを指定して、特定のフィールドのみを対象にすることも可能です。
///
/// # 使用例
///
/// ```bash
/// cat data.txt | distinct              # 重複する行を削除
/// cat data.csv | distinct --key 2      # 2列目の値に基づいて重複行を削除
/// cat data.txt | distinct --count      # 重複行を削除し、出現回数を表示
/// cat data.csv | distinct --keep-first # 重複がある場合、最初の行のみを保持
/// ```
pub struct DistinctCommand;

#[async_trait]
impl BuiltinCommand for DistinctCommand {
    fn name(&self) -> &'static str {
        "distinct"
    }

    fn description(&self) -> &'static str {
        "データから重複を削除します"
    }

    fn usage(&self) -> &'static str {
        "distinct [オプション] [入力]\n\n\
        オプション:\n\
        --key <NUM>         指定したフィールド/列に基づいて重複を判定（1から始まる）\n\
        --keys <NUM1,NUM2...> 複数のフィールド/列に基づいて重複を判定（1から始まる）\n\
        --delimiter <CHAR>  フィールド区切り文字（デフォルト: タブ）\n\
        --case-sensitive    大文字と小文字を区別（デフォルト: 区別しない）\n\
        --count             各値の出現回数を表示\n\
        --keep-first        重複がある場合、最初の行を保持（デフォルト）\n\
        --keep-last         重複がある場合、最後の行を保持\n\
        --keep-none         重複がある場合、全ての行を削除\n\
        --keep-all          重複している行を削除せず、ユニークなデータのみを出力\n\
        --min-count <NUM>   指定した回数以上出現する値のみを表示\n\
        --max-count <NUM>   指定した回数以下出現する値のみを表示\n\
        --zero-index        0から始まるインデックスを使用（デフォルトは1から始まる）"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // オプションの解析
        let mut delimiter = '\t';
        let mut key_indices = Vec::new();
        let mut case_sensitive = false;
        let mut show_count = false;
        let mut keep_strategy = "first";
        let mut min_count = 1;
        let mut max_count = usize::MAX;
        let mut zero_index = false;
        
        let mut i = 1;
        while i < context.args.len() {
            match context.args[i].as_str() {
                "--key" => {
                    i += 1;
                    if i < context.args.len() {
                        let key = context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("キー番号は数値である必要があります"))?;
                        
                        if !zero_index && key == 0 {
                            return Err(anyhow!("キー番号は1以上である必要があります（または --zero-index を使用）"));
                        }
                        
                        key_indices = vec![key];
                    } else {
                        return Err(anyhow!("--key オプションには値が必要です"));
                    }
                },
                "--keys" => {
                    i += 1;
                    if i < context.args.len() {
                        for key_str in context.args[i].split(',') {
                            let key = key_str.trim().parse::<usize>()
                                .map_err(|_| anyhow!("キー番号は数値である必要があります: {}", key_str))?;
                            
                            if !zero_index && key == 0 {
                                return Err(anyhow!("キー番号は1以上である必要があります（または --zero-index を使用）"));
                            }
                            
                            key_indices.push(key);
                        }
                    } else {
                        return Err(anyhow!("--keys オプションには値が必要です"));
                    }
                },
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
                "--case-sensitive" => {
                    case_sensitive = true;
                },
                "--count" => {
                    show_count = true;
                },
                "--keep-first" => {
                    keep_strategy = "first";
                },
                "--keep-last" => {
                    keep_strategy = "last";
                },
                "--keep-none" => {
                    keep_strategy = "none";
                },
                "--keep-all" => {
                    keep_strategy = "all";
                },
                "--min-count" => {
                    i += 1;
                    if i < context.args.len() {
                        min_count = context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("最小回数は数値である必要があります"))?;
                    } else {
                        return Err(anyhow!("--min-count オプションには値が必要です"));
                    }
                },
                "--max-count" => {
                    i += 1;
                    if i < context.args.len() {
                        max_count = context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("最大回数は数値である必要があります"))?;
                    } else {
                        return Err(anyhow!("--max-count オプションには値が必要です"));
                    }
                },
                "--zero-index" => {
                    zero_index = true;
                },
                _ => {
                    return Err(anyhow!("不明なオプション: {}", context.args[i]));
                }
            }
            i += 1;
        }
        
        // 1-indexed から 0-indexed へ変換（必要な場合）
        if !zero_index {
            key_indices = key_indices.iter()
                .map(|&k| k.saturating_sub(1))
                .collect();
        }
        
        // 標準入力からデータを読み取り
        if !context.stdin_connected {
            return Err(anyhow!("標準入力からデータを読み取れません"));
        }
        
        // 重複除去処理
        let stdin = std::io::stdin();
        let result = if key_indices.is_empty() {
            process_entire_lines(stdin, case_sensitive, show_count, keep_strategy, min_count, max_count)
        } else {
            process_by_keys(stdin, &key_indices, delimiter, case_sensitive, show_count, keep_strategy, min_count, max_count)
        }?;
        
        Ok(CommandResult::success().with_stdout(result))
    }
}

/// 行全体に基づいて重複を削除する処理
fn process_entire_lines<R: std::io::Read>(
    input: R,
    case_sensitive: bool,
    show_count: bool,
    keep_strategy: &str,
    min_count: usize,
    max_count: usize
) -> Result<Vec<u8>> {
    let reader = BufReader::new(input);
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let mut counts = HashMap::new();
    let mut line_map = HashMap::new();
    
    for (index, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        
        // 大文字小文字を区別するかどうか
        let key = if case_sensitive {
            line.clone()
        } else {
            line.to_lowercase()
        };
        
        // 出現回数をカウント
        *counts.entry(key.clone()).or_insert(0) += 1;
        
        // 保持戦略に従って処理
        match keep_strategy {
            "first" => {
                if !seen.contains(&key) {
                    seen.insert(key);
                    line_map.insert(key.clone(), (line.clone(), index));
                }
            },
            "last" => {
                seen.insert(key.clone());
                line_map.insert(key, (line.clone(), index));
            },
            "none" => {
                seen.insert(key);
            },
            "all" => {
                // 全てのユニークな値を出力
                if !seen.contains(&key) {
                    seen.insert(key.clone());
                    line_map.insert(key, (line.clone(), index));
                    // 重複はスキップ
                }
            },
            _ => return Err(anyhow!("不明な保持戦略: {}", keep_strategy)),
        }
    }
    
    // 保持戦略が "none" の場合は、1回だけ出現する行のみを保持
    if keep_strategy == "none" {
        for (line, count) in counts.iter() {
            if *count == 1 && min_count <= 1 && 1 <= max_count {
                result.extend_from_slice(line.as_bytes());
                result.push(b'\n');
            }
        }
    } else {
        // 出力生成
        let mut entries: Vec<_> = line_map
            .into_iter()
            .collect();
        
        // インデックス順にソート（保持戦略に従って）
        entries.sort_by_key(|&(_, (_, idx))| idx);
        
        for (key, (line, _)) in entries {
            let count = counts.get(&key).unwrap_or(&0);
            
            if min_count <= *count && *count <= max_count {
                if show_count {
                    result.extend_from_slice(format!("{}\t{}\n", count, line).as_bytes());
                } else {
                    result.extend_from_slice(line.as_bytes());
                    result.push(b'\n');
                }
            }
        }
    }
    
    Ok(result)
}

/// 指定したキーに基づいて重複を削除する処理
fn process_by_keys<R: std::io::Read>(
    input: R,
    key_indices: &[usize],
    delimiter: char,
    case_sensitive: bool,
    show_count: bool,
    keep_strategy: &str,
    min_count: usize,
    max_count: usize
) -> Result<Vec<u8>> {
    let reader = BufReader::new(input);
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let mut counts = HashMap::new();
    let mut line_map = HashMap::new();
    
    for (index, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let fields: Vec<&str> = line.split(delimiter).collect();
        
        // キーを構築
        let mut key_parts = Vec::new();
        for &key_idx in key_indices {
            if key_idx < fields.len() {
                key_parts.push(
                    if case_sensitive {
                        fields[key_idx].to_string()
                    } else {
                        fields[key_idx].to_lowercase()
                    }
                );
            } else {
                warn!("指定されたキーのインデックスが範囲外です: {}", key_idx);
                key_parts.push(String::new());
            }
        }
        
        let key = key_parts.join("\u{001F}"); // 単位区切り文字を使用
        
        // 出現回数をカウント
        *counts.entry(key.clone()).or_insert(0) += 1;
        
        // 保持戦略に従って処理
        match keep_strategy {
            "first" => {
                if !seen.contains(&key) {
                    seen.insert(key.clone());
                    line_map.insert(key, (line.clone(), index));
                }
            },
            "last" => {
                seen.insert(key.clone());
                line_map.insert(key, (line.clone(), index));
            },
            "none" => {
                seen.insert(key);
                // 何も保存しない
            },
            "all" => {
                // 全てのユニークな値を出力
                if !seen.contains(&key) {
                    seen.insert(key.clone());
                    line_map.insert(key, (line.clone(), index));
                }
            },
            _ => return Err(anyhow!("不明な保持戦略: {}", keep_strategy)),
        }
    }
    
    // 保持戦略が "none" の場合は、出力なし（1回だけ出現する行も削除）
    if keep_strategy == "none" {
        return Ok(Vec::new());
    }
    
    // 出力生成
    let mut entries: Vec<_> = line_map
        .into_iter()
        .collect();
    
    // インデックス順にソート（保持戦略に従って）
    entries.sort_by_key(|&(_, (_, idx))| idx);
    
    for (key, (line, _)) in entries {
        let count = counts.get(&key).unwrap_or(&0);
        
        if min_count <= *count && *count <= max_count {
            if show_count {
                result.extend_from_slice(format!("{}\t{}\n", count, line).as_bytes());
            } else {
                result.extend_from_slice(line.as_bytes());
                result.push(b'\n');
            }
        }
    }
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_distinct_basic() {
        let input = "apple\nbanana\napple\norange\nbanana".as_bytes();
        let result = process_entire_lines(input, false, false, "first", 1, usize::MAX).unwrap();
        let output = String::from_utf8(result).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines.contains(&"apple"));
        assert!(lines.contains(&"banana"));
        assert!(lines.contains(&"orange"));
    }
    
    #[tokio::test]
    async fn test_distinct_with_count() {
        let input = "apple\nbanana\napple\norange\nbanana".as_bytes();
        let result = process_entire_lines(input, false, true, "first", 1, usize::MAX).unwrap();
        let output = String::from_utf8(result).unwrap();
        assert!(output.contains("2\tapple"));
        assert!(output.contains("2\tbanana"));
        assert!(output.contains("1\torange"));
    }
    
    #[tokio::test]
    async fn test_distinct_by_key() {
        let input = "1\tapple\n2\tbanana\n3\tapple\n4\torange\n5\tbanana".as_bytes();
        let result = process_by_keys(input, &[1], '\t', false, false, "first", 1, usize::MAX).unwrap();
        let output = String::from_utf8(result).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines.contains(&"1\tapple"));
        assert!(lines.contains(&"2\tbanana"));
        assert!(lines.contains(&"4\torange"));
    }
    
    #[tokio::test]
    async fn test_distinct_keep_last() {
        let input = "apple\nbanana\napple\norange\nbanana".as_bytes();
        let result = process_entire_lines(input, false, false, "last", 1, usize::MAX).unwrap();
        let output = String::from_utf8(result).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines.contains(&"apple"));
        assert!(lines.contains(&"banana"));
        assert!(lines.contains(&"orange"));
    }
    
    #[tokio::test]
    async fn test_distinct_case_insensitive() {
        let input = "Apple\nbanana\napple\nOrange\nBanana".as_bytes();
        let result = process_entire_lines(input, false, false, "first", 1, usize::MAX).unwrap();
        let output = String::from_utf8(result).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
    }
    
    #[tokio::test]
    async fn test_distinct_case_sensitive() {
        let input = "Apple\nbanana\napple\nOrange\nBanana".as_bytes();
        let result = process_entire_lines(input, true, false, "first", 1, usize::MAX).unwrap();
        let output = String::from_utf8(result).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
    }
} 