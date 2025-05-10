// token.rs
// このファイルはトークン関連の定義を含みます

use std::fmt;

/// トークンの種類を表す列挙型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // 基本的なトークン
    Whitespace,
    Comment,
    
    // リテラル
    Boolean,
    Integer,
    Float,
    String,
    Variable,
    Identifier,
    Flag,
    
    // パイプライン関連
    Pipe,           // |
    PipeTyped,      // |>
    PipeConditional, // ||
    PipeParallel,   // &|
    PipeError,      // |&
    
    // リダイレクション
    RedirectOut,    // >
    RedirectAppend, // >>
    RedirectIn,     // <
    RedirectMerge,  // >&
    
    // 制御文字
    Ampersand,      // &
    Semicolon,      // ;
    
    // 括弧類
    ParenOpen,      // (
    ParenClose,     // )
    BraceOpen,      // {
    BraceClose,     // }
    BracketOpen,    // [
    BracketClose,   // ]
    
    // 演算子
    Equals,         // =
    PlusEquals,     // +=
    MinusEquals,    // -=
    StarEquals,     // *=
    SlashEquals,    // /=
    PercentEquals,  // %=
    And,            // &&
    Or,             // ||
    Not,            // !
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    EqualEqual,     // ==
    NotEqual,       // !=
    Less,           // <
    LessEqual,      // <=
    Greater,        // >
    GreaterEqual,   // >=
    
    // 区切り文字
    Dot,            // .
    Comma,          // ,
    Colon,          // :
    Question,       // ?
    
    // キーワード
    If,
    Else,
    For,
    While,
    In,
    Function,
    Return,
    
    // 特殊トークン
    Eof,            // 入力の終わり
    Error,          // エラートークン
}

/// トークン位置情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,     // バイトオフセット（開始位置）
    pub end: usize,       // バイトオフセット（終了位置）
    pub line: usize,      // 行番号（1始まり）
    pub column: usize,    // 列番号（1始まり）
}

impl Span {
    /// 新しいスパンを作成
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self { start, end, line, column }
    }
    
    /// 空のスパンを作成
    pub fn empty() -> Self {
        Self { start: 0, end: 0, line: 0, column: 0 }
    }
    
    /// スパンの長さを取得
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    
    /// スパンが空かどうか
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    
    /// 2つのスパンを結合
    pub fn merge(&self, other: &Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            // 行と列は開始位置のものを保持
            line: if self.start <= other.start { self.line } else { other.line },
            column: if self.start <= other.start { self.column } else { other.column },
        }
    }
    
    /// 位置情報を文字列で取得
    pub fn location(&self) -> String {
        format!("{}:{}", self.line, self.column)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} (offset: {}-{})", self.line, self.column, self.start, self.end)
    }
}

/// トークン構造体
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,   // トークンの種類
    pub lexeme: String,    // トークンの文字列表現
    pub span: Span,        // トークンの位置情報
}

