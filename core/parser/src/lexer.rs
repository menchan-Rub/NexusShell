use logos::{Logos, Lexer, SpannedIter, Source};
use crate::{Span, Token, TokenKind, Result, ParserError};
use crate::span::{SourceFile, SourceSnippet};
use std::sync::Arc;
use std::collections::HashMap;
use std::ops::Range;
use std::fmt::{self, Display, Formatter};
use std::rc::Rc;
use std::str::FromStr;
use regex;

/// 高度なトークンの種類を定義します
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
pub enum NexusToken {
    // 無視されるトークン
    #[regex(r"[ \t\n\r]+", logos::skip)]
    Whitespace,

    #[regex(r"#[^\n]*", logos::skip)]
    Comment,

    // リテラル
    #[regex(r"(true|false)", |lex| lex.slice().parse::<bool>().ok())]
    Boolean(bool),

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    Integer(i64),

    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    Float(f64),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let slice = lex.slice();
        let content = &slice[1..slice.len()-1]; // 引用符を削除
        Some(content.to_string())
    })]
    String(String),

    #[regex(r"'([^'\\]|\\.)*'", |lex| {
        let slice = lex.slice();
        let content = &slice[1..slice.len()-1]; // 引用符を削除
        Some(content.to_string())
    })]
    SingleQuotedString(String),

    #[regex(r"\$[a-zA-Z_][a-zA-Z0-9_]*", |lex| {
        let slice = lex.slice();
        Some(slice[1..].to_string()) // $ を削除
    })]
    Variable(String),

    #[regex(r"\${[^}]+}", |lex| {
        let slice = lex.slice();
        Some(slice[2..slice.len()-1].to_string()) // ${ と } を削除
    })]
    VariableExpression(String),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| Some(lex.slice().to_string()))]
    Identifier(String),

    #[regex(r"--[a-zA-Z][a-zA-Z0-9_-]*", |lex| {
        let slice = lex.slice();
        Some(slice[2..].to_string()) // -- を削除
    })]
    LongFlag(String),

    #[regex(r"-[a-zA-Z]", |lex| {
        let slice = lex.slice();
        Some(slice[1..].to_string()) // - を削除
    })]
    ShortFlag(String),

    // オペレーター
    #[token("|")]
    Pipe,

    #[token("||")]
    Or,

    #[token("&&")]
    And,

    #[token(";")]
    Semicolon,

    #[token("&")]
    Ampersand,

    #[token("<")]
    LessThan,

    #[token(">")]
    GreaterThan,

    #[token("<<")]
    HereDoc,

    #[token(">>")]
    Append,

    #[token("=")]
    Equals,

    #[token("(")]
    LeftParen,

    #[token(")")]
    RightParen,

    #[token("{")]
    LeftBrace,

    #[token("}")]
    RightBrace,

    #[token("[")]
    LeftBracket,

    #[token("]")]
    RightBracket,

    #[token(",")]
    Comma,

    #[token(".")]
    Dot,

    #[token(":")]
    Colon,

    #[token("!")]
    Bang,

    #[token("?")]
    QuestionMark,

    #[token("*")]
    Asterisk,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    #[token("^")]
    Caret,

    #[token("~")]
    Tilde,

    #[token("@")]
    At,

    #[token("$")]
    Dollar,

    #[token("`")]
    Backtick,

    #[token("\\")]
    Backslash,

    // 特殊トークン
    #[error]
    Error,

    // ファイル終端
    EOF,
}

impl Display for NexusToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            NexusToken::Whitespace => write!(f, "空白"),
            NexusToken::Comment => write!(f, "コメント"),
            NexusToken::Boolean(b) => write!(f, "真偽値: {}", b),
            NexusToken::Integer(i) => write!(f, "整数: {}", i),
            NexusToken::Float(fl) => write!(f, "浮動小数点: {}", fl),
            NexusToken::String(s) => write!(f, "文字列: \"{}\"", s),
            NexusToken::SingleQuotedString(s) => write!(f, "文字列: '{}'", s),
            NexusToken::Variable(v) => write!(f, "変数: ${}", v),
            NexusToken::VariableExpression(v) => write!(f, "変数式: ${{{}}}", v),
            NexusToken::Identifier(i) => write!(f, "識別子: {}", i),
            NexusToken::LongFlag(f) => write!(f, "長いフラグ: --{}", f),
            NexusToken::ShortFlag(f) => write!(f, "短いフラグ: -{}", f),
            NexusToken::Pipe => write!(f, "パイプ: |"),
            NexusToken::Or => write!(f, "論理和: ||"),
            NexusToken::And => write!(f, "論理積: &&"),
            NexusToken::Semicolon => write!(f, "セミコロン: ;"),
            NexusToken::Ampersand => write!(f, "アンパサンド: &"),
            NexusToken::LessThan => write!(f, "小なり: <"),
            NexusToken::GreaterThan => write!(f, "大なり: >"),
            NexusToken::HereDoc => write!(f, "ヒアドキュメント: <<"),
            NexusToken::Append => write!(f, "追記: >>"),
            NexusToken::Equals => write!(f, "等号: ="),
            NexusToken::LeftParen => write!(f, "左括弧: ("),
            NexusToken::RightParen => write!(f, "右括弧: )"),
            NexusToken::LeftBrace => write!(f, "左中括弧: {{"),
            NexusToken::RightBrace => write!(f, "右中括弧: }}"),
            NexusToken::LeftBracket => write!(f, "左角括弧: ["),
            NexusToken::RightBracket => write!(f, "右角括弧: ]"),
            NexusToken::Comma => write!(f, "カンマ: ,"),
            NexusToken::Dot => write!(f, "ドット: ."),
            NexusToken::Colon => write!(f, "コロン: :"),
            NexusToken::Bang => write!(f, "エクスクラメーション: !"),
            NexusToken::QuestionMark => write!(f, "クエスチョンマーク: ?"),
            NexusToken::Asterisk => write!(f, "アスタリスク: *"),
            NexusToken::Plus => write!(f, "プラス: +"),
            NexusToken::Minus => write!(f, "マイナス: -"),
            NexusToken::Slash => write!(f, "スラッシュ: /"),
            NexusToken::Percent => write!(f, "パーセント: %"),
            NexusToken::Caret => write!(f, "キャレット: ^"),
            NexusToken::Tilde => write!(f, "チルダ: ~"),
            NexusToken::At => write!(f, "アット: @"),
            NexusToken::Dollar => write!(f, "ドル: $"),
            NexusToken::Backtick => write!(f, "バッククォート: `"),
            NexusToken::Backslash => write!(f, "バックスラッシュ: \\"),
            NexusToken::Error => write!(f, "不正なトークン"),
            NexusToken::EOF => write!(f, "ファイル終端"),
        }
    }
}

