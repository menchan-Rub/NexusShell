//! # NexusShell Parser
//!
//! NexusShellのコマンド解析エンジン。
//! - 字句解析 (Lexer)
//! - 構文解析 (Parser)
//! - 意味解析 (Semantic Analyzer)
//! - 補完エンジン (Completion Engine)
//! を実装しています。

use std::fmt;
use thiserror::Error;

/// パーサーモジュール
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod completion;
pub mod grammar;
pub mod ast;
pub mod token;
pub mod error;
pub mod span;
pub mod context;
pub mod env_resolver;
pub mod interpreter;
pub mod tests;
pub mod completer;
pub mod predictor;
pub mod plugin;
pub mod metrics;
pub mod error_recovery;
pub mod type_system;

/// 位置情報
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Default for Span {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}-{}", self.line, self.column, self.column + (self.end - self.start))
    }
}

/// エラー型
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
    
    #[error("予期しないトークン: 期待={expected:?}, 実際={actual:?} at {span}")]
    UnexpectedToken {
        expected: String,
        actual: String,
        span: Span,
    },
    
    #[error("予期されるトークンが見つかりません: 期待={expected:?}, 実際={found:?} at {span}")]
    ExpectedToken {
        expected: TokenKind,
        found: TokenKind,
        span: Span,
    },
    
    #[error("複数のトークンのいずれかが予期されます: 期待={expected:?}, 実際={found:?} at {span}")]
    ExpectedOneOf {
        expected: Vec<TokenKind>,
        found: TokenKind,
        span: Span,
    },
    
    #[error("対応するデリミタが一致しません: 開始={opening:?}, 期待する終了={expected_closing:?}, 実際={found:?} at {span}")]
    MismatchedDelimiter {
        opening: TokenKind,
        expected_closing: TokenKind,
        found: Option<TokenKind>,
        span: Span,
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
    
    #[error("型の不一致: {0} at {1}")]
    TypeMismatch(String, Span),
    
    #[error("型システムエラー: {0} at {1}")]
    TypeError(String, Span),
    
    #[error("エラーの連鎖: {0}")]
    ChainedError(Box<ParserError>),
    
    #[error("I/Oエラー: {0}")]
    IoError(String),
    
    #[error("内部エラー: {0}")]
    InternalError(String),
    
    #[error("予期しないEOF: {0}")]
    UnexpectedEOF(String),
    
    #[error("プラグインエラー: {0}")]
    PluginError(String),
    
    #[error("補完エラー: {0}")]
    CompletionError(String),
    
    #[error("予測エラー: {0}")]
    PredictionError(String),
    
    #[error("メトリクスエラー: {0}")]
    MetricsError(String),
    
    #[error("エラー回復失敗: {0}")]
    RecoveryError(String),
    
    #[error("型検証エラー: {0}")]
    ValidationError(String),
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

/// 結果型のエイリアス
pub type Result<T> = std::result::Result<T, ParserError>;

/// トークンの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // キーワード
    Command,
    Argument,
    Option,
    Flag,
    Variable,
    String,
    Integer,
    Float,
    Boolean,
    
    // 演算子
    Pipe,              // |
    PipeTyped,         // |>
    PipeConditional,   // |?
    PipeParallel,      // ||
    PipeError,         // |!
    RedirectOut,       // >
    RedirectAppend,    // >>
    RedirectIn,        // <
    RedirectMerge,     // &>
    
    // 区切り文字
    Semicolon,         // ;
    Ampersand,         // &
    LeftBrace,         // {
    RightBrace,        // }
    LeftBracket,       // [
    RightBracket,      // ]
    LeftParen,         // (
    RightParen,        // )
    Comma,             // ,
    Dot,               // .
    Colon,             // :
    
    // その他
    Whitespace,
    Comment,
    Unknown,
    Eof,
}

/// トークン
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: String, span: Span) -> Self {
        Self { kind, lexeme, span }
    }
}

