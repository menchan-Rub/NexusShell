// type_system.rs - NexusShellの次世代型システム
//
// シェルスクリプトの変数と式に対する強力な型チェック・型推論を提供し、
// 実行前にエラーを検出し、コードの信頼性と安全性を向上させます。

use crate::{AstNode, Error, Result, Span, TokenKind, ParserContext};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use parking_lot::RwLock;

/// 型の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// 未知の型（型推論前の初期状態）
    Unknown,
    
    /// 任意の型（何でも受け入れる）
    Any,
    
    /// 文字列型
    String,
    
    /// 整数型
    Integer,
    
    /// 浮動小数点型
    Float,
    
    /// ブール型
    Boolean,
    
    /// ファイルパス型
    Path,
    
    /// 配列型
    Array(Box<TypeKind>),
    
    /// マップ型
    Map(Box<TypeKind>, Box<TypeKind>),
    
    /// 関数型
    Function {
        /// パラメータの型リスト
        params: Vec<TypeKind>,
        
        /// 戻り値の型
        return_type: Box<TypeKind>,
    },
    
    /// 共用体型（複数の型のいずれか）
    Union(Vec<TypeKind>),
    
    /// 交差型（複数の型のすべて）
    Intersection(Vec<TypeKind>),
    
    /// NULL型
    Null,
    
    /// コマンド結果型（終了コードとストリーム）
    CommandResult,
    
    /// ストリーム型
    Stream,
    
    /// ユーザー定義型
    Custom(String),
}

/// 型情報
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// 型の種類
    pub kind: TypeKind,
    
    /// 型の名前（表示用）
    pub name: String,
    
    /// 型制約
    pub constraints: Vec<TypeConstraint>,
    
    /// 型の説明（ドキュメント用）
    pub description: Option<String>,
    
    /// 型パラメータ（ジェネリック型用）
    pub type_params: Vec<TypeKind>,
    
    /// 型のソース位置
    pub span: Option<Span>,
}

/// 型制約
#[derive(Debug, Clone)]
pub enum TypeConstraint {
    /// 型の部分型関係制約
    Subtype(TypeKind),
    
    /// 型の値範囲制約
    Range {
        /// 最小値
        min: Option<i64>,
        
        /// 最大値
        max: Option<i64>,
    },
    
    /// 文字列パターン制約（正規表現）
    Pattern(String),
    
    /// 列挙値制約
    Enum(Vec<String>),
    
    /// カスタム制約（関数で評価）
    Custom(Arc<dyn Fn(&TypeKind, &AstNode) -> Result<()> + Send + Sync>),
}

/// 型環境
/// 変数や式の型情報を管理する
#[derive(Debug, Clone)]
pub struct TypeEnvironment {
    /// 変数の型マップ
    variables: HashMap<String, TypeInfo>,
    
    /// 関数の型マップ
    functions: HashMap<String, TypeInfo>,
    
    /// カスタム型の定義
    custom_types: HashMap<String, TypeInfo>,
    
    /// 親環境への参照（スコープチェーン）
    parent: Option<Arc<RwLock<TypeEnvironment>>>,
    
    /// 環境の名前（デバッグ用）
    name: String,
}

/// 型チェッカー
/// ASTを走査して型チェックを行う
#[derive(Debug)]
pub struct TypeChecker {
    /// 現在の型環境
    current_env: Arc<RwLock<TypeEnvironment>>,
    
    /// グローバル型環境
    global_env: Arc<RwLock<TypeEnvironment>>,
    
    /// 型エラー
    errors: Vec<Error>,
    
    /// 型推論ヒントマップ
    type_hints: HashMap<usize, TypeKind>,
    
    /// 厳格モードかどうか
    strict_mode: bool,
    
    /// 自動型変換を許可するかどうか
    allow_auto_conversion: bool,
}