/// レキサーエラーを表現する構造体
#[derive(Debug, Clone, PartialEq)]
pub struct LexerError {
    pub message: String,
}

/// エスケープシーケンスを処理する関数
fn unescape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('\'') => result.push('\''),
                Some('0') => result.push('\0'),
                Some(c) => result.push(c),
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// 位置情報を持つトークン
#[derive(Debug, Clone)]
pub struct TokenWithSpan {
    /// トークンの種類
    pub kind: NexusToken,
    /// トークンのソース内での位置
    pub span: Range<usize>,
    /// 行番号 (1-indexed)
    pub line: usize,
    /// 列番号 (1-indexed)
    pub column: usize,
}

impl TokenWithSpan {
    /// 新しいTokenWithSpanを作成
    pub fn new(kind: NexusToken, span: Range<usize>, line: usize, column: usize) -> Self {
        Self {
            kind,
            span,
            line,
            column,
        }
    }

    /// スパンの長さを取得
    pub fn len(&self) -> usize {
        self.span.end - self.span.start
    }

    /// スパンが空かどうかをチェック
    pub fn is_empty(&self) -> bool {
        self.span.start == self.span.end
    }
}

/// トークン解析結果
#[derive(Debug)]
pub struct TokenAnalysisResult {
    /// 解析したトークンのリスト
    pub tokens: Vec<TokenWithSpan>,
    /// 検出されたエラー
    pub errors: Vec<ParserError>,
    /// 統計情報
    pub statistics: HashMap<String, usize>,
    /// カスタム情報
    pub metadata: HashMap<String, String>,
}

/// 補完の種類を表す列挙体
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionType {
    /// コマンド補完
    Command(String),
    /// フラグ補完
    Flag(String),
    /// 引数補完
    Argument(String),
    /// ファイルパス補完
    FilePath(String),
    /// 変数補完
    Variable(String),
    /// 構文要素補完
    Syntax(String),
}

impl CompletionType {
    /// 補完の種類を文字列で返す
    pub fn to_string(&self) -> String {
        match self {
            CompletionType::Command(_) => "command".to_string(),
            CompletionType::Flag(_) => "flag".to_string(),
            CompletionType::Argument(_) => "argument".to_string(),
            CompletionType::FilePath(_) => "filepath".to_string(),
            CompletionType::Variable(_) => "variable".to_string(),
            CompletionType::Syntax(_) => "syntax".to_string(),
        }
    }
    
    /// 内部の値を取得
    pub fn value(&self) -> &str {
        match self {
            CompletionType::Command(s) => s,
            CompletionType::Flag(s) => s,
            CompletionType::Argument(s) => s,
            CompletionType::FilePath(s) => s,
            CompletionType::Variable(s) => s,
            CompletionType::Syntax(s) => s,
        }
    }
}

/// 補完候補を表す構造体
#[derive(Debug, Clone)]
pub struct CompletionSuggestion {
    /// 補完テキスト
    pub text: String,
    /// 補完の種類
    pub completion_type: CompletionType,
    /// 補完の説明（オプション）
    pub description: Option<String>,
    /// 優先度（低いほど高優先）
    pub priority: u8,
}

impl CompletionSuggestion {
    /// 新しい補完候補を作成
    pub fn new(
        text: String,
        completion_type: CompletionType,
        description: Option<String>,
        priority: u8,
    ) -> Self {
        Self {
            text,
            completion_type,
            description,
            priority,
        }
    }
    
    /// 補完候補がコマンド型かどうかをチェック
    pub fn is_command(&self) -> bool {
        matches!(self.completion_type, CompletionType::Command(_))
    }
    
    /// 補完候補がフラグ型かどうかをチェック
    pub fn is_flag(&self) -> bool {
        matches!(self.completion_type, CompletionType::Flag(_))
    }
    
    /// 補完候補がファイルパス型かどうかをチェック
    pub fn is_file_path(&self) -> bool {
        matches!(self.completion_type, CompletionType::FilePath(_))
    }
    
    /// 補完候補が変数型かどうかをチェック
    pub fn is_variable(&self) -> bool {
        matches!(self.completion_type, CompletionType::Variable(_))
    }
    
    /// 補完候補の表示文字列を取得
    pub fn display_text(&self) -> String {
        match &self.description {
            Some(desc) => format!("{} - {}", self.text, desc),
            None => self.text.clone(),
        }
    }
}

/// NexusLexerは、文字列からトークンを生成するためのレキサーです。
/// 行と列の追跡機能を提供し、エラーハンドリングも行います。
#[derive(Debug)]
pub struct NexusLexer {
    /// ソースコード
    source: Rc<String>,
    /// 行の開始位置のマップ
    line_starts: Vec<usize>,
    /// 現在の位置
    position: usize,
    /// 現在処理中のトークン
    current_token: Option<TokenWithSpan>,
    /// スキャン済みのトークンリスト
    tokens: Vec<TokenWithSpan>,
    /// エラーリスト
    errors: Vec<ParserError>,
}

impl NexusLexer {
    /// 新しいNexusLexerを作成
    pub fn new(source: &str) -> Self {
        let source = Rc::new(source.to_string());
        let mut lexer = Self {
            source: source.clone(),
            line_starts: vec![0],
            position: 0,
            current_token: None,
            tokens: Vec::new(),
            errors: Vec::new(),
        };
        lexer.scan_line_starts();
        lexer
    }

