/*!
# 評価モジュール

シェルスクリプトの評価や式の計算を行う高度なモジュールです。
変数展開、算術式評価、条件分岐、パターンマッチングなど
シェルスクリプトの言語機能をサポートします。
*/

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use dashmap::DashMap;
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

use crate::environment::Environment;

/// 式の型
#[derive(Debug, Clone, PartialEq)]
pub enum ValueType {
    /// 文字列
    String,
    /// 整数
    Integer,
    /// 浮動小数点数
    Float,
    /// 真偽値
    Boolean,
    /// 配列
    Array,
    /// マップ
    Map,
    /// Null（未定義）
    Null,
}

/// 評価値
#[derive(Debug, Clone)]
pub enum Value {
    /// 文字列
    String(String),
    /// 整数
    Integer(i64),
    /// 浮動小数点数
    Float(f64),
    /// 真偽値
    Boolean(bool),
    /// 配列
    Array(Vec<Value>),
    /// マップ
    Map(HashMap<String, Value>),
    /// Null（未定義）
    Null,
}

impl Value {
    /// 型を取得
    pub fn type_of(&self) -> ValueType {
        match self {
            Value::String(_) => ValueType::String,
            Value::Integer(_) => ValueType::Integer,
            Value::Float(_) => ValueType::Float,
            Value::Boolean(_) => ValueType::Boolean,
            Value::Array(_) => ValueType::Array,
            Value::Map(_) => ValueType::Map,
            Value::Null => ValueType::Null,
        }
    }
    
    /// 文字列に変換
    pub fn to_string(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Integer(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Array(arr) => {
                let elements: Vec<String> = arr.iter()
                    .map(|v| v.to_string())
                    .collect();
                format!("[{}]", elements.join(", "))
            },
            Value::Map(map) => {
                let elements: Vec<String> = map.iter()
                    .map(|(k, v)| format!("{}={}", k, v.to_string()))
                    .collect();
                format!("{{{}}}", elements.join(", "))
            },
            Value::Null => "null".to_string(),
        }
    }
    
    /// 整数に変換
    pub fn to_integer(&self) -> Result<i64> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Float(f) => Ok(*f as i64),
            Value::String(s) => s.parse::<i64>()
                .with_context(|| format!("文字列を整数に変換できません: {}", s)),
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            _ => Err(anyhow!("値を整数に変換できません: {:?}", self)),
        }
    }
    
    /// 浮動小数点数に変換
    pub fn to_float(&self) -> Result<f64> {
        match self {
            Value::Integer(i) => Ok(*i as f64),
            Value::Float(f) => Ok(*f),
            Value::String(s) => s.parse::<f64>()
                .with_context(|| format!("文字列を浮動小数点数に変換できません: {}", s)),
            Value::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
            _ => Err(anyhow!("値を浮動小数点数に変換できません: {:?}", self)),
        }
    }
    
    /// 真偽値に変換
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::Integer(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Boolean(b) => *b,
            Value::Array(arr) => !arr.is_empty(),
            Value::Map(map) => !map.is_empty(),
            Value::Null => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl FromStr for Value {
    type Err = anyhow::Error;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 整数として解析を試みる
        if let Ok(i) = s.parse::<i64>() {
            return Ok(Value::Integer(i));
        }
        
        // 浮動小数点数として解析を試みる
        if let Ok(f) = s.parse::<f64>() {
            return Ok(Value::Float(f));
        }
        
        // 真偽値として解析を試みる
        match s.to_lowercase().as_str() {
            "true" | "yes" | "y" | "on" => return Ok(Value::Boolean(true)),
            "false" | "no" | "n" | "off" => return Ok(Value::Boolean(false)),
            _ => {}
        }
        
        // デフォルトでは文字列として扱う
        Ok(Value::String(s.to_string()))
    }
}

/// スコープ（変数の名前空間）
#[derive(Debug, Clone)]
pub struct Scope {
    /// 変数
    variables: HashMap<String, Value>,
    /// 親スコープ
    parent: Option<Arc<Scope>>,
}

impl Scope {
    /// 新しいスコープを作成
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            parent: None,
        }
    }
    
    /// 親スコープを指定して新しいスコープを作成
    pub fn with_parent(parent: Arc<Scope>) -> Self {
        Self {
            variables: HashMap::new(),
            parent: Some(parent),
        }
    }
    
    /// 変数を設定
    pub fn set(&mut self, name: &str, value: Value) {
        self.variables.insert(name.to_string(), value);
    }
    
    /// 変数を取得
    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.variables.get(name) {
            return Some(value.clone());
        }
        
        // 親スコープから探索
        if let Some(parent) = &self.parent {
            return parent.get(name);
        }
        
        None
    }
    
    /// 変数が存在するか確認
    pub fn has(&self, name: &str) -> bool {
        if self.variables.contains_key(name) {
            return true;
        }
        
        // 親スコープから探索
        if let Some(parent) = &self.parent {
            return parent.has(name);
        }
        
        false
    }
    
    /// 変数を削除
    pub fn remove(&mut self, name: &str) -> bool {
        self.variables.remove(name).is_some()
    }
    
    /// ローカル変数のみを取得
    pub fn get_locals(&self) -> HashMap<String, Value> {
        self.variables.clone()
    }
    
    /// すべての変数を取得（親スコープも含む）
    pub fn get_all(&self) -> HashMap<String, Value> {
        let mut result = if let Some(parent) = &self.parent {
            parent.get_all()
        } else {
            HashMap::new()
        };
        
        // ローカル変数で上書き
        for (key, value) in &self.variables {
            result.insert(key.clone(), value.clone());
        }
        
        result
    }
}

/// トークンの種類
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// 識別子
    Identifier(String),
    /// 文字列リテラル
    String(String),
    /// 数値リテラル
    Number(f64),
    /// 演算子
    Operator(String),
    /// 区切り文字
    Delimiter(char),
    /// キーワード
    Keyword(String),
    /// 終端
    EOF,
}

/// トークン
#[derive(Debug, Clone)]
pub struct Token {
    /// トークンの種類
    pub token_type: TokenType,
    /// 行番号
    pub line: usize,
    /// 列番号
    pub column: usize,
}

/// 字句解析器
pub struct Lexer {
    /// 入力テキスト
    input: String,
    /// 現在の位置
    position: usize,
    /// 行番号
    line: usize,
    /// 列番号
    column: usize,
    /// キーワードセット
    keywords: Vec<String>,
}

impl Lexer {
    /// 新しい字句解析器を作成
    pub fn new(input: &str) -> Self {
        let keywords = vec![
            "if".to_string(), "then".to_string(), "else".to_string(), "elif".to_string(), 
            "fi".to_string(), "for".to_string(), "in".to_string(), "do".to_string(), 
            "done".to_string(), "while".to_string(), "until".to_string(), "case".to_string(), 
            "esac".to_string(), "function".to_string(), "return".to_string(), "break".to_string(), 
            "continue".to_string(), "local".to_string(), "export".to_string(), "readonly".to_string(),
        ];
        
        Self {
            input: input.to_string(),
            position: 0,
            line: 1,
            column: 1,
            keywords,
        }
    }
    
    /// 次のトークンを取得
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        
        if self.position >= self.input.len() {
            return Token {
                token_type: TokenType::EOF,
                line: self.line,
                column: self.column,
            };
        }
        
        let current_char = self.current_char();
        
        // TODO: トークン解析の実装
        // 識別子やキーワード
        if self.is_letter(current_char) {
            return self.read_identifier();
        }
        
        // 数値
        if self.is_digit(current_char) {
            return self.read_number();
        }
        
        // 文字列
        if current_char == '"' || current_char == '\'' {
            return self.read_string();
        }
        