impl Token {
    /// 新しいトークンを作成
    pub fn new(kind: TokenKind, lexeme: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            lexeme: lexeme.into(),
            span,
        }
    }
    
    /// 文字列からトークンを作成（テスト用）
    pub fn from_str(kind: TokenKind, lexeme: impl Into<String>) -> Self {
        let lexeme_str = lexeme.into();
        Self {
            kind,
            span: Span::new(0, lexeme_str.len(), 1, 1),
            lexeme: lexeme_str,
        }
    }
    
    /// EOFトークンを作成
    pub fn eof(pos: usize, line: usize, column: usize) -> Self {
        Self {
            kind: TokenKind::Eof,
            lexeme: String::new(),
            span: Span::new(pos, pos, line, column),
        }
    }
    
    /// エラートークンを作成
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: TokenKind::Error,
            lexeme: message.into(),
            span,
        }
    }
    
    /// トークンの長さを取得
    pub fn len(&self) -> usize {
        self.span.len()
    }
    
    /// トークンが空かどうか
    pub fn is_empty(&self) -> bool {
        self.span.is_empty()
    }
    
    /// トークンがEOFかどうか
    pub fn is_eof(&self) -> bool {
        self.kind == TokenKind::Eof
    }
    
    /// トークンがエラーかどうか
    pub fn is_error(&self) -> bool {
        self.kind == TokenKind::Error
    }
    
    /// トークンが指定した種類かどうか
    pub fn is_kind(&self, kind: TokenKind) -> bool {
        self.kind == kind
    }
    
    /// トークンが指定した種類のいずれかかどうか
    pub fn is_one_of(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.kind)
    }
    
    /// トークンがキーワードかどうか
    pub fn is_keyword(&self) -> bool {
        matches!(self.kind, 
            TokenKind::If | 
            TokenKind::Else | 
            TokenKind::For | 
            TokenKind::While | 
            TokenKind::In | 
            TokenKind::Function | 
            TokenKind::Return
        )
    }
    
    /// トークンが演算子かどうか
    pub fn is_operator(&self) -> bool {
        matches!(self.kind,
            TokenKind::Plus |
            TokenKind::Minus |
            TokenKind::Star |
            TokenKind::Slash |
            TokenKind::Percent |
            TokenKind::Equals |
            TokenKind::PlusEquals |
            TokenKind::MinusEquals |
            TokenKind::StarEquals |
            TokenKind::SlashEquals |
            TokenKind::PercentEquals |
            TokenKind::EqualEqual |
            TokenKind::NotEqual |
            TokenKind::Less |
            TokenKind::LessEqual |
            TokenKind::Greater |
            TokenKind::GreaterEqual |
            TokenKind::And |
            TokenKind::Or |
            TokenKind::Not
        )
    }
    
    /// トークンがリテラルかどうか
    pub fn is_literal(&self) -> bool {
        matches!(self.kind,
            TokenKind::Boolean |
            TokenKind::Integer |
            TokenKind::Float |
            TokenKind::String
        )
    }
    
    /// トークンの詳細な説明を取得
    pub fn description(&self) -> String {
        match self.kind {
            TokenKind::Eof => "入力の終わり".to_string(),
            TokenKind::Error => format!("エラー: {}", self.lexeme),
            _ => format!("{:?} '{}'", self.kind, self.lexeme),
        }
    }
    
    /// トークンの位置情報を文字列で取得
    pub fn location(&self) -> String {
        self.span.location()
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_eof() {
            write!(f, "EOF")
        } else {
            write!(f, "{}", self.lexeme)
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Whitespace => write!(f, "空白"),
            TokenKind::Comment => write!(f, "コメント"),
            TokenKind::Boolean => write!(f, "真偽値"),
            TokenKind::Integer => write!(f, "整数"),
            TokenKind::Float => write!(f, "浮動小数点数"),
            TokenKind::String => write!(f, "文字列"),
            TokenKind::Variable => write!(f, "変数"),
            TokenKind::Identifier => write!(f, "識別子"),
            TokenKind::Flag => write!(f, "フラグ"),
            TokenKind::Pipe => write!(f, "パイプ"),
            TokenKind::PipeTyped => write!(f, "型付きパイプ"),
            TokenKind::PipeConditional => write!(f, "条件付きパイプ"),
            TokenKind::PipeParallel => write!(f, "並列パイプ"),
            TokenKind::PipeError => write!(f, "エラーパイプ"),
            TokenKind::RedirectOut => write!(f, "出力リダイレクト"),
            TokenKind::RedirectAppend => write!(f, "追加リダイレクト"),
            TokenKind::RedirectIn => write!(f, "入力リダイレクト"),
            TokenKind::RedirectMerge => write!(f, "マージリダイレクト"),
            TokenKind::Ampersand => write!(f, "アンパサンド"),
            TokenKind::Semicolon => write!(f, "セミコロン"),
            TokenKind::ParenOpen => write!(f, "開き括弧"),
            TokenKind::ParenClose => write!(f, "閉じ括弧"),
            TokenKind::BraceOpen => write!(f, "開き波括弧"),
            TokenKind::BraceClose => write!(f, "閉じ波括弧"),
            TokenKind::BracketOpen => write!(f, "開き角括弧"),
            TokenKind::BracketClose => write!(f, "閉じ角括弧"),
            TokenKind::Equals => write!(f, "等号"),
            TokenKind::PlusEquals => write!(f, "加算代入"),
            TokenKind::MinusEquals => write!(f, "減算代入"),
            TokenKind::StarEquals => write!(f, "乗算代入"),
            TokenKind::SlashEquals => write!(f, "除算代入"),
            TokenKind::PercentEquals => write!(f, "剰余代入"),
            TokenKind::And => write!(f, "論理積"),
            TokenKind::Or => write!(f, "論理和"),
            TokenKind::Not => write!(f, "論理否定"),
            TokenKind::Plus => write!(f, "加算"),
            TokenKind::Minus => write!(f, "減算"),
            TokenKind::Star => write!(f, "乗算"),
            TokenKind::Slash => write!(f, "除算"),
            TokenKind::Percent => write!(f, "剰余"),
            TokenKind::EqualEqual => write!(f, "等価"),
            TokenKind::NotEqual => write!(f, "不等価"),
            TokenKind::Less => write!(f, "小なり"),
            TokenKind::LessEqual => write!(f, "以下"),
            TokenKind::Greater => write!(f, "大なり"),
            TokenKind::GreaterEqual => write!(f, "以上"),
            TokenKind::Dot => write!(f, "ドット"),
            TokenKind::Comma => write!(f, "カンマ"),
            TokenKind::Colon => write!(f, "コロン"),
            TokenKind::Question => write!(f, "疑問符"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::For => write!(f, "for"),
            TokenKind::While => write!(f, "while"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Function => write!(f, "function"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::Eof => write!(f, "EOF"),
            TokenKind::Error => write!(f, "エラー"),
        }
    }
}

/// トークン列
#[derive(Debug, Clone)]
pub struct TokenStream {
    tokens: Vec<Token>,
    position: usize,
}

impl TokenStream {
    /// 新しいトークン列を作成
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }
    
    /// 現在のトークンを取得
    pub fn current(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }
    
    /// 次のトークンを取得
    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position + 1)
    }
    
    /// n個先のトークンを取得
    pub fn peek_n(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.position + n)
    }
    
    /// 次のトークンに進む
    pub fn advance(&mut self) -> Option<&Token> {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
        self.current()
    }
    
    /// 先読みせずに次のトークンに進み、取得
    pub fn next(&mut self) -> Option<&Token> {
        self.advance()
    }
    
    /// 指定した位置まで巻き戻し
    pub fn rewind(&mut self, position: usize) {
        self.position = position.min(self.tokens.len());
    }
    
    /// 現在位置を保存
    pub fn save_position(&self) -> usize {
        self.position
    }
    
    /// 指定した種類のトークンを消費
    pub fn consume(&mut self, kind: TokenKind) -> Option<&Token> {
        if self.check(kind) {
            let token = self.current();
            self.advance();
            token
        } else {
            None
        }
    }
    
    /// 指定した種類のいずれかのトークンを消費
    pub fn consume_any(&mut self, kinds: &[TokenKind]) -> Option<&Token> {
        if self.check_any(kinds) {
            let token = self.current();
            self.advance();
            token
        } else {
            None
        }
    }
    
    /// 現在のトークンが指定した種類かどうか
    pub fn check(&self, kind: TokenKind) -> bool {
        self.current().map_or(false, |t| t.kind == kind)
    }
    
    /// 現在のトークンが指定した種類のいずれかかどうか
    pub fn check_any(&self, kinds: &[TokenKind]) -> bool {
        self.current().map_or(false, |t| kinds.contains(&t.kind))
    }
    
    /// 次のトークンが指定した種類かどうか
    pub fn check_next(&self, kind: TokenKind) -> bool {
        self.peek().map_or(false, |t| t.kind == kind)
    }
    
    /// 次のトークンが指定した種類のいずれかかどうか
    pub fn check_next_any(&self, kinds: &[TokenKind]) -> bool {
        self.peek().map_or(false, |t| kinds.contains(&t.kind))
    }
    
    /// EOFに達したかどうか
    pub fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len() || 
        self.current().map_or(true, |t| t.kind == TokenKind::Eof)
    }
    
    /// 指定した種類のトークンまでスキップ
    pub fn skip_until(&mut self, kind: TokenKind) {
        while let Some(token) = self.current() {
            if token.kind == kind {
                break;
            }
            self.advance();
        }
    }
    
    /// 指定した種類のいずれかのトークンまでスキップ
    pub fn skip_until_any(&mut self, kinds: &[TokenKind]) {
        while let Some(token) = self.current() {
            if kinds.contains(&token.kind) {
                break;
            }
            self.advance();
        }
    }
    
    /// 残りのトークン数を取得
    pub fn remaining(&self) -> usize {
        self.tokens.len() - self.position
    }
    
    /// 全てのトークンを取得
    pub fn all_tokens(&self) -> &[Token] {
        &self.tokens
    }
    
    /// 現在位置以降のトークンを取得
    pub fn remaining_tokens(&self) -> &[Token] {
        &self.tokens[self.position..]
    }
    
    /// トークン列の長さを取得
    pub fn len(&self) -> usize {
        self.tokens.len()
    }
    
    /// トークン列が空かどうか
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
    
    /// 現在の位置を取得
    pub fn position(&self) -> usize {
        self.position
    }
}

