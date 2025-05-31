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
    /// シンボル
    Symbol(char),
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
        
        // トークン解析の実装
        // 空白をスキップ
        if current_char.is_whitespace() {
            self.skip_whitespace();
            return self.next_token();
        }
        
        // コメントをスキップ
        if current_char == '#' {
            while self.position < self.input.len() && self.current_char() != '\n' {
                self.advance();
            }
            return self.next_token();
        }
        
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
    /// リスト
    List { items: Vec<AstNode> },
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
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
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
    
    /// 期待するトークンタイプかどうかを確認し、一致すれば次のトークンに進む
    fn expect_token(&mut self, expected: &TokenType) -> Result<()> {
        if &self.current_token.token_type == expected {
            self.next_token();
            Ok(())
        } else {
            Err(anyhow!("期待されるトークン {:?} ではなく {:?} が見つかりました", 
                expected, self.current_token.token_type))
        }
    }
    
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
            _ => return Err(anyhow!("{}:{} 変数名が必要です", line, column)),
        };
        
        // 次のトークンへ
        self.next_token();
        
        // 代入演算子を確認
        if let TokenType::Operator(op) = &self.current_token.token_type {
            if op != "=" {
                return Err(anyhow!("{}:{} 代入演算子 '=' が必要です", line, column));
            }
        } else {
            return Err(anyhow!("{}:{} 代入演算子 '=' が必要です", line, column));
        }
        
        // 代入演算子をスキップ
        self.next_token();
        
        // 値を解析
        let value_expr = self.parse_expression()?;
        
        // 文の終了を確認（セミコロンまたは改行）
        if !matches!(self.current_token.token_type, 
                    TokenType::Semicolon | TokenType::Newline | TokenType::EOF) {
            return Err(anyhow!("{}:{} 文の終了にはセミコロンまたは改行が必要です", 
                            self.current_token.line, self.current_token.column));
        }
        
        // セミコロンがある場合はスキップ
        if matches!(self.current_token.token_type, TokenType::Semicolon | TokenType::Newline) {
            self.next_token();
        }
        
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
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「readonly」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "readonly" {
                self.next_token();
            } else {
                return Err(anyhow!("'readonly'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'readonly'キーワードが必要です"));
        }
        
        // 変数名の取得
        let var_name;
        if let TokenType::Identifier(name) = &self.current_token.token_type {
            var_name = name.clone();
            self.next_token();
        } else {
            return Err(anyhow!("変数名が必要です"));
        }
        
        // 等号「=」があるか確認
        let has_value = if let TokenType::Operator(op) = &self.current_token.token_type {
            if op == "=" {
                self.next_token();
                true
            } else {
                false
            }
        } else {
            false
        };
        
        // 値の解析（存在する場合）
        let value = if has_value {
            self.parse_expression()?
        } else {
            // 値が指定されていない場合は空文字列をデフォルト値とする
            AstNode {
                node_type: AstNodeType::Literal(Value::String(String::new())),
                line: self.current_token.line,
                column: self.current_token.column,
            }
        };
        
        // readonly変数宣言のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::VariableDeclaration {
                name: var_name,
                value: Box::new(value),
                is_local: true,
                is_readonly: true,
            },
            line,
            column,
        })
    }
    
    /// exportを解析
    fn parse_export_declaration(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「export」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "export" {
                self.next_token();
            } else {
                return Err(anyhow!("'export'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'export'キーワードが必要です"));
        }
        
        // 変数名の取得
        let var_name;
        if let TokenType::Identifier(name) = &self.current_token.token_type {
            var_name = name.clone();
            self.next_token();
        } else {
            return Err(anyhow!("変数名が必要です"));
        }
        
        // 等号「=」があるか確認
        let has_value = if let TokenType::Operator(op) = &self.current_token.token_type {
            if op == "=" {
                self.next_token();
                true
            } else {
                false
            }
        } else {
            false
        };
        
        // 値の解析（存在する場合）
        let value = if has_value {
            self.parse_expression()?
        } else {
            // 値が指定されていない場合は既存の変数を参照
            AstNode {
                node_type: AstNodeType::VariableReference(var_name.clone()),
                line: self.current_token.line,
                column: self.current_token.column,
            }
        };
        
        // export変数宣言のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::VariableDeclaration {
                name: var_name,
                value: Box::new(value),
                is_local: false, // exportされた変数はグローバル
                is_readonly: false,
            },
            line,
            column,
        })
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
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「for」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "for" {
                self.next_token();
            } else {
                return Err(anyhow!("'for'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'for'キーワードが必要です"));
        }
        
        // ループ変数名を取得
        let variable_name;
        if let TokenType::Identifier(name) = &self.current_token.token_type {
            variable_name = name.clone();
            self.next_token();
        } else {
            return Err(anyhow!("ループ変数名が必要です"));
        }
        
        // 「in」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "in" {
                self.next_token();
            } else {
                return Err(anyhow!("'in'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'in'キーワードが必要です"));
        }
        
        // イテレート対象を解析
        let mut iterable_items = Vec::new();
        
        // 「do」キーワードが現れるまでイテレート対象を収集
        while let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "do" {
                break;
            }
            
            // 式を解析してイテレート対象として追加
            let item = self.parse_expression()?;
            iterable_items.push(item);
            
            self.next_token();
        }
        
        // 「do」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "do" {
                self.next_token();
            } else {
                return Err(anyhow!("'do'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'do'キーワードが必要です"));
        }
        
        // ループ本体を解析
        let mut body_statements = Vec::new();
        
        // 「done」キーワードが現れるまでステートメントを解析
        while let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "done" {
                break;
            }
            
            let statement = self.parse_statement()?;
            body_statements.push(statement);
        }
        
        // 「done」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "done" {
                self.next_token();
            } else {
                return Err(anyhow!("'done'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'done'キーワードが必要です"));
        }
        
        // 本体のブロックを作成
        let body = AstNode {
            node_type: AstNodeType::Block { statements: body_statements },
            line,
            column,
        };
        
        // for文のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::ForStatement {
                variable: variable_name,
                iterable: AstNodeType::List { items: iterable_items },
                body: Box::new(body),
            },
            line,
            column,
        })
    }
    
    /// while文を解析
    fn parse_while_statement(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「while」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "while" {
                self.next_token();
            } else {
                return Err(anyhow!("'while'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'while'キーワードが必要です"));
        }
        
        // 条件式を解析
        let condition = self.parse_expression()?;
        
        // 「do」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "do" {
                self.next_token();
            } else {
                return Err(anyhow!("'do'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'do'キーワードが必要です"));
        }
        
        // ループ本体を解析
        let mut body_statements = Vec::new();
        
        // 「done」キーワードが現れるまでステートメントを解析
        while let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "done" {
                break;
            }
            
            let statement = self.parse_statement()?;
            body_statements.push(statement);
        }
        
        // 「done」キーワードを確認
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "done" {
                self.next_token();
            } else {
                return Err(anyhow!("'done'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'done'キーワードが必要です"));
        }
        
        // 本体のブロックを作成
        let body = AstNode {
            node_type: AstNodeType::Block { statements: body_statements },
            line,
            column,
        };
        
        // while文のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::WhileStatement {
                condition: Box::new(condition),
                body: Box::new(body),
            },
            line,
            column,
        })
    }
    
    /// 関数定義を解析
    fn parse_function_definition(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「function」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "function" {
                self.next_token();
            } else {
                return Err(anyhow!("'function'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'function'キーワードが必要です"));
        }
        
        // 関数名を取得
        let function_name;
        if let TokenType::Identifier(name) = &self.current_token.token_type {
            function_name = name.clone();
            self.next_token();
        } else {
            return Err(anyhow!("関数名が必要です"));
        }
        
        // 引数リストを解析
        let mut parameters = Vec::new();
        
        // 開き括弧を確認
        if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == "(" {
                self.next_token();
            } else {
                return Err(anyhow!("引数リスト開始には'('が必要です"));
            }
        } else {
            return Err(anyhow!("引数リスト開始には'('が必要です"));
        }
        
        // 引数がある場合は解析
        while let TokenType::Identifier(param) = &self.current_token.token_type {
            parameters.push(param.clone());
            self.next_token();
            
            // カンマがあれば次の引数へ
            if let TokenType::Symbol(s) = &self.current_token.token_type {
                if s == "," {
                    self.next_token();
                }
            }
        }
        
        // 閉じ括弧を確認
        if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == ")" {
                self.next_token();
            } else {
                return Err(anyhow!("引数リスト終了には')'が必要です"));
            }
        } else {
            return Err(anyhow!("引数リスト終了には')'が必要です"));
        }
        
        // 関数本体を解析
        let mut body_statements = Vec::new();
        
        // 開き中括弧を確認
        if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == "{" {
                self.next_token();
            } else {
                return Err(anyhow!("関数本体開始には開き括弧が必要です"));
            }
        } else {
            return Err(anyhow!("関数本体開始には開き括弧が必要です"));
        }
        
        // 閉じ中括弧が現れるまでステートメントを解析
        while let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == "}" {
                break;
            }
            
            let statement = self.parse_statement()?;
            body_statements.push(statement);
        }
        
        // 閉じ中括弧を確認
        if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == "}" {
                self.next_token();
            } else {
                return Err(anyhow!("関数本体終了には閉じ括弧が必要です"));
            }
        } else {
            return Err(anyhow!("関数本体終了には閉じ括弧が必要です"));
        }
        
        // 本体のブロックを作成
        let body = AstNode {
            node_type: AstNodeType::Block { statements: body_statements },
            line,
            column,
        };
        
        // 関数定義のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::FunctionDefinition {
                name: function_name,
                parameters,
                body: Box::new(body),
            },
            line,
            column,
        })
    }
    
    /// return文を解析
    fn parse_return_statement(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「return」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "return" {
                self.next_token();
            } else {
                return Err(anyhow!("'return'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'return'キーワードが必要です"));
        }
        
        // 返り値があるかチェック（セミコロンまたは改行が来たら式なし）
        let value = if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == ";" {
                // 返り値なし
                self.next_token();
                None
            } else {
                // 式を解析
                let expr = self.parse_expression()?;
                
                // セミコロンがあればスキップ
                if let TokenType::Symbol(s) = &self.current_token.token_type {
                    if s == ";" {
                        self.next_token();
                    }
                }
                
                Some(Box::new(expr))
            }
        } else {
            // 式を解析
            let expr = self.parse_expression()?;
            
            // セミコロンがあればスキップ
            if let TokenType::Symbol(s) = &self.current_token.token_type {
                if s == ";" {
                    self.next_token();
                }
            }
            
            Some(Box::new(expr))
        };
        
        // return文のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::ReturnStatement { value },
            line,
            column,
        })
    }
    
    /// break文を解析
    fn parse_break_statement(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「break」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "break" {
                self.next_token();
            } else {
                return Err(anyhow!("'break'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'break'キーワードが必要です"));
        }
        
        // セミコロンがあればスキップ
        if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == ";" {
                self.next_token();
            }
        }
        
        // break文のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::BreakStatement,
            line,
            column,
        })
    }
    
    /// continue文を解析
    fn parse_continue_statement(&mut self) -> Result<AstNode> {
        // 現在のトークン位置を保存
        let line = self.current_token.line;
        let column = self.current_token.column;
        
        // 「continue」キーワードをスキップ
        if let TokenType::Keyword(k) = &self.current_token.token_type {
            if k == "continue" {
                self.next_token();
            } else {
                return Err(anyhow!("'continue'キーワードが必要です"));
            }
        } else {
            return Err(anyhow!("'continue'キーワードが必要です"));
        }
        
        // セミコロンがあればスキップ
        if let TokenType::Symbol(s) = &self.current_token.token_type {
            if s == ";" {
                self.next_token();
            }
        }
        
        // continue文のASTノードを作成
        Ok(AstNode {
            node_type: AstNodeType::ContinueStatement,
            line,
            column,
        })
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
    /// セキュリティマネージャー
    security_manager: SecurityManager,
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
        let security_manager = SecurityManager::new();
        
        let mut engine = Self {
            env,
            expander,
            builtin_functions,
            security_manager,
        };
        
        // 標準関数の登録
        engine.register_standard_functions();
        
        engine
    }
    
    /// 標準関数を登録
    fn register_standard_functions(&self) {
        // 標準関数のセットアップ
        let env = self.env.clone();
        setup_standard_functions(&mut env.as_ref().clone()).unwrap();
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
        // トークン解析実装
        let tokens = tokenize_input(expression)?;
        
        // 構文解析実装
        let ast = parse_tokens(tokens)?;
        
        // readonly変数のチェック
        process_readonly_variables(&mut self.env.as_ref().clone(), &ast)?;
        
        // export変数の処理
        process_export_variables(&mut self.env.as_ref().clone(), &ast)?;
        
        // セキュリティチェック
        security_check(&ast, &self.security_manager)?;
        
        self.evaluate_node(&ast, &mut context)
    }
    
    /// AST評価
    fn evaluate_node(&self, node: &AstNode, context: &mut EvaluationContext) -> Result<Value> {
        use crate::runtime::evaluation::AstNodeType;
        match &node.node_type {
            AstNodeType::Literal(val) => Ok(val.clone()),
            AstNodeType::VariableReference(name) => {
                context.scope.get(name).or_else(|| context.env.get(name)).ok_or_else(|| anyhow!("変数{}が未定義", name))
            },
            AstNodeType::BinaryOperation { left, operator, right } => {
                let l = self.evaluate_node(left, context)?;
                let r = self.evaluate_node(right, context)?;
                // 簡易: +,-,*,/のみ対応
                match operator.as_str() {
                    "+" => Ok(Value::Integer(l.to_integer()? + r.to_integer()?)),
                    "-" => Ok(Value::Integer(l.to_integer()? - r.to_integer()?)),
                    "*" => Ok(Value::Integer(l.to_integer()? * r.to_integer()?)),
                    "/" => Ok(Value::Integer(l.to_integer()? / r.to_integer()?)),
                    _ => Err(anyhow!("未対応の演算子: {}", operator)),
                }
            },
            AstNodeType::UnaryOperation { operator, operand } => {
                let v = self.evaluate_node(operand, context)?;
                match operator.as_str() {
                    "-" => Ok(Value::Integer(-v.to_integer()?)),
                    _ => Err(anyhow!("未対応の単項演算子: {}", operator)),
                }
            },
            AstNodeType::VariableDeclaration { name, value, is_local, is_readonly } => {
                let val = self.evaluate_node(value, context)?;
                context.scope.set(name, val.clone());
                Ok(val)
            },
            AstNodeType::FunctionCall { name, arguments } => {
                let args: Result<Vec<_>> = arguments.iter().map(|a| self.evaluate_node(a, context)).collect();
                if let Some(func) = self.builtin_functions.get(name) {
                    futures::executor::block_on(func.execute(args?, context))
                } else {
                    Err(anyhow!("関数{}が未定義", name))
                }
            },
            AstNodeType::IfStatement { condition, then_branch, else_branch } => {
                let cond = self.evaluate_node(condition, context)?.to_boolean();
                if cond {
                    self.evaluate_node(then_branch, context)
                } else if let Some(else_b) = else_branch {
                    self.evaluate_node(else_b, context)
                } else {
                    Ok(Value::Null)
                }
            },
            AstNodeType::ForStatement { variable, iterable, body } => {
                let iter_val = self.evaluate_node(iterable, context)?;
                if let Value::Array(arr) = iter_val {
                    for v in arr {
                        context.scope.set(variable, v);
                        self.evaluate_node(body, context)?;
                    }
                    Ok(Value::Null)
                } else {
                    Err(anyhow!("forのイテレータが配列でない"))
                }
            },
            AstNodeType::WhileStatement { condition, body } => {
                while self.evaluate_node(condition, context)?.to_boolean() {
                    self.evaluate_node(body, context)?;
                }
                Ok(Value::Null)
            },
            AstNodeType::CommandExecution { command, arguments, .. } => {
                // コマンド実行（簡易）
                let args: Result<Vec<_>> = arguments.iter().map(|a| self.evaluate_node(a, context)).collect();
                let output = std::process::Command::new(command).args(args?.iter().map(|v| v.to_string())).output()?;
                Ok(Value::String(String::from_utf8_lossy(&output.stdout).to_string()))
            },
            AstNodeType::Pipeline(nodes) => {
                // パイプライン実行（左から右へ）
                let mut last = Value::Null;
                for node in nodes {
                    last = self.evaluate_node(node, context)?;
                }
                Ok(last)
            },
            AstNodeType::Block(stmts) => {
                let mut last = Value::Null;
                for stmt in stmts {
                    last = self.evaluate_node(stmt, context)?;
                }
                Ok(last)
            },
            _ => Ok(Value::Null),
        }
    }
    
    /// スクリプトを評価
    pub async fn evaluate_script(&self, script_text: &str) -> Result<Value> {
        // スクリプト全体をパースし、各文を順次評価
        let ast = crate::parser::parse(script_text)?;
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
        self.evaluate_node(&ast, &mut context)
    }
    
    /// 文字列の変数展開
    pub fn expand_string(&self, text: &str, context: &EvaluationContext) -> Result<String> {
        self.expander.expand(text, &context.scope)
    }
}