impl TypeKind {
    /// 型の名前を取得
    pub fn name(&self) -> String {
        match self {
            TypeKind::Unknown => "unknown".to_string(),
            TypeKind::Any => "any".to_string(),
            TypeKind::String => "string".to_string(),
            TypeKind::Integer => "integer".to_string(),
            TypeKind::Float => "float".to_string(),
            TypeKind::Boolean => "boolean".to_string(),
            TypeKind::Path => "path".to_string(),
            TypeKind::Array(elem_type) => format!("array<{}>", elem_type.name()),
            TypeKind::Map(key_type, value_type) => format!("map<{}, {}>", key_type.name(), value_type.name()),
            TypeKind::Function { params, return_type } => {
                let params_str = params.iter()
                    .map(|p| p.name())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({}) -> {}", params_str, return_type.name())
            },
            TypeKind::Union(types) => {
                let types_str = types.iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("({})", types_str)
            },
            TypeKind::Intersection(types) => {
                let types_str = types.iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(" & ");
                format!("({})", types_str)
            },
            TypeKind::Null => "null".to_string(),
            TypeKind::CommandResult => "command_result".to_string(),
            TypeKind::Stream => "stream".to_string(),
            TypeKind::Custom(name) => name.clone(),
        }
    }
    
    /// 型が別の型に割り当て可能かどうかをチェック
    pub fn is_assignable_to(&self, target: &TypeKind) -> bool {
        match (self, target) {
            // 任意の型はAnyに割り当て可能
            (_, TypeKind::Any) => true,
            
            // Anyはany以外に直接割り当て不可
            (TypeKind::Any, _) => false,
            
            // 同じ型同士は割り当て可能
            (a, b) if a == b => true,
            
            // 数値型の互換性
            (TypeKind::Integer, TypeKind::Float) => true,
            
            // 配列型の互換性
            (TypeKind::Array(a), TypeKind::Array(b)) => a.is_assignable_to(b),
            
            // マップ型の互換性
            (TypeKind::Map(a_key, a_val), TypeKind::Map(b_key, b_val)) => 
                a_key.is_assignable_to(b_key) && a_val.is_assignable_to(b_val),
            
            // 共用体型の互換性
            (TypeKind::Union(types), target) => 
                types.iter().all(|t| t.is_assignable_to(target)),
            
            (source, TypeKind::Union(types)) => 
                types.iter().any(|t| source.is_assignable_to(t)),
            
            // 交差型の互換性
            (TypeKind::Intersection(types), target) => 
                types.iter().any(|t| t.is_assignable_to(target)),
            
            (source, TypeKind::Intersection(types)) => 
                types.iter().all(|t| source.is_assignable_to(t)),
            
            // その他の型は互換性なし
            _ => false,
        }
    }
    
    /// 自動変換が可能かどうかをチェック
    pub fn can_auto_convert_to(&self, target: &TypeKind) -> bool {
        // 割り当て可能なら変換不要
        if self.is_assignable_to(target) {
            return true;
        }
        
        match (self, target) {
            // 文字列へは多くの型から変換可能
            (_, TypeKind::String) => true,
            
            // 文字列から数値への変換
            (TypeKind::String, TypeKind::Integer) |
            (TypeKind::String, TypeKind::Float) => true,
            
            // 文字列からブール値への変換
            (TypeKind::String, TypeKind::Boolean) => true,
            
            // 数値からブール値への変換
            (TypeKind::Integer, TypeKind::Boolean) |
            (TypeKind::Float, TypeKind::Boolean) => true,
            
            // その他の型変換は不可
            _ => false,
        }
    }
    
