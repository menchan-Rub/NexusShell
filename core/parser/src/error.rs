use crate::span::Span;
use std::fmt;
use thiserror::Error;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::span::{SourceFile, SourceSnippet, merge_spans};
use crate::token::{TokenKind, Span};

/// パーサーで発生する可能性のあるエラーの種類
#[derive(Error, Debug, Clone)]
pub enum ParserError {
    #[error("字句解析エラー: {0} at {1}")]
    LexerError(String, Span),
    
    #[error("構文解析エラー: {0} at {1}")]
    SyntaxError(String, Span),
    
    #[error("意味解析エラー: {0} at {1}")]
    SemanticError(String, Span),
    
    #[error("未実装: {0}")]
    NotImplemented(String),
    
    #[error("予期しないトークンです (位置: {span})\n該当箇所: `{source_snippet}`\n期待したトークン: {expected:?} ({expected_type})\n実際のトークン: {actual:?} ({actual_type})")]
    UnexpectedToken {
        expected: String,
        expected_type: String,
        actual: String,
        actual_type: String,
        span: Span,
        source_snippet: String,
    },
    
    #[error("未知のトークン: {0} at {1}")]
    UnknownToken(String, Span),
    
    #[error("無効な文字: {0} at {1}")]
    InvalidCharacter(char, Span),
    
    #[error("無効な数値: {0} at {1}")]
    InvalidNumber(String, Span),
    
    #[error("無効な文字列: {0} at {1}")]
    InvalidString(String, Span),
    
    #[error("無効な識別子: {0} at {1}")]
    InvalidIdentifier(String, Span),
    
    #[error("未定義の変数: {0} at {1}")]
    UndefinedVariable(String, Span),
    
    #[error("無効なパス: {0} at {1}")]
    InvalidPath(String, Span),
    
    #[error("コマンドが見つかりません: {0} at {1}")]
    CommandNotFound(String, Span),
    
    #[error("型エラーが発生しました (位置: {span})\n該当箇所: `{source_snippet}`\n期待した型: {expected} ({expected_type})\n実際の型: {actual} ({actual_type})")]
    TypeError {
        expected: String,
        expected_type: String,
        actual: String,
        actual_type: String,
        span: Span,
        source_snippet: String,
    },
    
    #[error("エラーの連鎖: {0}")]
    ChainedError(Box<ParserError>),
    
    #[error("I/Oエラー: {0}")]
    IoError(String),
    
    #[error("内部エラー: {0}")]
    InternalError(String),
    
    #[error("予期しない入力の終わり: {0}")]
    UnexpectedEOF(String),
}

/// パーサーの結果型
pub type Result<T> = std::result::Result<T, ParserError>;

/// ヘルパーメソッドの実装
impl ParserError {
    /// エラーメッセージを取得
    pub fn message(&self) -> String {
        match self {
            ParserError::LexerError(msg, _) => msg.clone(),
            ParserError::SyntaxError(msg, _) => msg.clone(),
            ParserError::SemanticError(msg, _) => msg.clone(),
            ParserError::NotImplemented(msg) => msg.clone(),
            ParserError::UnexpectedToken { expected, actual, .. } => 
                format!("期待されたトークンは {} でしたが、実際には {} でした。", expected, actual),
            ParserError::UnknownToken(token, _) => format!("未知のトークン: {}", token),
            ParserError::InvalidCharacter(c, _) => format!("無効な文字: {}", c),
            ParserError::InvalidNumber(num, _) => format!("無効な数値: {}", num),
            ParserError::InvalidString(s, _) => format!("無効な文字列: {}", s),
            ParserError::InvalidIdentifier(id, _) => format!("無効な識別子: {}", id),
            ParserError::UndefinedVariable(name, _) => format!("未定義の変数: {}", name),
            ParserError::InvalidPath(path, _) => format!("無効なパス: {}", path),
            ParserError::CommandNotFound(cmd, _) => format!("コマンドが見つかりません: {}", cmd),
            ParserError::TypeError { expected, actual, .. } => format!("型が一致しません。期待された型は {} でしたが、実際の型は {} でした。", expected, actual),
            ParserError::ChainedError(e) => e.message(),
            ParserError::IoError(msg) => msg.clone(),
            ParserError::InternalError(msg) => msg.clone(),
            ParserError::UnexpectedEOF(msg) => msg.clone(),
        }
    }
    
    /// エラーの位置情報を取得
    pub fn span(&self) -> Option<Span> {
        match self {
            ParserError::LexerError(_, span) => Some(*span),
            ParserError::SyntaxError(_, span) => Some(*span),
            ParserError::SemanticError(_, span) => Some(*span),
            ParserError::UnexpectedToken { span, .. } => Some(*span),
            ParserError::UnknownToken(_, span) => Some(*span),
            ParserError::InvalidCharacter(_, span) => Some(*span),
            ParserError::InvalidNumber(_, span) => Some(*span),
            ParserError::InvalidString(_, span) => Some(*span),
            ParserError::InvalidIdentifier(_, span) => Some(*span),
            ParserError::UndefinedVariable(_, span) => Some(*span),
            ParserError::InvalidPath(_, span) => Some(*span),
            ParserError::CommandNotFound(_, span) => Some(*span),
            ParserError::TypeError { span, .. } => Some(*span),
            ParserError::ChainedError(e) => e.span(),
            _ => None,
        }
    }
    
    /// エラーの深刻度を取得
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ParserError::LexerError(_, _) => ErrorSeverity::Error,
            ParserError::SyntaxError(_, _) => ErrorSeverity::Error,
            ParserError::SemanticError(_, _) => ErrorSeverity::Warning,
            ParserError::NotImplemented(_) => ErrorSeverity::Error,
            ParserError::UnexpectedToken { .. } => ErrorSeverity::Error,
            ParserError::UnknownToken(_, _) => ErrorSeverity::Error,
            ParserError::InvalidCharacter(_, _) => ErrorSeverity::Error,
            ParserError::InvalidNumber(_, _) => ErrorSeverity::Error,
            ParserError::InvalidString(_, _) => ErrorSeverity::Error,
            ParserError::InvalidIdentifier(_, _) => ErrorSeverity::Error,
            ParserError::UndefinedVariable(_, _) => ErrorSeverity::Error,
            ParserError::InvalidPath(_, _) => ErrorSeverity::Error,
            ParserError::CommandNotFound(_, _) => ErrorSeverity::Error,
            ParserError::TypeError { .. } => ErrorSeverity::Error,
            ParserError::ChainedError(e) => e.severity(),
            ParserError::IoError(_) => ErrorSeverity::Fatal,
            ParserError::InternalError(_) => ErrorSeverity::Fatal,
            ParserError::UnexpectedEOF(_) => ErrorSeverity::Error,
        }
    }
    
    /// 新しいエラーを連鎖させる
    pub fn chain(self, error: ParserError) -> ParserError {
        ParserError::ChainedError(Box::new(error))
    }
}