/// セキュリティマネージャークラス
pub struct SecurityManager {
    // 危険なコマンドのリスト
    dangerous_commands: HashSet<String>,
    // 危険な引数パターンのリスト（コマンド -> パターン）
    dangerous_args: HashMap<String, Vec<Regex>>,
    // アクセス制限パス
    restricted_paths: Vec<(Regex, bool)>, // (パターン, 書き込み許可)
}

impl SecurityManager {
    /// 新しいセキュリティマネージャーを作成
    pub fn new() -> Self {
        let mut manager = Self {
            dangerous_commands: HashSet::new(),
            dangerous_args: HashMap::new(),
            restricted_paths: Vec::new(),
        };
        
        // デフォルトの危険なコマンドを設定
        manager.dangerous_commands.insert("rm".to_string());
        manager.dangerous_commands.insert("rmdir".to_string());
        manager.dangerous_commands.insert("chmod".to_string());
        manager.dangerous_commands.insert("chown".to_string());
        
        // デフォルトの危険な引数を設定
        let mut rm_patterns = Vec::new();
        rm_patterns.push(Regex::new(r"^-[^-]*f").unwrap()); // -f フラグ
        rm_patterns.push(Regex::new(r"--force").unwrap());  // --force フラグ
        rm_patterns.push(Regex::new(r"^-[^-]*r").unwrap()); // -r フラグ
        rm_patterns.push(Regex::new(r"--recursive").unwrap()); // --recursive フラグ
        rm_patterns.push(Regex::new(r"^/").unwrap()); // ルートディレクトリからのパス
        manager.dangerous_args.insert("rm".to_string(), rm_patterns);
        
        // デフォルトの制限パスを設定
        manager.restricted_paths.push((Regex::new(r"^/etc").unwrap(), false));
        manager.restricted_paths.push((Regex::new(r"^/var").unwrap(), false));
        manager.restricted_paths.push((Regex::new(r"^/usr").unwrap(), false));
        manager.restricted_paths.push((Regex::new(r"^/bin").unwrap(), false));
        manager.restricted_paths.push((Regex::new(r"^/sbin").unwrap(), false));
        
        manager
    }
    
