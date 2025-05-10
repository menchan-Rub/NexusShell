use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;

/// テキストを標準出力に表示するコマンド
///
/// UNIXの標準的なechoコマンドの実装です。指定されたテキストを標準出力に表示します。
/// `-n` オプションを指定すると、末尾の改行を出力しません。
/// `-e` オプションを指定すると、バックスラッシュエスケープシーケンスを解釈します。
///
/// # 使用例
///
/// ```bash
/// echo "Hello, World!"     # Hello, World! を表示（改行あり）
/// echo -n "No newline"     # 改行なしで表示
/// echo -e "Tab\tCharacter" # タブ文字を含めて表示
/// ```
pub struct EchoCommand;

#[async_trait]
impl BuiltinCommand for EchoCommand {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn description(&self) -> &'static str {
        "テキストを標準出力に表示します"
    }

    fn usage(&self) -> &'static str {
        "echo [-neE] [文字列...]\n\n-n オプションで末尾の改行を抑制します。\n-e オプションでバックスラッシュエスケープを有効にします。\n-E オプションでバックスラッシュエスケープを無効にします（デフォルト）。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // 引数を取得（最初の引数はコマンド名なので、それ以降を使用）
        let args = context.args.iter().skip(1).collect::<Vec<_>>();
        
        // オプションとテキスト引数を処理
        let mut suppress_newline = false;
        let mut interpret_backslash = false;
        let mut text_args = Vec::new();
        
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            
            // オプション解析
            if arg.starts_with('-') && arg.len() > 1 {
                for c in arg.chars().skip(1) {
                    match c {
                        'n' => suppress_newline = true,
                        'e' => interpret_backslash = true,
                        'E' => interpret_backslash = false,
                        _ => {
                            // 不明なオプションは通常のテキストとして扱う
                            text_args.push(arg.clone());
                            break;
                        }
                    }
                }
            } else {
                // 通常のテキスト引数
                text_args.push(arg.clone());
            }
            
            i += 1;
        }
        
        // テキストを連結
        let output_text = if text_args.is_empty() {
            String::new()
        } else {
            text_args.join(" ")
        };
        
        debug!("echoコマンド実行: text={}, suppress_newline={}, interpret_backslash={}", 
            output_text, suppress_newline, interpret_backslash);
        
        // バックスラッシュエスケープを解釈
        let processed_text = if interpret_backslash {
            process_backslash_escapes(&output_text)
        } else {
            output_text
        };
        
        // 出力を作成
        let mut output = processed_text.into_bytes();
        
        // 改行を追加（-nが指定されていない場合）
        if !suppress_newline {
            output.push(b'\n');
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// バックスラッシュエスケープシーケンスを解釈
fn process_backslash_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some() {
            // バックスラッシュエスケープを処理
            match chars.next().unwrap() {
                'a' => result.push('\x07'), // ベル
                'b' => result.push('\x08'), // バックスペース
                'e' => result.push('\x1B'), // エスケープ
                'f' => result.push('\x0C'), // フォームフィード
                'n' => result.push('\n'),   // 改行
                'r' => result.push('\r'),   // キャリッジリターン
                't' => result.push('\t'),   // タブ
                'v' => result.push('\x0B'), // 垂直タブ
                '\\' => result.push('\\'),  // バックスラッシュ
                '0' => {
                    // 8進数エスケープシーケンス（\0nnn）
                    let mut octal_value = String::new();
                    
                    // 最大3桁の8進数を読み取る
                    for _ in 0..3 {
                        if let Some(&next) = chars.peek() {
                            if next >= '0' && next <= '7' {
                                octal_value.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    
                    // 8進数を解析
                    if let Ok(value) = u8::from_str_radix(&octal_value, 8) {
                        // 有効なUTF-8文字に変換できる場合のみ追加
                        if let Some(c) = char::from_u32(value as u32) {
                            result.push(c);
                        }
                    }
                }
                'x' => {
                    // 16進数エスケープシーケンス（\xHH）
                    let mut hex_value = String::new();
                    
                    // 最大2桁の16進数を読み取る
                    for _ in 0..2 {
                        if let Some(&next) = chars.peek() {
                            if (next >= '0' && next <= '9') || 
                               (next >= 'a' && next <= 'f') || 
                               (next >= 'A' && next <= 'F') {
                                hex_value.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    
                    // 16進数を解析
                    if let Ok(value) = u8::from_str_radix(&hex_value, 16) {
                        // 有効なUTF-8文字に変換できる場合のみ追加
                        if let Some(c) = char::from_u32(value as u32) {
                            result.push(c);
                        }
                    }
                }
                'u' => {
                    // Unicode 16ビットエスケープシーケンス（\uHHHH）
                    let mut unicode_value = String::new();
                    
                    // 4桁の16進数を読み取る
                    for _ in 0..4 {
                        if let Some(&next) = chars.peek() {
                            if (next >= '0' && next <= '9') || 
                               (next >= 'a' && next <= 'f') || 
                               (next >= 'A' && next <= 'F') {
                                unicode_value.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    
                    // 16進数を解析
                    if let Ok(value) = u16::from_str_radix(&unicode_value, 16) {
                        // 有効なUTF-8文字に変換できる場合のみ追加
                        if let Some(c) = char::from_u32(value as u32) {
                            result.push(c);
                        }
                    }
                }
                'U' => {
                    // Unicode 32ビットエスケープシーケンス（\UHHHHHHHH）
                    let mut unicode_value = String::new();
                    
                    // 8桁の16進数を読み取る
                    for _ in 0..8 {
                        if let Some(&next) = chars.peek() {
                            if (next >= '0' && next <= '9') || 
                               (next >= 'a' && next <= 'f') || 
                               (next >= 'A' && next <= 'F') {
                                unicode_value.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    
                    // 16進数を解析
                    if let Ok(value) = u32::from_str_radix(&unicode_value, 16) {
                        // 有効なUTF-8文字に変換できる場合のみ追加
                        if let Some(c) = char::from_u32(value) {
                            result.push(c);
                        }
                    }
                }
                'c' => {
                    // 出力を終了（改行なし）
                    return result;
                }
                _ => {
                    // 未知のエスケープシーケンスはそのまま出力
                    result.push('\\');
                    result.push(chars.next().unwrap());
                }
            }
        } else {
            // 通常の文字はそのまま追加
            result.push(c);
        }
    }
    
    result
} 