/// エラーの深刻度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "情報"),
            ErrorSeverity::Warning => write!(f, "警告"),
            ErrorSeverity::Error => write!(f, "エラー"),
            ErrorSeverity::Fatal => write!(f, "致命的エラー"),
        }
    }
}

/// パーサーエラーの種類を表す列挙型
#[derive(Debug, Clone, PartialEq)]
pub enum ParserErrorKind {
    /// 字句解析エラー
    LexerError {
        /// エラーメッセージ
        message: String,
    },
    
    /// 構文解析エラー
    SyntaxError {
        /// エラーメッセージ
        message: String,
        /// 期待されたトークン（複数可）
        expected: Option<Vec<String>>,
    },
    
    /// 意味解析エラー
    SemanticError {
        /// エラーメッセージ
        message: String,
        /// エラーの種類
        kind: SemanticErrorKind,
    },
    
    /// ファイルI/Oエラー
    IoError {
        /// エラーメッセージ
        message: String,
        /// 操作対象のパス
        path: Option<PathBuf>,
    },
    
    /// その他のエラー
    OtherError {
        /// エラーメッセージ
        message: String,
    },
}

/// セマンティックエラーの種類を表す列挙型
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticErrorKind {
    /// 未定義の変数
    UndefinedVariable {
        /// 変数名
        name: String,
    },
    
    /// 未定義の関数
    UndefinedFunction {
        /// 関数名
        name: String,
    },
    
    /// 変数タイプのミスマッチ
    TypeMismatch {
        /// 期待された型
        expected: String,
        /// 実際の型
        actual: String,
    },
    
    /// コマンドが見つからない
    CommandNotFound {
        /// コマンド名
        name: String,
    },
    
    /// 引数の数が不正
    InvalidArgumentCount {
        /// 期待された引数の数
        expected: Range<usize>,
        /// 実際の引数の数
        actual: usize,
    },
    
    /// 引数の型が不正
    InvalidArgumentType {
        /// 引数のインデックス
        index: usize,
        /// 期待された型
        expected: String,
        /// 実際の型
        actual: String,
    },
    
    /// 未定義の演算子
    UndefinedOperator {
        /// 演算子名
        operator: String,
        /// オペランドの型
        operand_types: Vec<String>,
    },
    
    /// リダイレクトエラー
    RedirectError {
        /// エラーメッセージ
        message: String,
    },
    
    /// 不正なパス
    InvalidPath {
        /// パス
        path: String,
        /// 詳細メッセージ
        details: String,
    },
    
    /// 変数の再定義
    VariableRedefinition {
        /// 変数名
        name: String,
    },
    
    /// 関数の再定義
    FunctionRedefinition {
        /// 関数名
        name: String,
    },
    
    /// その他のセマンティックエラー
    Other {
        /// エラーメッセージ
        message: String,
    },
}

/// パーサーエラーを表す構造体
#[derive(Debug, Clone)]
pub struct ParserError {
    /// エラーの種類
    pub kind: ParserErrorKind,
    /// ソースコード内の位置情報
    pub span: Option<Span>,
    /// ソースファイル情報
    pub source_file: Option<Arc<SourceFile>>,
    /// 追加のエラーノート（説明や提案）
    pub notes: Vec<String>,
    /// 関連するエラー
    pub related: Vec<ParserError>,
}

impl ParserError {
    /// 字句解析エラーを作成
    pub fn lexer_error(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ParserErrorKind::LexerError {
                message: message.into(),
            },
            span: Some(span),
            source_file: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }
    
    /// 構文解析エラーを作成
    pub fn syntax_error(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ParserErrorKind::SyntaxError {
                message: message.into(),
                expected: None,
            },
            span: Some(span),
            source_file: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }
    
