use crate::error::ParserError;
use crate::Span;
use std::collections::HashMap;
use std::path::PathBuf;
use std::env;
use regex::Regex;
use std::str::FromStr;

/// 環境変数展開の種類
#[derive(Debug, Clone, PartialEq)]
pub enum VariableExpansionKind {
    /// 通常の変数展開 ($VAR または ${VAR})
    Normal,
    /// デフォルト値の設定 (${VAR:-default})
    DefaultValue,
    /// 代替値の設定 (${VAR:+alternative})
    AlternativeValue,
    /// エラーメッセージの表示 (${VAR:?error})
    ErrorIfUnset,
    /// 変数が未設定または空の場合に値を設定 (${VAR:=value})
    AssignDefaultValue,
    /// パス名展開 (${VAR%pattern} または ${VAR%%pattern})
    RemoveSuffix,
    /// パス名展開 (${VAR#pattern} または ${VAR##pattern})
    RemovePrefix,
    /// サブストリング展開 (${VAR:offset:length})
    Substring,
    /// パターン置換 (${VAR/pattern/replacement})
    PatternReplacement,
    /// 大文字変換 (${VAR^^})
    ToUppercase,
    /// 小文字変換 (${VAR,,})
    ToLowercase,
    /// 長さ取得 (${#VAR})
    Length,
    /// 配列展開 (${VAR[*]} または ${VAR[@]})
    ArrayExpansion,
}

/// 変数置換の結果
#[derive(Debug, Clone)]
pub struct VariableResolutionResult {
    /// 解決された値
    pub value: String,
    /// エラーがある場合
    pub error: Option<ParserError>,
    /// 元の変数式
    pub original_expression: String,
    /// 展開の種類
    pub expansion_kind: VariableExpansionKind,
    /// 位置情報
    pub span: Span,
}

/// 環境変数リゾルバ
/// 
/// 環境変数の解決を担当するモジュールです。
/// トークン列に含まれる環境変数参照（$VAR形式）を解決し、実際の値に置き換えます。
/// また、環境変数が見つからない場合はエラーを生成します。
pub struct EnvResolver {
    /// 環境変数のキャッシュ
    env_cache: HashMap<String, String>,
    
    /// カスタム環境変数
    custom_vars: HashMap<String, String>,
    
    /// ホームディレクトリパス
    home_dir: Option<PathBuf>,
    
    /// 配列変数
    array_vars: HashMap<String, Vec<String>>,
    
    /// 最後に解決された変数のキャッシュ
    last_resolved: HashMap<String, VariableResolutionResult>,
}

impl EnvResolver {
    /// 新しい環境変数リゾルバを作成します
    pub fn new() -> Self {
        let home_dir = dirs::home_dir();
        
        Self {
            env_cache: HashMap::new(),
            custom_vars: HashMap::new(),
            home_dir,
            array_vars: HashMap::new(),
            last_resolved: HashMap::new(),
        }
    }
    
    /// カスタム環境変数を設定します
    pub fn set_custom_var(&mut self, name: &str, value: &str) {
        self.custom_vars.insert(name.to_string(), value.to_string());
    }
    
    /// カスタム環境変数を削除します
    pub fn remove_custom_var(&mut self, name: &str) -> Option<String> {
        self.custom_vars.remove(name)
    }
    
    /// 全てのカスタム環境変数をクリアします
    pub fn clear_custom_vars(&mut self) {
        self.custom_vars.clear();
    }
    
    /// 環境変数キャッシュをクリアします
    pub fn clear_cache(&mut self) {
        self.env_cache.clear();
    }
    
    /// 配列変数を設定します
    pub fn set_array_var(&mut self, name: &str, values: Vec<String>) {
        self.array_vars.insert(name.to_string(), values);
    }
    
    /// 配列変数を取得します
    pub fn get_array_var(&self, name: &str) -> Option<&Vec<String>> {
        self.array_vars.get(name)
    }
    
    /// 配列変数を削除します
    pub fn remove_array_var(&mut self, name: &str) -> Option<Vec<String>> {
        self.array_vars.remove(name)
    }
    
