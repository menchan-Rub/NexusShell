use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::io::{self, BufRead, BufReader, Read, Write};
use tracing::{debug, warn, error};

/// データの部分抽出を行うコマンド
///
/// 入力データから指定した範囲の部分を抽出します。
/// バイト単位、行単位、フィールド単位で抽出が可能です。
///
/// # 使用例
///
/// ```bash
/// cat data.txt | slice --start 10 --end 20          # 10行目から20行目までを抽出
/// cat data.csv | slice --field 2 --start 5 --end 10 # 2列目のデータの5行目から10行目を抽出
/// cat file.bin | slice --bytes --start 100 --end 200 # ファイルの100バイト目から200バイト目を抽出
/// ```
pub struct SliceCommand;

#[async_trait]
impl BuiltinCommand for SliceCommand {
    fn name(&self) -> &'static str {
        "slice"
    }

    fn description(&self) -> &'static str {
        "データの部分抽出を行います"
    }

    fn usage(&self) -> &'static str {
        "slice [オプション] [入力]\n\n\
        オプション:\n\
        --start <NUM>       開始位置（デフォルト: 1）\n\
        --end <NUM>         終了位置（デフォルト: 最後まで）\n\
        --field <NUM>       フィールド/列番号を指定（1から始まる）\n\
        --delimiter <CHAR>  フィールド区切り文字（デフォルト: タブ）\n\
        --bytes             バイト単位で抽出\n\
        --include-bounds    範囲の境界を含める（デフォルト）\n\
        --exclude-bounds    範囲の境界を除外する\n\
        --zero-index        0から始まるインデックスを使用（デフォルトは1から始まる）\n\
        --reverse           選択範囲を反転（指定範囲以外を抽出）"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // オプションの解析
        let mut start: Option<usize> = None;
        let mut end: Option<usize> = None;
        let mut field: Option<usize> = None;
        let mut delimiter = '\t';
        let mut use_bytes = false;
        let mut include_bounds = true;
        let mut zero_index = false;
        let mut reverse = false;
        
        let mut i = 1;
        while i < context.args.len() {
            match context.args[i].as_str() {
                "--start" => {
                    i += 1;
                    if i < context.args.len() {
                        start = Some(context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("開始位置は数値である必要があります"))?);
                    } else {
                        return Err(anyhow!("--start オプションには値が必要です"));
                    }
                },
                "--end" => {
                    i += 1;
                    if i < context.args.len() {
                        end = Some(context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("終了位置は数値である必要があります"))?);
                    } else {
                        return Err(anyhow!("--end オプションには値が必要です"));
                    }
                },
                "--field" => {
                    i += 1;
                    if i < context.args.len() {
                        field = Some(context.args[i].parse::<usize>()
                            .map_err(|_| anyhow!("フィールド番号は数値である必要があります"))?);
                        
                        if !zero_index && field == Some(0) {
                            return Err(anyhow!("フィールド番号は1以上である必要があります（または --zero-index を使用）"));
                        }
                    } else {
                        return Err(anyhow!("--field オプションには値が必要です"));
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
                "--bytes" => {
                    use_bytes = true;
                },
                "--include-bounds" => {
                    include_bounds = true;
                },
                "--exclude-bounds" => {
                    include_bounds = false;
                },
                "--zero-index" => {
                    zero_index = true;
                },
                "--reverse" => {
                    reverse = true;
                },
                _ => {
                    return Err(anyhow!("不明なオプション: {}", context.args[i]));
                }
            }
            i += 1;
        }
        
        // 1-indexed から 0-indexed へ変換（必要な場合）
        if !zero_index {
            if let Some(s) = start {
                start = Some(s.saturating_sub(1));
            }
            
            if let Some(e) = end {
                end = Some(e.saturating_sub(1));
            }
            
            if let Some(f) = field {
                field = Some(f.saturating_sub(1));
            }
        }
        
        // 標準入力からデータを読み取り
        if !context.stdin_connected {
            return Err(anyhow!("標準入力からデータを読み取れません"));
        }
        
        let stdin = std::io::stdin();
        let result = if use_bytes {
            slice_bytes(stdin, start, end, reverse)
        } else if let Some(field_idx) = field {
            slice_field(stdin, start, end, field_idx, delimiter, reverse)
        } else {
            slice_lines(stdin, start, end, reverse)
        }?;
        
        Ok(CommandResult::success().with_stdout(result))
    }
}