    /// コマンド実行が許可されているかチェック
    pub fn can_execute_command(&self, command: &str) -> bool {
        // ここでさらに詳細なチェックを実装できる
        // 現在は単純にTrueを返す
        true
    }
    
    /// 危険なコマンドかどうかチェック
    pub fn is_dangerous_command(&self, command: &str) -> bool {
        self.dangerous_commands.contains(command)
    }
    
    /// 危険な引数かどうかチェック
    pub fn is_dangerous_argument(&self, command: &str, arg: &str) -> bool {
        if let Some(patterns) = self.dangerous_args.get(command) {
            for pattern in patterns {
                if pattern.is_match(arg) {
                    return true;
                }
            }
        }
        false
    }
    
    /// ファイルアクセスが許可されているかチェック
    pub fn can_access_file(&self, path: &str, write_access: bool) -> bool {
        for (pattern, allow_write) in &self.restricted_paths {
            if pattern.is_match(path) {
                // 書き込みアクセスの場合、書き込み許可が必要
                if write_access && !allow_write {
                    return false;
                }
            }
        }
        true
    }
}

/// 変数展開器
pub struct Expander {
    env: Arc<Environment>,
}

impl Expander {
    /// 新しい変数展開器を作成
    pub fn new(env: Arc<Environment>) -> Self {
        Self { env }
    }
    