    /// 共通の上位型を見つける
    pub fn common_supertype(&self, other: &TypeKind) -> TypeKind {
        match (self, other) {
            // 同じ型なら同じものを返す
            (a, b) if a == b => a.clone(),
            
            // Anyは常に上位型
            (TypeKind::Any, _) | (_, TypeKind::Any) => TypeKind::Any,
            
            // 数値型の共通上位型
            (TypeKind::Integer, TypeKind::Float) |
            (TypeKind::Float, TypeKind::Integer) => TypeKind::Float,
            
            // 配列型の共通上位型
            (TypeKind::Array(a), TypeKind::Array(b)) => 
                TypeKind::Array(Box::new(a.common_supertype(b))),
            
            // マップ型の共通上位型
            (TypeKind::Map(a_key, a_val), TypeKind::Map(b_key, b_val)) => 
                TypeKind::Map(
                    Box::new(a_key.common_supertype(b_key)),
                    Box::new(a_val.common_supertype(b_val))
                ),
            
            // 共通上位型がない場合は共用体型を作成
            _ => TypeKind::Union(vec![self.clone(), other.clone()]),
        }
    }
}

impl TypeInfo {
    /// 新しい型情報を作成
    pub fn new(kind: TypeKind) -> Self {
        Self {
            name: kind.name(),
            kind,
            constraints: Vec::new(),
            description: None,
            type_params: Vec::new(),
            span: None,
        }
    }
    
    /// 型制約を追加
    pub fn with_constraint(mut self, constraint: TypeConstraint) -> Self {
        self.constraints.push(constraint);
        self
    }
    
    /// 型の説明を設定
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
    
    /// 型パラメータを設定
    pub fn with_type_params(mut self, params: Vec<TypeKind>) -> Self {
        self.type_params = params;
        self
    }
    
    /// 型のソース位置を設定
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
    
    /// 型制約を検証
    pub fn validate_constraints(&self, node: &AstNode) -> Result<()> {
        for constraint in &self.constraints {
            match constraint {
                TypeConstraint::Subtype(target_type) => {
                    if !self.kind.is_assignable_to(target_type) {
                        return Err(Error::new(
                            format!("型 {} は {} の部分型ではありません", self.kind.name(), target_type.name()),
                            node.span().clone()
                        ));
                    }
                },
                
                TypeConstraint::Range { min, max } => {
                    if let AstNode::Literal { value, .. } = node {
                        if let Ok(num) = value.parse::<i64>() {
                            if let Some(min_val) = min {
                                if num < *min_val {
                                    return Err(Error::new(
                                        format!("値 {} は最小値 {} より小さいです", num, min_val),
                                        node.span().clone()
                                    ));
                                }
                            }
                            
                            if let Some(max_val) = max {
                                if num > *max_val {
                                    return Err(Error::new(
                                        format!("値 {} は最大値 {} より大きいです", num, max_val),
                                        node.span().clone()
                                    ));
                                }
                            }
                        }
                    }
                },
                
                TypeConstraint::Pattern(pattern) => {
                    if let AstNode::Literal { value, .. } = node {
                        let re = regex::Regex::new(pattern).map_err(|e| {
                            Error::new(
                                format!("無効な正規表現パターン: {}", e),
                                node.span().clone()
                            )
                        })?;
                        
                        if !re.is_match(value) {
                            return Err(Error::new(
                                format!("値 '{}' はパターン '{}' に一致しません", value, pattern),
                                node.span().clone()
                            ));
                        }
                    }
                },
                
                TypeConstraint::Enum(values) => {
                    if let AstNode::Literal { value, .. } = node {
                        if !values.contains(value) {
                            return Err(Error::new(
                                format!("値 '{}' は許可された値 {:?} の一つではありません", value, values),
                                node.span().clone()
                            ));
                        }
                    }
                },
                
                TypeConstraint::Custom(validator) => {
                    validator(&self.kind, node)?;
                },
            }
        }
        
        Ok(())
    }
}