/// 構文解析の結果生成される抽象構文木 (AST) のノード
#[derive(Debug, Clone)]
pub enum AstNode {
    Command {
        name: String,
        arguments: Vec<AstNode>,
        redirections: Vec<AstNode>,
        span: Span,
    },
    Argument {
        value: String,
        span: Span,
    },
    Option {
        name: String,
        value: Option<Box<AstNode>>,
        span: Span,
    },
    Pipeline {
        commands: Vec<AstNode>,
        kind: PipelineKind,
        span: Span,
    },
    Redirection {
        kind: RedirectionKind,
        target: Box<AstNode>,
        span: Span,
    },
    Block {
        commands: Vec<AstNode>,
        span: Span,
    },
    VariableAssignment {
        name: String,
        value: Box<AstNode>,
        export: bool,
        span: Span,
    },
    VariableReference {
        name: String,
        default_value: Option<Box<AstNode>>,
        span: Span,
    },
    Subshell {
        command: Box<AstNode>,
        span: Span,
    },
    Conditional {
        condition: Box<AstNode>,
        then_branch: Box<AstNode>,
        else_branch: Option<Box<AstNode>>,
        span: Span,
    },
    Loop {
        kind: LoopKind,
        initializer: Option<Box<AstNode>>,
        condition: Box<AstNode>,
        increment: Option<Box<AstNode>>,
        body: Box<AstNode>,
        span: Span,
    },
    FunctionDefinition {
        name: String,
        parameters: Vec<String>,
        body: Box<AstNode>,
        span: Span,
    },
    Alias {
        name: String,
        value: String,
        span: Span,
    },
    ArrayLiteral {
        elements: Vec<AstNode>,
        span: Span,
    },
    MapLiteral {
        entries: Vec<(String, AstNode)>,
        span: Span,
    },
    PathExpansion {
        pattern: String,
        span: Span,
    },
    Background {
        command: Box<AstNode>,
        span: Span,
    },
    Group {
        commands: Vec<AstNode>,
        span: Span,
    },
    Error {
        message: String,
        span: Span,
    },
    Terminal {
        token_kind: TokenKind,
        lexeme: String,
        span: Span,
    },
    NonTerminal {
        name: String,
        children: Vec<AstNode>,
        span: Span,
    },
    Sequence {
        children: Vec<AstNode>,
        span: Span,
    },
    Choice {
        value: Box<AstNode>,
        span: Span,
    },
    Repetition {
        children: Vec<AstNode>,
        span: Span,
    },
    Optional {
        value: Option<Box<AstNode>>,
        span: Span,
    },
    Literal {
        value: String,
        kind: TokenKind,
        span: Span,
    },
    Variable {
        name: Box<AstNode>,
        span: Span,
    },
    Assignment {
        left: Box<AstNode>,
        right: Box<AstNode>,
        span: Span,
    },
    Program {
        statements: Vec<AstNode>,
        span: Span,
    },
    Empty {
        span: Span,
    },
}

impl AstNode {
    /// ノードのスパン情報を返す
    pub fn span(&self) -> &Span {
        match self {
            AstNode::Command { span, .. } => span,
            AstNode::Argument { span, .. } => span,
            AstNode::Option { span, .. } => span,
            AstNode::Pipeline { span, .. } => span,
            AstNode::Redirection { span, .. } => span,
            AstNode::Block { span, .. } => span,
            AstNode::VariableAssignment { span, .. } => span,
            AstNode::VariableReference { span, .. } => span,
            AstNode::Subshell { span, .. } => span,
            AstNode::Conditional { span, .. } => span,
            AstNode::Loop { span, .. } => span,
            AstNode::FunctionDefinition { span, .. } => span,
            AstNode::Alias { span, .. } => span,
            AstNode::ArrayLiteral { span, .. } => span,
            AstNode::MapLiteral { span, .. } => span,
            AstNode::PathExpansion { span, .. } => span,
            AstNode::Background { span, .. } => span,
            AstNode::Group { span, .. } => span,
            AstNode::Error { span, .. } => span,
            AstNode::Terminal { span, .. } => span,
            AstNode::NonTerminal { span, .. } => span,
            AstNode::Sequence { span, .. } => span,
            AstNode::Choice { span, .. } => span,
            AstNode::Repetition { span, .. } => span,
            AstNode::Optional { span, .. } => span,
            AstNode::Literal { span, .. } => span,
            AstNode::Variable { span, .. } => span,
            AstNode::Assignment { span, .. } => span,
            AstNode::Program { span, .. } => span,
            AstNode::Empty { span } => span,
        }
    }
}