    /// 期待されたトークンを含む構文解析エラーを作成
    pub fn syntax_error_with_expected(
        message: impl Into<String>,
        expected: Vec<String>,
        span: Span,
    ) -> Self {
        Self {
            kind: ParserErrorKind::SyntaxError {
                message: message.into(),
                expected: Some(expected),
            },
            span: Some(span),
            source_file: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }
    
    /// 意味解析エラーを作成
    pub fn semantic_error(message: impl Into<String>, kind: SemanticErrorKind, span: Span) -> Self {
        Self {
            kind: ParserErrorKind::SemanticError {
                message: message.into(),
                kind,
            },
            span: Some(span),
            source_file: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }
    
    /// ファイルI/Oエラーを作成
    pub fn io_error(message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self {
            kind: ParserErrorKind::IoError {
                message: message.into(),
                path,
            },
            span: None,
            source_file: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }
    
    /// その他のエラーを作成
    pub fn other_error(message: impl Into<String>) -> Self {
        Self {
            kind: ParserErrorKind::OtherError {
                message: message.into(),
            },
            span: None,
            source_file: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }
    
    /// ソースファイル情報を設定
    pub fn with_source(mut self, source: Arc<SourceFile>) -> Self {
        self.source_file = Some(source);
        self
    }
    
    /// スパン情報を設定
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
    
    /// エラーノートを追加
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
    
    /// 関連エラーを追加
    pub fn with_related(mut self, related: ParserError) -> Self {
        self.related.push(related);
        self
    }
    
    /// 複数の関連エラーを追加
    pub fn with_related_errors(mut self, related: Vec<ParserError>) -> Self {
        self.related.extend(related);
        self
    }
    
    /// エラーメッセージを取得
    pub fn message(&self) -> String {
        match &self.kind {
            ParserErrorKind::LexerError { message } => message.clone(),
            ParserErrorKind::SyntaxError { message, .. } => message.clone(),
            ParserErrorKind::SemanticError { message, .. } => message.clone(),
            ParserErrorKind::IoError { message, .. } => message.clone(),
            ParserErrorKind::OtherError { message } => message.clone(),
        }
    }
    
    /// エラーの短い説明を取得
    pub fn short_description(&self) -> String {
        match &self.kind {
            ParserErrorKind::LexerError { .. } => "字句解析エラー".to_string(),
            ParserErrorKind::SyntaxError { .. } => "構文解析エラー".to_string(),
            ParserErrorKind::SemanticError { kind, .. } => match kind {
                SemanticErrorKind::UndefinedVariable { .. } => "未定義の変数".to_string(),
                SemanticErrorKind::UndefinedFunction { .. } => "未定義の関数".to_string(),
                SemanticErrorKind::TypeMismatch { .. } => "型の不一致".to_string(),
                SemanticErrorKind::CommandNotFound { .. } => "コマンドが見つかりません".to_string(),
                SemanticErrorKind::InvalidArgumentCount { .. } => "引数の数が不正".to_string(),
                SemanticErrorKind::InvalidArgumentType { .. } => "引数の型が不正".to_string(),
                SemanticErrorKind::UndefinedOperator { .. } => "未定義の演算子".to_string(),
                SemanticErrorKind::RedirectError { .. } => "リダイレクトエラー".to_string(),
                SemanticErrorKind::InvalidPath { .. } => "不正なパス".to_string(),
                SemanticErrorKind::VariableRedefinition { .. } => "変数の再定義".to_string(),
                SemanticErrorKind::FunctionRedefinition { .. } => "関数の再定義".to_string(),
                SemanticErrorKind::Other { .. } => "意味解析エラー".to_string(),
            },
            ParserErrorKind::IoError { .. } => "ファイルI/Oエラー".to_string(),
            ParserErrorKind::OtherError { .. } => "エラー".to_string(),
        }
    }
    
    /// エラーの詳細な説明を取得
    pub fn detailed_description(&self) -> String {
        match &self.kind {
            ParserErrorKind::SyntaxError { expected, .. } => {
                if let Some(expected) = expected {
                    if expected.is_empty() {
                        self.message()
                    } else {
                        format!("{}, 期待されるトークン: {}", self.message(), expected.join(", "))
                    }
                } else {
                    self.message()
                }
            }
            ParserErrorKind::SemanticError { kind, .. } => match kind {
                SemanticErrorKind::TypeMismatch { expected, actual } => {
                    format!("{} 期待される型: {}, 実際の型: {}", self.message(), expected, actual)
                }
                SemanticErrorKind::InvalidArgumentCount { expected, actual } => {
                    let expected_str = if expected.start == expected.end - 1 {
                        format!("{}", expected.start)
                    } else if expected.end == usize::MAX {
                        format!("{}以上", expected.start)
                    } else {
                        format!("{}から{}", expected.start, expected.end - 1)
                    };
                    format!("{} 期待される引数の数: {}, 実際の引数の数: {}", self.message(), expected_str, actual)
                }
                SemanticErrorKind::InvalidArgumentType { index, expected, actual } => {
                    format!("{} 引数 #{}: 期待される型: {}, 実際の型: {}", self.message(), index + 1, expected, actual)
                }
                SemanticErrorKind::UndefinedOperator { operator, operand_types } => {
                    format!("{}: 演算子 '{}' は型 ({}) に対して定義されていません", 
                           self.message(), operator, operand_types.join(", "))
                }
                _ => self.message(),
            },
            ParserErrorKind::IoError { path, .. } => {
                if let Some(path) = path {
                    format!("{}: {}", self.message(), path.display())
                } else {
                    self.message()
                }
            }
            _ => self.message(),
        }
    }
    
    /// ソースコードの該当部分を含むエラーメッセージを取得
    pub fn with_source_snippet(&self) -> String {
        let mut result = format!("{}: {}\n", self.short_description(), self.detailed_description());
        
        if let (Some(span), Some(source)) = (&self.span, &self.source_file) {
            let snippet = SourceSnippet::new(source.clone(), *span, 2);
            result.push_str(&snippet.with_context());
        }
        
        for note in &self.notes {
            result.push_str(&format!("Note: {}\n", note));
        }
        
        for related in &self.related {
            result.push_str(&format!("\n関連エラー: {}\n", related));
        }
        
        result
    }
    
    /// エラーをマージ
    pub fn merge(errors: Vec<ParserError>) -> Option<Self> {
        if errors.is_empty() {
            return None;
        }
        
        if errors.len() == 1 {
            return Some(errors[0].clone());
        }
        
        let first = &errors[0];
        let mut merged = Self {
            kind: first.kind.clone(),
            span: first.span,
            source_file: first.source_file.clone(),
            notes: first.notes.clone(),
            related: first.related.clone(),
        };
        
        for error in &errors[1..] {
            if let (Some(span1), Some(span2)) = (merged.span, error.span) {
                merged.span = merge_spans([span1, span2]);
            }
            
            merged.notes.extend(error.notes.clone());
            merged.related.extend(error.related.clone());
        }
        
        Some(merged)
    }
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            // 詳細なフォーマット
            write!(f, "{}", self.with_source_snippet())
        } else {
            // 簡潔なフォーマット
            if let Some(span) = &self.span {
                if let Some(source) = &self.source_file {
                    write!(
                        f,
                        "{}:{}:{}: {}: {}",
                        source.filename(),
                        span.line,
                        span.column,
                        self.short_description(),
                        self.detailed_description()
                    )
                } else {
                    write!(
                        f,
                        "{}:{}: {}: {}",
                        span.line,
                        span.column,
                        self.short_description(),
                        self.detailed_description()
                    )
                }
            } else {
                write!(f, "{}: {}", self.short_description(), self.detailed_description())
            }
        }
    }
}

impl std::error::Error for ParserError {}

/// エラーとなる可能性のある操作を試み、失敗した場合はエラーメッセージを追加
pub fn try_with_error_context<T, E>(
    result: std::result::Result<T, E>,
    context: impl FnOnce() -> String,
    span: Option<Span>,
) -> Result<T>
where
    E: Into<ParserError>,
{
    match result {
        Ok(value) => Ok(value),
        Err(err) => {
            let mut error: ParserError = err.into();
            error.notes.push(context());
            if let Some(span) = span {
                error.span = Some(span);
            }
            Err(error)
        }
    }
}

/// 複数のエラーを収集する機能を提供するヘルパー構造体
#[derive(Debug, Default)]
pub struct ErrorCollector {
    /// 収集したエラー
    pub errors: Vec<ParserError>,
}

impl ErrorCollector {
    /// 新しいエラーコレクターを作成
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
        }
    }
    
