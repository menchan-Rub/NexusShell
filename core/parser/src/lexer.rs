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
use std::cell::OnceCell;
use dashmap::DashMap;
use std::cmp;

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

    #[regex(r"\$\{[^}]+\}", |lex| {        // VariableExpressionのリテラルパターンを修正
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
                    let span_range = lexer.span(); // これは Range<usize>
                    let (line, column) = self.get_line_column(span_range.start);
                    let error_text = &self.source[span_range.clone()];
                    
                    // エラーを作成して記録 (Range<usize> から Span 構造体へ変換)
                    let error_span_struct = Span { // crate::Span を使用
                        start: span_range.start,
                        end: span_range.end,
                        line,
                        column,
                    };
                    let error = ParserError::LexerError(
                        format!("不正なトークン: '{}'", error_text),
                        error_span_struct // Span 構造体を渡す
                    );
                    self.errors.push(error);
                    
                    // エラーでも、スキップしないで不正なトークンとして追加
                    // TokenWithSpan の span も Range<usize> のまま
                    let token_with_span = TokenWithSpan::new(NexusToken::Error, span_range, line, column);
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
                _ if !matches!(token.kind, NexusToken::Whitespace | NexusToken::Comment) => { // Comment(_) を Comment に修正
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
                NexusToken::LessThan | // RedirectIn -> LessThan
                NexusToken::GreaterThan | // RedirectOut -> GreaterThan
                NexusToken::Append // RedirectAppend -> Append
                // NexusToken::RedirectFd // 未定義のためコメントアウト
            ))
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
                NexusToken::LessThan | // RedirectIn -> LessThan
                NexusToken::GreaterThan | // RedirectOut -> GreaterThan
                NexusToken::Append // RedirectAppend -> Append
                // NexusToken::RedirectFd // 未定義のためコメントアウト
            ))
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
            .filter(|t| !matches!(t.kind, NexusToken::Comment | NexusToken::Whitespace | NexusToken::EOF))
            .collect();
            
        self.format_tokens(&filtered_tokens)
    }

    /// トークンの文脈に基づいた詳細なエラー診断を行う
    pub fn perform_enhanced_error_checking(&self, tokens: &[TokenWithSpan]) -> Vec<ParserError> {
        let mut errors = Vec::new();
        let mut paren_stack = Vec::new();
        let mut bracket_stack = Vec::new();
        let mut brace_stack = Vec::new();
        // quote_stack は String と SingleQuotedString で代用を検討、または専用トークン追加が必要
        // let mut quote_stack = Vec::new(); 

        for (idx, token) in tokens.iter().enumerate() {
            match token.kind {
                NexusToken::LeftParen => paren_stack.push((idx, token.span.clone())),
                NexusToken::RightParen => {
                    if paren_stack.is_empty() {
                        errors.push(ParserError::UnmatchedClosingDelimiter {
                            span: token.span.clone(),
                            delimiter: ")".to_string(),
                        });
                    } else {
                        paren_stack.pop();
                    }
                }
                NexusToken::LeftBracket => bracket_stack.push((idx, token.span.clone())),
                NexusToken::RightBracket => {
                    if bracket_stack.is_empty() {
                        errors.push(ParserError::UnmatchedClosingDelimiter {
                            span: token.span.clone(),
                            delimiter: "]".to_string(),
                        });
                    } else {
                        bracket_stack.pop();
                    }
                }
                NexusToken::LeftBrace => brace_stack.push((idx, token.span.clone())),
                NexusToken::RightBrace => {
                    if brace_stack.is_empty() {
                        errors.push(ParserError::UnmatchedClosingDelimiter {
                            span: token.span.clone(),
                            delimiter: "}".to_string(),
                        });
                    } else {
                        brace_stack.pop();
                    }
                }
                // NexusToken::QuoteSingle | NexusToken::QuoteDouble => { // 未定義
                //     if quote_stack.last().map_or(false, |&(_, kind)| kind == token.kind) {
                //         quote_stack.pop();
                //     } else {
                //         quote_stack.push((idx, token.kind.clone()));
                //     }
                // }
                NexusToken::Variable(_) => { // Variable は String を持つタプルバリアント
                    // 変数式が空かチェック
                    let value = self.get_token_value(token);
                    if value == "$" || value == "${}" {
                        errors.push(ParserError::EmptyVariableExpression { // ParserErrorに定義が必要
                            span: token.span.clone(),
                        });
                    }
                }
                NexusToken::Pipe => { // PipeErr, PipeAll は Pipe で代替
                    // パイプの後に有効なトークンがあることを確認
                    if idx == tokens.len() - 1 || matches!(tokens.get(idx + 1).map(|t| &t.kind), Some(NexusToken::EOF) | Some(NexusToken::Semicolon)) {
                        errors.push(ParserError::InvalidPipeUsage { // ParserErrorに定義が必要
                            span: token.span.clone(),
                            message: "パイプの後に有効なコマンドが必要です".to_string(),
                        });
                    }
                }
                NexusToken::LessThan | NexusToken::GreaterThan | NexusToken::Append => { // RedirectIn, RedirectOut, RedirectAppend, RedirectErr を既存トークンで代替
                    // リダイレクトの後に有効なトークンがあることを確認
                    if idx == tokens.len() - 1 || 
                       !matches!(tokens.get(idx + 1).map(|t| &t.kind), Some(NexusToken::String(_)) | Some(NexusToken::Identifier(_)) | Some(NexusToken::Variable(_))) {
                        errors.push(ParserError::InvalidRedirection { // ParserErrorに定義が必要
                            span: token.span.clone(),
                            message: "リダイレクト操作の後にファイル名が必要です".to_string(),
                        });
                    }
                }
                _ => {}
            }

            // コマンド構文の検証
            if idx > 0 && matches!(token.kind, NexusToken::LongFlag(_) | NexusToken::ShortFlag(_)) {
                let prev = &tokens[idx - 1];
                if matches!(prev.kind, NexusToken::LongFlag(_) | NexusToken::ShortFlag(_)) {
                    // フラグに引数が必要かどうかの検証ロジックをここに追加
                    // 例: --fileがファイル名引数を必要とするフラグの場合
                }
            }
        }

        // 閉じられていない括弧をエラーとして追加
        for (_, span) in paren_stack {
            errors.push(ParserError::UnmatchedOpeningDelimiter { // ParserErrorに定義が必要
                span,
                delimiter: "(".to_string(),
            });
        }
        
        for (_, span) in bracket_stack {
            errors.push(ParserError::UnmatchedOpeningDelimiter { // ParserErrorに定義が必要
                span,
                delimiter: "[".to_string(),
            });
        }
        
        for (_, span) in brace_stack {
            errors.push(ParserError::UnmatchedOpeningDelimiter { // ParserErrorに定義が必要
                span,
                delimiter: "{".to_string(),
            });
        }
        
        // for (_, kind) in quote_stack { // 未定義のQuoteSingle/Doubleに依存するためコメントアウト
        //     let quote = match kind {
        //         NexusToken::String(_) => "\\"", // String で代用
        //         NexusToken::SingleQuotedString(_) => "'", // SingleQuotedString で代用
        //         _ => unreachable!(),
        //     };
        //     errors.push(ParserError::UnmatchedQuote { // ParserErrorに定義が必要
        //         span: Span::new(0, 0),  // この場合はソース全体を指すスパンを設定する必要がある
        //         quote: quote.to_string(),
        //     });
        // }

        // 重複エラーを除去 (ParserError が span() メソッドと PartialEq を持つので解除)
        errors.sort_by_key(|e| e.span().start);
        errors.dedup_by(|a, b| a.span() == b.span() && format!("{:?}", a) == format!("{:?}", b)); // span とエラー内容で比較
        
        errors
    }

    /// トークンの文脈分析を行い、コマンドとそれに関連するトークンのマッピングを作成
    pub fn analyze_token_context(&self, tokens: &[TokenWithSpan]) -> HashMap<usize, CommandContext> { // CommandContextの定義が必要
        let mut context_map = HashMap::new();
        let mut current_command_idx = None;
        
        for (idx, token) in tokens.iter().enumerate() {
            match token.kind {
                NexusToken::Identifier(_) => {
                    // 新しいコマンドの開始を検出
                    if current_command_idx.is_none() || 
                       matches!(tokens.get(idx.saturating_sub(1)).map(|t| &t.kind), Some(NexusToken::Semicolon) | Some(NexusToken::Pipe)) { // PipeErr, PipeAll は Pipe で代替
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
                NexusToken::LongFlag(_) | NexusToken::ShortFlag(_) => {
                    if let Some(cmd_idx) = current_command_idx {
                        if let Some(context) = context_map.get_mut(&cmd_idx) {
                            context.flags_count += 1;
                            context.related_tokens.push(idx);
                        }
                    }
                }
                NexusToken::LessThan | NexusToken::GreaterThan | NexusToken::Append => { // RedirectIn, RedirectOut, RedirectAppend, RedirectErr を既存トークンで代替
                    if let Some(cmd_idx) = current_command_idx {
                        if let Some(context) = context_map.get_mut(&cmd_idx) {
                            context.redirections.push((token.kind.clone(), idx)); // kind を clone
                            context.related_tokens.push(idx);
                            
                            // リダイレクト先もコマンドコンテキストに関連付ける
                            if idx + 1 < tokens.len() {
                                context.related_tokens.push(idx + 1);
                            }
                        }
                    }
                }
                NexusToken::Pipe => { // PipeErr, PipeAll は Pipe で代替
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

    /// コマンドの種類を判断する - 世界最高レベルのコマンド分類システム
    fn determine_command_type(&self, token: &TokenWithSpan) -> CommandType {
        let cmd_name = self.get_token_value(token);
        // --- 拡張: 外部DB/プラグイン/ユーザー定義コマンド分類の動的取得 ---
        if let Some(ext_type) = self.get_external_command_type(&cmd_name) {
            return ext_type;
        }
        // ... existing code ...
    }

    /// 外部DB・プラグイン・ユーザー定義からコマンド分類を取得
    fn get_external_command_type(&self, cmd_name: &str) -> Option<CommandType> {
        // 1. プラグインAPI経由で分類取得
        #[cfg(feature = "plugin-system")]
        {
            if let Some(plugin_type) = crate::plugin::get_command_type(cmd_name) {
                return Some(plugin_type);
            }
        }
        // 2. ユーザー定義コマンド分類（設定ファイル等）
        if let Some(user_type) = self.get_user_defined_command_type(cmd_name) {
            return Some(user_type);
        }
        // 3. 外部コマンドDB（例: コマンドDBキャッシュ/オンラインDB）
        if let Some(db_type) = self.get_command_type_from_db(cmd_name) {
            return Some(db_type);
        }
        None
    }

    /// ユーザー定義コマンド分類の取得（例: ~/.nexusshell/commands.toml）
    fn get_user_defined_command_type(&self, cmd_name: &str) -> Option<CommandType> {
        // ~/.nexusshell/commands.toml からコマンド分類を取得
        use std::fs;
        use std::path::PathBuf;
        let config_path = dirs::home_dir().map(|h| h.join(".nexusshell/commands.toml"));
        if let Some(path) = config_path {
            if let Ok(contents) = fs::read_to_string(path) {
                if let Ok(table) = contents.parse::<toml::Value>() {
                    if let Some(cmds) = table.get("commands").and_then(|v| v.as_table()) {
                        if let Some(cmd) = cmds.get(cmd_name) {
                            if let Some(ty) = cmd.get("type").and_then(|v| v.as_str()) {
                                return Some(CommandType::from_str(ty).unwrap_or(CommandType::Unknown));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 外部コマンドDBから分類取得（例: キャッシュ/オンラインDB）
    fn get_command_type_from_db(&self, cmd_name: &str) -> Option<CommandType> {
        // キャッシュ優先、なければオンラインDB/APIへ
        if let Some(ty) = self.db_command_type_cache.get(cmd_name) {
            return Some(ty.clone());
        }
        // 例: https://nexusshell.org/api/command_type?name=cmd_name
        if let Ok(resp) = ureq::get(&format!("https://nexusshell.org/api/command_type?name={}", cmd_name)).call() {
            if let Ok(json) = resp.into_json::<serde_json::Value>() {
                if let Some(ty) = json.get("type").and_then(|v| v.as_str()) {
                    let ty_enum = CommandType::from_str(ty).unwrap_or(CommandType::Unknown);
                    self.db_command_type_cache.insert(cmd_name.to_string(), ty_enum.clone());
                    return Some(ty_enum);
                }
            }
        }
        None
    }

    /// コマンド補完候補を動的に生成（外部DB/プラグイン/ユーザー定義も含む）
    fn generate_command_completions(
        &self,
        current_word: &str,
        available_commands: &[String],
    ) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        // 1. 標準コマンド
        for cmd in available_commands {
            if cmd.starts_with(current_word) {
                suggestions.push(CompletionSuggestion::new(
                    cmd.clone(),
                    CompletionType::Command(cmd.clone()),
                    self.get_command_description(cmd),
                    10,
                ));
            }
        }
        // 2. プラグインコマンド
        #[cfg(feature = "plugin-system")]
        {
            for (cmd, desc) in crate::plugin::list_plugin_commands_with_desc() {
                if cmd.starts_with(current_word) {
                    suggestions.push(CompletionSuggestion::new(
                        cmd.clone(),
                        CompletionType::Command(cmd.clone()),
                        Some(desc),
                        20,
                    ));
                }
            }
        }
        // 3. ユーザー定義コマンド
        for (cmd, desc) in self.list_user_defined_commands_with_desc() {
            if cmd.starts_with(current_word) {
                suggestions.push(CompletionSuggestion::new(
                    cmd.clone(),
                    CompletionType::Command(cmd.clone()),
                    Some(desc),
                    30,
                ));
            }
        }
        // 4. 外部コマンドDB
        for (cmd, desc) in self.list_commands_from_db_with_desc() {
            if cmd.starts_with(current_word) {
                suggestions.push(CompletionSuggestion::new(
                    cmd.clone(),
                    CompletionType::Command(cmd.clone()),
                    Some(desc),
                    40,
                ));
            }
        }
        suggestions
    }

    /// コマンドの説明を取得（標準/プラグイン/ユーザー/DBを横断）
    fn get_command_description(&self, cmd: &str) -> Option<String> {
        // 1. 標準コマンド
        if let Some(desc) = self.get_builtin_command_description(cmd) {
            return Some(desc);
        }
        // 2. プラグイン
        #[cfg(feature = "plugin-system")]
        {
            if let Some(desc) = crate::plugin::get_plugin_command_description(cmd) {
                return Some(desc);
            }
        }
        // 3. ユーザー定義
        if let Some(desc) = self.get_user_defined_command_description(cmd) {
            return Some(desc);
        }
        // 4. 外部DB
        if let Some(desc) = self.get_command_description_from_db(cmd) {
            return Some(desc);
        }
        None
    }
    // --- 以降、各補助メソッドのスタブ（TODO: 実装） ---
    fn get_builtin_command_description(&self, _cmd: &str) -> Option<String> { None }
    fn get_user_defined_command_description(&self, _cmd: &str) -> Option<String> { None }
    fn get_command_description_from_db(&self, _cmd: &str) -> Option<String> { None }
    fn list_user_defined_commands_with_desc(&self) -> Vec<(String, String)> { vec![] }
    fn list_commands_from_db_with_desc(&self) -> Vec<(String, String)> { vec![] }

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
        
        // 包括的なコマンドリスト - 世界最高レベルの補完データベース（NexusShellオリジナル）
        let common_commands = vec![
            // シェル組み込みコマンド
            ("cd", "現在の作業ディレクトリを変更する"),
            ("echo", "テキストを標準出力に出力する"),
            ("exit", "シェルを終了する"),
            ("export", "環境変数を設定・表示する"),
            ("pwd", "現在の作業ディレクトリを表示する"),
            ("source", "指定されたファイルをシェルスクリプトとして実行する"),
            ("alias", "コマンドのエイリアスを作成する"),
            ("unalias", "コマンドのエイリアスを削除する"),
            ("set", "シェル変数を設定または表示する"),
            ("unset", "シェル変数を削除する"),
            ("history", "コマンド履歴を表示する"),
            ("jobs", "バックグラウンドジョブのリストを表示する"),
            ("fg", "ジョブをフォアグラウンドで実行する"),
            ("bg", "ジョブをバックグラウンドで実行する"),
            ("wait", "バックグラウンドジョブの完了を待つ"),
            ("umask", "ファイル作成時の権限マスクを設定・表示する"),
            ("help", "ヘルプ情報を表示する"),
            ("exec", "現在のシェルを置き換えてコマンドを実行する"),
            ("eval", "引数を評価して実行する"),
            ("shift", "位置パラメータをシフトする"),
            ("readonly", "変数を読み取り専用にする"),
            ("type", "コマンドの種類を表示する"),
            ("ulimit", "シェルリソース制限を表示・変更する"),
            ("command", "シェル関数をバイパスしてコマンドを実行する"),
            ("mapfile", "配列に標準入力から読み込む"),
            ("read", "標準入力から行を読み込む"),
            ("declare", "変数を宣言して属性を設定する"),
            ("getopts", "位置パラメータを解析する"),
            ("local", "関数内でローカル変数を宣言する"),
            ("test", "条件式を評価する"), 
            (".", "sourceの別名、シェルスクリプトを現在のシェルで実行"),
            ("bind", "キーバインドを設定・表示する"),
            ("builtin", "シェル組み込みコマンドを実行する"),
            ("caller", "サブルーチン呼び出し元の情報を表示"),
            ("dirs", "ディレクトリスタックを表示する"),
            ("disown", "ジョブをジョブテーブルから削除する"),
            ("enable", "シェル組み込みコマンドを有効化・無効化する"),
            ("fc", "コマンド履歴を編集・再実行する"),
            ("hash", "コマンドのフルパスキャッシュを管理する"),
            ("pushd", "ディレクトリスタックを操作する"),
            ("popd", "ディレクトリスタックから移動する"),
            ("printf", "フォーマット指定に従って出力する"),
            ("shopt", "シェルオプションを設定・表示する"),
            ("suspend", "シェルを一時停止する"),
            ("times", "シェルの累積ユーザー・システム時間を表示"),
            ("trap", "シグナルや条件に対するハンドラを設定する"),
            ("typeset", "変数の宣言と属性設定（declareの別名）"),
            
            // ファイル操作コマンド
            ("cat", "ファイルの内容を表示する"),
            ("ls", "ディレクトリの内容を一覧表示する"),
            ("cp", "ファイル・ディレクトリをコピーする"),
            ("mv", "ファイル・ディレクトリを移動・名前変更する"),
            ("rm", "ファイル・ディレクトリを削除する"),
            ("mkdir", "ディレクトリを作成する"),
            ("rmdir", "空のディレクトリを削除する"),
            ("touch", "ファイルのタイムスタンプを更新または作成する"),
            ("ln", "ファイルのリンクを作成する"),
            ("chmod", "ファイルの権限を変更する"),
            ("chown", "ファイルの所有者を変更する"),
            ("chgrp", "ファイルのグループ所有権を変更する"),
            ("find", "ファイルやディレクトリを検索する"),
            ("grep", "ファイル内の文字列を検索する"),
            ("rg", "高速な検索ユーティリティ（ripgrep）"),
            ("ag", "高速な検索ユーティリティ（The Silver Searcher）"),
            ("fd", "高速なファイル検索ユーティリティ"),
            ("head", "ファイルの先頭部分を表示する"),
            ("tail", "ファイルの末尾部分を表示する"),
            ("less", "ファイルを１画面ずつ表示する"),
            ("more", "ファイルを１画面ずつ表示する"),
            ("sort", "テキストをソートする"),
            ("uniq", "重複行を削除または数える"),
            ("wc", "行数・単語数・バイト数をカウントする"),
            ("tee", "標準入力を表示しながらファイルに保存する"),
            ("diff", "ファイル間の差分を表示する"),
            ("diff3", "3つのファイル間の差分を表示する"),
            ("cmp", "2つのファイルをバイト単位で比較する"),
            ("comm", "2つのソート済みファイルの共通行と固有行を比較する"),
            ("patch", "diffファイルを適用する"),
            ("cksum", "ファイルのCRCチェックサムを計算する"),
            ("md5sum", "MD5チェックサムを計算する"),
            ("sha1sum", "SHA1チェックサムを計算する"),
            ("sha256sum", "SHA256チェックサムを計算する"),
            ("sha512sum", "SHA512チェックサムを計算する"),
            ("b2sum", "BLAKE2チェックサムを計算する"),
            ("dd", "ファイルを変換・コピーする"),
            ("df", "ディスクの使用状況を表示する"),
            ("du", "ディレクトリのディスク使用量を表示する"),
            ("file", "ファイルの種類を判定する"),
            ("stat", "ファイルの詳細情報を表示する"),
            ("chcon", "ファイルのSELinuxセキュリティコンテキストを変更する"),
            ("getfacl", "ファイルのアクセス制御リストを表示する"),
            ("setfacl", "ファイルのアクセス制御リストを設定する"),
            ("truncate", "ファイルサイズを変更する"),
            ("split", "ファイルを分割する"),
            ("csplit", "パターンに基づいてファイルを分割する"),
            ("shred", "ファイルを上書きして安全に削除する"),
            ("readlink", "シンボリックリンクの参照先を表示する"),
            ("realpath", "パスの標準化された絶対パスを表示する"),
            ("rename", "ファイル名を一括変更する"),
            ("mktemp", "一時ファイル・ディレクトリを作成する"),
            ("install", "ファイルをコピーして属性を設定する"),
            ("basename", "パスからファイル名部分を抽出する"),
            ("dirname", "パスからディレクトリ部分を抽出する"),
            ("pathchk", "パス名の有効性をチェックする"),
            ("mcopy", "MS-DOSファイルをコピーする"),
            ("mmove", "MS-DOSファイルを移動・名前変更する"),
            ("pinky", "ユーザー情報を軽量表示する"),
            ("sync", "ファイルシステムバッファを同期する"),
            ("fsck", "ファイルシステムを検査・修復する"),
            ("mkfs", "ファイルシステムを作成する"),
            ("locate", "ファイル名データベースから検索する"),
            ("updatedb", "locateのファイルデータベースを更新する"),
            
            // 高度なファイルシステム操作
            ("rsync", "効率的なファイル同期ツール"),
            ("fdupes", "重複ファイルを検索する"),
            ("ranger", "ファイルマネージャ"),
            ("nnn", "端末ファイルマネージャ"),
            ("ncdu", "ディスク使用量の対話的分析ツール"),
            ("dua", "高速なディスク使用量分析ツール"),
            ("exa", "lsの高度な代替ツール"),
            ("lsd", "lsの高度な代替ツール"),
            ("broot", "ディレクトリツリー表示・検索ツール"),
            ("bat", "catの高度な代替ツール"),
            ("delta", "diffの高度な代替ツール"),
            
            // テキスト処理
            ("awk", "テキスト処理言語"),
            ("gawk", "GNU awk"),
            ("mawk", "高速AWK実装"),
            ("nawk", "新AWK実装"),
            ("sed", "ストリームエディタ"),
            ("cut", "各行から特定のフィールドを抽出する"),
            ("paste", "ファイルを行ごとに結合する"),
            ("join", "共通フィールドに基づいてファイルを結合する"),
            ("tr", "文字の置換・削除を行う"),
            ("fold", "長い行を折りたたむ"),
            ("fmt", "テキストをフォーマットする"),
            ("pr", "印刷向けにテキストをフォーマットする"),
            ("column", "テキストを列形式に整形する"),
            ("nl", "行番号を付加する"),
            ("tac", "ファイルを逆順に表示する"),
            ("expand", "タブをスペースに変換する"),
            ("unexpand", "スペースをタブに変換する"),
            ("iconv", "文字コードを変換する"),
            ("dos2unix", "DOSテキスト形式をUNIX形式に変換する"),
            ("unix2dos", "UNIXテキスト形式をDOS形式に変換する"),
            ("xxd", "バイナリダンプを生成する"),
            ("hexdump", "バイナリファイルを16進数で表示する"),
            ("strings", "バイナリファイルから印刷可能な文字列を抽出する"),
            ("jq", "JSONプロセッサ"),
            ("yq", "YAMLプロセッサ"),
            ("xmlstarlet", "XMLプロセッサ"),
            ("pandoc", "ドキュメント形式変換ツール"),
            ("aspell", "スペルチェッカー"),
            ("hunspell", "スペルチェッカー"),
            ("ed", "行指向テキストエディタ"),
            ("ex", "行指向テキストエディタ"),
            ("xargs", "標準入力からコマンドラインを構築して実行する"),
            ("ptx", "置換テキストファイルの生成"),
            ("rev", "各行の文字を逆順にする"),
            ("unlink", "ファイルへのリンクを削除する"),
            ("tsort", "トポロジカルソートの実行"),
            ("stdbuf", "標準ストリームのバッファリングを調整する"),
            ("timeout", "指定時間後にコマンドを終了する"),
            ("seq", "連続した数値を生成する"),
            ("factor", "数値を素因数分解する"),
            ("numfmt", "数値のフォーマットを変換する"),
            ("sd", "sedの高速な代替ツール"),
            
            // 最新の高度テキスト処理ツール
            ("miller", "名前付きCSV/TSV/JSONデータ処理ツール"),
            ("xsv", "高速CSV操作ツール"),
            ("fx", "対話型JSONビューア"),
            ("gron", "JSONをgrepしやすい形式に変換するツール"),
            ("dasel", "JSON/YAML/TOML/XML構造データクエリツール"),
            ("csv2json", "CSVからJSONへの変換ツール"),
            ("xml2json", "XMLからJSONへの変換ツール"),
            ("jless", "対話型JSONビューア・ナビゲータ"),
            ("jsonfmt", "JSON整形ツール"),
            ("visidata", "ターミナルベースのテーブルデータエディタ"),
            ("jtc", "JSONコマンドラインプロセッサ"),
            ("csvkit", "CSVファイル操作ユーティリティ集"),
            ("textql", "SQLでテキストを処理"),
            ("json2yaml", "JSONからYAMLへの変換"),
            ("yaml2json", "YAMLからJSONへの変換"),
            ("toml2json", "TOMLからJSONへの変換"),
            ("turbocsv", "超高速CSV処理"),
            ("xml2csv", "XMLからCSVへの変換"),
            ("ansifilter", "ANSIエスケープシーケンス除去"),
            ("shyaml", "シェルからYAMLパース"),
            
            // アーカイブ・圧縮
            ("tar", "アーカイブを作成・展開する"),
            ("gzip", "ファイルを圧縮する"),
            ("gunzip", "gzipで圧縮されたファイルを展開する"),
            ("bzip2", "ファイルを圧縮する（高圧縮率）"),
            ("bunzip2", "bzip2で圧縮されたファイルを展開する"),
            ("xz", "ファイルを圧縮する（超高圧縮率）"),
            ("unxz", "xzで圧縮されたファイルを展開する"),
            ("lz4", "高速圧縮・展開アルゴリズム"),
            ("zstd", "高圧縮率・高速圧縮アルゴリズム"),
            ("zip", "zipアーカイブを作成する"),
            ("unzip", "zipアーカイブを展開する"),
            ("zcat", "圧縮ファイルの内容を表示する"),
            ("zless", "圧縮ファイルをページャで表示する"),
            ("zmore", "圧縮ファイルをページャで表示する"),
            ("7z", "7-Zipアーカイブを操作する"),
            ("rar", "RARアーカイブを作成する"),
            ("unrar", "RARアーカイブを展開する"),
            ("lzma", "LZMAで圧縮・展開する"),
            ("lzop", "LZOアルゴリズムで圧縮・展開する"),
            ("cpio", "cpioアーカイブを操作する"),
            ("ar", "arアーカイブを操作する"),
            ("pixz", "並列xz圧縮ツール"),
            ("pigz", "並列gzip圧縮ツール"),
            ("pbzip2", "並列bzip2圧縮ツール"),
            ("plzip", "並列lzip圧縮ツール"),
            ("zpaq", "最大圧縮率アーカイバ"),
            ("arc", "フリーウェアアーカイバ"),
            ("pax", "POSIX標準アーカイブユーティリティ"),
            ("dar", "ディスクアーカイブ"),
            ("arj", "ARJアーカイブ形式操作"),
            ("lha", "LHAアーカイブ形式操作"),
            ("dtrx", "インテリジェントアーカイブ抽出"),
            ("zbackup", "重複排除バックアップツール"),
            ("restic", "暗号化バックアップツール"),
            ("borg", "重複排除暗号化バックアップ"),
            ("brotli", "Googleの高効率圧縮アルゴリズム"),
            ("lrzip", "大容量ファイル向け圧縮ツール"),
            ("ouch", "モダンな圧縮・解凍ユーティリティ"),
            ("zpipe", "並列圧縮パイプライン"),
            ("unar", "多形式対応解凍ツール"),
            
            // ネットワーク
            ("ssh", "セキュアシェル接続を確立する"),
            ("mosh", "モバイル用セキュアシェル（接続断続に強い）"),
            ("scp", "ファイルをセキュアにコピーする"),
            ("sftp", "セキュアFTPクライアント"),
            ("ftp", "FTPクライアント"),
            ("lftp", "高機能FTPクライアント"),
            ("telnet", "TELNETプロトコルでサーバに接続する"),
            ("nc", "ネットワークソケットに対して読み書きを行う"),
            ("ncat", "強化されたネットワーク接続ユーティリティ"),
            ("socat", "多目的リレー・ソケット接続ユーティリティ"),
            ("ping", "ICMPエコーリクエストを送信する"),
            ("traceroute", "パケットの経路を追跡する"),
            ("mtr", "tracerouteとpingを組み合わせたネットワーク診断ツール"),
            ("dig", "DNSサーバに問い合わせる"),
            ("nslookup", "DNSサーバに問い合わせる"),
            ("host", "ホスト名の解決を行う"),
            ("drill", "DNSサーバのクエリツール"),
            ("whois", "ドメイン情報を検索する"),
            ("hostname", "システムのホスト名を表示・設定する"),
            ("domainname", "システムのNISドメイン名を表示・設定する"),
            ("ifconfig", "ネットワークインターフェース設定を表示・変更する"),
            ("ip", "ネットワークをルーティング・制御する"),
            ("iw", "無線デバイスの設定・情報表示を行う"),
            ("iwconfig", "無線ネットワークインターフェースを設定する"),
            ("netstat", "ネットワーク接続・ルーティングテーブルなどを表示する"),
            ("ss", "ソケット統計を表示する"),
            ("curl", "データ転送ツール"),
            ("wget", "ネットワーク経由でファイルをダウンロードする"),
            ("aria2c", "高機能ダウンロードツール"),
            ("httpie", "ユーザーフレンドリーなHTTPクライアント"),
            ("rsync", "リモートファイル同期ツール"),
            ("arp", "ARPキャッシュを表示・変更する"),
            ("route", "ルーティングテーブルを表示・変更する"),
            ("tcpdump", "ネットワークパケットをキャプチャ・分析する"),
            ("iptables", "パケットフィルタリングルールを設定する"),
            ("nft", "次世代パケットフィルタリング（nftables）"),
            ("nmap", "ネットワークスキャン・セキュリティ監査ツール"),
            ("openssl", "SSLプロトコルの各種操作を行う"),
            ("ssh-keygen", "SSH認証キーの生成・管理"),
            ("sshfs", "SSHを使用したファイルシステムマウント"),
            ("iptraf", "IPネットワークモニタ"),
            ("nethogs", "プロセス別ネットワーク帯域使用量モニタ"),
            ("bmon", "帯域幅モニタリングツール"),
            ("speedtest-cli", "インターネット接続速度テスト"),
            ("fast", "インターネット接続速度テスト（Fast.comベース）"),
            ("ethtool", "イーサネットカードのパラメータ表示・設定"),
            ("slurm", "ネットワーク負荷モニタ"),
            ("iperf", "ネットワーク帯域測定ツール"),
            ("hping", "TCPパケット送信・分析ツール"),
            ("nmcli", "NetworkManagerコマンドラインインターフェース"),
            ("nmtui", "NetworkManagerテキストユーザーインターフェース"),

            // ... existing code with more commands ...
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
        
        // コマンドに基づいてフラグを生成 - 世界最高レベルのコマンド別フラグデータベース
        let flags = match command {
            "ls" => vec![
                ("-l", "詳細なリスト形式で表示"),
                ("-a", "全てのファイル（隠しファイルを含む）を表示"),
                ("-h", "人間が読みやすい形式でサイズを表示"),
                ("-la", "詳細表示かつ全ファイル表示"),
                ("-lah", "詳細表示、全ファイル表示、人間可読サイズ"),
                ("--color", "カラー出力を有効にする"),
                ("--color=always", "常にカラー出力"),
                ("-d", "ディレクトリ自体を表示、中身は表示しない"),
                ("-F", "ファイルタイプを表示（/=ディレクトリ, *=実行可能）"),
                ("-R", "再帰的にディレクトリを表示"),
                ("--sort=time", "更新時間でソート"),
                ("--sort=size", "サイズでソート")
            ],
            "grep" => vec![
                ("-i", "大文字と小文字を区別しない"),
                ("-r", "ディレクトリを再帰的に検索"),
                ("-v", "一致しない行を表示"),
                ("-n", "行番号を表示"),
                ("-A", "マッチ後の行も表示"),
                ("-B", "マッチ前の行も表示"),
                ("-C", "マッチ前後の行を表示"),
                ("-E", "拡張正規表現を使用"),
                ("-f", "パターンファイルから読み込む"),
                ("--color", "一致部分をカラー表示"),
                ("-o", "一致部分のみ表示"),
                ("-l", "一致ファイル名のみ表示"),
                ("-c", "一致行数のみ表示"),
                ("--include", "指定パターンのファイルのみ検索"),
                ("--exclude", "指定パターンのファイルを除外")
            ],
            "find" => vec![
                ("-name", "ファイル名で検索"),
                ("-iname", "大文字小文字を区別せずファイル名で検索"),
                ("-type", "ファイルタイプで検索 (f:ファイル, d:ディレクトリ)"),
                ("-size", "サイズで検索 (+10M:10MB以上, -1G:1GB未満)"),
                ("-mtime", "更新日時で検索 (-7:7日以内, +30:30日以上前)"),
                ("-exec", "一致したファイルに対してコマンドを実行"),
                ("-delete", "一致したファイルを削除"),
                ("-empty", "空のファイル/ディレクトリを検索"),
                ("-path", "パスパターンで検索"),
                ("-not", "条件の否定"),
                ("-and", "AND条件"),
                ("-or", "OR条件"),
                ("-perm", "パーミッションで検索"),
                ("-newer", "指定ファイルより新しいファイルを検索"),
                ("-user", "所有者で検索")
            ],
            "git" => vec![
                ("add", "変更をステージングに追加"),
                ("commit", "変更をコミット"),
                ("commit -m", "メッセージ付きでコミット"),
                ("push", "リモートリポジトリに変更を送信"),
                ("pull", "リモートリポジトリから変更を取得"),
                ("status", "リポジトリのステータスを表示"),
                ("checkout", "ブランチ/ファイルを切り替え"),
                ("branch", "ブランチを一覧/作成"),
                ("merge", "ブランチをマージ"),
                ("reset", "変更をリセット"),
                ("stash", "変更を一時保存"),
                ("clone", "リポジトリをクローン"),
                ("log", "コミット履歴を表示"),
                ("diff", "変更の差分を表示"),
                ("fetch", "リモート情報を取得")
            ],
            "docker" => vec![
                ("build", "Dockerfileからイメージをビルド"),
                ("run", "コンテナを実行"),
                ("ps", "実行中のコンテナを表示"),
                ("images", "イメージを一覧表示"),
                ("exec", "実行中のコンテナでコマンドを実行"),
                ("stop", "コンテナを停止"),
                ("rm", "コンテナを削除"),
                ("rmi", "イメージを削除"),
                ("pull", "イメージをダウンロード"),
                ("push", "イメージをレジストリに送信"),
                ("logs", "コンテナのログを表示"),
                ("volume", "ボリューム操作"),
                ("network", "ネットワーク操作"),
                ("compose", "Docker Compose操作"),
                ("--help", "ヘルプを表示")
            ],
            _ => vec![
                ("--help", "ヘルプを表示"),
                ("--version", "バージョン情報を表示"),
                ("-v", "詳細情報を表示"),
                ("-f", "強制的に実行"),
                ("-r", "再帰的に処理"),
                ("-q", "出力を抑制（クワイエットモード）"),
                ("-o", "出力ファイルを指定"),
                ("-i", "入力ファイルを指定"),
                ("-c", "設定ファイルを指定"),
                ("-d", "デバッグモード"),
                ("-n", "ドライラン（実行せずシミュレーション）"),
                ("-y", "すべてのプロンプトに自動的に「はい」と応答"),
                ("--no-color", "カラー出力を無効化"),
                ("--json", "JSON形式で出力"),
                ("--verbose", "詳細出力")
            ],
        };
        
        for &(flag, desc) in flags.iter() {
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
        
        // 高度なパス解決とファイルシステムアクセスを実行
        let files_and_dirs = self.get_optimized_filesystem_entries(current_path);
        
        // 取得したファイルとディレクトリを補完候補に追加
        for (path, entry_type) in files_and_dirs {
            let (display_path, description) = match entry_type {
                EntryType::Directory => (format!("{}/", path), Some("ディレクトリ".to_string())),
                EntryType::Symlink(target) => (path.clone(), Some(format!("リンク → {}", target))),
                EntryType::File(size) => {
                    let desc = if size < 1024 {
                        format!("ファイル ({} B)", size)
                    } else if size < 1024 * 1024 {
                        format!("ファイル ({:.1} KB)", size as f64 / 1024.0)
                    } else {
                        format!("ファイル ({:.1} MB)", size as f64 / (1024.0 * 1024.0))
                    };
                    (path.clone(), Some(desc))
                },
                EntryType::Special(kind) => (path.clone(), Some(format!("特殊ファイル ({})", kind))),
            };
            
            suggestions.push(CompletionSuggestion::new(
                display_path.clone(),
                CompletionType::FilePath(display_path),
                description,
                1,
            ));
        }
        
        // コレクションが空の場合、代替として一般的なパスを表示
        if suggestions.is_empty() {
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
                        2, // 代替候補なので優先度を下げる
                    ));
                }
            }
        }

        // 環境変数の展開（チルダ展開）
        if current_path.starts_with("~") {
            let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
            let expanded_path = current_path.replacen("~", &home_dir, 1);
            let home_files = self.get_files_from_filesystem(&expanded_path);
            
            for (path, is_dir) in home_files {
                // ホームディレクトリパスを~に戻す
                let tilde_path = path.replacen(&home_dir, "~", 1);
                let display_path = if is_dir {
                    format!("{}/", tilde_path)
                } else {
                    tilde_path
                };
                
                let description = if is_dir {
                    Some("ディレクトリ".to_string())
                } else {
                    None
                };
                
                suggestions.push(CompletionSuggestion::new(
                    display_path.clone(),
                    CompletionType::FilePath(display_path),
                    description,
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

    // ファイルシステムから実際のファイルとディレクトリを取得する
    fn get_files_from_filesystem(&self, path_prefix: &str) -> Vec<(String, bool)> {
        use std::fs;
        use std::path::Path;
        let mut result = Vec::new();
        let path = Path::new(path_prefix);
        // 存在しない場合は親ディレクトリで補完
        let dir = if path.is_dir() {
            path
        } else if let Some(parent) = path.parent() {
            parent
        } else {
            Path::new(".")
        };
        let read_dir = fs::read_dir(dir);
        if let Ok(entries) = read_dir {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_dir = path.is_dir();
                let is_symlink = entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false);
                let is_hidden = path.file_name().map(|n| (&*n.to_string_lossy()).starts_with(".")).unwrap_or(false);
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // 隠しファイルやシンボリックリンクも区別して返す
                    let mut display = name.to_string();
                    if is_dir { display.push('/'); }
                    if is_symlink { display.push('@'); }
                    if is_hidden { display = format!(".{}", display.trim_start_matches('.')); }
                    result.push((display, is_dir));
                }
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
    
    // ファイルシステムに基づいたパス補完候補を生成
    fn suggest_path_completions(&self, input: &str) -> Vec<String> {
        // ワイルドカードや環境変数展開はシェルが通常行うため、
        // ここでは単純化のために直接的なパス照合のみを実装
        
        let mut results = Vec::new();
        
        // 入力パスからファイルとディレクトリを取得
        let files_and_dirs = self.get_files_from_filesystem(input);
        
        for (path, is_dir) in files_and_dirs {
            // ファイルとディレクトリを候補に追加
            results.push(path);
        }
        
        // ホームディレクトリの展開
        if input == "~" || input.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                let expanded = input.replacen("~", &home, 1);
                let home_files = self.get_files_from_filesystem(&expanded);
                
                for (path, _) in home_files {
                    // ホームディレクトリパスを~に戻す
                    let tilde_path = path.replacen(&home, "~", 1);
                    results.push(tilde_path);
                }
            }
        }
        
        results
    }

    /// 位置からオフセットを見つける（error_recovery.rsから参照される関数）
    /// 世界最高レベルの高精度オフセット計算エンジン
    fn find_position_offset(&self, text: &str, position: usize) -> Option<usize> {
        // 1. トークン位置の範囲チェック
        if position >= self.tokens.len() {
            // 範囲外の場合でも、テキスト末尾のオフセットを返す（より堅牢な実装）
            return Some(text.len().saturating_sub(1));
        }

        // 2. 直接的なトークンマッピング（高速パス）
        let token = &self.tokens[position];
        
        // 2.1 トークンのスパン情報が有効であればそれを使用（最も正確）
        if token.span.start < text.len() {
            return Some(token.span.start);
        }
        
        // 3. 行と列の情報を使用した高精度マッピング（バックアップ手法）
        if token.line > 0 && token.column > 0 && token.line <= self.line_starts.len() {
            let line_start = self.line_starts[token.line - 1];
            let column_offset = token.column.saturating_sub(1); // 1-indexed から 0-indexed へ
            
            // 行の開始位置 + 列のオフセット（マルチバイト文字対応）
            let mut char_offset = 0;
            let mut byte_offset = 0;
            for c in text[line_start..].chars() {
                if char_offset == column_offset {
                    return Some(line_start + byte_offset);
                }
                byte_offset += c.len_utf8();
                char_offset += 1;
            }
            
            // 列のオフセットが行の境界を超えている場合は行の末尾を返す
            return Some(line_start + byte_offset);
        }
        
        // 4. コンテキスト情報からの再計算（最終手段）
        // トークンの文字列表現に基づいてソース内の位置を特定
        let token_text = match &token.kind {
            NexusToken::Identifier(s) | 
            NexusToken::Variable(s) | 
            NexusToken::VariableExpression(s) | 
            NexusToken::String(s) |
            NexusToken::SingleQuotedString(s) |
            NexusToken::ShortFlag(s) |
            NexusToken::LongFlag(s) => s.as_str(),
            _ => "",
        };
        
        if !token_text.is_empty() {
            // トークン文字列でテキストを検索
            if let Some(found_pos) = text.find(token_text) {
                return Some(found_pos);
            }
        }
        
        // 5. トークン位置序列から概算（最も不正確だが最終手段）
        let preceding_tokens_length: usize = self.tokens
            .iter()
            .take(position)
            .map(|t| t.len())
            .sum();
        
        // 概算位置を返す（テキスト長を超えないように注意）
        Some(preceding_tokens_length.min(text.len().saturating_sub(1)))
    }

    /// エントリタイプの詳細情報
    #[derive(Debug, Clone)]
    enum EntryType {
        Directory,
        File(u64),  // サイズを持つファイル
        Symlink(String), // ターゲットパスを持つシンボリックリンク
        Special(String), // 特殊ファイルタイプ (socket, pipe, device, etc.)
    }

    /// 最適化されたファイルシステムエントリ取得 (キャッシュ付き)
    fn get_optimized_filesystem_entries(&self, path_prefix: &str) -> Vec<(String, EntryType)> {
        use std::fs;
        use std::path::Path;
        use std::time::{Duration, Instant};
        
        static DIR_CACHE: OnceCell<DashMap<String, (Vec<(String, EntryType)>, Instant)>> = OnceCell::new();
        
        // キャッシュの有効期間（500ms）
        const CACHE_TTL: Duration = Duration::from_millis(500);
        
        // キャッシュを初期化
        let cache = DIR_CACHE.get_or_init(|| DashMap::new());
        
        // チルダ展開
        let expanded_path = if path_prefix.starts_with("~") {
            match std::env::var("HOME") {
                Ok(home) => path_prefix.replacen("~", &home, 1),
                Err(_) => path_prefix.to_string(),
            }
        } else {
            path_prefix.to_string()
        };
        
        // 正規化されたパスを使用
        let absolute_path = if Path::new(&expanded_path).is_absolute() {
            expanded_path.clone()
        } else {
            match std::env::current_dir() {
                Ok(current_dir) => {
                    let joined = current_dir.join(&expanded_path);
                    joined.to_string_lossy().to_string()
                },
                Err(_) => expanded_path.clone(),
            }
        };
        
        // 存在するディレクトリ部分を取得
        let dir_path = {
            let path = Path::new(&expanded_path);
            if path.is_dir() {
                path.to_path_buf()
            } else if let Some(parent) = path.parent() {
                parent.to_path_buf()
            } else {
                Path::new(".").to_path_buf()
            }
        };
        
        // ディレクトリパスをキーとしてキャッシュを検索
        let dir_key = dir_path.to_string_lossy().to_string();
        
        // キャッシュチェック
        if let Some(cached) = cache.get(&dir_key) {
            if cached.1.elapsed() < CACHE_TTL {
                // 有効なキャッシュデータが存在する場合、それを返す
                return cached.0.clone();
            }
            // キャッシュが古い場合は削除
            cache.remove(&dir_key);
        }
        
        // キャッシュが無いか古い場合は新しいデータを取得
        let mut result = Vec::new();
        match fs::read_dir(&dir_path) {
            Ok(entries) => {
                for entry_result in entries {
                    if let Ok(entry) = entry_result {
                        let path = entry.path();
                        let file_name = match path.file_name() {
                            Some(name) => name.to_string_lossy().to_string(),
                            None => continue,
                        };
                        
                        let entry_type = match entry.file_type() {
                            Ok(file_type) => {
                                if file_type.is_dir() {
                                    EntryType::Directory
                                } else if file_type.is_symlink() {
                                    // シンボリックリンクの対象を解決
                                    match fs::read_link(&path) {
                                        Ok(target) => EntryType::Symlink(target.to_string_lossy().to_string()),
                                        Err(_) => EntryType::Symlink("unknown".to_string()),
                                    }
                                } else if file_type.is_file() {
                                    // ファイルサイズを取得
                                    match entry.metadata() {
                                        Ok(metadata) => EntryType::File(metadata.len()),
                                        Err(_) => EntryType::File(0),
                                    }
                                } else {
                                    // 特殊ファイル（socket、FIFO、デバイスなど）
                                    EntryType::Special("unknown".to_string())
                                }
                            },
                            Err(_) => continue,
                        };
                        
                        // パスプレフィックスに一致するエントリのみを含める
                        let file_path = path.to_string_lossy().to_string();
                        if file_path.starts_with(&absolute_path) || 
                           expanded_path.is_empty() || 
                           file_name.starts_with(Path::new(&expanded_path).file_name()
                                                .unwrap_or_else(|| std::ffi::OsStr::new(""))
                                                .to_string_lossy().as_ref()) {
                            result.push((file_name, entry_type));
                        }
                    }
                }
            },
            Err(e) => {
                trace!("ディレクトリ '{}' の読み取りに失敗: {}", dir_path.display(), e);
            }
        }
        
        // アルファベット順でディレクトリ優先にソート
        result.sort_by(|a, b| {
            // 1. ディレクトリをファイルより前に
            let a_is_dir = matches!(a.1, EntryType::Directory);
            let b_is_dir = matches!(b.1, EntryType::Directory);
            if a_is_dir != b_is_dir {
                return b_is_dir.cmp(&a_is_dir);
            }
            // 2. 名前でソート（大文字小文字は区別しない）
            a.0.to_lowercase().cmp(&b.0.to_lowercase())
        });
        
        // キャッシュに保存
        cache.insert(dir_key, (result.clone(), Instant::now()));
        
        result
    }

    // 以前の実装は下記に残します（互換性のため）
    fn get_files_from_filesystem(&self, path_prefix: &str) -> Vec<(String, bool)> {
        // 最適化された新しい実装を利用
        self.get_optimized_filesystem_entries(path_prefix)
            .into_iter()
            .map(|(name, entry_type)| {
                let is_dir = matches!(entry_type, EntryType::Directory);
                (name, is_dir)
            })
            .collect()
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

// CommandType enum の定義
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandType {
    Navigation,
    FileSystem,
    FileContents,
    Editor,
    VersionControl,
    Network,
    Process,
    Permission,
    PackageManager,
    Container,
    Output,
    ShellBuiltin,
    // 新しく追加するコマンドタイプ
    TextProcessing,
    Archive,
    Database,
    AI,
    Security,
    DataScience,
    WebDevelopment,
    DevOps,
    Multimedia,
    Monitoring,
    Cryptography,
    Unknown,
}

impl Display for CommandType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::Navigation => write!(f, "ナビゲーション"),
            CommandType::FileSystem => write!(f, "ファイルシステム"),
            CommandType::FileContents => write!(f, "ファイル内容"),
            CommandType::Editor => write!(f, "エディタ"),
            CommandType::VersionControl => write!(f, "バージョン管理"),
            CommandType::Network => write!(f, "ネットワーク"),
            CommandType::Process => write!(f, "プロセス"),
            CommandType::Permission => write!(f, "権限"),
            CommandType::PackageManager => write!(f, "パッケージマネージャ"),
            CommandType::Container => write!(f, "コンテナ・仮想化"),
            CommandType::Output => write!(f, "出力"),
            CommandType::ShellBuiltin => write!(f, "シェル組み込み"),
            CommandType::TextProcessing => write!(f, "テキスト処理"),
            CommandType::Archive => write!(f, "アーカイブ・圧縮"),
            CommandType::Database => write!(f, "データベース"),
            CommandType::AI => write!(f, "AI・機械学習"),
            CommandType::Security => write!(f, "セキュリティ"),
            CommandType::DataScience => write!(f, "データサイエンス"),
            CommandType::WebDevelopment => write!(f, "Web開発"),
            CommandType::DevOps => write!(f, "DevOps"),
            CommandType::Multimedia => write!(f, "マルチメディア"),
            CommandType::Monitoring => write!(f, "監視・ロギング"),
            CommandType::Cryptography => write!(f, "暗号化"),
            CommandType::Unknown => write!(f, "不明"),
        }
    }
}

// CommandContext 構造体の定義
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub command_type: CommandType,
    pub args_count: usize,
    pub flags_count: usize,
    pub redirections: Vec<(NexusToken, usize)>,
    pub related_tokens: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenKind; // TokenKindをインポート

    #[test]
    fn test_tokenize_simple_command() {
        let input = "ls -la /home";
        let mut lexer = NexusLexer::new(input); // newにinputを渡す
        
        let tokens = lexer.tokenize().unwrap(); // 引数を削除
        assert_eq!(tokens.len(), 4); // 3つのトークン + EOF
        
        assert_eq!(tokens[0].kind, NexusToken::Identifier("ls".to_string()));
        // lexemeの比較はTokenKindではなくNexusTokenの内部データで行う
        // assert_eq!(tokens[0].lexeme, "ls"); 
        
        assert_eq!(tokens[1].kind, NexusToken::ShortFlag("la".to_string()));
        // assert_eq!(tokens[1].lexeme, "-la");
        
        assert_eq!(tokens[2].kind, NexusToken::Identifier("/home".to_string()));
        // assert_eq!(tokens[2].lexeme, "/home");
        
        assert_eq!(tokens[3].kind, NexusToken::EOF);
    }

    #[test]
    fn test_tokenize_pipeline() {
        let input = "cat file.txt | grep pattern | wc -l";
        let mut lexer = NexusLexer::new(input);
        
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 8); // 7つのトークン + EOF
        
        assert_eq!(tokens[0].kind, NexusToken::Identifier("cat".to_string()));
        assert_eq!(tokens[1].kind, NexusToken::Identifier("file.txt".to_string()));
        assert_eq!(tokens[2].kind, NexusToken::Pipe);
        assert_eq!(tokens[3].kind, NexusToken::Identifier("grep".to_string()));
        assert_eq!(tokens[4].kind, NexusToken::Identifier("pattern".to_string()));
        assert_eq!(tokens[5].kind, NexusToken::Pipe);
        assert_eq!(tokens[6].kind, NexusToken::Identifier("wc".to_string()));
        assert_eq!(tokens[7].kind, NexusToken::ShortFlag("l".to_string()));
    }

    #[test]
    fn test_tokenize_redirections() {
        let input = "command > output.txt 2>&1";
        let mut lexer = NexusLexer::new(input);
        
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, NexusToken::Identifier("command".to_string()));
        assert_eq!(tokens[1].kind, NexusToken::GreaterThan); // RedirectOut から GreaterThan へ
        assert_eq!(tokens[2].kind, NexusToken::Identifier("output.txt".to_string()));
        
        // 2>&1 の残りのトークン検証 (Error, Ampersand, Integer, EOF)
        assert_eq!(tokens[3].kind, NexusToken::Integer(2)); 
        assert_eq!(tokens[4].kind, NexusToken::Ampersand);
        assert_eq!(tokens[5].kind, NexusToken::Integer(1));
        assert_eq!(tokens[6].kind, NexusToken::EOF);
    }

    #[test]
    fn test_tokenize_strings() {
        let input = r#"echo "Hello, world!" 'single quoted'"#;
        let mut lexer = NexusLexer::new(input);
        
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, NexusToken::Identifier("echo".to_string()));
        assert_eq!(tokens[1].kind, NexusToken::String("Hello, world!".to_string()));
        assert_eq!(tokens[2].kind, NexusToken::SingleQuotedString("single quoted".to_string()));
        assert_eq!(tokens[3].kind, NexusToken::EOF);
    }

    #[test]
    fn test_tokenize_variables() {
        let input = "echo $HOME ${USER}";
        let mut lexer = NexusLexer::new(input);
        
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, NexusToken::Identifier("echo".to_string()));
        assert_eq!(tokens[1].kind, NexusToken::Variable("HOME".to_string()));
        assert_eq!(tokens[2].kind, NexusToken::VariableExpression("USER".to_string()));
        assert_eq!(tokens[3].kind, NexusToken::EOF);
    }

    #[test]
    fn test_tokenize_invalid_token() {
        let input = "echo @invalid";
        let mut lexer = NexusLexer::new(input);
        
        let result = lexer.tokenize();
        assert!(result.is_err());
        
        if let Err(errors) = result {
            assert_eq!(errors.len(), 1); // エラーは1つのはず
            if let ParserError::LexerError(message, span) = &errors[0] {
                 assert!(message.contains("不正なトークン: '@'"));
                 assert_eq!(span.start, 5); // @ の位置
            } else {
                panic!("Expected LexerError variant");
            }
        } else {
            panic!("Expected error result");
        }
    }

    #[test]
    fn test_calculate_position() {
        let lexer = NexusLexer::new("line1\nline2\nline3");
        
        let (line, column) = lexer.get_line_column(0);
        assert_eq!(line, 1);
        assert_eq!(column, 1);
        
        let (line, column) = lexer.get_line_column(6);
        assert_eq!(line, 2);
        assert_eq!(column, 1);
        
        let (line, column) = lexer.get_line_column(8);
        assert_eq!(line, 2);
        assert_eq!(column, 3);
    }

    // get_line メソッドは削除されたためテストも削除
    // #[test]
    // fn test_get_line() {
    //     let lexer = NexusLexer::new("line1\nline2\nline3");
        
    //     assert_eq!(lexer.get_line(1), Some("line1\n"));
    //     assert_eq!(lexer.get_line(2), Some("line2\n"));
    //     assert_eq!(lexer.get_line(3), Some("line3"));
    //     assert_eq!(lexer.get_line(4), None);
    // }

    #[test]
    fn test_unescape_string() {
        assert_eq!(unescape_string(r"Hello\nWorld"), "Hello\nWorld");
        assert_eq!(unescape_string(r"Escaped\\Quote"), "Escaped\\Quote");
        assert_eq!(unescape_string(r"Backslash\\"), "Backslash\\");
    }
} 