/// パイプラインの種類
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineKind {
    Standard,    // 標準的なパイプ "|"
    StdErr,      // 標準エラー出力のパイプ "|&"
    Background,  // バックグラウンド実行 "&"
    Conditional, // 条件付きパイプ "&&" または "||"
    Process,     // プロセス置換 ">(" または "<("
}

/// リダイレクションの種類
#[derive(Debug, Clone, PartialEq)]
pub enum RedirectionKind {
    StdoutOverwrite,   // 標準出力を上書き ">"
    StdoutAppend,      // 標準出力に追記 ">>"
    StderrOverwrite,   // 標準エラー出力を上書き "2>"
    StderrAppend,      // 標準エラー出力に追記 "2>>"
    StdinFrom,         // 標準入力を読み込み "<"
    StdinHeredoc,      // ヒアドキュメント "<<"
    StdinHerestring,   // ヒアストリング "<<<"
    StdoutAndStderrOverwrite, // 標準出力と標準エラー出力を上書き "&>"
    StdoutAndStderrAppend,    // 標準出力と標準エラー出力に追記 "&>>"
    FileDescriptor,    // ファイル記述子 "[n]>&m"
    Close,             // ファイル記述子をクローズ "[n]>&-"
    OutputToInput,     // コマンド出力を別コマンドの入力に "<>"
}

/// ループの種類
#[derive(Debug, Clone, PartialEq)]
pub enum LoopKind {
    For,     // forループ
    While,   // whileループ
    Until,   // untilループ
    Foreach, // foreachループ
}

/// パーサーの状態を管理するコンテキスト
#[derive(Debug, Clone)]
pub struct ParserContext {
    pub source: String,
    pub tokens: Vec<Token>,
    pub current: usize,
    pub errors: Vec<ParserError>,
}

impl ParserContext {
    pub fn new(source: String) -> Self {
        Self {
            source,
            tokens: Vec::new(),
            current: 0,
            errors: Vec::new(),
        }
    }
    
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// パーサーの特性
pub trait Parser {
    fn parse(&mut self, input: &str) -> Result<AstNode>;
}

/// 字句解析器の特性
pub trait Lexer {
    fn tokenize(&mut self, input: &str) -> Result<Vec<Token>>;
}

/// 入力文字列を解析してAST（抽象構文木）を生成する
///
/// # 引数
/// * `input` - 解析する入力文字列
///
/// # 戻り値
/// * `Result<AstNode>` - 解析成功時はASTのルートノード、失敗時はエラー情報
///
/// # 例
/// ```
/// let ast = parse("ls -la | grep .rs");
/// ```
pub fn parse(input: &str) -> Result<AstNode> {
    let mut lexer = DefaultLexer::new();
    let mut parser = DefaultParser::new();
    
    // 字句解析を実行
    let tokens = lexer.tokenize(input)?;
    
    // 構文解析を実行
    let mut context = ParserContext::new(input.to_string());
    context.tokens = tokens;
    
    // パーサーにコンテキストを渡して解析を実行
    parser.parse_with_context(&mut context)
}

/// デフォルトの構文解析器
#[derive(Debug, Default)]
pub struct DefaultParser {
    // パーサーの内部状態
    current_scope: Vec<String>,
    error_recovery: bool,
}

impl DefaultParser {
    /// 新しいパーサーインスタンスを作成
    pub fn new() -> Self {
        Self {
            current_scope: Vec::new(),
            error_recovery: true,
        }
    }
    