impl TypeEnvironment {
    /// 新しい型環境を作成
    pub fn new(name: &str) -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
            custom_types: HashMap::new(),
            parent: None,
            name: name.to_string(),
        }
    }
    
    /// 親環境を持つ型環境を作成
    pub fn with_parent(name: &str, parent: Arc<RwLock<TypeEnvironment>>) -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
            custom_types: HashMap::new(),
            parent: Some(parent),
            name: name.to_string(),
        }
    }
    
    /// 変数の型を定義
    pub fn define_variable(&mut self, name: &str, type_info: TypeInfo) {
        self.variables.insert(name.to_string(), type_info);
    }
    
    /// 関数の型を定義
    pub fn define_function(&mut self, name: &str, type_info: TypeInfo) {
        self.functions.insert(name.to_string(), type_info);
    }
    
    /// カスタム型を定義
    pub fn define_custom_type(&mut self, name: &str, type_info: TypeInfo) {
        self.custom_types.insert(name.to_string(), type_info);
    }
    
    /// 変数の型を検索
    pub fn lookup_variable(&self, name: &str) -> Option<TypeInfo> {
        if let Some(type_info) = self.variables.get(name) {
            Some(type_info.clone())
        } else if let Some(parent) = &self.parent {
            parent.read().lookup_variable(name)
        } else {
            None
        }
    }
    
    /// 関数の型を検索
    pub fn lookup_function(&self, name: &str) -> Option<TypeInfo> {
        if let Some(type_info) = self.functions.get(name) {
            Some(type_info.clone())
        } else if let Some(parent) = &self.parent {
            parent.read().lookup_function(name)
        } else {
            None
        }
    }
    
    /// カスタム型を検索
    pub fn lookup_custom_type(&self, name: &str) -> Option<TypeInfo> {
        if let Some(type_info) = self.custom_types.get(name) {
            Some(type_info.clone())
        } else if let Some(parent) = &self.parent {
            parent.read().lookup_custom_type(name)
        } else {
            None
        }
    }
    
    /// 変数の型を更新
    pub fn update_variable(&mut self, name: &str, type_info: TypeInfo) -> bool {
        if self.variables.contains_key(name) {
            self.variables.insert(name.to_string(), type_info);
            true
        } else if let Some(parent) = &self.parent {
            let mut parent = parent.write();
            parent.update_variable(name, type_info)
        } else {
            false
        }
    }
}

impl TypeChecker {
    /// 新しい型チェッカーを作成
    pub fn new() -> Self {
        let global_env = Arc::new(RwLock::new(TypeEnvironment::new("global")));
        Self {
            current_env: global_env.clone(),
            global_env,
            errors: Vec::new(),
            type_hints: HashMap::new(),
            strict_mode: false,
            allow_auto_conversion: true,
        }
    }
    
    /// 厳格モードを設定
    pub fn set_strict_mode(&mut self, strict: bool) {
        self.strict_mode = strict;
    }
    
    /// 自動型変換の許可を設定
    pub fn set_allow_auto_conversion(&mut self, allow: bool) {
        self.allow_auto_conversion = allow;
    }
    
    /// 型ヒントを設定
    pub fn add_type_hint(&mut self, node_id: usize, type_kind: TypeKind) {
        self.type_hints.insert(node_id, type_kind);
    }
    