    /// エラーを追加
    pub fn add(&mut self, error: ParserError) {
        self.errors.push(error);
    }
    
    /// 結果がエラーの場合、エラーを追加して失敗以外の場合はそのまま返す
    pub fn collect<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.add(err);
                None
            }
        }
    }
    
    /// エラーがあるかどうかをチェック
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    /// 収集したエラーを結合して返す
    pub fn into_result<T>(self, ok_value: T) -> Result<T> {
        if self.errors.is_empty() {
            Ok(ok_value)
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            Err(ParserError::merge(self.errors).unwrap())
        }
    }
    
    /// 収集したエラーをすべて取得
    pub fn take_errors(&mut self) -> Vec<ParserError> {
        std::mem::take(&mut self.errors)
    }
}

/// エラーメッセージのフォーマットオプション
#[derive(Debug, Clone, Default)]
pub struct ErrorFormatOptions {
    /// ソースコードのスニペットを表示するかどうか
    pub show_source: bool,
    /// エラーの詳細情報を表示するかどうか
    pub show_details: bool,
    /// 関連するエラーを表示するかどうか
    pub show_related: bool,
    /// 行番号をハイライトするかどうか
    pub highlight_line_numbers: bool,
    /// エラーマーカー（^）の色
    pub marker_color: Option<String>,
    /// 行番号の色
    pub line_number_color: Option<String>,
    /// エラーメッセージの色
    pub error_message_color: Option<String>,
    /// コンソール出力の最大幅
    pub max_width: Option<usize>,
}

impl ErrorFormatOptions {
    /// デフォルトのフォーマットオプションを作成
    pub fn new() -> Self {
        Self {
            show_source: true,
            show_details: true,
            show_related: true,
            highlight_line_numbers: true,
            marker_color: Some("red".to_string()),
            line_number_color: Some("cyan".to_string()),
            error_message_color: Some("red".to_string()),
            max_width: None,
        }
    }
    
    /// 簡易フォーマットオプションを作成（ソースコードのスニペットなし）
    pub fn simple() -> Self {
        Self {
            show_source: false,
            show_details: true,
            show_related: false,
            highlight_line_numbers: false,
            marker_color: None,
            line_number_color: None,
            error_message_color: None,
            max_width: None,
        }
    }
    
    /// 詳細フォーマットオプションを作成（すべての情報を表示）
    pub fn verbose() -> Self {
        Self {
            show_source: true,
            show_details: true,
            show_related: true,
            highlight_line_numbers: true,
            marker_color: Some("red".to_string()),
            line_number_color: Some("cyan".to_string()),
            error_message_color: Some("red".to_string()),
            max_width: None,
        }
    }
}

/// エラーレポートを生成するためのヘルパー関数
pub fn format_error(error: &ParserError, options: &ErrorFormatOptions) -> String {
    let mut output = String::new();
    
    // エラーヘッダー
    let header = if let Some(source) = &error.source_file {
        if let Some(span) = error.span {
            if options.highlight_line_numbers {
                let line_num = format!("{}:{}:{}", source.filename(), span.line, span.column);
                let line_num = if let Some(color) = &options.line_number_color {
                    format!("\x1b[{}m{}\x1b[0m", color, line_num)
                } else {
                    line_num
                };
                format!("{}: {}", line_num, error.short_description())
            } else {
                format!("{}:{}:{}: {}", source.filename(), span.line, span.column, error.short_description())
            }
        } else {
            format!("{}: {}", source.filename(), error.short_description())
        }
    } else if let Some(span) = error.span {
        format!("{}:{}: {}", span.line, span.column, error.short_description())
    } else {
        error.short_description()
    };
    
    // エラーメッセージに色を付ける
    let header = if let Some(color) = &options.error_message_color {
        format!("\x1b[{}m{}\x1b[0m", color, header)
    } else {
        header
    };
    
    output.push_str(&header);
    output.push_str(": ");
    output.push_str(&error.detailed_description());
    output.push('\n');
    
    // ソースコードのスニペットを表示
    if options.show_source && error.span.is_some() && error.source_file.is_some() {
        let span = error.span.unwrap();
        let source = error.source_file.clone().unwrap();
        let snippet = SourceSnippet::new(source, span, 2);
        
        // マーカーの色を設定
        let marker_color = options.marker_color.clone();
        let formatted_snippet = if let Some(color) = marker_color {
            snippet.with_context_colored(&color)
        } else {
            snippet.with_context()
        };
        
        output.push_str(&formatted_snippet);
        output.push('\n');
    }
    
    // 追加のノートを表示
    if !error.notes.is_empty() {
        for note in &error.notes {
            output.push_str(&format!("注: {}\n", note));
        }
    }
    
    // 関連するエラーを表示
    if options.show_related && !error.related.is_empty() {
        output.push_str("\n関連するエラー:\n");
        for related in &error.related {
            // 再帰的に関連エラーをフォーマット（簡易バージョン）
            let mut related_options = options.clone();
            related_options.show_related = false;  // 無限ループを防ぐ
            let formatted = format_error(related, &related_options);
            // インデントを付けて表示
            for line in formatted.lines() {
                output.push_str(&format!("  {}\n", line));
            }
        }
    }
    
    output
}