/// バイト単位で抽出
fn slice_bytes<R: Read>(input: R, start: Option<usize>, end: Option<usize>, reverse: bool) -> Result<Vec<u8>> {
    let mut reader = BufReader::new(input);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    
    let len = buffer.len();
    let start_idx = start.unwrap_or(0);
    let end_idx = end.unwrap_or(len);
    
    if start_idx > len {
        return Err(anyhow!("開始位置がデータサイズを超えています"));
    }
    
    let end_idx = std::cmp::min(end_idx, len);
    
    if start_idx > end_idx {
        return Err(anyhow!("開始位置が終了位置を超えています"));
    }
    
    if reverse {
        let mut result = Vec::new();
        if start_idx > 0 {
            result.extend_from_slice(&buffer[0..start_idx]);
        }
        if end_idx < len {
            result.extend_from_slice(&buffer[end_idx..len]);
        }
        Ok(result)
    } else {
        Ok(buffer[start_idx..end_idx].to_vec())
    }
}

/// 行単位で抽出
fn slice_lines<R: Read>(input: R, start: Option<usize>, end: Option<usize>, reverse: bool) -> Result<Vec<u8>> {
    let reader = BufReader::new(input);
    let start_idx = start.unwrap_or(0);
    let end_idx = end.unwrap_or(usize::MAX);
    
    if start_idx > end_idx {
        return Err(anyhow!("開始行が終了行を超えています"));
    }
    
    let mut result = Vec::new();
    let mut selected_lines = Vec::new();
    let mut all_lines = Vec::new();
    
    for (i, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line_with_newline = line + "\n";
        
        if reverse {
            all_lines.push(line_with_newline);
        } else if i >= start_idx && i <= end_idx {
            selected_lines.push(line_with_newline);
        }
    }
    
    if reverse {
        for (i, line) in all_lines.iter().enumerate() {
            if i < start_idx || i > end_idx {
                result.extend_from_slice(line.as_bytes());
            }
        }
    } else {
        for line in selected_lines {
            result.extend_from_slice(line.as_bytes());
        }
    }
    
    Ok(result)
}

/// フィールド単位で抽出
fn slice_field<R: Read>(
    input: R,
    start: Option<usize>,
    end: Option<usize>,
    field_idx: usize,
    delimiter: char,
    reverse: bool
) -> Result<Vec<u8>> {
    let reader = BufReader::new(input);
    let start_idx = start.unwrap_or(0);
    let end_idx = end.unwrap_or(usize::MAX);
    
    if start_idx > end_idx {
        return Err(anyhow!("開始行が終了行を超えています"));
    }
    
    let mut result = Vec::new();
    
    for (i, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let fields: Vec<&str> = line.split(delimiter).collect();
        
        if field_idx >= fields.len() {
            // この行にはフィールドがないのでスキップ
            continue;
        }
        
        let in_range = i >= start_idx && i <= end_idx;
        if (!reverse && in_range) || (reverse && !in_range) {
            result.extend_from_slice(fields[field_idx].as_bytes());
            result.push(b'\n');
        }
    }
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_slice_lines_basic() {
        let input = "line1\nline2\nline3\nline4\nline5".as_bytes();
        let result = slice_lines(input, Some(1), Some(3), false).unwrap();
        let output = String::from_utf8(result).unwrap();
        assert_eq!(output, "line2\nline3\nline4\n");
    }
    
    #[tokio::test]
    async fn test_slice_bytes() {
        let input = "0123456789".as_bytes();
        let result = slice_bytes(input, Some(2), Some(7), false).unwrap();
        let output = String::from_utf8(result).unwrap();
        assert_eq!(output, "23456");
    }
    
    #[tokio::test]
    async fn test_slice_field() {
        let input = "a\tb\tc\nd\te\tf\ng\th\ti".as_bytes();
        let result = slice_field(input, Some(0), Some(1), 1, '\t', false).unwrap();
        let output = String::from_utf8(result).unwrap();
        assert_eq!(output, "b\ne\n");
    }
    
    #[tokio::test]
    async fn test_slice_reverse() {
        let input = "line1\nline2\nline3\nline4\nline5".as_bytes();
        let result = slice_lines(input, Some(1), Some(3), true).unwrap();
        let output = String::from_utf8(result).unwrap();
        assert_eq!(output, "line1\nline5\n");
    }
} 