        // 演算子
        if self.is_operator(current_char) {
            return self.read_operator();
        }
        
        // 区切り文字
        let token = Token {
            token_type: TokenType::Delimiter(current_char),
            line: self.line,
            column: self.column,
        };
        
        self.advance();
        token
    }
    
    /// 識別子の読み取り
    fn read_identifier(&mut self) -> Token {
        let start_position = self.position;
        let start_column = self.column;
        
        while self.position < self.input.len() && 
              (self.is_letter(self.current_char()) || 
               self.is_digit(self.current_char()) || 
               self.current_char() == '_') {
            self.advance();
        }
        
        let identifier = &self.input[start_position..self.position];
        
        // キーワードかどうかをチェック
        let token_type = if self.keywords.contains(&identifier.to_string()) {
            TokenType::Keyword(identifier.to_string())
        } else {
            TokenType::Identifier(identifier.to_string())
        };
        
        Token {
            token_type,
            line: self.line,
            column: start_column,
        }
    }
    
    /// 数値の読み取り
    fn read_number(&mut self) -> Token {
        let start_position = self.position;
        let start_column = self.column;
        
        // 整数部分
        while self.position < self.input.len() && self.is_digit(self.current_char()) {
            self.advance();
        }
        
        // 小数点があるかをチェック
        let mut has_decimal = false;
        if self.position < self.input.len() && self.current_char() == '.' {
            has_decimal = true;
            self.advance();
            
            // 小数部分
            while self.position < self.input.len() && self.is_digit(self.current_char()) {
                self.advance();
            }
        }
        
        let number_str = &self.input[start_position..self.position];
        let number = number_str.parse::<f64>().unwrap_or(0.0);
        
        Token {
            token_type: TokenType::Number(number),
            line: self.line,
            column: start_column,
        }
    }
    
    /// 文字列の読み取り
    fn read_string(&mut self) -> Token {
        let quote_char = self.current_char();
        let start_column = self.column;
        
        self.advance(); // 開始引用符をスキップ
        
        let start_position = self.position;
        
        // 閉じる引用符を探す
        while self.position < self.input.len() && self.current_char() != quote_char {
            // エスケープシーケンスの処理
            if self.current_char() == '\\' && self.position + 1 < self.input.len() {
                self.advance();
            }
            self.advance();
        }
        
        let string_content = &self.input[start_position..self.position];
        
        if self.position < self.input.len() {
            self.advance(); // 終了引用符をスキップ
        }
        
        Token {
            token_type: TokenType::String(string_content.to_string()),
            line: self.line,
            column: start_column,
        }
    }
    
    /// 演算子の読み取り
    fn read_operator(&mut self) -> Token {
        let start_position = self.position;
        let start_column = self.column;
        
        // 複合演算子をサポート
        let current_char = self.current_char();
        self.advance();
        
        // 2文字演算子のチェック
        if self.position < self.input.len() {
            let next_char = self.current_char();
            
            // 複合演算子の例: ==, !=, >=, <=, &&, ||, etc.
            if (current_char == '=' && next_char == '=') ||
               (current_char == '!' && next_char == '=') ||
               (current_char == '>' && next_char == '=') ||
               (current_char == '<' && next_char == '=') ||
               (current_char == '&' && next_char == '&') ||
               (current_char == '|' && next_char == '|') {
                self.advance();
            }
        }
        
        let operator = &self.input[start_position..self.position];
        
        Token {
            token_type: TokenType::Operator(operator.to_string()),
            line: self.line,
            column: start_column,
        }
    }
    
    /// 空白文字をスキップ
    fn skip_whitespace(&mut self) {
        while self.position < self.input.len() && self.current_char().is_whitespace() {
            if self.current_char() == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }
    
    /// 現在の文字を取得
    fn current_char(&self) -> char {
        if self.position >= self.input.len() {
            '\0'
        } else {
            self.input.chars().nth(self.position).unwrap_or('\0')
        }
    }
    
    /// ポインタを進める
    fn advance(&mut self) {
        if self.current_char() == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.position += 1;
    }
    
    /// 文字かどうかをチェック
    fn is_letter(&self, ch: char) -> bool {
        ch.is_alphabetic() || ch == '_'
    }
    
    /// 数字かどうかをチェック
    fn is_digit(&self, ch: char) -> bool {
        ch.is_digit(10)
    }
    
    /// 演算子かどうかをチェック
    fn is_operator(&self, ch: char) -> bool {
        match ch {
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '>' | '<' | '&' | '|' => true,
            _ => false,
        }
    }
}

/// 抽象構文木ノードの種類
#[derive(Debug, Clone)]
pub enum AstNodeType {
    /// プログラム
    Program(Vec<Box<AstNode>>),
    /// 変数宣言
    VariableDeclaration {
        name: String,
        value: Box<AstNode>,
        is_local: bool,
        is_readonly: bool,
    },
    /// 変数参照
    VariableReference(String),
    /// リテラル
    Literal(Value),
    /// 二項演算
    BinaryOperation {
        left: Box<AstNode>,
        operator: String,
        right: Box<AstNode>,
    },
    /// 単項演算
    UnaryOperation {
        operator: String,
        operand: Box<AstNode>,
    },
    /// 関数呼び出し
    FunctionCall {
        name: String,
        arguments: Vec<Box<AstNode>>,
    },
    /// if文
    IfStatement {
        condition: Box<AstNode>,
        then_branch: Box<AstNode>,
        else_branch: Option<Box<AstNode>>,
    },
    /// for文
    ForStatement {
        variable: String,
        iterable: Box<AstNode>,
        body: Box<AstNode>,
    },
    /// while文
    WhileStatement {
        condition: Box<AstNode>,
        body: Box<AstNode>,
    },
    /// 関数定義
    FunctionDefinition {
        name: String,
        parameters: Vec<String>,
        body: Box<AstNode>,
    },
    /// return文
    ReturnStatement(Option<Box<AstNode>>),
    /// break文
    BreakStatement,
    /// continue文
    ContinueStatement,
    /// コマンド実行
    CommandExecution {
        command: String,
        arguments: Vec<Box<AstNode>>,
        redirects: Vec<RedirectNode>,
    },
    /// パイプライン
    Pipeline(Vec<Box<AstNode>>),
    /// ブロック
    Block(Vec<Box<AstNode>>),
}

/// リダイレクトノード
#[derive(Debug, Clone)]
pub struct RedirectNode {
    /// リダイレクトの種類
    pub redirect_type: RedirectType,
    /// ファイルディスクリプタ
    pub fd: Option<u32>,
    /// ターゲット
    pub target: Box<AstNode>,
}

/// リダイレクトの種類
#[derive(Debug, Clone, PartialEq)]
pub enum RedirectType {
    /// 入力リダイレクト (<)
    Input,
    /// 出力リダイレクト (>)
    Output,
    /// 追記リダイレクト (>>)
    Append,
    /// エラー出力リダイレクト (2>)
    Error,
    /// エラー出力追記リダイレクト (2>>)
    ErrorAppend,
    /// 出力とエラー出力のマージ (&>)
    OutputAndError,
}

/// 抽象構文木ノード
#[derive(Debug, Clone)]
pub struct AstNode {
    /// ノードの種類
    pub node_type: AstNodeType,
    /// 行番号
    pub line: usize,
    /// 列番号
    pub column: usize,
}

/// 構文解析器
pub struct Parser {
    /// 字句解析器
    lexer: Lexer,
    /// 現在のトークン
    current_token: Token,
    /// 次のトークン
    peek_token: Token,
}

impl Parser {
    /// 新しい構文解析器を作成
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token();
        let peek_token = lexer.next_token();
        