/// エラーの要約を取得
pub fn summarize_errors(errors: &[ParserError]) -> String {
    if errors.is_empty() {
        return "エラーはありません".to_string();
    }
    
    let mut output = format!("{}個のエラーが見つかりました:\n", errors.len());
    
    // エラーの種類ごとにカウント
    let mut lexer_errors = 0;
    let mut syntax_errors = 0;
    let mut semantic_errors = 0;
    let mut io_errors = 0;
    let mut other_errors = 0;
    
    for error in errors {
        match &error.kind {
            ParserErrorKind::LexerError { .. } => lexer_errors += 1,
            ParserErrorKind::SyntaxError { .. } => syntax_errors += 1,
            ParserErrorKind::SemanticError { .. } => semantic_errors += 1,
            ParserErrorKind::IoError { .. } => io_errors += 1,
            ParserErrorKind::OtherError { .. } => other_errors += 1,
        }
    }
    
    // エラー種類の要約
    if lexer_errors > 0 {
        output.push_str(&format!("- 字句解析エラー: {}個\n", lexer_errors));
    }
    if syntax_errors > 0 {
        output.push_str(&format!("- 構文解析エラー: {}個\n", syntax_errors));
    }
    if semantic_errors > 0 {
        output.push_str(&format!("- 意味解析エラー: {}個\n", semantic_errors));
    }
    if io_errors > 0 {
        output.push_str(&format!("- ファイルI/Oエラー: {}個\n", io_errors));
    }
    if other_errors > 0 {
        output.push_str(&format!("- その他のエラー: {}個\n", other_errors));
    }
    
    // 最初の5つのエラーを表示
    output.push_str("\n最初の5つのエラー:\n");
    let display_count = std::cmp::min(5, errors.len());
    for (i, error) in errors.iter().take(display_count).enumerate() {
        let options = ErrorFormatOptions::simple();
        let formatted = format_error(error, &options);
        output.push_str(&format!("{}. {}\n", i + 1, formatted.trim()));
    }
    
    if errors.len() > 5 {
        output.push_str(&format!("\n...さらに{}個のエラーがあります。\n", errors.len() - 5));
    }
    
    output
}

/// 既存の古いParserErrorからより詳細な新しいParserError構造体に変換するためのFrom実装
impl From<crate::ParserError> for ParserError {
    fn from(error: crate::ParserError) -> Self {
        match error {
            crate::ParserError::LexerError(msg, span) => {
                ParserError::lexer_error(msg, span)
            },
            crate::ParserError::SyntaxError(msg, span) => {
                ParserError::syntax_error(msg, span)
            },
            crate::ParserError::SemanticError(msg, span) => {
                ParserError::semantic_error(
                    msg.clone(), 
                    SemanticErrorKind::Other { message: msg },
                    span
                )
            },
            crate::ParserError::NotImplemented(msg) => {
                ParserError::other_error(format!("未実装: {}", msg))
            },
            crate::ParserError::UnexpectedToken { expected, actual, span } => {
                ParserError::syntax_error_with_expected(
                    format!("予期しないトークン: '{}'", actual),
                    vec![expected],
                    span
                )
            },
            crate::ParserError::UnknownToken(token, span) => {
                ParserError::lexer_error(format!("未知のトークン: '{}'", token), span)
            },
            crate::ParserError::InvalidCharacter(c, span) => {
                ParserError::lexer_error(format!("無効な文字: '{}'", c), span)
            },
            crate::ParserError::InvalidNumber(num, span) => {
                ParserError::lexer_error(format!("無効な数値: '{}'", num), span)
            },
            crate::ParserError::InvalidString(s, span) => {
                ParserError::lexer_error(format!("無効な文字列: '{}'", s), span)
            },
            crate::ParserError::InvalidIdentifier(id, span) => {
                ParserError::lexer_error(format!("無効な識別子: '{}'", id), span)
            },
            crate::ParserError::UndefinedVariable(name, span) => {
                ParserError::semantic_error(
                    format!("未定義の変数: '{}'", name),
                    SemanticErrorKind::UndefinedVariable { name },
                    span
                )
            },
            crate::ParserError::InvalidPath(path, span) => {
                ParserError::semantic_error(
                    format!("無効なパス: '{}'", path),
                    SemanticErrorKind::InvalidPath { path: path.clone(), details: "パスが存在しないか、アクセスできません".to_string() },
                    span
                )
            },
            crate::ParserError::CommandNotFound(cmd, span) => {
                ParserError::semantic_error(
                    format!("コマンドが見つかりません: '{}'", cmd),
                    SemanticErrorKind::CommandNotFound { name: cmd },
                    span
                )
            },
            crate::ParserError::TypeMismatch(msg, span) => {
                ParserError::semantic_error(
                    format!("型の不一致: {}", msg),
                    SemanticErrorKind::TypeMismatch { expected: "不明".to_string(), actual: "不明".to_string() },
                    span
                )
            },
            crate::ParserError::ChainedError(e) => {
                let inner: ParserError = (*e).into();
                inner
            },
            crate::ParserError::IoError(msg) => {
                ParserError::io_error(msg, None)
            },
            crate::ParserError::InternalError(msg) => {
                ParserError::other_error(format!("内部エラー: {}", msg))
            },
            crate::ParserError::UnexpectedEOF(msg) => {
                let span = Span::default();
                ParserError::syntax_error(format!("予期しない入力の終わり: {}", msg), span)
            },
        }
    }
}

/// 逆向きの変換も実装（バックワードコンパチビリティのため）
impl From<ParserError> for crate::ParserError {
    fn from(error: ParserError) -> Self {
        let span = error.span.unwrap_or_default();
        match error.kind {
            ParserErrorKind::LexerError { message } => {
                crate::ParserError::LexerError(message, span)
            },
            ParserErrorKind::SyntaxError { message, .. } => {
                crate::ParserError::SyntaxError(message, span)
            },
            ParserErrorKind::SemanticError { message, .. } => {
                crate::ParserError::SemanticError(message, span)
            },
            ParserErrorKind::IoError { message, .. } => {
                crate::ParserError::IoError(message)
            },
            ParserErrorKind::OtherError { message } => {
                crate::ParserError::InternalError(message)
            },
        }
    }
}

/// エラー発生時にクリーンアップとともに処理を行うヘルパー関数
pub fn with_cleanup<T, F, C>(f: F, cleanup: C) -> Result<T>
where
    F: FnOnce() -> Result<T>,
    C: FnOnce() -> (),
{
    match f() {
        Ok(val) => {
            cleanup();
            Ok(val)
        }
        Err(e) => {
            cleanup();
            Err(e)
        }
    }
}