    /// コンテキストを使用して解析を実行
    pub fn parse_with_context(&mut self, context: &mut ParserContext) -> Result<AstNode> {
        // コマンドラインの解析を開始
        let mut commands = Vec::new();
        
        while context.current < context.tokens.len() {
            // コマンドを解析
            match self.parse_command(context) {
                Ok(cmd) => commands.push(cmd),
                Err(e) => {
                    // エラー回復が有効な場合は続行、そうでなければ即座に失敗
                    context.errors.push(e.clone());
                    if !self.error_recovery {
                        return Err(e);
                    }
                    // エラー回復：次のセミコロンまたは改行までスキップ
                    self.recover_from_error(context);
                }
            }
            
            // コマンド区切り（セミコロンや改行）をスキップ
            self.skip_command_separators(context);
        }
        
        // 解析結果をルートノードとして返す
        if commands.is_empty() {
            // 空の入力の場合は空のブロックを返す
            Ok(AstNode::Block { commands: Vec::new() })
        } else {
            Ok(AstNode::Block { commands })
        }
    }
    
    // コマンドを解析
    fn parse_command(&mut self, context: &mut ParserContext) -> Result<AstNode> {
        // 現在のトークンを取得
        let token = self.peek_token(context)?;
        
        // トークンの種類に応じて適切な解析を行う
        match token.kind {
            TokenKind::Word => self.parse_simple_command(context),
            TokenKind::LeftParen => self.parse_subshell(context),
            TokenKind::LeftBrace => self.parse_block(context),
            TokenKind::If => self.parse_if_statement(context),
            TokenKind::For => self.parse_for_loop(context),
            TokenKind::While => self.parse_while_loop(context),
            _ => Err(ParserError::UnexpectedToken(format!(
                "コマンドの開始として予期しないトークン: {:?}", token
            ))),
        }
    }
    
    // 単純なコマンドを解析
    fn parse_simple_command(&mut self, context: &mut ParserContext) -> Result<AstNode> {
        let mut args = Vec::new();
        let mut redirections = Vec::new();
        
        // コマンド名を取得
        let command_name = self.consume_token(context)?.value;
        args.push(command_name);
        
        // 引数とリダイレクションを解析
        while self.has_more_tokens(context) {
            let token = self.peek_token(context)?;
            
            match token.kind {
                // パイプやセミコロンなどの区切り文字が来たら終了
                TokenKind::Pipe | TokenKind::Semicolon | TokenKind::Newline |
                TokenKind::RightParen | TokenKind::RightBrace => break,
                
                // リダイレクション演算子
                TokenKind::GreaterThan | TokenKind::LessThan |
                TokenKind::GreaterGreater | TokenKind::AndGreater => {
                    redirections.push(self.parse_redirection(context)?);
                },
                
                // それ以外は引数として扱う
                _ => {
                    args.push(self.consume_token(context)?.value);
                }
            }
        }
        
        // パイプラインの解析
        if self.check_token_kind(context, TokenKind::Pipe) {
            self.consume_token(context)?; // パイプトークンを消費
            
            // 右側のコマンドを解析
            let right_command = self.parse_command(context)?;
            
            // パイプラインノードを作成
            return Ok(AstNode::Pipeline {
                left: Box::new(AstNode::Command { args, redirections }),
                right: Box::new(right_command),
                kind: PipelineKind::Standard,
            });
        }
        
        // 単純なコマンドノードを返す
        Ok(AstNode::Command { args, redirections })
    }
    
    // エラーから回復する
    fn recover_from_error(&mut self, context: &mut ParserContext) {
        // 次のセミコロンまたは改行までスキップ
        while self.has_more_tokens(context) {
            let token = self.peek_token(context).unwrap_or_default();
            if token.kind == TokenKind::Semicolon || token.kind == TokenKind::Newline {
                self.consume_token(context).unwrap_or_default();
                break;
            }
            self.consume_token(context).unwrap_or_default();
        }
    }
    
    // コマンド区切りをスキップ
    fn skip_command_separators(&mut self, context: &mut ParserContext) {
        while self.has_more_tokens(context) {
            let token = self.peek_token(context).unwrap_or_default();
            if token.kind != TokenKind::Semicolon && token.kind != TokenKind::Newline {
                break;
            }
            self.consume_token(context).unwrap_or_default();
        }
    }
    