    /// 標準ライブラリの型を初期化
    pub fn initialize_standard_library(&mut self) {
        let mut global_env = self.global_env.write();
        
        // 基本的な型を定義
        let string_type = TypeInfo::new(TypeKind::String)
            .with_description("文字列型");
        
        let integer_type = TypeInfo::new(TypeKind::Integer)
            .with_description("整数型");
        
        let float_type = TypeInfo::new(TypeKind::Float)
            .with_description("浮動小数点型");
        
        let boolean_type = TypeInfo::new(TypeKind::Boolean)
            .with_description("ブール型");
        
        let path_type = TypeInfo::new(TypeKind::Path)
            .with_description("ファイルパス型");
        
        // 標準関数の型を定義
        
        // echo関数
        let echo_type = TypeInfo::new(TypeKind::Function {
            params: vec![TypeKind::Any],
            return_type: Box::new(TypeKind::Integer),
        }).with_description("コンソールに値を出力する関数");
        
        global_env.define_function("echo", echo_type);
        
        // cd関数
        let cd_type = TypeInfo::new(TypeKind::Function {
            params: vec![TypeKind::Path],
            return_type: Box::new(TypeKind::Integer),
        }).with_description("カレントディレクトリを変更する関数");
        
        global_env.define_function("cd", cd_type);
        
        // ls関数
        let ls_type = TypeInfo::new(TypeKind::Function {
            params: vec![TypeKind::Path],
            return_type: Box::new(TypeKind::CommandResult),
        }).with_description("ディレクトリの内容を一覧表示する関数");
        
        global_env.define_function("ls", ls_type);
        
        // grep関数
        let grep_type = TypeInfo::new(TypeKind::Function {
            params: vec![TypeKind::String, TypeKind::String],
            return_type: Box::new(TypeKind::CommandResult),
        }).with_description("パターンに一致する行を検索する関数");
        
        global_env.define_function("grep", grep_type);
        
        // その他の標準関数も同様に定義...
    }
    