/// 複数のエラーのうち、最も重要なものを選択する
pub fn select_most_relevant_error(errors: &[ParserError]) -> Option<ParserError> {
    if errors.is_empty() {
        return None;
    }
    
    // 優先順位: Fatal > Error > Warning > Info
    let error_priority = |err: &ParserError| {
        match err.kind {
            ParserErrorKind::IoError { .. } => 3, // 最高優先度
            ParserErrorKind::LexerError { .. } => 2,
            ParserErrorKind::SyntaxError { .. } => 1,
            ParserErrorKind::SemanticError { .. } => 0,
            ParserErrorKind::OtherError { .. } => 0,
        }
    };
    
    // スパン情報があるエラーを優先
    let span_priority = |err: &ParserError| {
        err.span.is_some() as u8
    };
    
    errors.iter()
        .max_by_key(|err| (error_priority(err), span_priority(err)))
        .cloned()
}

/// エラーを整理して表示するためのヘルパー関数
pub fn organize_and_display_errors(errors: &[ParserError]) -> String {
    // エラーをファイルごとにグループ化
    let mut file_groups: std::collections::HashMap<String, Vec<&ParserError>> = std::collections::HashMap::new();
    
    for error in errors {
        let file_key = if let Some(source) = &error.source_file {
            source.filename().to_string()
        } else {
            "未知のファイル".to_string()
        };
        
        file_groups.entry(file_key).or_default().push(error);
    }
    
    let mut output = String::new();
    output.push_str(&format!("{}個のエラーが見つかりました:\n\n", errors.len()));
    
    // ファイルごとにエラーを表示
    for (file, file_errors) in file_groups {
        output.push_str(&format!("ファイル '{}':\n", file));
        
        // エラーを行番号でソート
        let mut sorted_errors = file_errors.clone();
        sorted_errors.sort_by_key(|err| {
            err.span.map(|s| s.line).unwrap_or(0)
        });
        
        // エラーをフォーマット
        for error in sorted_errors {
            let options = ErrorFormatOptions::verbose();
            let formatted = format_error(error, &options);
            output.push_str(&formatted);
            output.push_str("\n\n");
        }
    }
    
    output
}

/// ソースファイル拡張トレイト
pub trait SourceFileExt {
    /// 指定した行を取得
    fn get_line(&self, line_num: usize) -> Option<&str>;
    
    /// 指定した範囲のテキストを取得
    fn get_text_range(&self, range: std::ops::Range<usize>) -> Option<&str>;
    
    /// エラースパンの周囲のコンテキストを取得
    /// 
    /// 返り値は (行番号, 行の内容, エラー行かどうか) のタプルのベクター
    fn get_context(&self, span: Span, context_lines: usize) -> Vec<(usize, &str, bool)>;
}

impl SourceFileExt for SourceFile {
    fn get_line(&self, line_num: usize) -> Option<&str> {
        if line_num == 0 || line_num > self.content.lines().count() {
            return None;
        }
        
        self.content.lines().nth(line_num - 1)
    }
    
    fn get_text_range(&self, range: std::ops::Range<usize>) -> Option<&str> {
        if range.start >= self.content.len() || range.end > self.content.len() {
            return None;
        }
        
        Some(&self.content[range])
    }
    
    fn get_context(&self, span: Span, context_lines: usize) -> Vec<(usize, &str, bool)> {
        let mut result = Vec::new();
        
        // スパンに行番号がない場合は空のベクターを返す
        if span.line == 0 {
            return result;
        }
        
        // 表示する行の範囲を計算
        let start_line = if span.line <= context_lines {
            1
        } else {
            span.line - context_lines
        };
        
        let end_line = std::cmp::min(
            span.line + context_lines,
            self.content.lines().count()
        );
        
        // 各行を処理
        for line_num in start_line..=end_line {
            if let Some(line) = self.get_line(line_num) {
                let is_error_line = line_num == span.line;
                result.push((line_num, line, is_error_line));
            }
        }
        
        result
    }
}

/// ソースコードのスニペットを表現するためのクラス
#[derive(Debug, Clone)]
pub struct SourceSnippet {
    /// ソースファイル
    source: SourceFile,
    /// エラー位置のスパン
    span: Span,
    /// 表示するコンテキスト行数
    context_lines: usize,
}

impl SourceSnippet {
    /// 新しいソーススニペットを作成
    pub fn new(source: SourceFile, span: Span, context_lines: usize) -> Self {
        Self {
            source,
            span,
            context_lines,
        }
    }
    
    /// スニペットをコンテキスト付きで取得
    pub fn with_context(&self) -> String {
        let contexts = self.source.get_context(self.span, self.context_lines);
        let mut result = String::new();
        
        // 行番号の最大桁数を計算
        let max_line_num_width = contexts.last()
            .map(|(line_num, _, _)| line_num.to_string().len())
            .unwrap_or(1);
        
        for (line_num, line_content, is_error_line) in &contexts {
            // 行番号を右揃えでフォーマット
            let line_num_str = format!("{:>width$}", line_num, width = max_line_num_width);
            
            if *is_error_line {
                // エラー行
                result.push_str(&format!(" {} | {}", line_num_str, line_content));
                
                // エラー位置にマーカーを追加
                if let Some(col) = self.get_marker_position(*line_num) {
                    let marker_indent = " ".repeat(max_line_num_width + 3 + col - 1);
                    let marker_length = self.get_marker_length();
                    let marker = "^".repeat(marker_length);
                    result.push_str(&format!("\n{}{}",marker_indent, marker));
                }
            } else {
                // 通常行
                result.push_str(&format!(" {} | {}", line_num_str, line_content));
            }
            
            // 最後の行以外には改行を追加
            if contexts.last().map(|(num, _, _)| num != line_num).unwrap_or(false) {
                result.push('\n');
            }
        }
        
        result
    }
    