    /// 最後に解決された変数を取得します
    pub fn get_last_resolved(&self, expression: &str) -> Option<&VariableResolutionResult> {
        self.last_resolved.get(expression)
    }
    
    /// トークン列の環境変数を解決します
    pub fn resolve_tokens(&mut self, tokens: &[Token]) -> (Vec<Token>, Vec<ParserError>) {
        let mut resolved_tokens = Vec::with_capacity(tokens.len());
        let mut errors = Vec::new();
        
        for token in tokens {
            if let TokenKind::Variable = token.kind {
                // 変数トークンを解決
                let resolution = self.resolve_pattern_expansion(&token.lexeme, &token.span);
                
                if let Some(err) = resolution.error {
                    errors.push(err);
                }
                
                // 解決された値をトークンとして追加
                resolved_tokens.push(Token {
                    kind: TokenKind::String,
                    lexeme: resolution.value,
                    span: token.span.clone(),
                });
            } else if let TokenKind::String = token.kind {
                // 文字列トークン内の環境変数参照を解決
                if token.lexeme.contains('$') || token.lexeme.contains('~') {
                    let (resolved_str, str_errors) = self.resolve_complex_string(&token.lexeme, &token.span);
                    resolved_tokens.push(Token {
                        kind: TokenKind::String,
                        lexeme: resolved_str,
                        span: token.span.clone(),
                    });
                    errors.extend(str_errors);
                } else {
                    resolved_tokens.push(token.clone());
                }
            } else {
                resolved_tokens.push(token.clone());
            }
        }
        
        (resolved_tokens, errors)
    }
    
    /// 文字列内の環境変数を解決します
    fn resolve_string_vars(&mut self, input: &str, span: &Span) -> (String, Vec<ParserError>) {
        let mut result = String::with_capacity(input.len());
        let mut errors = Vec::new();
        let mut chars = input.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '$' {
                if let Some(&next_char) = chars.peek() {
                    if next_char == '{' {
                        // ${VAR} 形式の環境変数
                        chars.next(); // '{' を消費
                        let var_name = self.collect_var_name_braced(&mut chars);
                        match self.resolve_variable(&var_name) {
                            Some(value) => result.push_str(&value),
                            None => {
                                errors.push(ParserError::EnvironmentError {
                                    message: format!("環境変数 '{}' が見つかりません", var_name),
                                    span: span.clone(),
                                    severity: ErrorSeverity::Warning,
                                });
                                result.push_str(&format!("${{{}}}", var_name));
                            }
                        }
                    } else {
                        // $VAR 形式の環境変数
                        let var_name = self.collect_var_name(&mut chars);
                        if var_name.is_empty() {
                            result.push('$');
                        } else {
                            match self.resolve_variable(&var_name) {
                                Some(value) => result.push_str(&value),
                                None => {
                                    errors.push(ParserError::EnvironmentError {
                                        message: format!("環境変数 '{}' が見つかりません", var_name),
                                        span: span.clone(),
                                        severity: ErrorSeverity::Warning,
                                    });
                                    result.push_str(&format!("${}", var_name));
                                }
                            }
                        }
                    }
                } else {
                    result.push('$');
                }
            } else if c == '~' && (result.is_empty() || result.ends_with('/')) {
                // チルダ展開 (~/ → ホームディレクトリ)
                if let Some(home) = &self.home_dir {
                    result.push_str(home.to_string_lossy().as_ref());
                } else {
                    result.push('~');
                    errors.push(ParserError::EnvironmentError {
                        message: "ホームディレクトリが見つかりません".to_string(),
                        span: span.clone(),
                        severity: ErrorSeverity::Warning,
                    });
                }
            } else {
                result.push(c);
            }
        }
        