        Self {
            lexer,
            current_token,
            peek_token,
        }
    }
    
    /// プログラムを解析
    pub fn parse_program(&mut self) -> Result<AstNode> {
        let mut statements = Vec::new();
        
        while self.current_token.token_type != TokenType::EOF {
            let statement = self.parse_statement()?;
            statements.push(Box::new(statement));
        }
        
        Ok(AstNode {
            node_type: AstNodeType::Program(statements),
            line: 1,
            column: 1,
        })
    }
    
    /// 文を解析
    fn parse_statement(&mut self) -> Result<AstNode> {
        // TODO: 構文解析の実装
        
        // 例: 変数宣言解析
        match &self.current_token.token_type {
            TokenType::Keyword(k) if k == "local" => self.parse_variable_declaration(true),
            TokenType::Keyword(k) if k == "readonly" => self.parse_readonly_declaration(),
            TokenType::Keyword(k) if k == "export" => self.parse_export_declaration(),
            TokenType::Keyword(k) if k == "if" => self.parse_if_statement(),
            TokenType::Keyword(k) if k == "for" => self.parse_for_statement(),
            TokenType::Keyword(k) if k == "while" => self.parse_while_statement(),
            TokenType::Keyword(k) if k == "function" => self.parse_function_definition(),
            TokenType::Keyword(k) if k == "return" => self.parse_return_statement(),
            TokenType::Keyword(k) if k == "break" => self.parse_break_statement(),
            TokenType::Keyword(k) if k == "continue" => self.parse_continue_statement(),
            _ => {
                // 変数代入または式文
                if let TokenType::Identifier(_) = &self.current_token.token_type {
                    if let TokenType::Operator(op) = &self.peek_token.token_type {
                        if op == "=" {
                            return self.parse_variable_declaration(false);
                        }
                    }
                }
                
                // その他はコマンド実行として扱う
                self.parse_command_execution()
            }
        }
    }
    
    /// 次のトークンに進む
    fn next_token(&mut self) {
        self.current_token = self.peek_token.clone();
        self.peek_token = self.lexer.next_token();
    }
    
    // 各種解析メソッド（スタブ実装）
    
    /// 変数宣言を解析
    fn parse_variable_declaration(&mut self, is_local: bool) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // ローカル変数宣言の場合は「local」キーワードをスキップ
        if is_local {
            if let TokenType::Keyword(k) = &self.current_token.token_type {
                if k == "local" {
                    self.next_token();
                } else {
                    return Err(anyhow!("予期しないキーワード: {}", k));
                }
            }
        }
        
        // 変数名を取得
        let var_name = match &self.current_token.token_type {
            TokenType::Identifier(name) => name.clone(),
            _ => return Err(anyhow!("変数名が必要です")),
        };
        
        // 次のトークンへ
        self.next_token();
        
        // 代入演算子を確認
        if let TokenType::Operator(op) = &self.current_token.token_type {
            if op != "=" {
                return Err(anyhow!("代入演算子 '=' が必要です"));
            }
        } else {
            return Err(anyhow!("代入演算子 '=' が必要です"));
        }
        
        // 代入演算子をスキップ
        self.next_token();
        
        // 値を解析
        let value_expr = self.parse_expression()?;
        
        // AST ノードを作成
        Ok(AstNode {
            node_type: AstNodeType::VariableDeclaration {
                name: var_name,
                value: Box::new(value_expr),
                is_local,
                is_readonly: false,
            },
            line,
            column,
        })
    }
    
    /// readonlyを解析
    fn parse_readonly_declaration(&mut self) -> Result<AstNode> {
        // TODO: readonly解析実装
        unimplemented!("readonly解析はまだ実装されていません")
    }
    
    /// exportを解析
    fn parse_export_declaration(&mut self) -> Result<AstNode> {
        // TODO: export解析実装
        unimplemented!("export解析はまだ実装されていません")
    }
    
    /// if文を解析
    fn parse_if_statement(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「if」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "if" {
                self.next_token();
            } else {
                return Err(anyhow!("'if'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'if'キーワードが必要です"));
        }
        
        // 条件式を解析
        let condition = self.parse_expression()?;
        
        // 「then」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "then" {
                self.next_token();
            } else {
                return Err(anyhow!("'then'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'then'キーワードが必要です"));
        }
        
        // then ブロックを解析
        let mut then_statements = Vec::new();
        
        // 「else」または「fi」キーワードが現れるまでステートメントを解析
        while !matches!(self.current_token.token_type, 
                        TokenType::Keyword(ref k) if k == "else" || k == "fi") {
            let stmt = self.parse_statement()?;
            then_statements.push(Box::new(stmt));
        }
        
        // then ブロックをASTノードに変換
        let then_branch = AstNode {
            node_type: AstNodeType::Block(then_statements),
            line: self.current_token.line,
            column: self.current_token.column,
        };
        
        // else ブロックを解析（存在する場合）
        let else_branch = if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "else" {
                self.next_token();
                
                // else ブロックのステートメントを解析
                let mut else_statements = Vec::new();
                
                // 「fi」キーワードが現れるまでステートメントを解析
                while !matches!(self.current_token.token_type, 
                                TokenType::Keyword(ref k) if k == "fi") {
                    let stmt = self.parse_statement()?;
                    else_statements.push(Box::new(stmt));
                }
                
                // else ブロックをASTノードに変換
                Some(Box::new(AstNode {
                    node_type: AstNodeType::Block(else_statements),
                    line: self.current_token.line,
                    column: self.current_token.column,
                }))
            } else {
                None
            }
        } else {
            None
        };
        
        // 「fi」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "fi" {
                self.next_token();
            } else {
                return Err(anyhow!("'fi'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'fi'キーワードが必要です"));
        }
        
        // ifステートメントのASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::IfStatement {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            line,
            column,
        })
    }
    
    /// for文を解析
    fn parse_for_statement(&mut self) -> Result<AstNode> {
        // TODO: for文解析実装
        unimplemented!("for文解析はまだ実装されていません")
    }
    
    /// while文を解析
    fn parse_while_statement(&mut self) -> Result<AstNode> {
        // TODO: while文解析実装
        unimplemented!("while文解析はまだ実装されていません")
    }
    
    /// 関数定義を解析
    fn parse_function_definition(&mut self) -> Result<AstNode> {
        // TODO: 関数定義解析実装
        unimplemented!("関数定義解析はまだ実装されていません")
    }
    
    /// return文を解析
    fn parse_return_statement(&mut self) -> Result<AstNode> {
        // TODO: return解析実装
        unimplemented!("return解析はまだ実装されていません")
    }
    
    /// break文を解析
    fn parse_break_statement(&mut self) -> Result<AstNode> {
        // TODO: break解析実装
        unimplemented!("break解析はまだ実装されていません")
    }
    
    /// continue文を解析
    fn parse_continue_statement(&mut self) -> Result<AstNode> {
        // TODO: continue解析実装
        unimplemented!("continue解析はまだ実装されていません")
    }
    
    /// コマンド実行を解析
    fn parse_command_execution(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // コマンド名を取得
        let command = match &self.current_token.token_type {
            TokenType::Identifier(name) => name.clone(),
            TokenType::String(s) => s.clone(),
            _ => return Err(anyhow!("コマンド名が必要です")),
        };
        
        // コマンド名をスキップ
        self.next_token();
        
        // 引数リストを解析
        let mut arguments = Vec::new();
        let mut redirects = Vec::new();
        
        // セミコロン、パイプ、改行またはEOFが現れるまで引数とリダイレクトを解析
        while !matches!(self.current_token.token_type, 
                       TokenType::Delimiter(';') | 
                       TokenType::Operator(ref op) if op == "|" | 
                       TokenType::EOF) {
            
            // リダイレクト解析
            if let TokenType::Operator(op) = &self.current_token.token_type {
                if op == ">" || op == ">>" || op == "<" || op == "2>" || op == "2>>" || op == "&>" {
                    redirects.push(self.parse_redirect()?);
                    continue;
                }
            }
            
            // 引数解析
            let arg = self.parse_expression()?;
            arguments.push(Box::new(arg));
        }
        
        // コマンド実行ノードを作成
        Ok(AstNode {
            node_type: AstNodeType::CommandExecution {
                command,
                arguments,
                redirects,
            },
            line,
            column,
        })
    }
    
    /// リダイレクトを解析
    fn parse_redirect(&mut self) -> Result<RedirectNode> {
        let redirect_type = match &self.current_token.token_type {
            TokenType::Operator(op) => {
                match op.as_str() {
                    ">" => RedirectType::Output,
                    ">>" => RedirectType::Append,
                    "<" => RedirectType::Input,
                    "2>" => RedirectType::Error,
                    "2>>" => RedirectType::ErrorAppend,
                    "&>" => RedirectType::OutputAndError,
                    _ => return Err(anyhow!("無効なリダイレクト演算子: {}", op)),
                }
            },
            _ => return Err(anyhow!("リダイレクト演算子が必要です")),
        };
        
        // リダイレクト演算子をスキップ
        self.next_token();
        
        // ファイルパスを解析
        let target = self.parse_expression()?;
        
        // リダイレクトノードを作成
        Ok(RedirectNode {
            redirect_type,
            fd: None, // ファイルディスクリプタは現在未サポート
            target: Box::new(target),
        })
    }
    
    /// 式を解析 (新規追加)
    fn parse_expression(&mut self) -> Result<AstNode> {
        match &self.current_token.token_type {
            TokenType::String(s) => {
                let node = AstNode {
                    node_type: AstNodeType::Literal(Value::String(s.clone())),
                    line: self.current_token.line,
                    column: self.current_token.column,
                };
                self.next_token();
                Ok(node)
            },
            TokenType::Number(n) => {
                let node = AstNode {
                    node_type: AstNodeType::Literal(Value::Float(*n)),
                    line: self.current_token.line,
                    column: self.current_token.column,
                };
                self.next_token();
                Ok(node)
            },
            TokenType::Identifier(name) => {
                let node = AstNode {
                    node_type: AstNodeType::VariableReference(name.clone()),
                    line: self.current_token.line,
                    column: self.current_token.column,
                };
                self.next_token();
                Ok(node)
            },
            _ => Err(anyhow!("式の解析に失敗しました: 予期しないトークン {:?}", self.current_token)),
        }
    }
}