    /// スニペットをカラー付きで取得
    pub fn with_context_colored(&self, marker_color: &str) -> String {
        let contexts = self.source.get_context(self.span, self.context_lines);
        let mut result = String::new();
        
        // 行番号の最大桁数を計算
        let max_line_num_width = contexts.last()
            .map(|(line_num, _, _)| line_num.to_string().len())
            .unwrap_or(1);
        
        for (line_num, line_content, is_error_line) in &contexts {
            // 行番号を右揃えでフォーマット
            let line_num_str = format!("{:>width$}", line_num, width = max_line_num_width);
            
            if *is_error_line {
                // エラー行 (行番号を強調)
                result.push_str(&format!(" \x1b[1;36m{}\x1b[0m | {}", line_num_str, line_content));
                
                // エラー位置にカラー付きマーカーを追加
                if let Some(col) = self.get_marker_position(*line_num) {
                    let marker_indent = " ".repeat(max_line_num_width + 3 + col - 1);
                    let marker_length = self.get_marker_length();
                    let marker = "^".repeat(marker_length);
                    result.push_str(&format!("\n{}\x1b[1;{}m{}\x1b[0m", marker_indent, marker_color, marker));
                }
            } else {
                // 通常行
                result.push_str(&format!(" \x1b[36m{}\x1b[0m | {}", line_num_str, line_content));
            }
            
            // 最後の行以外には改行を追加
            if contexts.last().map(|(num, _, _)| num != line_num).unwrap_or(false) {
                result.push('\n');
            }
        }
        
        result
    }
    
    /// マーカーの位置を計算
    fn get_marker_position(&self, line_num: usize) -> Option<usize> {
        if line_num != self.span.line {
            return None;
        }
        
        Some(self.span.column)
    }
    
    /// マーカーの長さを計算
    fn get_marker_length(&self) -> usize {
        if let Some(length) = self.span.length {
            std::cmp::max(1, length)
        } else {
            1  // デフォルトは1文字
        }
    }
    
    /// スパンのテキストを取得
    pub fn get_span_text(&self) -> Option<&str> {
        if self.span.offset.is_none() || self.span.length.is_none() {
            return None;
        }
        
        let offset = self.span.offset.unwrap();
        let length = self.span.length.unwrap();
        
        self.source.get_text_range(offset..offset + length)
    }
}

/// エラー修正の提案を表す構造体
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    /// 修正の説明
    pub description: String,
    
    /// 置換するテキスト
    pub replacement: String,
}

/// エラーフォーマッタ
pub struct ErrorFormatter<'a> {
    /// フォーマットするエラー
    error: &'a ParserError,
    
    /// エラーの発生したソースコード
    source: &'a str,
    
    /// エラーの発生した行番号と列番号のマッピング
    line_col_map: Vec<(usize, usize)>,
}

impl<'a> ErrorFormatter<'a> {
    /// 新しいErrorFormatterインスタンスを作成
    pub fn new(error: &'a ParserError, source: &'a str) -> Self {
        let line_col_map = Self::build_line_col_map(source);
        
        Self {
            error,
            source,
            line_col_map,
        }
    }
    
    /// ソースコードから行番号と列番号のマッピングを構築
    fn build_line_col_map(source: &str) -> Vec<(usize, usize)> {
        let mut result = Vec::with_capacity(source.len() + 1);
        let mut line = 1;
        let mut col = 1;
        
        result.push((line, col)); // 0バイト目の位置
        
        for c in source.chars() {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            result.push((line, col));
        }
        
        result
    }
    
    /// エラーを整形された文字列として取得
    pub fn format(&self) -> String {
        let mut result = String::new();
        
        // エラーコードとメッセージ
        result.push_str(&format!("{}\n", self.error.colored_message()));
        
        // エラー位置に関する情報を追加
        if let Some(span) = self.error.span() {
            // 行番号と列番号を取得
            if span.start as usize <= self.line_col_map.len() && 
               span.end as usize <= self.line_col_map.len() {
                let (line_start, col_start) = self.line_col_map[span.start as usize];
                let (line_end, col_end) = self.line_col_map[span.end as usize];
                
                // 位置情報を追加
                result.push_str(&format!(
                    " --> line {}, column {} to line {}, column {}\n",
                    line_start, col_start, line_end, col_end
                ));
                
                // 対象行のコードを表示
                if line_start == line_end {
                    self.add_source_line(&mut result, line_start, col_start, col_end);
                } else {
                    // 複数行にまたがる場合は各行を表示
                    for line in line_start..=line_end {
                        if line == line_start {
                            self.add_source_line(&mut result, line, col_start, usize::MAX);
                        } else if line == line_end {
                            self.add_source_line(&mut result, line, 1, col_end);
                        } else {
                            self.add_source_line(&mut result, line, 1, usize::MAX);
                        }
                    }
                }
            }
        }
        
        // ドキュメントリンクを追加
        if let Some(link) = self.error.doc_link() {
            result.push_str(&format!("\n詳細なドキュメント: {}\n", link));
        }
        
        // 修正候補を追加
        let suggestions = self.error.fix_suggestions();
        if !suggestions.is_empty() {
            result.push_str("\n修正候補:\n");
            
            for (i, suggestion) in suggestions.iter().enumerate() {
                result.push_str(&format!("  {}. {}\n", i + 1, suggestion.description));
            }
        }
        
        result
    }
    
    /// ソースコードの指定行を結果に追加
    fn add_source_line(&self, result: &mut String, line: usize, col_start: usize, col_end: usize) {
        let line_content = self.get_line_content(line);
        
        // 行番号と内容を追加
        result.push_str(&format!("{:4} | {}\n", line, line_content));
        
        // エラー位置を示すマーカーを追加
        let mut marker = String::from("     | ");
        for i in 1..col_start {
            marker.push(' ');
        }
        
        let marker_length = std::cmp::min(
            col_end.saturating_sub(col_start).max(1),
            line_content.len().saturating_sub(col_start - 1)
        );
        
        for _ in 0..marker_length {
            marker.push('^');
        }
        
        marker.push_str(" ここ");
        result.push_str(&format!("{}\n", marker));
    }
    