    /// 型チェックを実行
    pub fn check(&mut self, node: &AstNode) -> Result<TypeKind> {
        match node {
            AstNode::Program { statements, .. } => {
                let mut result_type = TypeKind::Null;
                
                // 各文を型チェック
                for stmt in statements {
                    result_type = self.check(stmt)?;
                }
                
                Ok(result_type)
            },
            
            AstNode::Command { name, args, .. } => {
                // コマンド名を型チェック
                let name_str = match name.as_ref() {
                    AstNode::Terminal { lexeme, .. } => lexeme,
                    _ => {
                        self.errors.push(Error::new(
                            "コマンド名は識別子であるべきです".to_string(),
                            node.span().clone()
                        ));
                        return Err(Error::new(
                            "コマンド名の型エラー".to_string(),
                            node.span().clone()
                        ));
                    }
                };
                
                // 関数の型を検索
                let func_type_info = self.current_env.read().lookup_function(name_str);
                
                if let Some(type_info) = func_type_info {
                    match &type_info.kind {
                        TypeKind::Function { params, return_type } => {
                            // 引数の数をチェック
                            if self.strict_mode && args.len() != params.len() {
                                let span = node.span().clone();
                                self.errors.push(Error::new(
                                    format!("関数 {} の引数の数が一致しません: 期待={}, 実際={}", 
                                        name_str, params.len(), args.len()),
                                    span,
                                    Some(format!("関数定義: {}({})\n呼び出し: {}({})", name_str, params.iter().map(|p| p.name()).collect::<Vec<_>>().join(","), name_str, args.iter().map(|a| a.name()).collect::<Vec<_>>().join(",")))
                                ));
                                
                                return Err(Error::new(
                                    "関数呼び出しの型エラー".to_string(),
                                    node.span().clone()
                                ));
                            }
                            
                            // 引数の型をチェック
                            for (i, arg) in args.iter().enumerate() {
                                if i < params.len() {
                                    let arg_type = self.check(arg)?;
                                    let param_type = &params[i];
                                    
                                    if !arg_type.is_assignable_to(param_type) {
                                        if self.allow_auto_conversion && arg_type.can_auto_convert_to(param_type) {
                                            // 自動変換可能なら警告のみ
                                            if self.strict_mode {
                                                let span = arg.span().clone();
                                                self.errors.push(Error::new(
                                                    format!("引数 #{} の型が一致しません: 期待={}, 実際={} (自動変換あり)", 
                                                        i+1, param_type.name(), arg_type.name()),
                                                    span,
                                                    Some(format!("関数定義: {}({})\n呼び出し: {}({})", name_str, params.iter().map(|p| p.name()).collect::<Vec<_>>().join(","), name_str, args.iter().map(|a| a.name()).collect::<Vec<_>>().join(",")))
                                                ));
                                            }
                                        } else {
                                            // 型が一致せず変換も不可なら型エラー
                                            let span = arg.span().clone();
                                            self.errors.push(Error::new(
                                                format!("引数 #{} の型が一致しません: 期待={}, 実際={}", 
                                                    i+1, param_type.name(), arg_type.name()),
                                                span,
                                                Some(format!("関数定義: {}({})\n呼び出し: {}({})", name_str, params.iter().map(|p| p.name()).collect::<Vec<_>>().join(","), name_str, args.iter().map(|a| a.name()).collect::<Vec<_>>().join(",")))
                                            ));
                                            
                                            return Err(Error::new(
                                                "関数呼び出しの型エラー".to_string(),
                                                node.span().clone()
                                            ));
                                        }
                                    }
                                }
                            }
                            
                            // 戻り値の型を返す
                            Ok(*return_type.clone())
                        },
                        _ => {
                            self.errors.push(Error::new(
                                format!("{} は関数ではありません", name_str),
                                node.span().clone()
                            ));
                            
                            Err(Error::new(
                                "関数呼び出しの型エラー".to_string(),
                                node.span().clone()
                            ))
                        }
                    }
                } else {
                    // 未知のコマンドはCommandResult型と仮定
                    // （厳格モードでは警告または失敗も考えられる）
                    if self.strict_mode {
                        self.errors.push(Error::new(
                            format!("未知のコマンド: {}", name_str),
                            node.span().clone()
                        ));
                    }
                    
                    Ok(TypeKind::CommandResult)
                }
            },
            
            AstNode::Literal { value, kind, .. } => {
                // リテラルの型を決定
                match kind {
                    TokenKind::String => Ok(TypeKind::String),
                    TokenKind::Number => {
                        // 整数か浮動小数点数かを判定
                        if value.contains('.') {
                            Ok(TypeKind::Float)
                        } else {
                            Ok(TypeKind::Integer)
                        }
                    },
                    TokenKind::True | TokenKind::False => Ok(TypeKind::Boolean),
                    TokenKind::Null => Ok(TypeKind::Null),
                    _ => Ok(TypeKind::Any),
                }
            },
            
            AstNode::Variable { name, .. } => {
                // 変数の型を検索
                let var_name = match name.as_ref() {
                    AstNode::Terminal { lexeme, .. } => lexeme,
                    _ => {
                        self.errors.push(Error::new(
                            "変数名は識別子であるべきです".to_string(),
                            node.span().clone()
                        ));
                        return Err(Error::new(
                            "変数の型エラー".to_string(),
                            node.span().clone()
                        ));
                    }
                };
                
                if let Some(type_info) = self.current_env.read().lookup_variable(var_name) {
                    Ok(type_info.kind)
                } else {
                    if self.strict_mode {
                        self.errors.push(Error::new(
                            format!("未定義の変数: {}", var_name),
                            node.span().clone()
                        ));
                        
                        Err(Error::new(
                            "変数の型エラー".to_string(),
                            node.span().clone()
                        ))
                    } else {
                        // 非厳格モードでは未知の変数はAny型と仮定
                        Ok(TypeKind::Any)
                    }
                }
            },
            
            AstNode::Assignment { left, right, .. } => {
                // 右辺の型を評価
                let right_type = self.check(right)?;
                
                // 左辺が変数かチェック
                let var_name = match left.as_ref() {
                    AstNode::Variable { name, .. } => {
                        match name.as_ref() {
                            AstNode::Terminal { lexeme, .. } => lexeme.clone(),
                            _ => {
                                self.errors.push(Error::new(
                                    "代入の左辺は変数であるべきです".to_string(),
                                    left.span().clone()
                                ));
                                return Err(Error::new(
                                    "代入の型エラー".to_string(),
                                    node.span().clone()
                                ));
                            }
                        }
                    },
                    _ => {
                        self.errors.push(Error::new(
                            "代入の左辺は変数であるべきです".to_string(),
                            left.span().clone()
                        ));
                        return Err(Error::new(
                            "代入の型エラー".to_string(),
                            node.span().clone()
                        ));
                    }
                };
                
                // 変数の既存の型をチェック
                let mut env = self.current_env.write();
                if let Some(existing_type) = env.lookup_variable(&var_name) {
                    // 型の互換性をチェック
                    if !right_type.is_assignable_to(&existing_type.kind) {
                        if self.allow_auto_conversion && right_type.can_auto_convert_to(&existing_type.kind) {
                            // 自動変換可能なら警告のみ
                            if self.strict_mode {
                                self.errors.push(Error::new(
                                    format!("変数 {} への代入の型が一致しません: 変数={}, 値={} (自動変換あり)", 
                                        var_name, existing_type.kind.name(), right_type.name()),
                                    node.span().clone()
                                ));
                            }
                        } else {
                            // 型が一致せず変換も不可なら型エラー
                            self.errors.push(Error::new(
                                format!("変数 {} への代入の型が一致しません: 変数={}, 値={}", 
                                    var_name, existing_type.kind.name(), right_type.name()),
                                node.span().clone()
                            ));
                            
                            return Err(Error::new(
                                "代入の型エラー".to_string(),
                                node.span().clone()
                            ));
                        }
                    }
                } else {
                    // 新しい変数を定義
                    env.define_variable(&var_name, TypeInfo::new(right_type.clone()));
                }
                
                // 代入の結果は右辺の型
                Ok(right_type)
            },
            
            // 他のノードタイプも同様に実装...
            _ => Ok(TypeKind::Any),
        }
    }
    