/// 変数展開
pub struct Expander {
    /// 環境変数
    env: Arc<Environment>,
    /// 変数パターン正規表現
    var_pattern: Regex,
}

impl Expander {
    /// 新しい展開器を作成
    pub fn new(env: Arc<Environment>) -> Self {
        let var_pattern = Regex::new(r"\$(\w+|\{[^}]+\}|\([^)]+\))").unwrap();
        
        Self {
            env,
            var_pattern,
        }
    }
    
    /// テキストの変数展開を実行
    pub fn expand(&self, text: &str, scope: &Scope) -> Result<String> {
        let mut result = text.to_string();
        
        // 変数展開
        for cap in self.var_pattern.captures_iter(text) {
            let full_match = cap.get(0).unwrap().as_str();
            let var_name = cap.get(1).unwrap().as_str();
            
            // ${var} または $(command) 形式の処理
            let cleaned_var_name = if var_name.starts_with('{') && var_name.ends_with('}') {
                &var_name[1..var_name.len() - 1]
            } else if var_name.starts_with('(') && var_name.ends_with(')') {
                // コマンド置換はここでは実装しない
                return Err(anyhow!("コマンド置換はまだサポートされていません: {}", full_match));
            } else {
                var_name
            };
            
            // スコープから変数を探す
            let value = if let Some(value) = scope.get(cleaned_var_name) {
                value.to_string()
            } else if let Some(env_value) = self.env.get(cleaned_var_name) {
                env_value
            } else {
                // 変数が見つからない場合は空文字列
                String::new()
            };
            
            // 置換
            result = result.replace(full_match, &value);
        }
        
        Ok(result)
    }
}

/// 実行コンテキスト
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// 現在のスコープ
    pub scope: Arc<Scope>,
    /// 関数テーブル
    pub functions: HashMap<String, AstNode>,
    /// 環境変数
    pub env: Arc<Environment>,
    /// 現在の作業ディレクトリ
    pub current_dir: PathBuf,
    /// 最後の実行結果
    pub last_result: Option<Value>,
    /// ループフラグ（break/continue）
    pub loop_control: Option<LoopControl>,
    /// リターン値
    pub return_value: Option<Value>,
}

/// ループ制御
#[derive(Debug, Clone, PartialEq)]
pub enum LoopControl {
    /// break文
    Break,
    /// continue文
    Continue,
}

/// 評価エンジン
pub struct EvaluationEngine {
    /// 環境変数
    env: Arc<Environment>,
    /// 変数展開器
    expander: Expander,
    /// 定義済み関数
    builtin_functions: DashMap<String, Arc<dyn BuiltinFunction>>,
}

/// 組み込み関数インターフェース
#[async_trait]
pub trait BuiltinFunction: Send + Sync {
    /// 関数名
    fn name(&self) -> &str;
    /// 関数を実行
    async fn execute(&self, args: Vec<Value>, context: &mut EvaluationContext) -> Result<Value>;
}

impl EvaluationEngine {
    /// 新しい評価エンジンを作成
    pub fn new(env: Arc<Environment>) -> Self {
        let expander = Expander::new(env.clone());
        let builtin_functions = DashMap::new();
        
        let mut engine = Self {
            env,
            expander,
            builtin_functions,
        };
        
        // 標準関数の登録
        engine.register_standard_functions();
        
        engine
    }
    
    /// 標準関数を登録
    fn register_standard_functions(&self) {
        // TODO: 標準関数のセットアップ
    }
    
    /// 組み込み関数を登録
    pub fn register_function(&self, function: Arc<dyn BuiltinFunction>) {
        self.builtin_functions.insert(function.name().to_string(), function);
    }
    
    /// 式を評価
    pub fn evaluate(&self, expression: &str) -> Result<Value> {
        // グローバルスコープを作成
        let scope = Arc::new(Scope::new());
        let mut context = EvaluationContext {
            scope,
            functions: HashMap::new(),
            env: self.env.clone(),
            current_dir: std::env::current_dir()?,
            last_result: None,
            loop_control: None,
            return_value: None,
        };
        
        // パースして評価
        let mut parser = Parser::new(expression);
        let ast = parser.parse_program()?;
        
        self.evaluate_node(&ast, &mut context)
    }
    