        (result, errors)
    }
    
    /// $VAR 形式の環境変数名を抽出します
    fn extract_var_name(&self, var_token: &str) -> String {
        if var_token.starts_with('$') {
            if var_token.len() > 1 && var_token.chars().nth(1) == Some('{') 
               && var_token.ends_with('}') {
                // ${VAR} 形式
                var_token[2..var_token.len()-1].to_string()
            } else {
                // $VAR 形式
                var_token[1..].to_string()
            }
        } else {
            var_token.to_string()
        }
    }
    
    /// 環境変数を解決します
    fn resolve_variable(&mut self, name: &str) -> Option<String> {
        // まずカスタム変数を確認
        if let Some(value) = self.custom_vars.get(name) {
            return Some(value.clone());
        }
        
        // キャッシュを確認
        if let Some(value) = self.env_cache.get(name) {
            return Some(value.clone());
        }
        
        // 環境から取得
        if let Ok(value) = env::var(name) {
            // キャッシュに保存
            self.env_cache.insert(name.to_string(), value.clone());
            Some(value)
        } else {
            // 特殊変数の処理
            match name {
                "HOME" => {
                    if let Some(home) = &self.home_dir {
                        let home_str = home.to_string_lossy().to_string();
                        self.env_cache.insert("HOME".to_string(), home_str.clone());
                        Some(home_str)
                    } else {
                        None
                    }
                },
                "PWD" => {
                    if let Ok(pwd) = env::current_dir() {
                        let pwd_str = pwd.to_string_lossy().to_string();
                        self.env_cache.insert("PWD".to_string(), pwd_str.clone());
                        Some(pwd_str)
                    } else {
                        None
                    }
                },
                "RANDOM" => {
                    // 0-32767の乱数を生成
                    use rand::Rng;
                    let mut rng = rand::thread_rng();
                    let random_value = rng.gen_range(0..32768).to_string();
                    Some(random_value)
                },
                "SECONDS" => {
                    // プロセス起動からの秒数
                    use std::time::{SystemTime, UNIX_EPOCH};
                    if let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) {
                        Some(duration.as_secs().to_string())
                    } else {
                        Some("0".to_string())
                    }
                },
                "HOSTNAME" => {
                    if let Ok(hostname) = hostname::get() {
                        if let Ok(hostname_str) = hostname.into_string() {
                            self.env_cache.insert("HOSTNAME".to_string(), hostname_str.clone());
                            Some(hostname_str)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                },
                "UID" => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::parent_id;
                        Some(parent_id().to_string())
                    }
                    #[cfg(not(unix))]
                    {
                        Some("1000".to_string())
                    }
                },
                "$" => {
                    // プロセスID
                    use std::process;
                    Some(process::id().to_string())
                },
                // その他の特殊変数がある場合ここに追加
                _ => None
            }
        }
    }
    
    /// ${VAR} 形式の環境変数名を文字ストリームから収集します
    fn collect_var_name_braced<I>(&self, chars: &mut std::iter::Peekable<I>) -> String
    where
        I: Iterator<Item = char>
    {
        let mut var_name = String::new();
        let mut depth = 1;
        
        while let Some(&c) = chars.peek() {
            if c == '}' {
                depth -= 1;
                if depth == 0 {
                    chars.next(); // '}' を消費
                    break;
                } else {
                    var_name.push(c);
                    chars.next();
                }
            } else if c == '{' {
                depth += 1;
                var_name.push(c);
                chars.next();
            } else {
                var_name.push(c);
                chars.next();
            }
        }
        
        var_name
    }
    
    /// $VAR 形式の環境変数名を文字ストリームから収集します
    fn collect_var_name<I>(&self, chars: &mut std::iter::Peekable<I>) -> String
    where
        I: Iterator<Item = char>
    {
        let mut var_name = String::new();
        
        while let Some(&c) = chars.peek() {
            if is_alphanumeric_or_underscore(c) {
                var_name.push(c);
                chars.next();
            } else {
                break;
            }
        }
        
        var_name
    }
    
    /// パターン式を解析し、${VAR:mod} 形式の環境変数を解決します
    pub fn resolve_pattern_expansion(&mut self, expression: &str, span: &Span) -> VariableResolutionResult {
        // 基本パターン: ${VAR...} または $VAR
        let mut var_name = String::new();
        let mut pattern = String::new();
        let mut replacement = String::new();
        let mut expansion_kind = VariableExpansionKind::Normal;
        
        // 式の解析
        if expression.starts_with("${") && expression.ends_with('}') {
            let content = &expression[2..expression.len()-1];
            
            // 長さ取得パターン: ${#VAR}
            if content.starts_with('#') && !content.contains(':') && !content.contains('/') {
                var_name = content[1..].to_string();
                expansion_kind = VariableExpansionKind::Length;
            }
            // 大文字変換パターン: ${VAR^^}
            else if content.ends_with("^^") {
                var_name = content[..content.len()-2].to_string();
                expansion_kind = VariableExpansionKind::ToUppercase;
            }
            // 小文字変換パターン: ${VAR,,}
            else if content.ends_with(",,") {
                var_name = content[..content.len()-2].to_string();
                expansion_kind = VariableExpansionKind::ToLowercase;
            }
            // 配列展開パターン: ${VAR[*]} または ${VAR[@]}
            else if content.contains('[') && (content.contains("[*]") || content.contains("[@]")) {
                let end_idx = content.find('[').unwrap_or(content.len());
                var_name = content[..end_idx].to_string();
                expansion_kind = VariableExpansionKind::ArrayExpansion;
            }
            // デフォルト値パターン: ${VAR:-default}
            else if content.contains(":-") {
                let parts: Vec<&str> = content.splitn(2, ":-").collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                }
                expansion_kind = VariableExpansionKind::DefaultValue;
            }
            // 代替値パターン: ${VAR:+alternative}
            else if content.contains(":+") {
                let parts: Vec<&str> = content.splitn(2, ":+").collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                }
                expansion_kind = VariableExpansionKind::AlternativeValue;
            }
            // エラーメッセージパターン: ${VAR:?error}
            else if content.contains(":?") {
                let parts: Vec<&str> = content.splitn(2, ":?").collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                }
                expansion_kind = VariableExpansionKind::ErrorIfUnset;
            }
            // 代入パターン: ${VAR:=value}
            else if content.contains(":=") {
                let parts: Vec<&str> = content.splitn(2, ":=").collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                }
                expansion_kind = VariableExpansionKind::AssignDefaultValue;
            }
            // サフィックス削除パターン: ${VAR%pattern} または ${VAR%%pattern}
            else if content.contains('%') {
                // %% (最長一致) と % (最短一致) を区別
                let is_greedy = content.contains("%%");
                let separator = if is_greedy { "%%" } else { "%" };
                let parts: Vec<&str> = content.splitn(2, separator).collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                }
                expansion_kind = VariableExpansionKind::RemoveSuffix;
            }
            // プレフィックス削除パターン: ${VAR#pattern} または ${VAR##pattern}
            else if content.contains('#') && !content.starts_with('#') {
                // ## (最長一致) と # (最短一致) を区別
                let is_greedy = content.contains("##");
                let separator = if is_greedy { "##" } else { "#" };
                let parts: Vec<&str> = content.splitn(2, separator).collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                }
                expansion_kind = VariableExpansionKind::RemovePrefix;
            }
            // サブストリングパターン: ${VAR:offset:length}
            else if content.contains(':') && !content.contains(":-") && !content.contains(":+") && 
                     !content.contains(":?") && !content.contains(":=") {
                let parts: Vec<&str> = content.splitn(3, ':').collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                    if parts.len() > 2 {
                        replacement = parts[2].to_string();
                    }
                }
                expansion_kind = VariableExpansionKind::Substring;
            }
            // パターン置換: ${VAR/pattern/replacement}
            else if content.contains('/') {
                let parts: Vec<&str> = content.splitn(3, '/').collect();
                var_name = parts[0].to_string();
                if parts.len() > 1 {
                    pattern = parts[1].to_string();
                    if parts.len() > 2 {
                        replacement = parts[2].to_string();
                    }
                }
                expansion_kind = VariableExpansionKind::PatternReplacement;
            }
            // 通常の変数展開: ${VAR}
            else {
                var_name = content.to_string();
                expansion_kind = VariableExpansionKind::Normal;
            }
        } else if expression.starts_with('$') {
            var_name = expression[1..].to_string();
            expansion_kind = VariableExpansionKind::Normal;
        } else {
            return VariableResolutionResult {
                value: expression.to_string(),
                error: Some(ParserError::EnvironmentError {
                    message: format!("無効な変数式: {}", expression),
                    span: span.clone(),
                    severity: crate::ErrorSeverity::Warning,
                }),
                original_expression: expression.to_string(),
                expansion_kind,
                span: span.clone(),
            };
        }
        
        // 変数の値を取得
        let var_value = self.resolve_variable(&var_name);
        
        // 展開の種類に応じて処理
        let result = match expansion_kind {
            VariableExpansionKind::Normal => {
                match var_value {
                    Some(value) => VariableResolutionResult {
                        value,
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::Length => {
                match var_value {
                    Some(value) => VariableResolutionResult {
                        value: value.len().to_string(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    None => VariableResolutionResult {
                        value: "0".to_string(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::ToUppercase => {
                match var_value {
                    Some(value) => VariableResolutionResult {
                        value: value.to_uppercase(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::ToLowercase => {
                match var_value {
                    Some(value) => VariableResolutionResult {
                        value: value.to_lowercase(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::ArrayExpansion => {
                // 配列変数の展開
                if let Some(array) = self.get_array_var(&var_name) {
                    VariableResolutionResult {
                        value: array.join(" "),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    }
                } else {
                    // 通常の変数値を空白で分割して配列として扱う
                    match var_value {
                        Some(value) => VariableResolutionResult {
                            value: value.to_string(),
                            error: None,
                            original_expression: expression.to_string(),
                            expansion_kind,
                            span: span.clone(),
                        },
                        None => VariableResolutionResult {
                            value: String::new(),
                            error: Some(ParserError::EnvironmentError {
                                message: format!("配列変数 '{}' が見つかりません", var_name),
                                span: span.clone(),
                                severity: crate::ErrorSeverity::Warning,
                            }),
                            original_expression: expression.to_string(),
                            expansion_kind,
                            span: span.clone(),
                        },
                    }
                }
            },
            VariableExpansionKind::DefaultValue => {
                // 変数が未設定または空の場合にデフォルト値を使用
                match var_value {
                    Some(value) if !value.is_empty() => VariableResolutionResult {
                        value,
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    _ => VariableResolutionResult {
                        value: pattern.clone(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::AlternativeValue => {
                // 変数が設定されている場合に代替値を使用
                match var_value {
                    Some(value) if !value.is_empty() => VariableResolutionResult {
                        value: pattern.clone(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    _ => VariableResolutionResult {
                        value: String::new(),
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::ErrorIfUnset => {
                // 変数が未設定または空の場合にエラーメッセージを表示
                match var_value {
                    Some(value) if !value.is_empty() => VariableResolutionResult {
                        value,
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    _ => {
                        let error_message = if pattern.is_empty() {
                            format!("環境変数 '{}' が未設定です", var_name)
                        } else {
                            pattern.clone()
                        };
                        
                        VariableResolutionResult {
                            value: String::new(),
                            error: Some(ParserError::EnvironmentError {
                                message: error_message,
                                span: span.clone(),
                                severity: crate::ErrorSeverity::Error,
                            }),
                            original_expression: expression.to_string(),
                            expansion_kind,
                            span: span.clone(),
                        }
                    },
                }
            },
            VariableExpansionKind::AssignDefaultValue => {
                // 変数が未設定または空の場合に値を設定
                match var_value {
                    Some(value) if !value.is_empty() => VariableResolutionResult {
                        value,
                        error: None,
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                    _ => {
                        // 変数を設定
                        self.set_custom_var(&var_name, &pattern);
                        
                        VariableResolutionResult {
                            value: pattern.clone(),
                            error: None,
                            original_expression: expression.to_string(),
                            expansion_kind,
                            span: span.clone(),
                        }
                    },
                }
            },
            VariableExpansionKind::RemoveSuffix => {
                match var_value {
                    Some(value) => {
                        if pattern.is_empty() {
                            return VariableResolutionResult {
                                value,
                                error: None,
                                original_expression: expression.to_string(),
                                expansion_kind,
                                span: span.clone(),
                            };
                        }
                        
                        // パターンがワイルドカードを含むかチェック
                        if pattern.contains('*') || pattern.contains('?') {
                            // ワイルドカードパターンをregexに変換
                            let regex_pattern = Self::wildcard_to_regex(&pattern);
                            match Regex::new(&regex_pattern) {
                                Ok(re) => {
                                    // 接尾辞を検索
                                    let result = if let Some(cap) = re.find(&value) {
                                        if cap.start() > 0 && cap.end() == value.len() {
                                            value[0..cap.start()].to_string()
                                        } else {
                                            value
                                        }
                                    } else {
                                        value
                                    };
                                    
                                    VariableResolutionResult {
                                        value: result,
                                        error: None,
                                        original_expression: expression.to_string(),
                                        expansion_kind,
                                        span: span.clone(),
                                    }
                                },
                                Err(_) => VariableResolutionResult {
                                    value,
                                    error: Some(ParserError::EnvironmentError {
                                        message: format!("無効なパターン: {}", pattern),
                                        span: span.clone(),
                                        severity: crate::ErrorSeverity::Warning,
                                    }),
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                },
                            }
                        } else {
                            // 単純な文字列サフィックス
                            if value.ends_with(&pattern) {
                                let new_len = value.len() - pattern.len();
                                VariableResolutionResult {
                                    value: value[..new_len].to_string(),
                                    error: None,
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                }
                            } else {
                                VariableResolutionResult {
                                    value,
                                    error: None,
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                }
                            }
                        }
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::RemovePrefix => {
                match var_value {
                    Some(value) => {
                        if pattern.is_empty() {
                            return VariableResolutionResult {
                                value,
                                error: None,
                                original_expression: expression.to_string(),
                                expansion_kind,
                                span: span.clone(),
                            };
                        }
                        
                        // パターンがワイルドカードを含むかチェック
                        if pattern.contains('*') || pattern.contains('?') {
                            // ワイルドカードパターンをregexに変換
                            let regex_pattern = Self::wildcard_to_regex(&pattern);
                            match Regex::new(&regex_pattern) {
                                Ok(re) => {
                                    // 接頭辞を検索
                                    let result = if let Some(cap) = re.find(&value) {
                                        if cap.start() == 0 {
                                            value[cap.end()..].to_string()
                                        } else {
                                            value
                                        }
                                    } else {
                                        value
                                    };
                                    
                                    VariableResolutionResult {
                                        value: result,
                                        error: None,
                                        original_expression: expression.to_string(),
                                        expansion_kind,
                                        span: span.clone(),
                                    }
                                },
                                Err(_) => VariableResolutionResult {
                                    value,
                                    error: Some(ParserError::EnvironmentError {
                                        message: format!("無効なパターン: {}", pattern),
                                        span: span.clone(),
                                        severity: crate::ErrorSeverity::Warning,
                                    }),
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                },
                            }
                        } else {
                            // 単純な文字列プレフィックス
                            if value.starts_with(&pattern) {
                                VariableResolutionResult {
                                    value: value[pattern.len()..].to_string(),
                                    error: None,
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                }
                            } else {
                                VariableResolutionResult {
                                    value,
                                    error: None,
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                }
                            }
                        }
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::Substring => {
                match var_value {
                    Some(value) => {
                        // offset と length を解析
                        let offset = pattern.parse::<isize>().unwrap_or(0);
                        let length = replacement.parse::<usize>().ok();
                        
                        // offsetが負の場合は末尾からのオフセット
                        let start_idx = if offset < 0 {
                            value.len().saturating_sub(offset.unsigned_abs())
                        } else {
                            offset as usize
                        };
                        
                        // 範囲外のインデックスを処理
                        if start_idx >= value.len() {
                            return VariableResolutionResult {
                                value: String::new(),
                                error: None,
                                original_expression: expression.to_string(),
                                expansion_kind,
                                span: span.clone(),
                            };
                        }
                        
                        // 指定された長さを適用
                        let result = if let Some(len) = length {
                            let end_idx = start_idx.saturating_add(len).min(value.len());
                            value[start_idx..end_idx].to_string()
                        } else {
                            value[start_idx..].to_string()
                        };
                        
                        VariableResolutionResult {
                            value: result,
                            error: None,
                            original_expression: expression.to_string(),
                            expansion_kind,
                            span: span.clone(),
                        }
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
            VariableExpansionKind::PatternReplacement => {
                match var_value {
                    Some(value) => {
                        if pattern.is_empty() {
                            return VariableResolutionResult {
                                value,
                                error: None,
                                original_expression: expression.to_string(),
                                expansion_kind,
                                span: span.clone(),
                            };
                        }
                        
                        // パターンがワイルドカードを含むかチェック
                        if pattern.contains('*') || pattern.contains('?') {
                            // ワイルドカードパターンをregexに変換
                            let regex_pattern = Self::wildcard_to_regex(&pattern);
                            match Regex::new(&regex_pattern) {
                                Ok(re) => {
                                    // パターンを置換
                                    let result = re.replace(&value, &replacement);
                                    VariableResolutionResult {
                                        value: result.to_string(),
                                        error: None,
                                        original_expression: expression.to_string(),
                                        expansion_kind,
                                        span: span.clone(),
                                    }
                                },
                                Err(_) => VariableResolutionResult {
                                    value,
                                    error: Some(ParserError::EnvironmentError {
                                        message: format!("無効なパターン: {}", pattern),
                                        span: span.clone(),
                                        severity: crate::ErrorSeverity::Warning,
                                    }),
                                    original_expression: expression.to_string(),
                                    expansion_kind,
                                    span: span.clone(),
                                },
                            }
                        } else {
                            // 単純な文字列置換
                            VariableResolutionResult {
                                value: value.replace(&pattern, &replacement),
                                error: None,
                                original_expression: expression.to_string(),
                                expansion_kind,
                                span: span.clone(),
                            }
                        }
                    },
                    None => VariableResolutionResult {
                        value: String::new(),
                        error: Some(ParserError::EnvironmentError {
                            message: format!("環境変数 '{}' が見つかりません", var_name),
                            span: span.clone(),
                            severity: crate::ErrorSeverity::Warning,
                        }),
                        original_expression: expression.to_string(),
                        expansion_kind,
                        span: span.clone(),
                    },
                }
            },
        };
        
        // 結果をキャッシュして返す
        self.last_resolved.insert(expression.to_string(), result.clone());
        result
    }
    
    /// ワイルドカードパターンを正規表現に変換
    fn wildcard_to_regex(pattern: &str) -> String {
        let mut regex = String::with_capacity(pattern.len() * 2);
        regex.push('^');
        
        for c in pattern.chars() {
            match c {
                '*' => regex.push_str(".*"),
                '?' => regex.push('.'),
                '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' | '^' | '$' | '|' => {
                    regex.push('\\');
                    regex.push(c);
                },
                _ => regex.push(c),
            }
        }
        
        regex.push('$');
        regex
    }
    
    /// 高度な変数展開を含む文字列を解決します
    pub fn resolve_complex_string(&mut self, input: &str, span: &Span) -> (String, Vec<ParserError>) {
        let mut result = String::with_capacity(input.len());
        let mut errors = Vec::new();
        let mut chars = input.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '$' {
                // $ で始まる変数展開
                if let Some(&next_char) = chars.peek() {
                    if next_char == '{' {
                        // ${VAR} 形式の変数展開
                        chars.next(); // '{' を消費
                        
                        // } までの全ての文字を収集
                        let mut expression = String::from("${");
                        let mut depth = 1;
                        
                        while let Some(c) = chars.next() {
                            expression.push(c);
                            
                            if c == '{' {
                                depth += 1;
                            } else if c == '}' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                        
                        // 変数展開を解決
                        let resolution = self.resolve_pattern_expansion(&expression, span);
                        
                        if let Some(err) = resolution.error {
                            errors.push(err);
                        }
                        
                        result.push_str(&resolution.value);
                    } else {
                        // $VAR 形式の変数展開
                        let var_name = self.collect_var_name(&mut chars);
                        
                        if var_name.is_empty() {
                            result.push('$');
                        } else {
                            let expression = format!("${}", var_name);
                            let resolution = self.resolve_pattern_expansion(&expression, span);
                            
                            if let Some(err) = resolution.error {
                                errors.push(err);
                            }
                            
                            result.push_str(&resolution.value);
                        }
                    }
                } else {
                    result.push('$');
                }
            } else if c == '~' && (result.is_empty() || result.ends_with('/')) {
                // チルダ展開 (~/ → ホームディレクトリ)
                if let Some(home) = &self.home_dir {
                    result.push_str(home.to_string_lossy().as_ref());
                } else {
                    result.push('~');
                    errors.push(ParserError::EnvironmentError {
                        message: "ホームディレクトリが見つかりません".to_string(),
                        span: span.clone(),
                        severity: crate::ErrorSeverity::Warning,
                    });
                }
            } else {
                result.push(c);
            }
        }
        
        (result, errors)
    }
    
    /// 文字列解析用の拡張関数
    pub fn parse_numeric_value(&self, value: &str) -> Option<i64> {
        // 様々な形式の数値をパース（10進数、16進数、8進数、2進数）
        if value.starts_with("0x") || value.starts_with("0X") {
            // 16進数
            i64::from_str_radix(&value[2..], 16).ok()
        } else if value.starts_with("0b") || value.starts_with("0B") {
            // 2進数
            i64::from_str_radix(&value[2..], 2).ok()
        } else if value.starts_with('0') && value.len() > 1 && value.chars().skip(1).all(|c| c.is_digit(8)) {
            // 8進数
            i64::from_str_radix(&value[1..], 8).ok()
        } else {
            // 10進数
            value.parse::<i64>().ok()
        }
    }
}

/// 英数字またはアンダースコアかどうかを判定
fn is_alphanumeric_or_underscore(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resolve_simple_var() {
        let mut resolver = EnvResolver::new();
        resolver.set_custom_var("TEST_VAR", "test_value");
        
        let tokens = vec![
            Token {
                kind: TokenKind::Variable,
                lexeme: "$TEST_VAR".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "test_value");
    }
    
    #[test]
    fn test_resolve_string_with_vars() {
        let mut resolver = EnvResolver::new();
        resolver.set_custom_var("USER", "tester");
        resolver.set_custom_var("HOME", "/home/tester");
        
        let tokens = vec![
            Token {
                kind: TokenKind::String,
                lexeme: "Hello $USER, your home is $HOME".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "Hello tester, your home is /home/tester");
    }
    
    #[test]
    fn test_resolve_missing_var() {
        let mut resolver = EnvResolver::new();
        
        let tokens = vec![
            Token {
                kind: TokenKind::Variable,
                lexeme: "$NONEXISTENT_VAR".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 1);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "");
    }
    
    #[test]
    fn test_tilde_expansion() {
        let mut resolver = EnvResolver::new();
        
        // ホームディレクトリを手動で設定
        resolver.home_dir = Some(PathBuf::from("/home/tester"));
        
        let tokens = vec![
            Token {
                kind: TokenKind::String,
                lexeme: "~/documents".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "/home/tester/documents");
    }
    
    #[test]
    fn test_default_value_expansion() {
        let mut resolver = EnvResolver::new();
        
        let tokens = vec![
            Token {
                kind: TokenKind::Variable,
                lexeme: "${NONEXISTENT_VAR:-default_value}".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "default_value");
    }
    
    #[test]
    fn test_substring_expansion() {
        let mut resolver = EnvResolver::new();
        resolver.set_custom_var("FULL_TEXT", "Hello, World!");
        
        let tokens = vec![
            Token {
                kind: TokenKind::Variable,
                lexeme: "${FULL_TEXT:7:5}".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "World");
    }
    
    #[test]
    fn test_pattern_replacement() {
        let mut resolver = EnvResolver::new();
        resolver.set_custom_var("TEXT", "Hello, World!");
        
        let tokens = vec![
            Token {
                kind: TokenKind::Variable,
                lexeme: "${TEXT/World/NexusShell}".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "Hello, NexusShell!");
    }
    
    #[test]
    fn test_array_expansion() {
        let mut resolver = EnvResolver::new();
        resolver.set_array_var("FRUITS", vec!["apple".to_string(), "banana".to_string(), "orange".to_string()]);
        
        let tokens = vec![
            Token {
                kind: TokenKind::Variable,
                lexeme: "${FRUITS[*]}".to_string(),
                span: Span::default(),
            }
        ];
        
        let (resolved, errors) = resolver.resolve_tokens(&tokens);
        assert_eq!(errors.len(), 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].lexeme, "apple banana orange");
    }
} 