    /// 行の開始位置を全てスキャン
    fn scan_line_starts(&mut self) {
        let chars: Vec<char> = self.source.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\n' {
                self.line_starts.push(i + 1);
            }
            i += 1;
        }
    }

    /// 位置から行と列を計算
    pub fn get_line_column(&self, position: usize) -> (usize, usize) {
        let mut line = 0;
        while line + 1 < self.line_starts.len() && self.line_starts[line + 1] <= position {
            line += 1;
        }
        let column = position - self.line_starts[line] + 1;
        (line + 1, column)
    }

    /// ソースコードからトークンを生成
    pub fn tokenize(&mut self) -> Result<Vec<TokenWithSpan>, Vec<ParserError>> {
        let mut lexer = NexusToken::lexer(&self.source);
        
        while let Some(token_result) = lexer.next() {
            match token_result {
                Ok(token) => {
                    let span = lexer.span();
                    let (line, column) = self.get_line_column(span.start);
                    
                    // トークンをリストに追加
                    let token_with_span = TokenWithSpan::new(token, span, line, column);
                    self.tokens.push(token_with_span);
                }
                Err(_) => {
                    let span = lexer.span();
                    let (line, column) = self.get_line_column(span.start);
                    let error_span = span.clone();
                    let error_text = &self.source[span.clone()];
                    
                    // エラーを作成して記録
                    let error = ParserError::LexerError {
                        message: format!("不正なトークン: '{}'", error_text),
                        span: error_span,
                        severity: ErrorSeverity::Error,
                    };
                    self.errors.push(error);
                    
                    // エラーでも、スキップしないで不正なトークンとして追加
                    let token_with_span = TokenWithSpan::new(NexusToken::Error, span, line, column);
                    self.tokens.push(token_with_span);
                }
            }
        }
        
        // EOF トークンを追加
        let end_pos = self.source.len();
        let (line, column) = self.get_line_column(end_pos);
        let eof_token = TokenWithSpan::new(
            NexusToken::EOF,
            end_pos..end_pos,
            line,
            column,
        );
        self.tokens.push(eof_token);
        
        if self.errors.is_empty() {
            Ok(self.tokens.clone())
        } else {
            Err(self.errors.clone())
        }
    }

    /// トークン値を文字列として取得
    pub fn get_token_value(&self, token: &TokenWithSpan) -> String {
        self.source[token.span.clone()].to_string()
    }

    /// 特定の範囲のソースコードを取得
    pub fn get_source_slice(&self, range: Range<usize>) -> &str {
        &self.source[range]
    }

    /// トークン間のホワイトスペースを取得
    pub fn get_whitespace_between(&self, prev_token: &TokenWithSpan, next_token: &TokenWithSpan) -> &str {
        &self.source[prev_token.span.end..next_token.span.start]
    }

    /// エラーリストを取得
    pub fn get_errors(&self) -> &[ParserError] {
        &self.errors
    }

    /// エラーを追加
    pub fn add_error(&mut self, error: ParserError) {
        self.errors.push(error);
    }

    /// 指定した位置のトークンを取得
    pub fn get_token_at_position(&self, position: usize) -> Option<&TokenWithSpan> {
        self.tokens.iter().find(|token| {
            token.span.start <= position && position < token.span.end
        })
    }

    /// 指定した行と列のトークンを取得
    pub fn get_token_at_line_column(&self, line: usize, column: usize) -> Option<&TokenWithSpan> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        
        let line_start = self.line_starts[line - 1];
        let position = line_start + column - 1;
        
        self.get_token_at_position(position)
    }

    /// 特定のタイプのトークンをすべて取得
    pub fn get_tokens_by_type(&self, token_type: NexusToken) -> Vec<&TokenWithSpan> {
        self.tokens.iter()
            .filter(|token| std::mem::discriminant(&token.kind) == std::mem::discriminant(&token_type))
            .collect()
    }

    /// 高度なトークン解析
    pub fn analyze_tokens(&self) -> HashMap<String, usize> {
        let mut analysis = HashMap::new();
        
        // トークンタイプごとの出現回数
        for token in &self.tokens {
            let key = match &token.kind {
                NexusToken::Identifier(_) => "identifier".to_string(),
                NexusToken::LongFlag(_) => "long_flag".to_string(),
                NexusToken::ShortFlag(_) => "short_flag".to_string(),
                NexusToken::String(_) => "string".to_string(),
                NexusToken::SingleQuotedString(_) => "single_quoted_string".to_string(),
                NexusToken::Variable(_) => "variable".to_string(),
                NexusToken::VariableExpression(_) => "variable_expression".to_string(),
                NexusToken::Integer(_) => "integer".to_string(),
                NexusToken::Float(_) => "float".to_string(),
                NexusToken::Boolean(_) => "boolean".to_string(),
                _ => format!("{:?}", token.kind),
            };
            
            *analysis.entry(key).or_insert(0) += 1;
        }
        
        // 行数のカウント
        analysis.insert("line_count".to_string(), self.line_starts.len());
        
        // エラー数のカウント
        analysis.insert("error_count".to_string(), self.errors.len());
        
        analysis
    }

    /// 詳細なトークン解析を実行
    pub fn perform_detailed_analysis(&self) -> TokenAnalysisResult {
        let mut result = TokenAnalysisResult {
            tokens: self.tokens.clone(),
            errors: self.errors.clone(),
            statistics: self.analyze_tokens(),
            metadata: HashMap::new(),
        };
        
        // コマンド構造の分析
        let mut command_count = 0;
        let mut current_command = Vec::new();
        let mut commands = Vec::new();
        
        for token in &self.tokens {
            match token.kind {
                NexusToken::Pipe => {
                    if !current_command.is_empty() {
                        commands.push(current_command.clone());
                        current_command.clear();
                        command_count += 1;
                    }
                },
                NexusToken::Semicolon => {
                    if !current_command.is_empty() {
                        commands.push(current_command.clone());
                        current_command.clear();
                        command_count += 1;
                    }
                },
                NexusToken::EOF => {
                    if !current_command.is_empty() {
                        commands.push(current_command.clone());
                        command_count += 1;
                    }
                },
                _ if !matches!(token.kind, NexusToken::Whitespace | NexusToken::Comment(_)) => {
                    current_command.push(token.clone());
                },
                _ => {}
            }
        }
        
        result.statistics.insert("command_count".to_string(), command_count);
        
        // 変数使用分析
        let variables = self.get_tokens_by_type(NexusToken::Variable("".to_string()));
        let var_expressions = self.get_tokens_by_type(NexusToken::VariableExpression("".to_string()));
        
        result.statistics.insert("variable_usage_count".to_string(), variables.len() + var_expressions.len());
        
        // リダイレクト分析
        let redirects = self.tokens.iter()
            .filter(|t| matches!(t.kind, 
                NexusToken::RedirectIn | 
                NexusToken::RedirectOut | 
                NexusToken::RedirectAppend | 
                NexusToken::RedirectFd))
            .count();
        
        result.statistics.insert("redirect_count".to_string(), redirects);
        
        // メタデータ生成
        if !self.tokens.is_empty() {
            let first_token = &self.tokens[0];
            let last_token = &self.tokens[self.tokens.len() - 1];
            
            result.metadata.insert("first_token_line".to_string(), first_token.line.to_string());
            result.metadata.insert("last_token_line".to_string(), last_token.line.to_string());
            
            if command_count > 0 {
                result.metadata.insert("avg_tokens_per_command".to_string(), 
                    format!("{:.2}", self.tokens.len() as f64 / command_count as f64));
            }
        }
        
        // 複雑性分析
        let complexity_score = self.calculate_complexity_score();
        result.metadata.insert("complexity_score".to_string(), format!("{:.2}", complexity_score));
        
        result
    }
    
    /// コードの複雑性スコアを計算
    fn calculate_complexity_score(&self) -> f64 {
        let mut score = 0.0;
        
        // 変数・変数式の使用はスコアを上げる
        let var_count = self.tokens.iter()
            .filter(|t| matches!(t.kind, NexusToken::Variable(_) | NexusToken::VariableExpression(_)))
            .count();
        score += var_count as f64 * 0.5;
        
        // リダイレクトはスコアを上げる
        let redirect_count = self.tokens.iter()
            .filter(|t| matches!(t.kind, 
                NexusToken::RedirectIn | 
                NexusToken::RedirectOut | 
                NexusToken::RedirectAppend | 
                NexusToken::RedirectFd))
            .count();
        score += redirect_count as f64 * 0.7;
        
        // パイプもスコアを上げる
        let pipe_count = self.tokens.iter()
            .filter(|t| matches!(t.kind, NexusToken::Pipe))
            .count();
        score += pipe_count as f64 * 1.0;
        
        // 論理演算子はスコアをさらに上げる
        let logic_op_count = self.tokens.iter()
            .filter(|t| matches!(t.kind, NexusToken::And | NexusToken::Or))
            .count();
        score += logic_op_count as f64 * 1.5;
        
        // エラーがあればスコアは悪くなる
        score -= self.errors.len() as f64 * 2.0;
        
        score.max(0.0)
    }
    
    /// トークンリストから特定範囲のトークンを抽出
    pub fn extract_tokens(&self, start_idx: usize, end_idx: usize) -> Vec<TokenWithSpan> {
        if start_idx >= self.tokens.len() || end_idx > self.tokens.len() || start_idx > end_idx {
            return Vec::new();
        }
        
        self.tokens[start_idx..end_idx].to_vec()
    }
    
    /// 2つのトークン間のすべてのトークンを取得
    pub fn get_tokens_between(&self, start_token: &TokenWithSpan, end_token: &TokenWithSpan) -> Vec<TokenWithSpan> {
        let start_idx = self.tokens.iter().position(|t| t.span.start == start_token.span.start);
        let end_idx = self.tokens.iter().position(|t| t.span.start == end_token.span.start);
        
        if let (Some(start), Some(end)) = (start_idx, end_idx) {
            if start < end {
                return self.extract_tokens(start + 1, end);
            }
        }
        
        Vec::new()
    }
    
    /// トークンリストを文字列にフォーマットして出力
    pub fn format_tokens(&self, tokens: &[TokenWithSpan]) -> String {
        let mut result = String::new();
        
        for token in tokens {
            let token_text = self.get_token_value(token);
            result.push_str(&token_text);
            
            // 特定のトークン後にスペースを追加してフォーマットを整える
            match token.kind {
                NexusToken::Identifier(_) => {
                    // 次のトークンがフラグかリダイレクトの場合はスペースを追加
                    if let Some(next_idx) = tokens.iter().position(|t| t.span.start == token.span.start).map(|idx| idx + 1) {
                        if next_idx < tokens.len() {
                            let next = &tokens[next_idx];
                            if !matches!(next.kind, 
                                NexusToken::Equal | 
                                NexusToken::LeftParen | 
                                NexusToken::RightParen | 
                                NexusToken::EOF) {
                                result.push(' ');
                            }
                        }
                    }
                },
                NexusToken::Equal | NexusToken::LongFlag(_) | NexusToken::ShortFlag(_) => {
                    // イコールや各種フラグの後にはスペースを入れる
                    result.push(' ');
                },
                _ => {}
            }
        }
        
        result
    }
    
    /// コマンドラインをリライトする
    pub fn rewrite_command_line(&self) -> String {
        // コメント、余分な空白を除去して整形したコマンドラインを生成
        let filtered_tokens: Vec<_> = self.tokens.iter()
            .filter(|t| !matches!(t.kind, NexusToken::Comment(_) | NexusToken::Whitespace | NexusToken::EOF))
            .collect();
            
        self.format_tokens(&filtered_tokens)
    }

    /// トークンの文脈に基づいた詳細なエラー診断を行う
    pub fn perform_enhanced_error_checking(&self, tokens: &[TokenWithSpan]) -> Vec<ParserError> {
        let mut errors = Vec::new();
        let mut paren_stack = Vec::new();
        let mut bracket_stack = Vec::new();
        let mut brace_stack = Vec::new();
        let mut quote_stack = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            match token.kind {
                NexusToken::LeftParen => paren_stack.push((idx, token.span)),
                NexusToken::RightParen => {
                    if paren_stack.is_empty() {
                        errors.push(ParserError::UnmatchedClosingDelimiter {
                            span: token.span,
                            delimiter: ")".to_string(),
                        });
                    } else {
                        paren_stack.pop();
                    }
                }
                NexusToken::LeftBracket => bracket_stack.push((idx, token.span)),
                NexusToken::RightBracket => {
                    if bracket_stack.is_empty() {
                        errors.push(ParserError::UnmatchedClosingDelimiter {
                            span: token.span,
                            delimiter: "]".to_string(),
                        });
                    } else {
                        bracket_stack.pop();
                    }
                }
                NexusToken::LeftBrace => brace_stack.push((idx, token.span)),
                NexusToken::RightBrace => {
                    if brace_stack.is_empty() {
                        errors.push(ParserError::UnmatchedClosingDelimiter {
                            span: token.span,
                            delimiter: "}".to_string(),
                        });
                    } else {
                        brace_stack.pop();
                    }
                }
                NexusToken::QuoteSingle | NexusToken::QuoteDouble => {
                    if quote_stack.last().map_or(false, |&(_, kind)| kind == token.kind) {
                        quote_stack.pop();
                    } else {
                        quote_stack.push((idx, token.kind));
                    }
                }
                NexusToken::Variable => {
                    // 変数式が空かチェック
                    let value = self.get_token_value(token);
                    if value == "$" || value == "${}" {
                        errors.push(ParserError::EmptyVariableExpression {
                            span: token.span,
                        });
                    }
                }
                NexusToken::Pipe | NexusToken::PipeErr | NexusToken::PipeAll => {
                    // パイプの後に有効なトークンがあることを確認
                    if idx == tokens.len() - 1 || matches!(tokens[idx + 1].kind, NexusToken::Eof | NexusToken::Semicolon) {
                        errors.push(ParserError::InvalidPipeUsage {
                            span: token.span,
                            message: "パイプの後に有効なコマンドが必要です".to_string(),
                        });
                    }
                }
                NexusToken::RedirectIn | NexusToken::RedirectOut | NexusToken::RedirectAppend | NexusToken::RedirectErr => {
                    // リダイレクトの後に有効なトークンがあることを確認
                    if idx == tokens.len() - 1 || 
                       !matches!(tokens[idx + 1].kind, NexusToken::String | NexusToken::Identifier | NexusToken::Variable) {
                        errors.push(ParserError::InvalidRedirection {
                            span: token.span,
                            message: "リダイレクト操作の後にファイル名が必要です".to_string(),
                        });
                    }
                }
                _ => {}
            }

            // コマンド構文の検証
            if idx > 0 && matches!(token.kind, NexusToken::LongFlag | NexusToken::ShortFlag) {
                let prev = &tokens[idx - 1];
                if matches!(prev.kind, NexusToken::LongFlag | NexusToken::ShortFlag) {
                    // フラグに引数が必要かどうかの検証ロジックをここに追加
                    // 例: --fileがファイル名引数を必要とするフラグの場合
                }
            }
        }

        // 閉じられていない括弧をエラーとして追加
        for (_, span) in paren_stack {
            errors.push(ParserError::UnmatchedOpeningDelimiter {
                span,
                delimiter: "(".to_string(),
            });
        }
        
        for (_, span) in bracket_stack {
            errors.push(ParserError::UnmatchedOpeningDelimiter {
                span,
                delimiter: "[".to_string(),
            });
        }
        
        for (_, span) in brace_stack {
            errors.push(ParserError::UnmatchedOpeningDelimiter {
                span,
                delimiter: "{".to_string(),
            });
        }
        
        for (_, kind) in quote_stack {
            let quote = match kind {
                NexusToken::QuoteSingle => "'",
                NexusToken::QuoteDouble => "\"",
                _ => unreachable!(),
            };
            errors.push(ParserError::UnmatchedQuote {
                span: Span::new(0, 0),  // この場合はソース全体を指すスパンを設定する必要がある
                quote: quote.to_string(),
            });
        }

        // 重複エラーを除去
        errors.sort_by_key(|e| e.span().start);
        errors.dedup_by(|a, b| a.span() == b.span());
        
        errors
    }

    /// トークンの文脈分析を行い、コマンドとそれに関連するトークンのマッピングを作成
    pub fn analyze_token_context(&self, tokens: &[TokenWithSpan]) -> HashMap<usize, CommandContext> {
        let mut context_map = HashMap::new();
        let mut current_command_idx = None;
        
        for (idx, token) in tokens.iter().enumerate() {
            match token.kind {
                NexusToken::Identifier => {
                    // 新しいコマンドの開始を検出
                    if current_command_idx.is_none() || 
                       matches!(tokens[idx-1].kind, NexusToken::Semicolon | NexusToken::Pipe | NexusToken::PipeErr | NexusToken::PipeAll) {
                        current_command_idx = Some(idx);
                        context_map.insert(idx, CommandContext {
                            command_type: self.determine_command_type(token),
                            args_count: 0,
                            flags_count: 0,
                            redirections: Vec::new(),
                            related_tokens: vec![idx],
                        });
                    } else if let Some(cmd_idx) = current_command_idx {
                        // 既存のコマンドに引数として追加
                        if let Some(context) = context_map.get_mut(&cmd_idx) {
                            context.args_count += 1;
                            context.related_tokens.push(idx);
                        }
                    }
                }
                NexusToken::LongFlag | NexusToken::ShortFlag => {
                    if let Some(cmd_idx) = current_command_idx {
                        if let Some(context) = context_map.get_mut(&cmd_idx) {
                            context.flags_count += 1;
                            context.related_tokens.push(idx);
                        }
                    }
                }
                NexusToken::RedirectIn | NexusToken::RedirectOut | NexusToken::RedirectAppend | NexusToken::RedirectErr => {
                    if let Some(cmd_idx) = current_command_idx {
                        if let Some(context) = context_map.get_mut(&cmd_idx) {
                            context.redirections.push((token.kind, idx));
                            context.related_tokens.push(idx);
                            
                            // リダイレクト先もコマンドコンテキストに関連付ける
                            if idx + 1 < tokens.len() {
                                context.related_tokens.push(idx + 1);
                            }
                        }
                    }
                }
                NexusToken::Pipe | NexusToken::PipeErr | NexusToken::PipeAll => {
                    // パイプはコマンド区切りとなるため、次のトークンを新しいコマンドとして扱う準備
                    current_command_idx = None;
                }
                NexusToken::Semicolon => {
                    // セミコロンは完全にコマンドを区切る
                    current_command_idx = None;
                }
                _ => {
                    // その他のトークンは現在のコマンドに関連付ける
                    if let Some(cmd_idx) = current_command_idx {
                        if let Some(context) = context_map.get_mut(&cmd_idx) {
                            context.related_tokens.push(idx);
                        }
                    }
                }
            }
        }
        
        context_map
    }

    /// コマンドの種類を判断する
    fn determine_command_type(&self, token: &TokenWithSpan) -> CommandType {
        let cmd_name = self.get_token_value(token);
        match cmd_name.as_str() {
            "cd" | "pushd" | "popd" => CommandType::Navigation,
            "ls" | "dir" | "find" | "grep" => CommandType::FileSystem,
            "cat" | "less" | "more" | "head" | "tail" => CommandType::FileContents,
            "vim" | "nano" | "emacs" | "vi" => CommandType::Editor,
            "git" | "svn" | "hg" => CommandType::VersionControl,
            "ssh" | "scp" | "rsync" | "curl" | "wget" => CommandType::Network,
            "ps" | "top" | "htop" | "kill" => CommandType::Process,
            "chmod" | "chown" | "sudo" | "su" => CommandType::Permission,
            "apt" | "yum" | "pacman" | "brew" => CommandType::PackageManager,
            "docker" | "podman" | "kubectl" => CommandType::Container,
            "echo" | "printf" => CommandType::Output,
            "export" | "set" | "env" | "alias" => CommandType::ShellBuiltin,
            _ => CommandType::Unknown,
        }
    }

    /// 入力コンテキストに基づいて補完候補を生成する
    pub fn generate_context_aware_completions(
        &self,
        input: &str,
        cursor_position: usize,
        available_commands: &[String],
        env_variables: &HashMap<String, String>,
    ) -> Vec<CompletionSuggestion> {
        // 入力コンテキストを分析
        let (context, current_word) = self.analyze_input_context(input, cursor_position);
        
        // 現在のトークンの種類を判断
        let token_type = self.determine_token_type(&context, &current_word);
        
        // トークンの種類に基づいて補完候補を生成
        let mut suggestions = match token_type {
            CompletionType::Command(_) => self.generate_command_completions(&current_word, available_commands),
            CompletionType::Flag(_) => self.generate_flag_completions(&context, &current_word),
            CompletionType::Argument(_) => self.generate_argument_completions(&context, &current_word),
            CompletionType::FilePath(_) => self.generate_file_completions(&current_word),
            CompletionType::Variable(_) => self.generate_variable_completions(&current_word, env_variables),
            CompletionType::Syntax(_) => self.generate_syntax_completions(&current_word),
        };
        
        // 自動修正候補を追加（コマンドの場合のみ）
        if let CompletionType::Command(_) = token_type {
            let corrections = self.generate_auto_corrections(&current_word, available_commands);
            suggestions.extend(corrections);
        }
        
        // 関連コマンド候補を追加（コマンドの場合のみ）
        if let CompletionType::Command(cmd) = token_type {
            if !cmd.is_empty() {
                let related = self.generate_related_command_suggestions(&cmd);
                suggestions.extend(related);
            }
        }
        
        // 優先度でソート
        suggestions.sort_by(|a, b| a.priority.cmp(&b.priority));
        
        suggestions
    }
    
    /// 入力を分析してコンテキストと現在の単語を抽出
    fn analyze_input_context(&self, input: &str, cursor_position: usize) -> (String, String) {
        let safe_pos = std::cmp::min(cursor_position, input.len());
        let input_before_cursor = &input[..safe_pos];
        
        // 現在の単語を特定（カーソル位置から前方に単語境界まで）
        let mut word_start = safe_pos;
        for (i, c) in input_before_cursor.char_indices().rev() {
            if c.is_whitespace() || c == '|' || c == '<' || c == '>' || c == ';' {
                word_start = i + 1;
                break;
            }
            word_start = i;
        }
        
        let current_word = input_before_cursor[word_start..].to_string();
        let context = input_before_cursor[..word_start].trim().to_string();
        
        (context, current_word)
    }
    
    /// コンテキストと現在の単語に基づいてトークンの種類を判断
    fn determine_token_type(&self, context: &str, current_word: &str) -> CompletionType {
        // 空のコンテキストまたはパイプ直後の場合はコマンド
        if context.is_empty() || context.ends_with('|') || context.ends_with(';') {
            return CompletionType::Command(current_word.to_string());
        }
        
        // 変数の場合
        if current_word.starts_with('$') {
            return CompletionType::Variable(current_word[1..].to_string());
        }
        
        // フラグの場合
        if current_word.starts_with('-') {
            return CompletionType::Flag(current_word.to_string());
        }
        
        // ファイルパスの場合
        if current_word.contains('/') || context.ends_with("cd ") || context.ends_with("cp ") || 
           context.ends_with("mv ") || context.ends_with("rm ") || context.ends_with("cat ") {
            return CompletionType::FilePath(current_word.to_string());
        }
        
        // コンテキストに基づく判断
        let context_tokens: Vec<&str> = context.split_whitespace().collect();
        if !context_tokens.is_empty() {
            // リダイレクトの後はファイルパス
            if ["<", ">", ">>", "2>"].contains(&context_tokens.last().unwrap()) {
                return CompletionType::FilePath(current_word.to_string());
            }
            
            // 引数と判断
            return CompletionType::Argument(current_word.to_string());
        }
        
        // デフォルトではコマンドと判断
        CompletionType::Command(current_word.to_string())
    }
    
    /// コマンド補完候補を生成
    fn generate_command_completions(
        &self,
        current_word: &str,
        available_commands: &[String],
    ) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 利用可能なコマンドから補完候補を生成
        for cmd in available_commands {
            if cmd.starts_with(current_word) {
                suggestions.push(CompletionSuggestion::new(
                    cmd.clone(),
                    CompletionType::Command(cmd.clone()),
                    None,
                    1,
                ));
            }
        }
        
        // 一般的なコマンド（実際の実装ではより広範なコマンドリストを使用）
        let common_commands = [
            ("ls", "ディレクトリ内容を一覧表示"),
            ("cd", "ディレクトリを変更"),
            ("grep", "テキストの検索"),
            ("cat", "ファイル内容を表示"),
            ("echo", "テキストを出力"),
            ("mkdir", "ディレクトリを作成"),
            ("rm", "ファイルまたはディレクトリを削除"),
            ("cp", "ファイルまたはディレクトリをコピー"),
            ("mv", "ファイルまたはディレクトリを移動"),
            ("chmod", "ファイル権限を変更"),
            ("find", "ファイル検索"),
            ("ps", "プロセス一覧"),
            ("kill", "プロセス終了"),
            ("history", "コマンド履歴を表示"),
        ];
        
        for (cmd, desc) in common_commands.iter() {
            if cmd.starts_with(current_word) && !suggestions.iter().any(|s| s.text == *cmd) {
                suggestions.push(CompletionSuggestion::new(
                    cmd.to_string(),
                    CompletionType::Command(cmd.to_string()),
                    Some(desc.to_string()),
                    2,
                ));
            }
        }
        
        suggestions
    }
    
    /// フラグ補完候補を生成
    fn generate_flag_completions(&self, context: &str, current_flag: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // コンテキストからコマンドを特定
        let command = context.split_whitespace().next().unwrap_or("");
        
        // コマンドに基づいてフラグを生成
        let flags = match command {
            "ls" => vec![
                ("-l", "詳細なリスト形式で表示"),
                ("-a", "全てのファイル（隠しファイルを含む）を表示"),
                ("-h", "人間が読みやすい形式でサイズを表示"),
                ("--color", "カラー出力を有効にする"),
            ],
            "grep" => vec![
                ("-i", "大文字と小文字を区別しない"),
                ("-r", "ディレクトリを再帰的に検索"),
                ("-v", "一致しない行を表示"),
                ("-n", "行番号を表示"),
            ],
            "find" => vec![
                ("-name", "ファイル名で検索"),
                ("-type", "ファイルタイプで検索"),
                ("-size", "サイズで検索"),
                ("-exec", "一致したファイルに対してコマンドを実行"),
            ],
            _ => vec![
                ("--help", "ヘルプを表示"),
                ("--version", "バージョン情報を表示"),
                ("-v", "詳細情報を表示"),
                ("-f", "強制的に実行"),
            ],
        };
        
        for (flag, desc) in flags {
            if flag.starts_with(current_flag) {
                suggestions.push(CompletionSuggestion::new(
                    flag.to_string(),
                    CompletionType::Flag(flag.to_string()),
                    Some(desc.to_string()),
                    1,
                ));
            }
        }
        
        suggestions
    }
    
    /// 引数補完候補を生成
    fn generate_argument_completions(&self, context: &str, current_arg: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // コンテキストからコマンドを特定
        let command = context.split_whitespace().next().unwrap_or("");
        
        // ディレクトリ補完（cd向け）
        if command == "cd" {
            let dirs = [
                "/home",
                "/usr",
                "/etc",
                "/var",
                "/tmp",
                ".",
                "..",
                "~/Documents",
                "~/Downloads",
            ];
            
            for dir in dirs.iter() {
                if dir.starts_with(current_arg) {
                    suggestions.push(CompletionSuggestion::new(
                        dir.to_string(),
                        CompletionType::FilePath(dir.to_string()),
                        None,
                        1,
                    ));
                }
            }
        }
        
        // gitサブコマンド補完
        if command == "git" {
            let git_commands = [
                ("add", "変更をステージングする"),
                ("commit", "変更をコミットする"),
                ("push", "リモートリポジトリに変更をプッシュする"),
                ("pull", "リモートリポジトリから変更をプルする"),
                ("status", "ワーキングツリーの状態を表示"),
                ("log", "コミットログを表示"),
                ("branch", "ブランチを一覧表示または作成"),
                ("checkout", "ブランチを切り替える"),
                ("merge", "ブランチをマージする"),
                ("clone", "リポジトリをクローンする"),
            ];
            
            for (cmd, desc) in git_commands.iter() {
                if cmd.starts_with(current_arg) {
                    suggestions.push(CompletionSuggestion::new(
                        cmd.to_string(),
                        CompletionType::Argument(cmd.to_string()),
                        Some(desc.to_string()),
                        1,
                    ));
                }
            }
        }
        
        suggestions
    }
    
    /// ファイルパス補完候補を生成
    fn generate_file_completions(&self, current_path: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 実際の実装では、ファイルシステムにアクセスして実際のファイルを取得
        // ここではデモとして一般的なパスのみ表示
        let common_paths = [
            "/home/user",
            "/etc/passwd",
            "/usr/bin",
            "/var/log",
            "./README.md",
            "../project",
            "~/Documents",
            "~/Downloads",
            "/tmp",
        ];
        
        for path in common_paths.iter() {
            if path.starts_with(current_path) {
                suggestions.push(CompletionSuggestion::new(
                    path.to_string(),
                    CompletionType::FilePath(path.to_string()),
                    None,
                    1,
                ));
            }
        }
        
        suggestions
    }
    
    /// 変数補完候補を生成
    fn generate_variable_completions(
        &self,
        current_var: &str,
        env_variables: &HashMap<String, String>,
    ) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 環境変数から補完候補を生成
        for (var, value) in env_variables {
            if var.starts_with(current_var) {
                suggestions.push(CompletionSuggestion::new(
                    format!("${}", var),
                    CompletionType::Variable(var.clone()),
                    Some(format!("値: {}", value)),
                    1,
                ));
            }
        }
        
        // 一般的な環境変数
        let common_vars = [
            "HOME", "PATH", "USER", "SHELL", "PWD", "TERM", "LANG", "EDITOR"
        ];
        
        for var in common_vars.iter() {
            if var.starts_with(current_var) && !suggestions.iter().any(|s| s.text == format!("${}", var)) {
                suggestions.push(CompletionSuggestion::new(
                    format!("${}", var),
                    CompletionType::Variable(var.to_string()),
                    Some("環境変数".to_string()),
                    2,
                ));
            }
        }
        
        suggestions
    }
    
    /// 構文要素の補完候補を生成
    fn generate_syntax_completions(&self, current_syntax: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        let syntax_elements = [
            ("|", "パイプ - コマンドの出力を次のコマンドの入力に渡す"),
            (">", "出力リダイレクト - 標準出力をファイルに書き込む"),
            (">>", "出力追加リダイレクト - 標準出力をファイルに追加"),
            ("<", "入力リダイレクト - ファイルから標準入力を読み込む"),
            ("2>", "エラー出力リダイレクト - 標準エラー出力をファイルに書き込む"),
            ("&&", "AND演算子 - 前のコマンドが成功した場合のみ次のコマンドを実行"),
            ("||", "OR演算子 - 前のコマンドが失敗した場合のみ次のコマンドを実行"),
            (";", "コマンド区切り - 複数のコマンドを順番に実行"),
        ];
        
        for (syntax, desc) in syntax_elements.iter() {
            if syntax.starts_with(current_syntax) {
                suggestions.push(CompletionSuggestion::new(
                    syntax.to_string(),
                    CompletionType::Syntax(syntax.to_string()),
                    Some(desc.to_string()),
                    1,
                ));
            }
        }
        
        suggestions
    }
    
    /// 自動修正候補を生成（タイプミスなどの修正）
    fn generate_auto_corrections(
        &self,
        mistyped_word: &str,
        available_commands: &[String],
    ) -> Vec<CompletionSuggestion> {
        let mut corrections = Vec::new();
        
        // タイプミスの可能性がある場合のみ処理（短すぎる単語は処理しない）
        if mistyped_word.len() >= 2 {
            for cmd in available_commands {
                let distance = self.calculate_levenshtein_distance(mistyped_word, cmd);
                
                // 距離が2以下で、元の単語との長さの差が3以内の場合に修正候補とする
                if distance <= 2 && (cmd.len() as i32 - mistyped_word.len() as i32).abs() <= 3 {
                    corrections.push(CompletionSuggestion::new(
                        cmd.clone(),
                        CompletionType::Command(cmd.clone()),
                        Some(format!("「{}」の修正候補", mistyped_word)),
                        // 距離によって優先度を変える（距離が小さいほど優先度が高い）
                        10 + distance as u8,
                    ));
                }
            }
        }
        
        corrections
    }
    
    /// 関連コマンド候補を生成
    fn generate_related_command_suggestions(&self, command: &str) -> Vec<CompletionSuggestion> {
        let mut related = Vec::new();
        
        // コマンドカテゴリのマッピング
        let command_categories: HashMap<&str, Vec<(&str, &str)>> = [
            ("file", vec![
                ("ls", "ファイル一覧"),
                ("cat", "ファイル表示"),
                ("rm", "ファイル削除"),
                ("cp", "ファイルコピー"),
                ("mv", "ファイル移動"),
                ("touch", "ファイル作成"),
                ("find", "ファイル検索"),
            ]),
            ("network", vec![
                ("ping", "ネットワーク疎通確認"),
                ("curl", "HTTPリクエスト"),
                ("wget", "ファイルダウンロード"),
                ("ssh", "リモート接続"),
                ("netstat", "ネットワーク統計"),
                ("ifconfig", "ネットワークインターフェース表示"),
                ("ip", "IPアドレス管理"),
            ]),
            ("process", vec![
                ("ps", "プロセス一覧"),
                ("top", "システムモニター"),
                ("kill", "プロセス終了"),
                ("pkill", "プロセス名での終了"),
                ("nice", "プロセス優先度設定"),
                ("pgrep", "プロセス検索"),
            ]),
        ].iter().cloned().collect();
        
        // 入力コマンドのカテゴリを特定
        let mut category_commands = Vec::new();
        for (category, commands) in &command_categories {
            for (cmd, _) in commands {
                if *cmd == command {
                    category_commands = commands.clone();
                    break;
                }
            }
            if !category_commands.is_empty() {
                break;
            }
        }
        
        // 同じカテゴリのコマンドを関連候補として追加
        for (cmd, desc) in category_commands {
            if cmd != command {
                related.push(CompletionSuggestion::new(
                    cmd.to_string(),
                    CompletionType::Command(cmd.to_string()),
                    Some(format!("関連: {}", desc)),
                    20, // 関連コマンドは通常の候補より優先度を下げる
                ));
            }
        }
        
        related
    }
    
    /// レーベンシュタイン距離を計算（文字列間の編集距離）
    fn calculate_levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let len1 = s1.chars().count();
        let len2 = s2.chars().count();
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();
        
        // 空の文字列の場合は相手の長さを返す
        if len1 == 0 { return len2; }
        if len2 == 0 { return len1; }
        
        // 動的計画法でレーベンシュタイン距離を計算
        let mut dp = vec![vec![0; len2 + 1]; len1 + 1];
        
        // 初期化
        for i in 0..=len1 {
            dp[i][0] = i;
        }
        for j in 0..=len2 {
            dp[0][j] = j;
        }
        
        // 計算
        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
                dp[i][j] = std::cmp::min(
                    std::cmp::min(
                        dp[i - 1][j] + 1,      // 削除
                        dp[i][j - 1] + 1       // 挿入
                    ),
                    dp[i - 1][j - 1] + cost    // 置換
                );
            }
        }
        
        dp[len1][len2]
    }
}

/// シンタックスハイライトのタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxHighlightType {
    Command,    // コマンド名
    Identifier, // 識別子（コマンド以外）
    String,     // 文字列リテラル
    Number,     // 数値リテラル
    Boolean,    // 真偽値
    Variable,   // 変数
    Flag,       // コマンドラインフラグ
    Operator,   // 演算子（リダイレクト、パイプなど）
    Delimiter,  // 区切り記号（括弧など）
    Comment,    // コメント
    Default,    // その他
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_command() {
        let mut lexer = NexusLexer::new("");
        let input = "ls -la /home";
        
        let tokens = lexer.tokenize(input).unwrap();
        assert_eq!(tokens.len(), 4); // 3つのトークン + EOF
        
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme, "ls");
        
        assert_eq!(tokens[1].kind, TokenKind::Flag);
        assert_eq!(tokens[1].lexeme, "-la");
        
        assert_eq!(tokens[2].kind, TokenKind::Identifier);
        assert_eq!(tokens[2].lexeme, "/home");
        
        assert_eq!(tokens[3].kind, TokenKind::Eof);
    }

    #[test]
    fn test_tokenize_pipeline() {
        let mut lexer = NexusLexer::new("");
        let input = "cat file.txt | grep pattern | wc -l";
        
        let tokens = lexer.tokenize(input).unwrap();
        assert_eq!(tokens.len(), 8); // 7つのトークン + EOF
        
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme, "cat");
        
        assert_eq!(tokens[1].kind, TokenKind::Identifier);
        assert_eq!(tokens[1].lexeme, "file.txt");
        
        assert_eq!(tokens[2].kind, TokenKind::Pipe);
        
        assert_eq!(tokens[3].kind, TokenKind::Identifier);
        assert_eq!(tokens[3].lexeme, "grep");
        
        assert_eq!(tokens[4].kind, TokenKind::Identifier);
        assert_eq!(tokens[4].lexeme, "pattern");
        
        assert_eq!(tokens[5].kind, TokenKind::Pipe);
        
        assert_eq!(tokens[6].kind, TokenKind::Identifier);
        assert_eq!(tokens[6].lexeme, "wc");
        
        assert_eq!(tokens[7].kind, TokenKind::Flag);
        assert_eq!(tokens[7].lexeme, "-l");
    }

    #[test]
    fn test_tokenize_redirections() {
        let mut lexer = NexusLexer::new("");
        let input = "command > output.txt 2>&1";
        
        let tokens = lexer.tokenize(input).unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme, "command");
        
        assert_eq!(tokens[1].kind, TokenKind::RedirectOut);
        
        assert_eq!(tokens[2].kind, TokenKind::Identifier);
        assert_eq!(tokens[2].lexeme, "output.txt");
        
        // 2>&1 はさらに実装が必要
    }

    #[test]
    fn test_tokenize_strings() {
        let mut lexer = NexusLexer::new("");
        let input = r#"echo "Hello, world!" 'single quoted'"#;
        
        let tokens = lexer.tokenize(input).unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme, "echo");
        
        assert_eq!(tokens[1].kind, TokenKind::String);
        assert_eq!(tokens[1].lexeme, "Hello, world!");
        
        assert_eq!(tokens[2].kind, TokenKind::String);
        assert_eq!(tokens[2].lexeme, "single quoted");
    }

    #[test]
    fn test_tokenize_variables() {
        let mut lexer = NexusLexer::new("");
        let input = "echo $HOME ${USER}";
        
        let tokens = lexer.tokenize(input).unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme, "echo");
        
        assert_eq!(tokens[1].kind, TokenKind::Variable);
        assert_eq!(tokens[1].lexeme, "HOME");
        
        assert_eq!(tokens[2].kind, TokenKind::Variable);
        assert_eq!(tokens[2].lexeme, "USER");
    }

    #[test]
    fn test_tokenize_invalid_token() {
        let mut lexer = NexusLexer::new("");
        let input = "echo @invalid";
        
        let result = lexer.tokenize(input);
        assert!(result.is_err());
        
        if let Err(ParserError::LexerError(message, span)) = result {
            assert_eq!(span.start, 5);
            assert_eq!(span.column, 6);
        } else {
            panic!("Expected LexerError");
        }
    }

    #[test]
    fn test_calculate_position() {
        let lexer = NexusLexer::new("line1\nline2\nline3");
        
        let (line, column) = lexer.calculate_position(0);
        assert_eq!(line, 1);
        assert_eq!(column, 1);
        
        let (line, column) = lexer.calculate_position(6);
        assert_eq!(line, 2);
        assert_eq!(column, 1);
        
        let (line, column) = lexer.calculate_position(8);
        assert_eq!(line, 2);
        assert_eq!(column, 3);
    }

    #[test]
    fn test_get_line() {
        let lexer = NexusLexer::new("line1\nline2\nline3");
        
        assert_eq!(lexer.get_line(1), Some("line1\n"));
        assert_eq!(lexer.get_line(2), Some("line2\n"));
        assert_eq!(lexer.get_line(3), Some("line3"));
        assert_eq!(lexer.get_line(4), None);
    }

    #[test]
    fn test_unescape_string() {
        assert_eq!(unescape_string(r"Hello\nWorld"), "Hello\nWorld");
        assert_eq!(unescape_string(r"Escaped\"Quote"), "Escaped\"Quote");
        assert_eq!(unescape_string(r"Backslash\\"), "Backslash\\");
    }
} 