    /// AST評価（スタブ実装）
    fn evaluate_node(&self, node: &AstNode, context: &mut EvaluationContext) -> Result<Value> {
        match &node.node_type {
            AstNodeType::Program(statements) => {
                let mut result = Value::Null;
                
                for stmt in statements {
                    result = self.evaluate_node(stmt, context)?;
                    
                    // 制御フローをチェック
                    if context.loop_control.is_some() || context.return_value.is_some() {
                        break;
                    }
                }
                
                Ok(result)
            },
            AstNodeType::Literal(value) => {
                // リテラル値をそのまま返す
                Ok(value.clone())
            },
            AstNodeType::VariableReference(name) => {
                // 変数の値を取得
                if let Some(value) = context.scope.get(name) {
                    Ok(value)
                } else if let Some(env_value) = self.env.get(name) {
                    Ok(Value::String(env_value))
                } else {
                    Err(anyhow!("未定義の変数: {}", name))
                }
            },
            AstNodeType::VariableDeclaration { name, value, is_local, is_readonly } => {
                // 値を評価
                let evaluated_value = self.evaluate_node(value, context)?;
                
                // スコープに変数を設定
                let mut scope = context.scope.clone();
                if *is_local {
                    let mut scope_ref = Arc::make_mut(&mut scope);
                    scope_ref.set(name, evaluated_value.clone());
                } else {
                    // グローバルスコープを取得（実際の実装では修正が必要）
                    let mut scope_ref = Arc::make_mut(&mut scope);
                    scope_ref.set(name, evaluated_value.clone());
                    
                    // readonlyまたはexportの場合は環境変数にも設定
                    if *is_readonly {
                        // 実際の実装では読み取り専用フラグを設定
                    }
                    
                    // 環境変数として設定（実際の実装では追加処理が必要）
                    self.env.set(name, &evaluated_value.to_string());
                }
                context.scope = scope;
                
                Ok(evaluated_value)
            },
            AstNodeType::CommandExecution { command, arguments, redirects } => {
                // コマンド名を展開
                let command_name = match self.expand_string(command, context) {
                    Ok(expanded) => expanded,
                    Err(e) => {
                        debug!("コマンド名の展開に失敗: {}", e);
                        command.clone()
                    }
                };
                
                let mut args = Vec::new();
                
                // 引数を評価して展開
                for arg in arguments {
                    let evaluated_arg = self.evaluate_node(arg, context)?;
                    // 引数の変数展開を試みる
                    let arg_str = match self.expand_string(&evaluated_arg.to_string(), context) {
                        Ok(expanded) => expanded,
                        Err(_) => evaluated_arg.to_string(),
                    };
                    args.push(arg_str);
                }
                
                debug!("コマンド実行: {} {:?}", command_name, args);
                
                // リダイレクトを処理
                let mut input_from = None;
                let mut output_to = None;
                let mut error_to = None;
                let mut append_mode = false;
                let mut error_append_mode = false;
                
                for redirect in redirects {
                    // リダイレクトターゲットを評価
                    let target_value = self.evaluate_node(&redirect.target, context)?;
                    let target_str = target_value.to_string();
                    
                    match redirect.redirect_type {
                        RedirectType::Input => {
                            debug!("入力リダイレクト: {}", target_str);
                            // ファイルからの入力リダイレクト
                            input_from = Some(target_str);
                        },
                        RedirectType::Output => {
                            debug!("出力リダイレクト: {}", target_str);
                            // ファイルへの出力リダイレクト
                            output_to = Some(target_str);
                            append_mode = false;
                        },
                        RedirectType::Append => {
                            debug!("追記リダイレクト: {}", target_str);
                            // ファイルへの追記リダイレクト
                            output_to = Some(target_str);
                            append_mode = true;
                        },
                        RedirectType::Error => {
                            debug!("エラー出力リダイレクト: {}", target_str);
                            // エラー出力のリダイレクト
                            error_to = Some(target_str);
                            error_append_mode = false;
                        },
                        RedirectType::ErrorAppend => {
                            debug!("エラー出力追記リダイレクト: {}", target_str);
                            // エラー出力の追記リダイレクト
                            error_to = Some(target_str);
                            error_append_mode = true;
                        },
                        RedirectType::OutputAndError => {
                            debug!("標準出力＆エラー出力リダイレクト: {}", target_str);
                            // 標準出力とエラー出力を同じファイルにリダイレクト
                            output_to = Some(target_str.clone());
                            error_to = Some(target_str);
                            append_mode = false;
                            error_append_mode = false;
                        },
                    }
                }
                
                // 組み込み関数を探す
                if let Some(builtin) = self.builtin_functions.get(&command_name) {
                    // 組み込み関数を実行
                    let args_values: Vec<Value> = args.into_iter()
                        .map(|s| Value::String(s))
                        .collect();
                    
                    // 入出力リダイレクトの情報をコンテキストに追加
                    let old_input = context.env.get("STDIN_REDIRECT");
                    let old_output = context.env.get("STDOUT_REDIRECT");
                    let old_error = context.env.get("STDERR_REDIRECT");
                    
                    if let Some(input) = &input_from {
                        self.env.set("STDIN_REDIRECT", input);
                    }
                    if let Some(output) = &output_to {
                        self.env.set("STDOUT_REDIRECT", output);
                        if append_mode {
                            self.env.set("STDOUT_APPEND", "true");
                        } else {
                            self.env.set("STDOUT_APPEND", "false");
                        }
                    }
                    if let Some(error) = &error_to {
                        self.env.set("STDERR_REDIRECT", error);
                        if error_append_mode {
                            self.env.set("STDERR_APPEND", "true");
                        } else {
                            self.env.set("STDERR_APPEND", "false");
                        }
                    }
                    
                    // 関数実行
                    let result = match builtin.execute(args_values, context).await {
                        Ok(result) => {
                            context.last_result = Some(result.clone());
                            Ok(result)
                        }
                        Err(e) => {
                            // エラーを記録して返す
                            error!("組み込み関数 {} の実行エラー: {}", command_name, e);
                            context.last_result = Some(Value::Integer(1)); // エラーコード
                            Err(e)
                        }
                    };
                    
                    // リダイレクト設定を元に戻す
                    if let Some(input) = old_input {
                        self.env.set("STDIN_REDIRECT", &input);
                    } else {
                        self.env.remove("STDIN_REDIRECT");
                    }
                    if let Some(output) = old_output {
                        self.env.set("STDOUT_REDIRECT", &output);
                    } else {
                        self.env.remove("STDOUT_REDIRECT");
                    }
                    if let Some(error) = old_error {
                        self.env.set("STDERR_REDIRECT", &error);
                    } else {
                        self.env.remove("STDERR_REDIRECT");
                    }
                    
                    result
                } else {
                    // 外部コマンドとして実行
                    debug!("外部コマンド実行: {} {:?}", command_name, args);
                    
                    // コマンド実行前にセキュリティチェック
                    let is_allowed = match context.env.get("SECURITY_CHECK_ENABLED") {
                        Some(val) if val == "true" => {
                            // TODO: セキュリティマネージャーによるチェック
                            // ここでは常に許可する
                            true
                        },
                        _ => true,
                    };
                    
                    if !is_allowed {
                        error!("コマンド {} の実行が許可されていません", command_name);
                        context.last_result = Some(Value::Integer(126)); // permission denied
                        return Err(anyhow!("セキュリティポリシーによりコマンド {} の実行が拒否されました", command_name));
                    }
                    
                    // 実際のシェルでは、ここでコマンド実行を行う
                    // ここではモック実装
                    let mock_result = format!("コマンド {} を実行しました（引数: {:?}）", command_name, args);
                    let exit_code = 0; // 成功を表す終了コード
                    
                    // 終了コードをコンテキストと環境変数に設定
                    context.last_result = Some(Value::Integer(exit_code));
                    self.env.set("?", &exit_code.to_string());
                    
                    Ok(Value::String(mock_result))
                }
            },
            AstNodeType::IfStatement { condition, then_branch, else_branch } => {
                // 条件を評価
                let condition_value = self.evaluate_node(condition, context)?;
                
                if condition_value.to_boolean() {
                    // thenブランチを評価
                    self.evaluate_node(then_branch, context)
                } else if let Some(else_branch) = else_branch {
                    // elseブランチを評価
                    self.evaluate_node(else_branch, context)
                } else {
                    // else節がない場合はnull
                    Ok(Value::Null)
                }
            },
            AstNodeType::ForStatement { variable, iterable, body } => {
                // イテラブルを評価
                let iterable_value = self.evaluate_node(iterable, context)?;
                
                // イテラブルを配列に変換
                let items: Vec<Value> = match &iterable_value {
                    Value::Array(arr) => arr.clone(),
                    Value::String(s) => {
                        // 文字列の処理方法を選択（空白区切りまたは文字ごと）
                        let is_expanded = s.contains(' ');
                        
                        if is_expanded {
                            // 文字列を空白で分割
                            s.split_whitespace()
                                .map(|s| Value::String(s.to_string()))
                                .collect()
                        } else {
                            // 空白がない場合、環境変数展開を試みる
                            if let Ok(expanded) = self.expand_string(s, context) {
                                // 展開結果を空白で分割
                                expanded.split_whitespace()
                                    .map(|s| Value::String(s.to_string()))
                                    .collect()
                            } else {
                                // 展開に失敗したら各文字を個別の要素として扱う
                                s.chars()
                                    .map(|c| Value::String(c.to_string()))
                                    .collect()
                            }
                        }
                    },
                    Value::Map(map) => {
                        // マップのキーと値をセットで反復
                        map.iter()
                            .map(|(k, v)| {
                                let mut entry = HashMap::new();
                                entry.insert("key".to_string(), Value::String(k.clone()));
                                entry.insert("value".to_string(), v.clone());
                                Value::Map(entry)
                            })
                            .collect()
                    },
                    Value::Integer(n) => {
                        // 数値は0からn-1までの範囲として扱う
                        if *n <= 0 {
                            Vec::new()
                        } else {
                            (0..*n).map(Value::Integer).collect()
                        }
                    },
                    _ => return Err(anyhow!("forステートメントに非イテラブルな値が使用されました: {:?}", iterable_value)),
                };
                
                debug!("forループ: 変数={}, 要素数={}", variable, items.len());
                let mut result = Value::Null;
                
                // 各アイテムに対してループを実行
                for (i, item) in items.into_iter().enumerate() {
                    // ループ変数を設定
                    let mut new_scope = Arc::new(Scope::with_parent(context.scope.clone()));
                    Arc::make_mut(&mut new_scope).set(variable, item.clone());
                    
                    // ループインデックスも設定
                    Arc::make_mut(&mut new_scope).set(&format!("{}_index", variable), Value::Integer(i as i64));
                    
                    // 元のスコープを一時保存
                    let old_scope = context.scope.clone();
                    context.scope = new_scope;
                    
                    // ループ本体を実行
                    result = self.evaluate_node(body, context)?;
                    
                    // スコープを元に戻す
                    context.scope = old_scope;
                    
                    // ループ制御をチェック
                    if let Some(control) = &context.loop_control {
                        if *control == LoopControl::Break {
                            context.loop_control = None;
                            break;
                        } else if *control == LoopControl::Continue {
                            context.loop_control = None;
                            continue;
                        }
                    }
                    
                    // return文をチェック
                    if context.return_value.is_some() {
                        break;
                    }
                }
                
                Ok(result)
            },
            AstNodeType::WhileStatement { condition, body } => {
                let mut result = Value::Null;
                let mut iteration_count = 0;
                const MAX_ITERATIONS_WARNING = 1000;
                const MAX_ITERATIONS_ERROR = 10000;
                
                // 条件が真である限りループを実行
                loop {
                    iteration_count += 1;
                    
                    // 無限ループ検出
                    if iteration_count == MAX_ITERATIONS_WARNING {
                        warn!("whileループが{}回以上実行されています。無限ループの可能性があります。", MAX_ITERATIONS_WARNING);
                    }
                    if iteration_count > MAX_ITERATIONS_ERROR {
                        return Err(anyhow!("whileループが{}回を超えて実行されました。無限ループを停止します。", MAX_ITERATIONS_ERROR));
                    }
                    
                    // 条件を評価
                    let condition_value = self.evaluate_node(condition, context)?;
                    
                    if !condition_value.to_boolean() {
                        break;
                    }
                    
                    // ループ本体を実行
                    result = self.evaluate_node(body, context)?;
                    
                    // ループ制御をチェック
                    if let Some(control) = &context.loop_control {
                        if *control == LoopControl::Break {
                            context.loop_control = None;
                            break;
                        } else if *control == LoopControl::Continue {
                            context.loop_control = None;
                            continue;
                        }
                    }
                    
                    // return文をチェック
                    if context.return_value.is_some() {
                        break;
                    }
                }
                
                Ok(result)
            },
            AstNodeType::FunctionDefinition { name, parameters, body } => {
                debug!("関数定義: {} (パラメータ: {:?})", name, parameters);
                
                // 関数メタデータを作成
                let function_metadata = Value::Map({
                    let mut metadata = HashMap::new();
                    metadata.insert("parameters".to_string(), Value::Array(
                        parameters.iter()
                            .map(|p| Value::String(p.clone()))
                            .collect()
                    ));
                    metadata.insert("body".to_string(), Value::String(format!("{:?}", body)));
                    metadata
                });
                
                // 関数をコンテキストに登録
                context.functions.insert(name.clone(), *body.clone());
                
                // 関数メタデータをスコープに保存
                let mut scope = context.scope.clone();
                Arc::make_mut(&mut scope).set(&format!("__func_{}", name), function_metadata);
                context.scope = scope;
                
                // 関数定義自体は何も返さない
                Ok(Value::Null)
            },
            AstNodeType::FunctionCall { name, arguments } => {
                // 関数を探す
                if let Some(function_body) = context.functions.get(name) {
                    // 引数を評価
                    let mut arg_values = Vec::new();
                    for arg in arguments {
                        let value = self.evaluate_node(arg, context)?;
                        arg_values.push(value);
                    }
                    
                    // 新しいスコープを作成
                    let new_scope = Arc::new(Scope::with_parent(context.scope.clone()));
                    let old_scope = context.scope.clone();
                    context.scope = new_scope;
                    
                    // 関数本体を実行
                    let result = self.evaluate_node(&function_body, context)?;
                    
                    // スコープを元に戻す
                    context.scope = old_scope;
                    
                    // return値があればそれを返す
                    if let Some(return_value) = context.return_value.take() {
                        Ok(return_value)
                    } else {
                        Ok(result)
                    }
                } else if let Some(builtin) = self.builtin_functions.get(name) {
                    // 組み込み関数を実行
                    let mut arg_values = Vec::new();
                    for arg in arguments {
                        let value = self.evaluate_node(arg, context)?;
                        arg_values.push(value);
                    }
                    
                    builtin.execute(arg_values, context).await
                } else {
                    Err(anyhow!("未定義の関数: {}", name))
                }
            },
            AstNodeType::ReturnStatement(value_opt) => {
                debug!("return文を実行");
                
                // 戻り値を評価
                let return_value = if let Some(value) = value_opt {
                    self.evaluate_node(value, context)?
                } else {
                    Value::Null
                };
                
                // 戻り値をコンテキストに設定
                context.return_value = Some(return_value.clone());
                debug!("return値を設定: {:?}", return_value);
                
                Ok(return_value)
            },
            AstNodeType::BreakStatement => {
                debug!("break文を実行");
                // break文を設定
                context.loop_control = Some(LoopControl::Break);
                // break文はループを囲むスコープで処理されるので
                // ここでは何もせずにnullを返す
                Ok(Value::Null)
            },
            AstNodeType::ContinueStatement => {
                debug!("continue文を実行");
                // continue文を設定
                context.loop_control = Some(LoopControl::Continue);
                // continue文はループを囲むスコープで処理されるので
                // ここでは何もせずにnullを返す
                Ok(Value::Null)
            },
            AstNodeType::Block(statements) => {
                let mut result = Value::Null;
                
                // 新しいスコープを作成
                let new_scope = Arc::new(Scope::with_parent(context.scope.clone()));
                let old_scope = context.scope.clone();
                context.scope = new_scope;
                
                // 各ステートメントを評価
                for stmt in statements {
                    result = self.evaluate_node(stmt, context)?;
                    
                    // 制御フローをチェック
                    if context.loop_control.is_some() || context.return_value.is_some() {
                        break;
                    }
                }
                
                // スコープを元に戻す
                context.scope = old_scope;
                
                Ok(result)
            },
            AstNodeType::Pipeline(commands) => {
                // パイプラインの実装
                let mut result = Value::Null;
                let mut last_output: Option<String> = None;
                
                // 各コマンドを順番に実行
                for (i, cmd) in commands.iter().enumerate() {
                    // 前のコマンドの出力を次のコマンドの入力として渡す
                    if let Some(output) = last_output.take() {
                        // パイプライン中間コマンド用の一時的なコンテキストを作成
                        let mut pipe_context = EvaluationContext {
                            scope: context.scope.clone(),
                            functions: context.functions.clone(),
                            env: context.env.clone(),
                            current_dir: context.current_dir.clone(),
                            last_result: context.last_result.clone(),
                            loop_control: None,
                            return_value: None,
                        };
                        
                        // 前のコマンドの出力を特殊変数に保存
                        Arc::make_mut(&mut pipe_context.scope).set("PIPE_IN", Value::String(output));
                        
                        // コマンドを実行
                        result = self.evaluate_node(cmd, &mut pipe_context)?;
                        
                        // 制御フローの状態を親コンテキストに反映
                        context.loop_control = pipe_context.loop_control;
                        context.return_value = pipe_context.return_value;
                        context.last_result = pipe_context.last_result;
                    } else {
                        // パイプラインの最初のコマンド
                        result = self.evaluate_node(cmd, context)?;
                    }
                    
                    // 結果を文字列に変換して次のコマンドの入力にする（最後のコマンド以外）
                    if i < commands.len() - 1 {
                        last_output = Some(result.to_string());
                    }
                    
                    // 制御フローチェック
                    if context.loop_control.is_some() || context.return_value.is_some() {
                        break;
                    }
                }
                
                Ok(result)
            },
            AstNodeType::BinaryOperation { left, operator, right } => {
                // 左辺と右辺を評価
                let left_value = self.evaluate_node(left, context)?;
                
                // ショートサーキット評価のための特別処理
                if operator == "&&" {
                    let left_bool = left_value.to_boolean();
                    if !left_bool {
                        return Ok(Value::Boolean(false));
                    }
                    let right_value = self.evaluate_node(right, context)?;
                    return Ok(Value::Boolean(right_value.to_boolean()));
                } else if operator == "||" {
                    let left_bool = left_value.to_boolean();
                    if left_bool {
                        return Ok(Value::Boolean(true));
                    }
                    let right_value = self.evaluate_node(right, context)?;
                    return Ok(Value::Boolean(right_value.to_boolean()));
                }
                
                // 通常の二項演算の場合は右辺も評価
                let right_value = self.evaluate_node(right, context)?;
                
                // 演算子に基づいて計算
                match operator.as_str() {
                    // 算術演算
                    "+" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a + *b as f64)),
                            (Value::String(a), Value::String(b)) => Ok(Value::String(a.clone() + b)),
                            (Value::String(a), _) => Ok(Value::String(a.clone() + &right_value.to_string())),
                            (_, Value::String(b)) => Ok(Value::String(left_value.to_string() + b)),
                            (Value::Array(a), Value::Array(b)) => {
                                let mut result = a.clone();
                                result.extend(b.clone());
                                Ok(Value::Array(result))
                            },
                            _ => Err(anyhow!("無効な演算: {:?} + {:?}", left_value, right_value)),
                        }
                    },
                    "-" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a - *b as f64)),
                            _ => Err(anyhow!("無効な演算: {:?} - {:?}", left_value, right_value)),
                        }
                    },
                    "*" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a * *b as f64)),
                            (Value::String(a), Value::Integer(b)) => {
                                // 文字列の繰り返し
                                if *b < 0 {
                                    Err(anyhow!("文字列を負の回数繰り返すことはできません: {} * {}", a, b))
                                } else {
                                    Ok(Value::String(a.repeat(*b as usize)))
                                }
                            },
                            (Value::Array(a), Value::Integer(b)) => {
                                // 配列の繰り返し
                                if *b < 0 {
                                    Err(anyhow!("配列を負の回数繰り返すことはできません: {:?} * {}", a, b))
                                } else {
                                    let mut result = Vec::new();
                                    for _ in 0..*b {
                                        result.extend(a.clone());
                                    }
                                    Ok(Value::Array(result))
                                }
                            },
                            _ => Err(anyhow!("無効な演算: {:?} * {:?}", left_value, right_value)),
                        }
                    },
                    "/" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => {
                                if *b == 0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Integer(a / b))
                                }
                            },
                            (Value::Float(a), Value::Float(b)) => {
                                if *b == 0.0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Float(a / b))
                                }
                            },
                            (Value::Integer(a), Value::Float(b)) => {
                                if *b == 0.0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Float(*a as f64 / b))
                                }
                            },
                            (Value::Float(a), Value::Integer(b)) => {
                                if *b == 0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Float(a / *b as f64))
                                }
                            },
                            _ => Err(anyhow!("無効な演算: {:?} / {:?}", left_value, right_value)),
                        }
                    },
                    "%" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => {
                                if *b == 0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Integer(a % b))
                                }
                            },
                            (Value::Float(a), Value::Float(b)) => {
                                if *b == 0.0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Float(a % b))
                                }
                            },
                            (Value::Integer(a), Value::Float(b)) => {
                                if *b == 0.0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Float((*a as f64) % b))
                                }
                            },
                            (Value::Float(a), Value::Integer(b)) => {
                                if *b == 0 {
                                    Err(anyhow!("ゼロ除算"))
                                } else {
                                    Ok(Value::Float(a % (*b as f64)))
                                }
                            },
                            _ => Err(anyhow!("無効な演算: {:?} % {:?}", left_value, right_value)),
                        }
                    },
                    "**" => { // 冪乗
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => {
                                if *b < 0 {
                                    // 負の指数の場合、浮動小数点で計算
                                    Ok(Value::Float((*a as f64).powf(*b as f64)))
                                } else {
                                    // 正の指数の場合、整数で計算
                                    Ok(Value::Integer(a.pow(*b as u32)))
                                }
                            },
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a.powi(*b as i32))),
                            _ => Err(anyhow!("無効な演算: {:?} ** {:?}", left_value, right_value)),
                        }
                    },
                    // 比較演算
                    "==" => Ok(Value::Boolean(left_value.to_string() == right_value.to_string())),
                    "!=" => Ok(Value::Boolean(left_value.to_string() != right_value.to_string())),
                    "===" => { // 型も含めて厳密比較
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a == b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a == b)),
                            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a == b)),
                            (Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a == b)),
                            // 型が異なる場合は常にfalse
                            _ => Ok(Value::Boolean(false)),
                        }
                    },
                    "!==" => { // 型も含めて厳密不等価
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a != b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a != b)),
                            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a != b)),
                            (Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a != b)),
                            // 型が異なる場合は常にtrue
                            _ => Ok(Value::Boolean(true)),
                        }
                    },
                    ">" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a > b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a > b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Boolean((*a as f64) > *b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Boolean(*a > (*b as f64))),
                            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a > b)),
                            _ => Err(anyhow!("無効な比較: {:?} > {:?}", left_value, right_value)),
                        }
                    },
                    "<" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a < b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a < b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Boolean((*a as f64) < *b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Boolean(*a < (*b as f64))),
                            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a < b)),
                            _ => Err(anyhow!("無効な比較: {:?} < {:?}", left_value, right_value)),
                        }
                    },
                    ">=" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a >= b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a >= b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Boolean((*a as f64) >= *b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Boolean(*a >= (*b as f64))),
                            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a >= b)),
                            _ => Err(anyhow!("無効な比較: {:?} >= {:?}", left_value, right_value)),
                        }
                    },
                    "<=" => {
                        match (&left_value, &right_value) {
                            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a <= b)),
                            (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a <= b)),
                            (Value::Integer(a), Value::Float(b)) => Ok(Value::Boolean((*a as f64) <= *b)),
                            (Value::Float(a), Value::Integer(b)) => Ok(Value::Boolean(*a <= (*b as f64))),
                            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a <= b)),
                            _ => Err(anyhow!("無効な比較: {:?} <= {:?}", left_value, right_value)),
                        }
                    },
                    // 文字列演算
                    "=~" => { // 正規表現マッチ
                        let str_value = left_value.to_string();
                        let pattern = right_value.to_string();
                        
                        match Regex::new(&pattern) {
                            Ok(re) => Ok(Value::Boolean(re.is_match(&str_value))),
                            Err(e) => Err(anyhow!("無効な正規表現パターン '{}': {}", pattern, e)),
                        }
                    },
                    // その他
                    _ => Err(anyhow!("未知の演算子: {}", operator)),
                }
            },
            AstNodeType::UnaryOperation { operator, operand } => {
                // 被演算子を評価
                let operand_value = self.evaluate_node(operand, context)?;
                
                // 演算子に基づいて計算
                match operator.as_str() {
                    "-" => {
                        match &operand_value {
                            Value::Integer(a) => Ok(Value::Integer(-a)),
                            Value::Float(a) => Ok(Value::Float(-a)),
                            _ => Err(anyhow!("無効な単項演算: -{:?}", operand_value)),
                        }
                    },
                    "!" => {
                        Ok(Value::Boolean(!operand_value.to_boolean()))
                    },
                    "+" => {
                        // 単項プラスは値をそのまま返す
                        match &operand_value {
                            Value::Integer(_) | Value::Float(_) => Ok(operand_value),
                            Value::String(s) => {
                                // 文字列を数値に変換
                                if let Ok(i) = s.parse::<i64>() {
                                    Ok(Value::Integer(i))
                                } else if let Ok(f) = s.parse::<f64>() {
                                    Ok(Value::Float(f))
                                } else {
                                    Err(anyhow!("文字列 '{}' を数値に変換できません", s))
                                }
                            },
                            _ => Err(anyhow!("無効な単項演算: +{:?}", operand_value)),
                        }
                    },
                    "~" => {
                        // ビット反転
                        match &operand_value {
                            Value::Integer(a) => Ok(Value::Integer(!a)),
                            _ => Err(anyhow!("無効なビット反転: ~{:?}", operand_value)),
                        }
                    },
                    "++" => {
                        // インクリメント（前置）
                        match &operand_value {
                            Value::Integer(a) => Ok(Value::Integer(a + 1)),
                            Value::Float(a) => Ok(Value::Float(a + 1.0)),
                            _ => Err(anyhow!("無効なインクリメント: ++{:?}", operand_value)),
                        }
                    },
                    "--" => {
                        // デクリメント（前置）
                        match &operand_value {
                            Value::Integer(a) => Ok(Value::Integer(a - 1)),
                            Value::Float(a) => Ok(Value::Float(a - 1.0)),
                            _ => Err(anyhow!("無効なデクリメント: --{:?}", operand_value)),
                        }
                    },
                    "typeof" => {
                        // 型を返す
                        let type_name = match &operand_value {
                            Value::String(_) => "string",
                            Value::Integer(_) => "integer",
                            Value::Float(_) => "float",
                            Value::Boolean(_) => "boolean",
                            Value::Array(_) => "array",
                            Value::Map(_) => "object",
                            Value::Null => "null",
                        };
                        Ok(Value::String(type_name.to_string()))
                    },
                    "defined" => {
                        // 変数が定義されているかチェック
                        if let AstNodeType::VariableReference(name) = &operand.node_type {
                            Ok(Value::Boolean(context.scope.has(name) || self.env.has(name)))
                        } else {
                            Err(anyhow!("defined演算子は変数参照にのみ使用できます"))
                        }
                    },
                    _ => Err(anyhow!("未知の単項演算子: {}", operator)),
                }
            },
            // その他のノードタイプは必要に応じて実装
            _ => Err(anyhow!("未実装のノードタイプ: {:?}", node.node_type)),
        }
    }
    
    /// スクリプトを評価（スタブ実装）
    pub async fn evaluate_script(&self, script_text: &str) -> Result<Value> {
        debug!("スクリプト評価開始");
        
        // パーサーを作成して構文解析
        let mut parser = Parser::new(script_text);
        let ast = match parser.parse_program() {
            Ok(ast) => ast,
            Err(e) => {
                error!("スクリプト解析エラー: {}", e);
                return Err(anyhow!("構文解析エラー: {}", e));
            }
        };
        
        // 評価コンテキストを作成
        let scope = Arc::new(Scope::new());
        let mut context = EvaluationContext {
            scope,
            functions: HashMap::new(),
            env: self.env.clone(),
            current_dir: env::current_dir()?,
            last_result: None,
            loop_control: None,
            return_value: None,
        };
        
        // スクリプト引数をセットアップ
        if let Some(args) = self.env.get("SCRIPT_ARGS") {
            let args_vec: Vec<Value> = args.split_whitespace()
                .map(|s| Value::String(s.to_string()))
                .collect();
            
            // $0, $1, $2, ... を設定
            if let Some(script_name) = self.env.get("SCRIPT_NAME") {
                Arc::make_mut(&mut context.scope).set("0", Value::String(script_name));
            }
            
            for (i, arg) in args_vec.iter().enumerate() {
                Arc::make_mut(&mut context.scope).set(&(i + 1).to_string(), arg.clone());
            }
            
            // $# (引数の数) を設定
            Arc::make_mut(&mut context.scope).set("#", Value::Integer(args_vec.len() as i64));
            
            // $* (全引数を空白区切りで) を設定
            Arc::make_mut(&mut context.scope).set("*", Value::String(args));
            
            // $@ (全引数を配列で) を設定
            Arc::make_mut(&mut context.scope).set("@", Value::Array(args_vec));
        }
        
        // スクリプト実行制限を設定
        let max_execution_time = match self.env.get("MAX_SCRIPT_EXECUTION_TIME") {
            Some(val) => val.parse::<u64>().unwrap_or(60),
            None => 60, // デフォルトは60秒
        };
        
        // 実行タイムアウトを設定（実際の実装ではfuturesタイムアウトを使用）
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(max_execution_time),
            async {
                // AST を評価
                let result = self.evaluate_node(&ast, &mut context);
                
                // 終了コードを設定
                let exit_code = match &result {
                    Ok(val) => {
                        match val {
                            Value::Integer(i) => *i,
                            Value::String(s) if s.is_empty() => 0,
                            Value::Boolean(b) => if *b { 0 } else { 1 },
                            Value::Null => 0,
                            _ => 0,
                        }
                    },
                    Err(_) => 1,
                };
                
                self.env.set("?", &exit_code.to_string());
                result
            }
        ).await;
        
        match result {
            Ok(eval_result) => eval_result,
            Err(_) => {
                error!("スクリプト実行がタイムアウトしました ({}秒)", max_execution_time);
                Err(anyhow!("スクリプト実行がタイムアウトしました ({}秒)", max_execution_time))
            }
        }
    }
    
    /// 文字列の変数展開
    pub fn expand_string(&self, text: &str, context: &EvaluationContext) -> Result<String> {
        self.expander.expand(text, &context.scope)
    }
} 