    // リダイレクションを解析
    fn parse_redirection(&mut self, context: &mut ParserContext) -> Result<Redirection> {
        let operator = self.consume_token(context)?;
        let kind = match operator.kind {
            TokenKind::GreaterThan => RedirectionKind::Output,
            TokenKind::GreaterGreater => RedirectionKind::Append,
            TokenKind::LessThan => RedirectionKind::Input,
            TokenKind::AndGreater => RedirectionKind::Merge,
            _ => return Err(ParserError::UnexpectedToken(
                format!("リダイレクション演算子として無効なトークン: {:?}", operator)
            )),
        };
        
        // ファイル名を取得
        let target = self.consume_token(context)?;
        if target.kind != TokenKind::Word {
            return Err(ParserError::UnexpectedToken(
                format!("リダイレクション先として無効なトークン: {:?}", target)
            ));
        }
        
        Ok(Redirection {
            kind,
            target: target.value,
        })
    }
    
    // 現在のトークンを取得（消費しない）
    fn peek_token(&self, context: &ParserContext) -> Result<Token> {
        if context.current >= context.tokens.len() {
            return Err(ParserError::UnexpectedEOF("予期しない入力の終わり".to_string()));
        }
        Ok(context.tokens[context.current].clone())
    }
    
    // 現在のトークンを消費して返す
    fn consume_token(&mut self, context: &mut ParserContext) -> Result<Token> {
        if context.current >= context.tokens.len() {
            return Err(ParserError::UnexpectedEOF("予期しない入力の終わり".to_string()));
        }
        let token = context.tokens[context.current].clone();
        context.current += 1;
        Ok(token)
    }
    
    // 特定の種類のトークンかどうかをチェック
    fn check_token_kind(&self, context: &ParserContext, kind: TokenKind) -> bool {
        if context.current >= context.tokens.len() {
            return false;
        }
        context.tokens[context.current].kind == kind
    }
    
    // まだトークンが残っているかチェック
    fn has_more_tokens(&self, context: &ParserContext) -> bool {
        context.current < context.tokens.len()
    }
    
    // サブシェル、ブロック、if文、ループなどの解析メソッドは省略
    fn parse_subshell(&mut self, _context: &mut ParserContext) -> Result<AstNode> {
        // 実装は省略
        Err(ParserError::NotImplemented("サブシェルの解析はまだ実装されていません".to_string()))
    }
    
    fn parse_block(&mut self, _context: &mut ParserContext) -> Result<AstNode> {
        // 実装は省略
        Err(ParserError::NotImplemented("ブロックの解析はまだ実装されていません".to_string()))
    }
    
    fn parse_if_statement(&mut self, _context: &mut ParserContext) -> Result<AstNode> {
        // 実装は省略
        Err(ParserError::NotImplemented("if文の解析はまだ実装されていません".to_string()))
    }
    
    fn parse_for_loop(&mut self, _context: &mut ParserContext) -> Result<AstNode> {
        // 実装は省略
        Err(ParserError::NotImplemented("forループの解析はまだ実装されていません".to_string()))
    }
    
    fn parse_while_loop(&mut self, _context: &mut ParserContext) -> Result<AstNode> {
        // 実装は省略
        Err(ParserError::NotImplemented("whileループの解析はまだ実装されていません".to_string()))
    }
}

/// デフォルトの字句解析器
#[derive(Debug, Default)]
pub struct DefaultLexer {
    // 字句解析器の内部状態
}

impl DefaultLexer {
    /// 新しい字句解析器インスタンスを作成
    pub fn new() -> Self {
        Self {}
    }
}

impl Lexer for DefaultLexer {
    fn tokenize(&mut self, input: &str) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let mut chars = input.chars().peekable();
        let mut position = 0;
        