    /// 文字列内の変数を展開
    pub fn expand(&self, text: &str, scope: &Arc<Scope>) -> Result<String> {
        // 実装省略（既存の実装を使用）
        Ok(text.to_string())
    }
}

/// スコープ
#[derive(Debug, Clone)]
pub struct Scope {
    /// 変数マップ
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
    
    /// 親スコープを持つ新しいスコープを作成
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
            Some(value.clone())
        } else if let Some(parent) = &self.parent {
            parent.get(name)
        } else {
            None
        }
    }
    
    /// スコープに変数が存在するかチェック
    pub fn has(&self, name: &str) -> bool {
        self.variables.contains_key(name) || 
            self.parent.as_ref().map_or(false, |p| p.has(name))
    }
}

/// 値
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
    /// Null値
    Null,
}

impl Value {
    /// 真偽値に変換
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Integer(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => !arr.is_empty(),
            Value::Map(map) => !map.is_empty(),
            Value::Null => false,
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
                let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                format!("[{}]", items.join(", "))
            },
            Value::Map(map) => {
                let items: Vec<String> = map.iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_string()))
                    .collect();
                format!("{{{}}}", items.join(", "))
            },
            Value::Null => "null".to_string(),
        }
    }
}

/// 式の評価
pub fn evaluate_expression(expr: &AstNode, env: &Environment) -> Result<String, ShellError> {
    // 簡易版の実装
    match expr {
        AstNode::Literal { value, .. } => Ok(value.clone()),
        AstNode::VariableReference { name, .. } => {
            if let Some(value) = env.get(name) {
                Ok(value)
            } else {
                Ok("".to_string())
            }
        },
        _ => Ok("".to_string()), // デフォルト値
    }
} 