impl IntoIterator for TokenStream {
    type Item = Token;
    type IntoIter = std::vec::IntoIter<Token>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.tokens.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_token_creation() {
        let token = Token::new(
            TokenKind::Identifier,
            "test",
            Span::new(0, 4, 1, 1)
        );
        
        assert_eq!(token.kind, TokenKind::Identifier);
        assert_eq!(token.lexeme, "test");
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 4);
        assert_eq!(token.span.line, 1);
        assert_eq!(token.span.column, 1);
    }
    
    #[test]
    fn test_token_methods() {
        let token = Token::from_str(TokenKind::String, "hello");
        
        assert_eq!(token.len(), 5);
        assert!(!token.is_empty());
        assert!(!token.is_eof());
        assert!(!token.is_error());
        assert!(token.is_kind(TokenKind::String));
        assert!(token.is_one_of(&[TokenKind::Integer, TokenKind::String]));
        assert!(!token.is_keyword());
        assert!(!token.is_operator());
        assert!(token.is_literal());
    }
    
    #[test]
    fn test_span_methods() {
        let span1 = Span::new(5, 10, 1, 6);
        let span2 = Span::new(8, 15, 1, 9);
        
        assert_eq!(span1.len(), 5);
        assert!(!span1.is_empty());
        
        let merged = span1.merge(&span2);
        assert_eq!(merged.start, 5);
        assert_eq!(merged.end, 15);
        assert_eq!(merged.line, 1);
        assert_eq!(merged.column, 6);
    }
    
    #[test]
    fn test_token_stream() {
        let tokens = vec![
            Token::from_str(TokenKind::Identifier, "foo"),
            Token::from_str(TokenKind::Plus, "+"),
            Token::from_str(TokenKind::Integer, "42"),
            Token::eof(5, 1, 6),
        ];
        
        let mut stream = TokenStream::new(tokens);
        
        assert_eq!(stream.len(), 4);
        assert!(!stream.is_empty());
        
        assert_eq!(stream.current().unwrap().kind, TokenKind::Identifier);
        assert_eq!(stream.peek().unwrap().kind, TokenKind::Plus);
        
        stream.advance();
        assert_eq!(stream.current().unwrap().kind, TokenKind::Plus);
        
        assert!(stream.consume(TokenKind::Plus).is_some());
        assert_eq!(stream.current().unwrap().kind, TokenKind::Integer);
        
        assert!(stream.consume(TokenKind::String).is_none());
        assert_eq!(stream.current().unwrap().kind, TokenKind::Integer);
        
        stream.advance();
        assert!(stream.check(TokenKind::Eof));
        assert!(stream.is_at_end());
    }
    
    #[test]
    fn test_token_stream_rewind() {
        let tokens = vec![
            Token::from_str(TokenKind::Identifier, "foo"),
            Token::from_str(TokenKind::Plus, "+"),
            Token::from_str(TokenKind::Integer, "42"),
        ];
        
        let mut stream = TokenStream::new(tokens);
        
        let pos = stream.save_position();
        stream.advance();
        stream.advance();
        assert_eq!(stream.current().unwrap().kind, TokenKind::Integer);
        
        stream.rewind(pos);
        assert_eq!(stream.current().unwrap().kind, TokenKind::Identifier);
    }
}