    /// 指定行の内容を取得
    fn get_line_content(&self, line: usize) -> String {
        let mut start_idx = 0;
        let mut end_idx = self.source.len();
        let mut current_line = 1;
        
        // 行の開始位置を検索
        for (i, c) in self.source.char_indices() {
            if current_line == line {
                start_idx = i;
                break;
            }
            if c == '\n' {
                current_line += 1;
            }
        }
        
        // 行の終了位置を検索
        current_line = line;
        for (i, c) in self.source[start_idx..].char_indices() {
            if c == '\n' {
                end_idx = start_idx + i;
                break;
            }
        }
        
        self.source[start_idx..end_idx].to_string()
    }
}

/// エラーコレクション（複数のエラーを管理）
#[derive(Debug, Default)]
pub struct ErrorCollection {
    /// エラーのリスト
    errors: Vec<ParserError>,
}

impl ErrorCollection {
    /// 新しいErrorCollectionインスタンスを作成
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
        }
    }
    
    /// エラーを追加
    pub fn add(&mut self, error: ParserError) {
        self.errors.push(error);
    }
    
    /// エラーのリストを取得
    pub fn errors(&self) -> &[ParserError] {
        &self.errors
    }
    
    /// エラーの数を取得
    pub fn count(&self) -> usize {
        self.errors.len()
    }
    
    /// 深刻度に基づいてエラーを取得
    pub fn by_severity(&self, severity: ErrorSeverity) -> Vec<&ParserError> {
        self.errors.iter()
            .filter(|&err| err.severity() == severity)
            .collect()
    }
    
    /// エラーの有無を判定
    pub fn has_errors(&self) -> bool {
        self.errors.iter().any(|err| matches!(err.severity(), ErrorSeverity::Error | ErrorSeverity::Fatal))
    }
    
    /// 警告の有無を判定
    pub fn has_warnings(&self) -> bool {
        self.errors.iter().any(|err| err.severity() == ErrorSeverity::Warning)
    }
    
    /// 致命的エラーの有無を判定
    pub fn has_fatal_errors(&self) -> bool {
        self.errors.iter().any(|err| err.severity() == ErrorSeverity::Fatal)
    }
    
    /// エラーコレクションを整形された文字列として取得
    pub fn format(&self, source: &str) -> String {
        let mut result = String::new();
        
        if self.errors.is_empty() {
            return "エラーはありません。".to_string();
        }
        
        // エラーの数を表示
        let error_count = self.by_severity(ErrorSeverity::Error).len()
            + self.by_severity(ErrorSeverity::Fatal).len();
        let warning_count = self.by_severity(ErrorSeverity::Warning).len();
        let hint_count = self.by_severity(ErrorSeverity::Hint).len()
            + self.by_severity(ErrorSeverity::Info).len();
        
        result.push_str(&format!(
            "解析結果: {}エラー, {}警告, {}ヒント\n\n",
            error_count, warning_count, hint_count
        ));
        
        // エラーを深刻度順にソート
        let mut sorted_errors = self.errors.clone();
        sorted_errors.sort_by(|a, b| b.severity().cmp(&a.severity()));
        
        // 各エラーを整形して追加
        for error in sorted_errors {
            let formatter = ErrorFormatter::new(&error, source);
            result.push_str(&formatter.format());
            result.push('\n');
        }
        
        result
    }
    
    /// エラーをクリア
    pub fn clear(&mut self) {
        self.errors.clear();
    }
    
    /// エラーコレクションをマージ
    pub fn merge(&mut self, other: ErrorCollection) {
        self.errors.extend(other.errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_message() {
        let error = ParserError::UnknownCommand {
            span: Span::new(0, 10),
            command: "unknown".to_string(),
        };
        
        assert_eq!(error.message(), "不明なコマンド: 'unknown'");
    }
    
    #[test]
    fn test_error_severity() {
        let error1 = ParserError::OptimizationHint {
            span: Span::new(0, 10),
            message: "最適化のヒント".to_string(),
        };
        assert_eq!(error1.severity(), ErrorSeverity::Hint);
        
        let error2 = ParserError::UnusedVariable {
            span: Span::new(0, 10),
            name: "VAR".to_string(),
        };
        assert_eq!(error2.severity(), ErrorSeverity::Warning);
        
        let error3 = ParserError::SyntaxError {
            span: Span::new(0, 10),
            expected: Some(TokenKind::Identifier),
            found: Some(TokenKind::Integer),
            message: "構文エラー".to_string(),
        };
        assert_eq!(error3.severity(), ErrorSeverity::Error);
    }
    
    #[test]
    fn test_error_collection() {
        let mut collection = ErrorCollection::new();
        
        collection.add(ParserError::UnknownCommand {
            span: Span::new(0, 10),
            command: "unknown".to_string(),
        });
        
        collection.add(ParserError::UnusedVariable {
            span: Span::new(12, 15),
            name: "VAR".to_string(),
        });
        
        assert_eq!(collection.count(), 2);
        assert!(collection.has_errors());
        assert!(collection.has_warnings());
        assert!(!collection.has_fatal_errors());
        
        let warnings = collection.by_severity(ErrorSeverity::Warning);
        assert_eq!(warnings.len(), 1);
    }
    
    #[test]
    fn test_string_similarity() {
        assert!(ParserError::string_similarity("hello", "jello") > 0.7);
        assert!(ParserError::string_similarity("cd", "ls") < 0.5);
        assert_eq!(ParserError::string_similarity("test", "test"), 1.0);
        assert_eq!(ParserError::string_similarity("", ""), 1.0);
        assert_eq!(ParserError::string_similarity("a", ""), 0.0);
    }
    
    #[test]
    fn test_fix_suggestions() {
        let error = ParserError::UnclosedQuote {
            span: Span::new(0, 10),
            quote_type: '"',
        };
        
        let suggestions = error.fix_suggestions();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].replacement, "\"");
    }
    
    #[test]
    fn test_error_formatter() {
        let source = "echo Hello World\nunknown_cmd arg1 arg2\necho Done";
        
        let error = ParserError::UnknownCommand {
            span: Span::new(18, 29),
            command: "unknown_cmd".to_string(),
        };
        
        let formatter = ErrorFormatter::new(&error, source);
        let formatted = formatter.format();
        
        assert!(formatted.contains("不明なコマンド: 'unknown_cmd'"));
        assert!(formatted.contains("line 2"));
    }
} 