        while let Some(&c) = chars.peek() {
            match c {
                // 空白文字をスキップ
                ' ' | '\t' => {
                    chars.next();
                    position += 1;
                },
                
                // 改行
                '\n' => {
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        value: "\n".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                // セミコロン
                ';' => {
                    tokens.push(Token {
                        kind: TokenKind::Semicolon,
                        value: ";".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                // パイプ
                '|' => {
                    chars.next();
                    position += 1;
                    
                    // パイプの種類を判定
                    if let Some(&next) = chars.peek() {
                        match next {
                            '>' => {
                                tokens.push(Token {
                                    kind: TokenKind::Pipe,
                                    value: "|>".to_string(),
                                    position: position - 1,
                                });
                                chars.next();
                                position += 1;
                            },
                            '?' => {
                                tokens.push(Token {
                                    kind: TokenKind::Pipe,
                                    value: "|?".to_string(),
                                    position: position - 1,
                                });
                                chars.next();
                                position += 1;
                            },
                            '|' => {
                                tokens.push(Token {
                                    kind: TokenKind::Pipe,
                                    value: "||".to_string(),
                                    position: position - 1,
                                });
                                chars.next();
                                position += 1;
                            },
                            '!' => {
                                tokens.push(Token {
                                    kind: TokenKind::Pipe,
                                    value: "|!".to_string(),
                                    position: position - 1,
                                });
                                chars.next();
                                position += 1;
                            },
                            _ => {
                                tokens.push(Token {
                                    kind: TokenKind::Pipe,
                                    value: "|".to_string(),
                                    position: position - 1,
                                });
                            }
                        }
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Pipe,
                            value: "|".to_string(),
                            position: position - 1,
                        });
                    }
                },
                
                // リダイレクション
                '>' => {
                    chars.next();
                    position += 1;
                    
                    if let Some(&next) = chars.peek() {
                        if next == '>' {
                            tokens.push(Token {
                                kind: TokenKind::GreaterGreater,
                                value: ">>".to_string(),
                                position: position - 1,
                            });
                            chars.next();
                            position += 1;
                        } else {
                            tokens.push(Token {
                                kind: TokenKind::GreaterThan,
                                value: ">".to_string(),
                                position: position - 1,
                            });
                        }
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::GreaterThan,
                            value: ">".to_string(),
                            position: position - 1,
                        });
                    }
                },
                
                '<' => {
                    tokens.push(Token {
                        kind: TokenKind::LessThan,
                        value: "<".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                '&' => {
                    chars.next();
                    position += 1;
                    
                    if let Some(&next) = chars.peek() {
                        if next == '>' {
                            tokens.push(Token {
                                kind: TokenKind::AndGreater,
                                value: "&>".to_string(),
                                position: position - 1,
                            });
                            chars.next();
                            position += 1;
                        } else {
                            tokens.push(Token {
                                kind: TokenKind::Word,
                                value: "&".to_string(),
                                position: position - 1,
                            });
                        }
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Word,
                            value: "&".to_string(),
                            position: position - 1,
                        });
                    }
                },
                
                // 括弧
                '(' => {
                    tokens.push(Token {
                        kind: TokenKind::LeftParen,
                        value: "(".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                ')' => {
                    tokens.push(Token {
                        kind: TokenKind::RightParen,
                        value: ")".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                '{' => {
                    tokens.push(Token {
                        kind: TokenKind::LeftBrace,
                        value: "{".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                '}' => {
                    tokens.push(Token {
                        kind: TokenKind::RightBrace,
                        value: "}".to_string(),
                        position,
                    });
                    chars.next();
                    position += 1;
                },
                
                // 引用符
                '"' => {
                    let (token, len) = self.tokenize_double_quoted_string(&mut chars, position);
                    tokens.push(token);
                    position += len;
                },
                
                '\'' => {
                    let (token, len) = self.tokenize_single_quoted_string(&mut chars, position);
                    tokens.push(token);
                    position += len;
                },
                
                // 単語（コマンド、引数など）
                _ => {
                    let (token, len) = self.tokenize_word(&mut chars, position);
                    tokens.push(token);
                    position += len;
                }
            }
        }
        
        Ok(tokens)
    }
}

impl DefaultLexer {
    // 単語（コマンド、引数など）をトークン化
    fn tokenize_word<I>(&self, chars: &mut std::iter::Peekable<I>, start_pos: usize) -> (Token, usize)
    where
        I: Iterator<Item = char>,
    {
        let mut word = String::new();
        let mut length = 0;
        
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || "()<>|;&".contains(c) {
                break;
            }
            
            word.push(c);
            chars.next();
            length += 1;
        }
        
        // キーワードの判定
        let kind = match word.as_str() {
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "elif" => TokenKind::Elif,
            "fi" => TokenKind::Fi,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "do" => TokenKind::Do,
            "done" => TokenKind::Done,
            _ => TokenKind::Word,
        };
        
        (Token {
            kind,
            value: word,
            position: start_pos,
        }, length)
    }
    
    // ダブルクォート文字列をトークン化
    fn tokenize_double_quoted_string<I>(&self, chars: &mut std::iter::Peekable<I>, start_pos: usize) -> (Token, usize)
    where
        I: Iterator<Item = char>,
    {
        let mut string = String::new();
        let mut length = 0;
        
        // 開始の引用符をスキップ
        chars.next();
        length += 1;
        
        string.push('"');
        
        let mut escaped = false;
        while let Some(&c) = chars.peek() {
            chars.next();
            length += 1;
            
            if escaped {
                string.push(c);
                escaped = false;
            } else if c == '\\' {
                string.push(c);
                escaped = true;
            } else if c == '"' {
                string.push(c);
                break;
            } else {
                string.push(c);
            }
        }
        
        (Token {
            kind: TokenKind::Word,
            value: string,
            position: start_pos,
        }, length)
    }
    
    // シングルクォート文字列をトークン化
    fn tokenize_single_quoted_string<I>(&self, chars: &mut std::iter::Peekable<I>, start_pos: usize) -> (Token, usize)
    where
        I: Iterator<Item = char>,
    {
        let mut string = String::new();
        let mut length = 0;
        
        // 開始の引用符をスキップ
        chars.next();
        length += 1;
        
        string.push('\'');
        
        while let Some(&c) = chars.peek() {
            chars.next();
            length += 1;
            
            if c == '\'' {
                string.push(c);
                break;
            } else {
                string.push(c);
            }
        }
        
        (Token {
            kind: TokenKind::Word,
            value: string,
            position: start_pos,
        }, length)
    }
}

/// 高度な解析・型検証・エラー回復を備えた解析を行う
///
/// # 引数
/// * `input` - 解析する入力文字列
///
/// # 戻り値
/// * `Result<AstNode>` - 解析成功時はASTのルートノード、失敗時はエラー情報
pub fn parse_with_recovery(input: &str) -> Result<AstNode> {
    let mut lexer = DefaultLexer::new();
    let mut parser = DefaultParser::new();
    
    // 字句解析を実行
    let tokens = lexer.tokenize(input)?;
    
    // 構文解析を実行
    let mut context = ParserContext::new(input.to_string());
    context.tokens = tokens;
    
    // エラー回復マネージャーを作成
    let mut recovery_manager = error_recovery::create_error_recovery_manager();
    
    // 解析を試行
    match parser.parse_with_context(&mut context) {
        Ok(ast) => {
            // 型チェックを実行
            let mut type_checker = type_system::create_type_checker();
            match type_checker.check(&ast) {
                Ok(_) => Ok(ast),
                Err(e) => {
                    if type_checker.error_count() > 0 {
                        // 型エラーはあるが、ASTは返す（警告として）
                        Ok(ast)
                    } else {
                        // 致命的な型エラー
                        Err(e)
                    }
                }
            }
        },
        Err(e) => {
            // エラー回復を試みる
            match error_recovery::recover_from_error(&mut recovery_manager, &mut context, &e) {
                Ok(repair_result) => {
                    // 回復成功、再度解析を試みる
                    parser.parse_with_context(&mut context)
                },
                Err(_) => {
                    // 回復失敗
                    Err(e)
                }
            }
        }
    }
}

/// 厳格な型チェックを行う解析を実行
///
/// # 引数
/// * `input` - 解析する入力文字列
///
/// # 戻り値
/// * `Result<AstNode>` - 解析成功時はASTのルートノード、失敗時はエラー情報
pub fn parse_with_strict_typing(input: &str) -> Result<AstNode> {
    let mut lexer = DefaultLexer::new();
    let mut parser = DefaultParser::new();
    
    // 字句解析を実行
    let tokens = lexer.tokenize(input)?;
    
    // 構文解析を実行
    let mut context = ParserContext::new(input.to_string());
    context.tokens = tokens;
    
    // 解析を実行
    let ast = parser.parse_with_context(&mut context)?;
    
    // 厳格な型チェックを実行
    let mut type_checker = type_system::create_strict_type_checker();
    type_checker.check(&ast)?;
    
    Ok(ast)
}