    /// 型エラーを取得
    pub fn get_errors(&self) -> &[Error] {
        &self.errors
    }
    
    /// 型エラーの数を取得
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
    
    /// 新しいスコープを作成
    pub fn enter_scope(&mut self, name: &str) {
        let new_env = TypeEnvironment::with_parent(name, self.current_env.clone());
        self.current_env = Arc::new(RwLock::new(new_env));
    }
    
    /// スコープを終了
    pub fn exit_scope(&mut self) -> Result<()> {
        let parent = {
            let env = self.current_env.read();
            env.parent.clone()
        };
        
        if let Some(parent_env) = parent {
            self.current_env = parent_env;
            Ok(())
        } else {
            Err(Error::new(
                "グローバルスコープから抜けようとしました".to_string(),
                Span::new(0, 0)
            ))
        }
    }
}

// ================================
// パブリックAPI関数
// ================================

/// 新しい型チェッカーを作成
pub fn create_type_checker() -> TypeChecker {
    let mut checker = TypeChecker::new();
    checker.initialize_standard_library();
    checker
}

/// 厳格モードの型チェッカーを作成
pub fn create_strict_type_checker() -> TypeChecker {
    let mut checker = TypeChecker::new();
    checker.initialize_standard_library();
    checker.set_strict_mode(true);
    checker
}

/// ASTの型チェックを実行
pub fn check_types(node: &AstNode) -> Result<TypeKind> {
    let mut checker = create_type_checker();
    checker.check(node)
}

/// エラーを表示用にフォーマット
pub fn format_type_errors(errors: &[Error]) -> String {
    let mut output = String::new();
    
    for (i, error) in errors.iter().enumerate() {
        output.push_str(&format!("型エラー #{}: {} (位置: {:?})\n", 
            i + 1, error.message, error.span));
    }
    
    output
}

// テスト用コンポーネント
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_type_checking() {
        // 基本的な型チェックのテスト実装
    }
    
    #[test]
    fn test_type_inference() {
        // 型推論のテスト実装
    }
    
    #[test]
    fn test_type_constraints() {
        // 型制約のテスト実装
    }
} 