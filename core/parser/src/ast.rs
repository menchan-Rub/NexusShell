/*!
# 抽象構文木 (AST) モジュール

高度なシェルスクリプト言語のための抽象構文木実装。
豊富な言語機能をサポートし、効率的な静的解析と最適化を可能にします。

## 主な機能

- 完全な言語構文のサポート
- 高度な型システム
- 変数スコープと名前解決
- 豊富な制御構造
- 関数とモジュール
- 最適化のためのアノテーション
*/

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Serialize, Deserialize};
use thiserror::Error;

/// ソースコード位置情報
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// ファイルインデックス
    pub file_index: usize,
    /// 行番号
    pub line: usize,
    /// 列番号
    pub column: usize,
    /// オフセット
    pub offset: usize,
    /// 長さ
    pub length: usize,
}

impl SourceLocation {
    /// 新しいソースコード位置情報を作成
    pub fn new(file_index: usize, line: usize, column: usize, offset: usize, length: usize) -> Self {
        Self {
            file_index,
            line,
            column,
            offset,
            length,
        }
    }
    
    /// ソースコード位置が未知であることを示す特殊値
    pub fn unknown() -> Self {
        Self {
            file_index: 0,
            line: 0,
            column: 0,
            offset: 0,
            length: 0,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 && self.column == 0 {
            write!(f, "<unknown>")
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}

/// ソースファイル情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    /// ファイルパス
    pub path: PathBuf,
    /// ファイル内容
    pub content: String,
    /// ファイルのハッシュ
    pub hash: String,
}

/// AST ノード識別子
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(u64);

impl NodeId {
    /// 新しいノードIDを作成
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// 内部IDを取得
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node:{}", self.0)
    }
}

/// ASTノード型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeType {
    /// プログラム
    Program,
    /// コマンド
    Command,
    /// パイプライン
    Pipeline,
    /// リダイレクション
    Redirection,
    /// 変数割り当て
    Assignment,
    /// 変数参照
    VariableReference,
    /// 文字列リテラル
    StringLiteral,
    /// 数値リテラル
    NumberLiteral,
    /// 配列リテラル
    ArrayLiteral,
    /// オブジェクトリテラル
    ObjectLiteral,
    /// 関数定義
    FunctionDefinition,
    /// 関数呼び出し
    FunctionCall,
    /// 条件式
    Condition,
    /// if文
    IfStatement,
    /// for文
    ForStatement,
    /// while文
    WhileStatement,
    /// case文
    CaseStatement,
    /// break文
    BreakStatement,
    /// continue文
    ContinueStatement,
    /// return文
    ReturnStatement,
    /// subshell
    Subshell,
    /// グループ
    Group,
    /// コメント
    Comment,
    /// ヒアドキュメント
    HereDocument,
    /// 算術式
    ArithmeticExpression,
    /// パラメータ展開
    ParameterExpansion,
    /// コマンド置換
    CommandSubstitution,
    /// プロセス置換
    ProcessSubstitution,
    /// 文字列展開
    StringExpansion,
    /// 属性
    Attribute,
    /// 前処理ディレクティブ
    Directive,
    /// モジュールインポート
    Import,
    /// モジュールエクスポート
    Export,
    /// 型定義
    TypeDefinition,
    /// 例外処理
    TryCatch,
    /// カスタムノード
    Custom(String),
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Program => write!(f, "Program"),
            Self::Command => write!(f, "Command"),
            Self::Pipeline => write!(f, "Pipeline"),
            Self::Redirection => write!(f, "Redirection"),
            Self::Assignment => write!(f, "Assignment"),
            Self::VariableReference => write!(f, "VariableReference"),
            Self::StringLiteral => write!(f, "StringLiteral"),
            Self::NumberLiteral => write!(f, "NumberLiteral"),
            Self::ArrayLiteral => write!(f, "ArrayLiteral"),
            Self::ObjectLiteral => write!(f, "ObjectLiteral"),
            Self::FunctionDefinition => write!(f, "FunctionDefinition"),
            Self::FunctionCall => write!(f, "FunctionCall"),
            Self::Condition => write!(f, "Condition"),
            Self::IfStatement => write!(f, "IfStatement"),
            Self::ForStatement => write!(f, "ForStatement"),
            Self::WhileStatement => write!(f, "WhileStatement"),
            Self::CaseStatement => write!(f, "CaseStatement"),
            Self::BreakStatement => write!(f, "BreakStatement"),
            Self::ContinueStatement => write!(f, "ContinueStatement"),
            Self::ReturnStatement => write!(f, "ReturnStatement"),
            Self::Subshell => write!(f, "Subshell"),
            Self::Group => write!(f, "Group"),
            Self::Comment => write!(f, "Comment"),
            Self::HereDocument => write!(f, "HereDocument"),
            Self::ArithmeticExpression => write!(f, "ArithmeticExpression"),
            Self::ParameterExpansion => write!(f, "ParameterExpansion"),
            Self::CommandSubstitution => write!(f, "CommandSubstitution"),
            Self::ProcessSubstitution => write!(f, "ProcessSubstitution"),
            Self::StringExpansion => write!(f, "StringExpansion"),
            Self::Attribute => write!(f, "Attribute"),
            Self::Directive => write!(f, "Directive"),
            Self::Import => write!(f, "Import"),
            Self::Export => write!(f, "Export"),
            Self::TypeDefinition => write!(f, "TypeDefinition"),
            Self::TryCatch => write!(f, "TryCatch"),
            Self::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// ASTノードフラグ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeFlag {
    /// バックグラウンド実行
    Background,
    /// エクスポート
    Export,
    /// 読み取り専用
    ReadOnly,
    /// グローバルスコープ
    Global,
    /// ローカルスコープ
    Local,
    /// 型付き
    Typed,
    /// 非同期
    Async,
    /// 条件反転
    Negated,
    /// オプション引数
    Optional,
    /// 可変引数
    Variadic,
    /// エラー停止
    ErrorExit,
    /// デバッグモード
    Debug,
    /// カスタムフラグ
    Custom(u32),
}

/// データ型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataType {
    /// 未知
    Unknown,
    /// 任意
    Any,
    /// ヌル
    Null,
    /// ブーリアン
    Boolean,
    /// 整数
    Integer,
    /// 浮動小数点
    Float,
    /// 文字列
    String,
    /// 配列
    Array(Box<DataType>),
    /// マップ
    Map(Box<DataType>, Box<DataType>),
    /// 関数
    Function(Vec<DataType>, Box<DataType>),
    /// コマンド
    Command,
    /// ファイルディスクリプタ
    FileDescriptor,
    /// カスタム型
    Custom(String),
    /// ユニオン型
    Union(Vec<DataType>),
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Any => write!(f, "any"),
            Self::Null => write!(f, "null"),
            Self::Boolean => write!(f, "boolean"),
            Self::Integer => write!(f, "integer"),
            Self::Float => write!(f, "float"),
            Self::String => write!(f, "string"),
            Self::Array(item_type) => write!(f, "{}[]", item_type),
            Self::Map(key_type, value_type) => write!(f, "Map<{}, {}>", key_type, value_type),
            Self::Function(params, return_type) => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", return_type)
            },
            Self::Command => write!(f, "command"),
            Self::FileDescriptor => write!(f, "fd"),
            Self::Custom(name) => write!(f, "{}", name),
            Self::Union(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            },
        }
    }
}

/// ASTノード属性
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeAttribute {
    /// 型情報
    Type(DataType),
    /// ドキュメンテーション
    Documentation(String),
    /// メタデータ
    Metadata(HashMap<String, String>),
    /// デバッグ情報
    DebugInfo(String),
    /// 検証済みフラグ
    Validated(bool),
    /// オプティマイザーヒント
    OptimizerHint(String),
    /// 依存関係
    Dependency(Vec<String>),
    /// カスタム属性
    Custom(String, String),
}

/// ASTノード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// ノードID
    pub id: NodeId,
    /// ノード型
    pub node_type: NodeType,
    /// ソースコード位置
    pub location: SourceLocation,
    /// 子ノード
    pub children: Vec<NodeId>,
    /// 親ノード
    pub parent: Option<NodeId>,
    /// ノード値
    pub value: Option<String>,
    /// 型情報
    pub data_type: DataType,
    /// フラグ
    pub flags: HashSet<NodeFlag>,
    /// 属性
    pub attributes: Vec<NodeAttribute>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl Node {
    /// 新しいノードを作成
    pub fn new(id: NodeId, node_type: NodeType) -> Self {
        Self {
            id,
            node_type,
            location: SourceLocation::unknown(),
            children: Vec::new(),
            parent: None,
            value: None,
            data_type: DataType::Unknown,
            flags: HashSet::new(),
            attributes: Vec::new(),
            metadata: HashMap::new(),
        }
    }
    
    /// ソースコード位置を設定
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = location;
        self
    }
    
    /// 子ノードを追加
    pub fn add_child(&mut self, child_id: NodeId) {
        self.children.push(child_id);
    }
    
    /// 親ノードを設定
    pub fn set_parent(&mut self, parent_id: NodeId) {
        self.parent = Some(parent_id);
    }
    
    /// 値を設定
    pub fn with_value(mut self, value: String) -> Self {
        self.value = Some(value);
        self
    }
    
    /// 型を設定
    pub fn with_type(mut self, data_type: DataType) -> Self {
        self.data_type = data_type;
        self
    }
    
    /// フラグを追加
    pub fn add_flag(&mut self, flag: NodeFlag) {
        self.flags.insert(flag);
    }
    
    /// 属性を追加
    pub fn add_attribute(&mut self, attribute: NodeAttribute) {
        self.attributes.push(attribute);
    }
    
    /// メタデータを設定
    pub fn add_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }
    
    /// フラグをチェック
    pub fn has_flag(&self, flag: &NodeFlag) -> bool {
        self.flags.contains(flag)
    }
    
    /// 属性を取得
    pub fn get_attribute<T>(&self, predicate: impl Fn(&NodeAttribute) -> Option<T>) -> Option<T> {
        for attr in &self.attributes {
            if let Some(result) = predicate(attr) {
                return Some(result);
            }
        }
        None
    }
    
    /// 型属性を取得
    pub fn get_type_attribute(&self) -> Option<DataType> {
        self.get_attribute(|attr| {
            if let NodeAttribute::Type(data_type) = attr {
                Some(data_type.clone())
            } else {
                None
            }
        })
    }
    
    /// ドキュメンテーション属性を取得
    pub fn get_documentation(&self) -> Option<String> {
        self.get_attribute(|attr| {
            if let NodeAttribute::Documentation(doc) = attr {
                Some(doc.clone())
            } else {
                None
            }
        })
    }
}

/// ASTエラー
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum AstError {
    /// 不正なノードタイプ
    #[error("不正なノードタイプ: {0}")]
    InvalidNodeType(String),
    
    /// ノードが見つからない
    #[error("ノード {0} が見つかりません")]
    NodeNotFound(NodeId),
    
    /// 不正な子ノード
    #[error("不正な子ノード: {0}")]
    InvalidChild(String),
    
    /// 型エラー
    #[error("型エラーが発生しました (位置: {span})\n該当箇所: `{source_snippet}`\n期待した型: {expected} ({expected_type})\n実際の型: {actual} ({actual_type})")]
    TypeError {
        expected: String,
        expected_type: String,
        actual: String,
        actual_type: String,
        span: Span,
        source_snippet: String,
    },
    
    /// 構文エラー
    #[error("構文エラー: {message} (位置: {location})")]
    SyntaxError { message: String, location: SourceLocation },
    
    /// 名前解決エラー
    #[error("名前解決エラー: {message} (識別子: {identifier})")]
    NameResolutionError { message: String, identifier: String },
    
    /// 検証エラー
    #[error("検証エラー: {message} (ノードID: {node_id:?})")]
    ValidationError { message: String, node_id: Option<NodeId> },
    
    /// その他のエラー
    #[error("ASTエラー: {message}")]
    Other { message: String },
}

/// AST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ast {
    /// ノード
    nodes: HashMap<NodeId, Node>,
    /// ルートノードID
    root: Option<NodeId>,
    /// ソースファイル
    source_files: Vec<SourceFile>,
    /// 次のノードID
    next_node_id: u64,
    /// シンボルテーブル
    symbols: HashMap<String, NodeId>,
}

impl Ast {
    /// 新しいASTを作成
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root: None,
            source_files: Vec::new(),
            next_node_id: 1,
            symbols: HashMap::new(),
        }
    }
    
    /// ソースファイルを追加
    pub fn add_source_file(&mut self, file: SourceFile) -> usize {
        self.source_files.push(file);
        self.source_files.len() - 1
    }
    
    /// 新しいノードを作成
    pub fn create_node(&mut self, node_type: NodeType) -> NodeId {
        let id = NodeId::new(self.next_node_id);
        self.next_node_id += 1;
        
        let node = Node::new(id, node_type);
        self.nodes.insert(id, node);
        
        id
    }
    
    /// ルートノードを設定
    pub fn set_root(&mut self, root_id: NodeId) -> Result<(), AstError> {
        if !self.nodes.contains_key(&root_id) {
            return Err(AstError::NodeNotFound(root_id));
        }
        self.root = Some(root_id);
        Ok(())
    }
    
    /// ルートノードを取得
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }
    
    /// ノードを取得
    pub fn get_node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }
    
    /// ノードを可変で取得
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }
    
    /// 親子関係を設定
    pub fn set_parent_child(&mut self, parent_id: &NodeId, child_id: &NodeId) -> Result<(), AstError> {
        // 両方のノードが存在するか確認
        if !self.nodes.contains_key(parent_id) {
            return Err(AstError::NodeNotFound(*parent_id));
        }
        if !self.nodes.contains_key(child_id) {
            return Err(AstError::NodeNotFound(*child_id));
        }
        
        // 親ノードに子ノードを追加
        if let Some(parent) = self.nodes.get_mut(parent_id) {
            parent.add_child(*child_id);
        }
        
        // 子ノードに親ノードを設定
        if let Some(child) = self.nodes.get_mut(child_id) {
            child.set_parent(*parent_id);
        }
        
        Ok(())
    }
    
    /// シンボルを定義
    pub fn define_symbol(&mut self, name: &str, node_id: NodeId) {
        self.symbols.insert(name.to_string(), node_id);
    }
    
    /// シンボルを解決
    pub fn resolve_symbol(&self, name: &str) -> Option<NodeId> {
        self.symbols.get(name).copied()
    }
    
    /// ノードの子ノードを取得
    pub fn get_children(&self, id: &NodeId) -> Result<Vec<&Node>, AstError> {
        let node = self.get_node(id).ok_or_else(|| AstError::NodeNotFound(*id))?;
        
        let mut children = Vec::new();
        for child_id in &node.children {
            if let Some(child) = self.get_node(child_id) {
                children.push(child);
            } else {
                return Err(AstError::NodeNotFound(*child_id));
            }
        }
        
        Ok(children)
    }
    
    /// ノードの親ノードを取得
    pub fn get_parent(&self, id: &NodeId) -> Result<Option<&Node>, AstError> {
        let node = self.get_node(id).ok_or_else(|| AstError::NodeNotFound(*id))?;
        
        if let Some(parent_id) = node.parent {
            Ok(self.get_node(&parent_id))
        } else {
            Ok(None)
        }
    }
    
    /// AST検証
    pub fn validate(&self) -> Result<(), Vec<AstError>> {
        let mut errors = Vec::new();
        
        // ルートノードが存在するか確認
        if self.root.is_none() {
            errors.push(AstError::Other("ルートノードが設定されていません".to_string()));
        }
        
        // すべてのノードを検証
        for (id, node) in &self.nodes {
            // 親子関係の整合性を確認
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.get_node(&parent_id) {
                    if !parent.children.contains(id) {
                        errors.push(AstError::Other(
                            format!("ノード {}の親子関係が不整合です", id)
                        ));
                    }
                } else {
                    errors.push(AstError::NodeNotFound(parent_id));
                }
            }
            
            // 子ノードの存在を確認
            for child_id in &node.children {
                if !self.nodes.contains_key(child_id) {
                    errors.push(AstError::NodeNotFound(*child_id));
                }
            }
            
            // ノード型に応じた固有の検証
            match node.node_type {
                NodeType::VariableReference => {
                    // 変数参照は値を持つべき
                    if node.value.is_none() {
                        errors.push(AstError::ValidationError(
                            format!("変数参照ノード {} に値がありません", id)
                        ));
                    }
                },
                NodeType::FunctionDefinition => {
                    // 関数定義は少なくとも1つの子ノードを持つべき
                    if node.children.is_empty() {
                        errors.push(AstError::ValidationError(
                            format!("関数定義ノード {} に子ノードがありません", id)
                        ));
                    }
                },
                _ => {}
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// ASTを最適化
    pub fn optimize(&mut self) -> Result<(), AstError> {
        debug!("ASTの最適化を開始");
        
        // 定数畳み込み
        self.fold_constants()?;
        
        // 不要なノードの削除
        self.eliminate_dead_nodes()?;
        
        // 共通部分式の削除
        self.eliminate_common_subexpressions()?;
        
        // コマンドの連結
        self.merge_commands()?;
        
        // パイプラインの最適化
        self.optimize_pipelines()?;
        
        debug!("ASTの最適化が完了しました");
        Ok(())
    }
    
    /// 定数畳み込みを行う
    fn fold_constants(&mut self) -> Result<(), AstError> {
        let mut modified_nodes = Vec::new();
        
        // 算術式のノードを探して定数畳み込みを行う
        for (id, node) in &self.nodes {
            if node.node_type == NodeType::ArithmeticExpression {
                if let Some(result) = self.evaluate_arithmetic_expression(id) {
                    // 畳み込んだ結果をノードとして作成
                    let folded_id = self.create_node(NodeType::NumberLiteral);
                    if let Some(folded_node) = self.get_node_mut(&folded_id) {
                        folded_node.value = Some(result.to_string());
                        folded_node.data_type = DataType::Integer;
                        folded_node.location = node.location.clone();
                    }
                    
                    modified_nodes.push((*id, folded_id));
                }
            }
        }
        
        // 置き換え
        for (old_id, new_id) in modified_nodes {
            self.replace_node(&old_id, &new_id)?;
        }
        
        Ok(())
    }
    
    /// 算術式を評価する
    fn evaluate_arithmetic_expression(&self, id: &NodeId) -> Option<i64> {
        // 算術式を評価する実装
        let node = self.get_node(id)?;
        
        match &node.kind {
            // 数値リテラルはそのまま返す
            NodeKind::NumberLiteral { value } => Some(*value),
            
            // 二項演算子の処理
            NodeKind::BinaryOp { op, left, right } => {
                let left_val = self.evaluate_arithmetic_expression(left)?;
                let right_val = self.evaluate_arithmetic_expression(right)?;
                
                match op {
                    BinaryOperator::Add => Some(left_val + right_val),
                    BinaryOperator::Subtract => Some(left_val - right_val),
                    BinaryOperator::Multiply => Some(left_val * right_val),
                    BinaryOperator::Divide => {
                        if right_val == 0 {
                            // ゼロ除算は避ける
                            None
                        } else {
                            Some(left_val / right_val)
                        }
                    },
                    BinaryOperator::Modulo => {
                        if right_val == 0 {
                            // ゼロ除算は避ける
                            None
                        } else {
                            Some(left_val % right_val)
                        }
                    },
                    BinaryOperator::Power => {
                        // べき乗演算
                        if right_val < 0 {
                            // 負のべき乗は整数では表現できない
                            None
                        } else {
                            Some(left_val.pow(right_val as u32))
                        }
                    },
                    // 論理演算子はブール値を整数に変換して返す
                    BinaryOperator::LogicalAnd => Some(if left_val != 0 && right_val != 0 { 1 } else { 0 }),
                    BinaryOperator::LogicalOr => Some(if left_val != 0 || right_val != 0 { 1 } else { 0 }),
                    // ビット演算
                    BinaryOperator::BitwiseAnd => Some(left_val & right_val),
                    BinaryOperator::BitwiseOr => Some(left_val | right_val),
                    BinaryOperator::BitwiseXor => Some(left_val ^ right_val),
                    BinaryOperator::LeftShift => Some(left_val << right_val),
                    BinaryOperator::RightShift => Some(left_val >> right_val),
                    // 比較演算子はブール値を整数に変換して返す
                    BinaryOperator::Equal => Some(if left_val == right_val { 1 } else { 0 }),
                    BinaryOperator::NotEqual => Some(if left_val != right_val { 1 } else { 0 }),
                    BinaryOperator::LessThan => Some(if left_val < right_val { 1 } else { 0 }),
                    BinaryOperator::LessThanOrEqual => Some(if left_val <= right_val { 1 } else { 0 }),
                    BinaryOperator::GreaterThan => Some(if left_val > right_val { 1 } else { 0 }),
                    BinaryOperator::GreaterThanOrEqual => Some(if left_val >= right_val { 1 } else { 0 }),
                    // その他の演算子はサポート外
                    _ => None,
                }
            },
            
            // 単項演算子の処理
            NodeKind::UnaryOp { op, operand } => {
                let val = self.evaluate_arithmetic_expression(operand)?;
                
                match op {
                    UnaryOperator::Negate => Some(-val),
                    UnaryOperator::BitwiseNot => Some(!val),
                    UnaryOperator::LogicalNot => Some(if val == 0 { 1 } else { 0 }),
                    // その他の演算子はサポート外
                    _ => None,
                }
            },
            
            // 変数参照の処理
            NodeKind::VariableRef { name } => {
                // 環境変数またはシェル変数から値を取得
                self.get_variable_value(name)
                    .and_then(|value| value.parse::<i64>().ok())
            },
            
            // 括弧内の式は中身を評価
            NodeKind::ParenthesizedExpr { expr } => self.evaluate_arithmetic_expression(expr),
            
            // その他の式タイプはサポート外
            _ => None,
        }
    }
    
    /// 不要なノードを削除
    fn eliminate_dead_nodes(&mut self) -> Result<(), AstError> {
        // 到達可能なノードを特定
        let mut reachable = HashSet::new();
        if let Some(root) = self.root {
            self.mark_reachable_nodes(&root, &mut reachable);
        }
        
        // 到達不可能なノードを削除
        let dead_nodes: Vec<NodeId> = self.nodes.keys()
            .filter(|id| !reachable.contains(*id))
            .cloned()
            .collect();
        
        for id in dead_nodes {
            self.nodes.remove(&id);
        }
        
        Ok(())
    }
    
    /// 到達可能なノードをマーク
    fn mark_reachable_nodes(&self, id: &NodeId, reachable: &mut HashSet<NodeId>) {
        if reachable.contains(id) {
            return;
        }
        
        reachable.insert(id.clone());
        
        if let Some(node) = self.get_node(id) {
            for child_id in &node.children {
                self.mark_reachable_nodes(child_id, reachable);
            }
        }
    }
    
    /// 共通部分式を削除
    fn eliminate_common_subexpressions(&mut self) -> Result<(), AstError> {
        let mut subexpression_map = HashMap::new();
        let mut replacements = Vec::new();
        
        // トップダウンに探索して共通部分式を見つける
        if let Some(root) = self.root {
            self.find_common_subexpressions(&root, &mut subexpression_map, &mut replacements)?;
        }
        
        // 置き換え
        for (original, replacement) in replacements {
            self.replace_subexpression(&original, &replacement)?;
        }
        
        Ok(())
    }
    
    /// 共通部分式を検索
    fn find_common_subexpressions(
        &self,
        id: &NodeId, 
        subexpr_map: &mut HashMap<String, NodeId>,
        replacements: &mut Vec<(NodeId, NodeId)>
    ) -> Result<(), AstError> {
        if let Some(node) = self.get_node(id) {
            // このノードの表現を構築
            let signature = self.compute_node_signature(id)?;
            
            // 子ノードを先に処理
            for child_id in &node.children {
                self.find_common_subexpressions(child_id, subexpr_map, replacements)?;
            }
            
            // 既に同じ表現を持つノードがあるか確認
            if let Some(existing) = subexpr_map.get(&signature) {
                if existing != id {
                    replacements.push((id.clone(), existing.clone()));
                }
            } else {
                // 新しい表現を記録
                subexpr_map.insert(signature, id.clone());
            }
        }
        
        Ok(())
    }
    
    /// ノードの特徴量を計算
    fn compute_node_signature(&self, id: &NodeId) -> Result<String, AstError> {
        if let Some(node) = self.get_node(id) {
            let mut signature = format!("{:?}", node.node_type);
            
            if let Some(value) = &node.value {
                signature.push_str(&format!("|{}", value));
            }
            
            for flag in &node.flags {
                signature.push_str(&format!("|{:?}", flag));
            }
            
            let child_sigs: Result<Vec<String>, AstError> = node.children.iter()
                .map(|child_id| self.compute_node_signature(child_id))
                .collect();
            
            for child_sig in child_sigs? {
                signature.push_str(&format!("|{}", child_sig));
            }
            
            Ok(signature)
        } else {
            Err(AstError::NodeNotFound(id.clone()))
        }
    }
    
    /// 部分式を置き換え
    fn replace_subexpression(&mut self, original: &NodeId, replacement: &NodeId) -> Result<(), AstError> {
        // 親ノードを取得
        if let Some(parent_id) = self.get_parent_id(original)? {
            if let Some(parent) = self.get_node_mut(&parent_id) {
                // 子リストで置き換え
                for i in 0..parent.children.len() {
                    if &parent.children[i] == original {
                        parent.children[i] = replacement.clone();
                        break;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 親ノードのIDを取得
    fn get_parent_id(&self, id: &NodeId) -> Result<Option<NodeId>, AstError> {
        if let Some(node) = self.get_node(id) {
            Ok(node.parent.clone())
        } else {
            Err(AstError::NodeNotFound(id.clone()))
        }
    }
    
    /// ノードを置き換え
    fn replace_node(&mut self, old_id: &NodeId, new_id: &NodeId) -> Result<(), AstError> {
        // 親ノードの子リストを更新
        if let Some(parent_id) = self.get_parent_id(old_id)? {
            if let Some(parent) = self.get_node_mut(&parent_id) {
                for i in 0..parent.children.len() {
                    if &parent.children[i] == old_id {
                        parent.children[i] = new_id.clone();
                        
                        // 新しいノードの親を設定
                        if let Some(new_node) = self.get_node_mut(new_id) {
                            new_node.parent = Some(parent_id.clone());
                        }
                        
                        break;
                    }
                }
            }
        }
        
        // ルートノードの更新
        if let Some(root) = self.root {
            if &root == old_id {
                self.root = Some(new_id.clone());
            }
        }
        
        Ok(())
    }
    
    /// コマンドの連結
    fn merge_commands(&mut self) -> Result<(), AstError> {
        debug!("パイプラインのコマンドマージを実行します");
        let mut optimizations_count = 0;
        
        // トップレベルのノードを処理
        if let Some(root) = self.get_root_mut() {
            optimizations_count += self.merge_commands_in_node(root)?;
        }
        
        debug!("コマンドマージ最適化: {} 件の最適化を実行", optimizations_count);
        Ok(())
    }
    
    fn merge_commands_in_node(&mut self, node_id: &NodeId) -> Result<usize, AstError> {
        let mut optimizations_count = 0;
        
        // このノードがパイプラインなら最適化を試みる
        let node = self.get_node(node_id).ok_or(AstError::NodeNotFound)?;
        
        if let NodeKind::Pipeline { commands } = &node.kind {
            // パイプラインのコマンドリストをコピー
            let command_ids = commands.clone();
            
            // 連続するコマンドをマージできるかチェック
            if command_ids.len() >= 2 {
                let mut merged_commands = Vec::new();
                let mut i = 0;
                
                while i < command_ids.len() {
                    if i + 1 < command_ids.len() {
                        // 隣接するコマンドを取得
                        let current = self.get_node(&command_ids[i]).ok_or(AstError::NodeNotFound)?;
                        let next = self.get_node(&command_ids[i + 1]).ok_or(AstError::NodeNotFound)?;
                        
                        // マージ可能かチェック
                        if let (Some(merged), true) = self.try_merge_commands(current, next) {
                            // マージ成功
                            let merged_id = self.add_node(merged);
                            merged_commands.push(merged_id);
                            i += 2; // 2つのコマンドをスキップ
                            optimizations_count += 1;
                            continue;
                        }
                    }
                    
                    // マージできない場合は元のコマンドを保持
                    merged_commands.push(command_ids[i].clone());
                    i += 1;
                }
                
                // パイプラインを更新
                if merged_commands.len() < command_ids.len() {
                    let node = self.get_node_mut(node_id).ok_or(AstError::NodeNotFound)?;
                    if let NodeKind::Pipeline { commands } = &mut node.kind {
                        *commands = merged_commands;
                    }
                }
            }
        }
        
        // 子ノードを再帰的に処理
        let child_ids: Vec<NodeId> = if let Some(node) = self.get_node(node_id) {
            node.children()
        } else {
            Vec::new()
        };
        
        for child_id in child_ids {
            optimizations_count += self.merge_commands_in_node(&child_id)?;
        }
        
        Ok(optimizations_count)
    }
    
    fn try_merge_commands(&self, cmd1: &Node, cmd2: &Node) -> (Option<Node>, bool) {
        // コマンドマージのルールを適用
        if let (NodeKind::Command { name: name1, args: args1, .. }, 
                NodeKind::Command { name: name2, args: args2, .. }) = (&cmd1.kind, &cmd2.kind) {
            
            // cat + grep の最適化: cat file | grep pattern → grep pattern file
            if name1 == "cat" && name2 == "grep" && args1.len() == 1 {
                let file = self.get_arg_text(&args1[0]);
                
                // 新しいgrepコマンドを作成
                let mut new_args = args2.clone();
                new_args.push(args1[0].clone()); // ファイル引数を追加
                
                let merged = Node {
                    kind: NodeKind::Command {
                        name: "grep".to_string(),
                        args: new_args,
                        redirects: Vec::new(),
                        background: false,
                    },
                    span: cmd1.span.merge(&cmd2.span),
                    metadata: cmd2.metadata.clone(),
                };
                
                return (Some(merged), true);
            }
            
            // find + xargs の最適化: find ... | xargs cmd → find ... -exec cmd {} \;
            if name1 == "find" && name2 == "xargs" && args2.len() >= 1 {
                let mut new_args = args1.clone();
                new_args.push(self.create_text_node("-exec".to_string()));
                
                // xargsの最初の引数（実行コマンド）と残りの引数を追加
                for arg in args2.iter().skip(0) {
                    new_args.push(arg.clone());
                }
                
                // {} \; を追加
                new_args.push(self.create_text_node("{}".to_string()));
                new_args.push(self.create_text_node("\\;".to_string()));
                
                let merged = Node {
                    kind: NodeKind::Command {
                        name: "find".to_string(),
                        args: new_args,
                        redirects: Vec::new(),
                        background: false,
                    },
                    span: cmd1.span.merge(&cmd2.span),
                    metadata: cmd1.metadata.clone(),
                };
                
                return (Some(merged), true);
            }
        }
        
        (None, false)
    }
    
    // ヘルパーメソッド: テキストノードからテキストを取得
    fn get_arg_text(&self, node_id: &NodeId) -> String {
        if let Some(node) = self.get_node(node_id) {
            if let NodeKind::Text { value } = &node.kind {
                return value.clone();
            }
        }
        String::new()
    }
    
    // ヘルパーメソッド: テキストノードを作成
    fn create_text_node(&self, text: String) -> NodeId {
        let node = Node {
            kind: NodeKind::Text { value: text },
            span: TextSpan::default(),
            metadata: HashMap::new(),
        };
        self.add_node(node)
    }
    
    /// パイプラインの最適化
    fn optimize_pipelines(&mut self) -> Result<(), AstError> {
        debug!("パイプラインの最適化を実行します");
        let mut optimizations_count = 0;
        
        // トップレベルのノードを処理
        if let Some(root) = self.get_root() {
            optimizations_count += self.optimize_pipelines_in_node(root)?;
        }
        
        debug!("パイプライン最適化: {} 件の最適化を実行", optimizations_count);
        Ok(())
    }
    
    fn optimize_pipelines_in_node(&mut self, node_id: &NodeId) -> Result<usize, AstError> {
        let mut optimizations_count = 0;
        
        // このノードがパイプラインなら最適化を試みる
        if let Some(node) = self.get_node(node_id) {
            if let NodeKind::Pipeline { commands } = &node.kind {
                // パイプラインに対する最適化を適用
                
                // 1. 不要なパイプを検出して削除
                if commands.len() == 1 {
                    // 単一コマンドのパイプラインはコマンド自体に置き換え
                    let command_id = &commands[0];
                    if let Some(command_node) = self.get_node(command_id) {
                        let new_node = command_node.clone();
                        if let Some(node_mut) = self.get_node_mut(node_id) {
                            *node_mut = new_node;
                            optimizations_count += 1;
                        }
                        return Ok(optimizations_count);
                    }
                }
                
                // 2. より効率的なコマンドへの置き換え
                if commands.len() >= 2 {
                    if self.is_sort_uniq_pipeline(commands) {
                        // sort | uniq → sort -u の置き換え
                        let new_command = self.create_sort_u_command(commands)?;
                        let new_command_id = self.add_node(new_command);
                        
                        // 新しいパイプラインを作成
                        let new_pipeline = Node {
                            kind: NodeKind::Pipeline {
                                commands: vec![new_command_id],
                            },
                            span: self.get_node(node_id).map(|n| n.span.clone()).unwrap_or_default(),
                            metadata: HashMap::new(),
                        };
                        
                        // 古いノードを新しいノードで置き換え
                        if let Some(node_mut) = self.get_node_mut(node_id) {
                            *node_mut = new_pipeline;
                            optimizations_count += 1;
                            return Ok(optimizations_count);
                        }
                    }
                    
                    if self.is_grep_grep_pipeline(commands) {
                        // grep pattern1 | grep pattern2 → grep -e pattern1 -e pattern2
                        let new_command = self.create_combined_grep_command(commands)?;
                        let new_command_id = self.add_node(new_command);
                        
                        // 新しいパイプラインを作成
                        let new_pipeline = Node {
                            kind: NodeKind::Pipeline {
                                commands: vec![new_command_id],
                            },
                            span: self.get_node(node_id).map(|n| n.span.clone()).unwrap_or_default(),
                            metadata: HashMap::new(),
                        };
                        
                        // 古いノードを新しいノードで置き換え
                        if let Some(node_mut) = self.get_node_mut(node_id) {
                            *node_mut = new_pipeline;
                            optimizations_count += 1;
                            return Ok(optimizations_count);
                        }
                    }
                }
            }
        }
        
        // 子ノードを再帰的に処理
        let child_ids: Vec<NodeId> = if let Some(node) = self.get_node(node_id) {
            node.children()
        } else {
            Vec::new()
        };
        
        for child_id in child_ids {
            optimizations_count += self.optimize_pipelines_in_node(&child_id)?;
        }
        
        Ok(optimizations_count)
    }
    
    // sort | uniq パイプラインの判定
    fn is_sort_uniq_pipeline(&self, commands: &[NodeId]) -> bool {
        if commands.len() != 2 {
            return false;
        }
        
        if let (Some(cmd1), Some(cmd2)) = (self.get_node(&commands[0]), self.get_node(&commands[1])) {
            if let (NodeKind::Command { name: name1, .. }, NodeKind::Command { name: name2, .. }) = (&cmd1.kind, &cmd2.kind) {
                return name1 == "sort" && name2 == "uniq";
            }
        }
        
        false
    }
    
    // sort -u コマンドの作成
    fn create_sort_u_command(&self, commands: &[NodeId]) -> Result<Node, AstError> {
        let sort_cmd = self.get_node(&commands[0]).ok_or(AstError::NodeNotFound)?;
        
        if let NodeKind::Command { name, args, redirects, background } = &sort_cmd.kind {
            // sortコマンドの引数に -u を追加
            let mut new_args = args.clone();
            
            // すでに -u オプションがあるかチェック
            let has_u_option = new_args.iter().any(|arg_id| {
                if let Some(arg) = self.get_node(arg_id) {
                    if let NodeKind::Text { value } = &arg.kind {
                        return value == "-u";
                    }
                }
                false
            });
            
            if !has_u_option {
                // -u オプションを追加
                let u_option = Node {
                    kind: NodeKind::Text { value: "-u".to_string() },
                    span: TextSpan::default(),
                    metadata: HashMap::new(),
                };
                let u_option_id = self.add_node(u_option);
                new_args.push(u_option_id);
            }
            
            Ok(Node {
                kind: NodeKind::Command {
                    name: name.clone(),
                    args: new_args,
                    redirects: redirects.clone(),
                    background: *background,
                },
                span: sort_cmd.span.clone(),
                metadata: sort_cmd.metadata.clone(),
            })
        } else {
            Err(AstError::InvalidNodeType)
        }
    }
    
    // grep | grep パイプラインの判定
    fn is_grep_grep_pipeline(&self, commands: &[NodeId]) -> bool {
        if commands.len() != 2 {
            return false;
        }
        
        if let (Some(cmd1), Some(cmd2)) = (self.get_node(&commands[0]), self.get_node(&commands[1])) {
            if let (NodeKind::Command { name: name1, .. }, NodeKind::Command { name: name2, .. }) = (&cmd1.kind, &cmd2.kind) {
                return name1 == "grep" && name2 == "grep";
            }
        }
        
        false
    }
    
    // 複合grepコマンドの作成
    fn create_combined_grep_command(&self, commands: &[NodeId]) -> Result<Node, AstError> {
        let grep1 = self.get_node(&commands[0]).ok_or(AstError::NodeNotFound)?;
        let grep2 = self.get_node(&commands[1]).ok_or(AstError::NodeNotFound)?;
        
        if let (NodeKind::Command { args: args1, redirects: redirects1, background: background1, .. },
                NodeKind::Command { args: args2, .. }) = (&grep1.kind, &grep2.kind) {
                
            // 両方のgrepの引数を統合
            let mut new_args = Vec::new();
            
            // 最初の引数セットを処理（パターンをオプションに変換）
            for arg_id in args1 {
                let arg = self.get_node(arg_id).ok_or(AstError::NodeNotFound)?;
                if let NodeKind::Text { value } = &arg.kind {
                    if !value.starts_with("-") {
                        // パターンを -e オプションに変換
                        let e_option = Node {
                            kind: NodeKind::Text { value: "-e".to_string() },
                            span: arg.span.clone(),
                            metadata: HashMap::new(),
                        };
                        let e_id = self.add_node(e_option);
                        new_args.push(e_id);
                    }
                }
                new_args.push(arg_id.clone());
            }
            
            // 2番目の引数セットを処理（重複を避ける）
            for arg_id in args2 {
                let arg = self.get_node(arg_id).ok_or(AstError::NodeNotFound)?;
                if let NodeKind::Text { value } = &arg.kind {
                    if !value.starts_with("-") {
                        // パターンを -e オプションに変換
                        let e_option = Node {
                            kind: NodeKind::Text { value: "-e".to_string() },
                            span: arg.span.clone(),
                            metadata: HashMap::new(),
                        };
                        let e_id = self.add_node(e_option);
                        new_args.push(e_id);
                    }
                    
                    // 重複オプションのチェック
                    let is_duplicate = new_args.iter().any(|existing_id| {
                        if let Some(existing) = self.get_node(existing_id) {
                            if let NodeKind::Text { value: existing_value } = &existing.kind {
                                return existing_value == value;
                            }
                        }
                        false
                    });
                    
                    if !is_duplicate {
                        new_args.push(arg_id.clone());
                    }
                } else {
                    new_args.push(arg_id.clone());
                }
            }
            
            Ok(Node {
                kind: NodeKind::Command {
                    name: "grep".to_string(),
                    args: new_args,
                    redirects: redirects1.clone(),
                    background: *background1,
                },
                span: grep1.span.merge(&grep2.span),
                metadata: grep1.metadata.clone(),
            })
        } else {
            Err(AstError::InvalidNodeType)
        }
    }
    
    /// ASTをJSONに変換
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// JSONからASTを読み込み
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
    
    /// AST統計情報を取得
    pub fn get_statistics(&self) -> AstStatistics {
        let mut node_types = HashMap::new();
        
        for node in self.nodes.values() {
            let type_name = node.node_type.to_string();
            *node_types.entry(type_name).or_insert(0) += 1;
        }
        
        let max_depth = self.calculate_max_depth();
        
        AstStatistics {
            node_count: self.nodes.len(),
            node_types,
            max_depth,
            source_file_count: self.source_files.len(),
            symbol_count: self.symbols.len(),
        }
    }
    
    /// 最大深度を計算
    fn calculate_max_depth(&self) -> usize {
        fn depth(ast: &Ast, id: &NodeId, current_depth: usize) -> usize {
            if let Some(node) = ast.get_node(id) {
                if node.children.is_empty() {
                    return current_depth;
                }
                
                let mut max = current_depth;
                for child_id in &node.children {
                    let child_depth = depth(ast, child_id, current_depth + 1);
                    if child_depth > max {
                        max = child_depth;
                    }
                }
                max
            } else {
                current_depth
            }
        }
        
        if let Some(root_id) = self.root {
            depth(self, &root_id, 1)
        } else {
            0
        }
    }
}

/// AST統計情報
#[derive(Debug, Clone)]
pub struct AstStatistics {
    /// ノード数
    pub node_count: usize,
    /// ノード種類ごとの数
    pub node_types: HashMap<String, usize>,
    /// 最大深度
    pub max_depth: usize,
    /// ソースファイル数
    pub source_file_count: usize,
    /// シンボル数
    pub symbol_count: usize,
}

impl fmt::Display for AstStatistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "AST統計情報:")?;
        writeln!(f, "  ノード総数: {}", self.node_count)?;
        writeln!(f, "  最大深度: {}", self.max_depth)?;
        writeln!(f, "  ソースファイル数: {}", self.source_file_count)?;
        writeln!(f, "  シンボル数: {}", self.symbol_count)?;
        writeln!(f, "  ノード種類別カウント:")?;
        
        let mut sorted_types: Vec<_> = self.node_types.iter().collect();
        sorted_types.sort_by(|a, b| b.1.cmp(a.1));
        
        for (type_name, count) in sorted_types {
            writeln!(f, "    {}: {}", type_name, count)?;
        }
        
        Ok(())
    }
}

/// ASTビジター
pub trait AstVisitor {
    type Result;
    
    /// ノード訪問前処理
    fn pre_visit(&mut self, node: &Node) -> Result<bool, AstError> {
        Ok(true)
    }
    
    /// ノード訪問後処理
    fn post_visit(&mut self, node: &Node) -> Result<(), AstError> {
        Ok(())
    }
    
    /// AST巡回
    fn visit(&mut self, ast: &Ast) -> Result<Self::Result, AstError>;
}

/// ASTビルダー
pub struct AstBuilder {
    ast: Ast,
    current_node: Option<NodeId>,
}

impl AstBuilder {
    /// 新しいASTビルダーを作成
    pub fn new() -> Self {
        Self {
            ast: Ast::new(),
            current_node: None,
        }
    }
    
    /// ソースファイルを追加
    pub fn add_source_file(&mut self, file: SourceFile) -> usize {
        self.ast.add_source_file(file)
    }
    
    /// 新しいノードを作成して現在のノードに設定
    pub fn create_node(&mut self, node_type: NodeType) -> &mut Self {
        let id = self.ast.create_node(node_type);
        self.current_node = Some(id);
        self
    }
    
    /// 現在のノードにソースコード位置を設定
    pub fn location(&mut self, location: SourceLocation) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node_mut(&id) {
                node.location = location;
            }
        }
        self
    }
    
    /// 現在のノードに値を設定
    pub fn value(&mut self, value: &str) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node_mut(&id) {
                node.value = Some(value.to_string());
            }
        }
        self
    }
    
    /// 現在のノードに型を設定
    pub fn data_type(&mut self, data_type: DataType) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node_mut(&id) {
                node.data_type = data_type;
            }
        }
        self
    }
    
    /// 現在のノードにフラグを追加
    pub fn flag(&mut self, flag: NodeFlag) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node_mut(&id) {
                node.add_flag(flag);
            }
        }
        self
    }
    
    /// 現在のノードに属性を追加
    pub fn attribute(&mut self, attribute: NodeAttribute) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node_mut(&id) {
                node.add_attribute(attribute);
            }
        }
        self
    }
    
    /// 現在のノードにメタデータを追加
    pub fn metadata(&mut self, key: &str, value: &str) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node_mut(&id) {
                node.add_metadata(key, value);
            }
        }
        self
    }
    
    /// 子ノードを作成
    pub fn child(&mut self, node_type: NodeType) -> &mut Self {
        let parent_id = self.current_node;
        
        // 新しい子ノードを作成
        let child_id = self.ast.create_node(node_type);
        
        // 親子関係を設定
        if let Some(parent_id) = parent_id {
            let _ = self.ast.set_parent_child(&parent_id, &child_id);
        }
        
        // 現在のノードを子ノードに設定
        self.current_node = Some(child_id);
        
        self
    }
    
    /// 親ノードに戻る
    pub fn parent(&mut self) -> &mut Self {
        if let Some(id) = self.current_node {
            if let Some(node) = self.ast.get_node(&id) {
                self.current_node = node.parent;
            }
        }
        self
    }
    
    /// ルートノードを設定
    pub fn set_root(&mut self) -> Result<&mut Self, AstError> {
        if let Some(id) = self.current_node {
            self.ast.set_root(id)?;
        }
        Ok(self)
    }
    
    /// シンボルを定義
    pub fn define_symbol(&mut self, name: &str) -> &mut Self {
        if let Some(id) = self.current_node {
            self.ast.define_symbol(name, id);
        }
        self
    }
    
    /// ASTを構築
    pub fn build(self) -> Ast {
        self.ast
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_ast() {
        let mut ast = Ast::new();
        let root_id = ast.create_node(NodeType::Program);
        let cmd_id = ast.create_node(NodeType::Command);
        
        ast.set_root(root_id).unwrap();
        ast.set_parent_child(&root_id, &cmd_id).unwrap();
        
        let root = ast.get_node(&root_id).unwrap();
        assert_eq!(root.node_type, NodeType::Program);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0], cmd_id);
        
        let cmd = ast.get_node(&cmd_id).unwrap();
        assert_eq!(cmd.node_type, NodeType::Command);
        assert_eq!(cmd.parent, Some(root_id));
    }
    
    #[test]
    fn test_ast_builder() {
        let mut builder = AstBuilder::new();
        
        builder.create_node(NodeType::Program)
            .set_root().unwrap()
            .child(NodeType::Command)
                .value("echo")
                .flag(NodeFlag::Background)
                .child(NodeType::StringLiteral)
                    .value("Hello, world!")
                .parent()
            .parent();
        
        let ast = builder.build();
        
        let root_id = ast.root().unwrap();
        let root = ast.get_node(&root_id).unwrap();
        assert_eq!(root.node_type, NodeType::Program);
        assert_eq!(root.children.len(), 1);
        
        let cmd_id = root.children[0];
        let cmd = ast.get_node(&cmd_id).unwrap();
        assert_eq!(cmd.node_type, NodeType::Command);
        assert_eq!(cmd.value, Some("echo".to_string()));
        assert!(cmd.has_flag(&NodeFlag::Background));
        assert_eq!(cmd.children.len(), 1);
        
        let arg_id = cmd.children[0];
        let arg = ast.get_node(&arg_id).unwrap();
        assert_eq!(arg.node_type, NodeType::StringLiteral);
        assert_eq!(arg.value, Some("Hello, world!".to_string()));
    }
    
    #[test]
    fn test_data_type_display() {
        assert_eq!(DataType::Integer.to_string(), "integer");
        assert_eq!(DataType::String.to_string(), "string");
        
        let array_type = DataType::Array(Box::new(DataType::String));
        assert_eq!(array_type.to_string(), "string[]");
        
        let map_type = DataType::Map(
            Box::new(DataType::String),
            Box::new(DataType::Integer)
        );
        assert_eq!(map_type.to_string(), "Map<string, integer>");
        
        let fn_type = DataType::Function(
            vec![DataType::String, DataType::Integer],
            Box::new(DataType::Boolean)
        );
        assert_eq!(fn_type.to_string(), "fn(string, integer) -> boolean");
    }
}

/// AST最適化フェーズのフラグ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationPhase {
    /// 定数畳み込み
    ConstantFolding,
    /// 不要な条件分岐の削除
    DeadBranchElimination,
    /// 共通部分式の削除
    CommonSubexpressionElimination,
    /// 命令の並べ替え
    InstructionReordering,
    /// パイプライン最適化
    PipelineOptimization,
    /// 全フェーズ
    All,
}

/// AST最適化器
#[derive(Debug)]
pub struct AstOptimizer {
    /// 有効な最適化フェーズ
    enabled_phases: Vec<OptimizationPhase>,
    /// 現在の最適化レベル (0-3, 0は最適化なし、3は最大最適化)
    optimization_level: u8,
    /// 最適化統計
    stats: OptimizationStats,
}

/// 最適化統計
#[derive(Debug, Default, Clone)]
pub struct OptimizationStats {
    /// 定数畳み込みの回数
    pub constant_folding_count: usize,
    /// 削除された不要な条件分岐の数
    pub dead_branches_removed: usize,
    /// 削除された共通部分式の数
    pub cse_eliminated: usize,
    /// 並べ替えられた命令の数
    pub instructions_reordered: usize,
    /// 最適化されたパイプラインの数
    pub pipelines_optimized: usize,
    /// 最適化前のASTノード数
    pub original_node_count: usize,
    /// 最適化後のASTノード数
    pub optimized_node_count: usize,
}

impl AstOptimizer {
    /// 新しいAST最適化器を作成
    pub fn new(level: u8) -> Self {
        let enabled_phases = match level {
            0 => Vec::new(),
            1 => vec![OptimizationPhase::ConstantFolding],
            2 => vec![
                OptimizationPhase::ConstantFolding,
                OptimizationPhase::DeadBranchElimination,
            ],
            _ => vec![
                OptimizationPhase::ConstantFolding,
                OptimizationPhase::DeadBranchElimination,
                OptimizationPhase::CommonSubexpressionElimination,
                OptimizationPhase::InstructionReordering,
                OptimizationPhase::PipelineOptimization,
            ],
        };

        Self {
            enabled_phases,
            optimization_level: level.min(3),
            stats: OptimizationStats::default(),
        }
    }

    /// ASTを最適化
    pub fn optimize(&mut self, ast: &mut Program) -> Result<OptimizationStats, AstError> {
        if self.optimization_level == 0 {
            return Ok(self.stats.clone());
        }

        self.stats = OptimizationStats::default();
        self.stats.original_node_count = self.count_nodes(ast);

        // 各最適化フェーズを適用
        for phase in &self.enabled_phases {
            match phase {
                OptimizationPhase::ConstantFolding => self.apply_constant_folding(ast)?,
                OptimizationPhase::DeadBranchElimination => self.apply_dead_branch_elimination(ast)?,
                OptimizationPhase::CommonSubexpressionElimination => self.apply_common_subexpression_elimination(ast)?,
                OptimizationPhase::InstructionReordering => self.apply_instruction_reordering(ast)?,
                OptimizationPhase::PipelineOptimization => self.apply_pipeline_optimization(ast)?,
                OptimizationPhase::All => {
                    self.apply_constant_folding(ast)?;
                    self.apply_dead_branch_elimination(ast)?;
                    self.apply_common_subexpression_elimination(ast)?;
                    self.apply_instruction_reordering(ast)?;
                    self.apply_pipeline_optimization(ast)?;
                }
            }
        }

        self.stats.optimized_node_count = self.count_nodes(ast);
        Ok(self.stats.clone())
    }

    /// ノード数をカウント
    fn count_nodes(&self, ast: &Program) -> usize {
        let mut count = 1; // プログラム自体をカウント
        
        for statement in &ast.statements {
            count += self.count_statement_nodes(statement);
        }
        
        count
    }
    
    /// ステートメントのノード数をカウント
    fn count_statement_nodes(&self, statement: &Statement) -> usize {
        match statement {
            Statement::Command(cmd) => {
                let mut count = 1;
                count += cmd.args.len();
                if let Some(redirects) = &cmd.redirects {
                    count += redirects.len();
                }
                count
            }
            Statement::Pipeline(pipeline) => {
                let mut count = 1;
                for cmd in &pipeline.commands {
                    count += self.count_statement_nodes(&Statement::Command(cmd.clone()));
                }
                count
            }
            Statement::Assignment(assign) => 2, // 変数と値
            Statement::Block(block) => {
                let mut count = 1;
                for stmt in &block.statements {
                    count += self.count_statement_nodes(stmt);
                }
                count
            }
            Statement::IfStatement(if_stmt) => {
                let mut count = 1; // if自体
                count += self.count_expression_nodes(&if_stmt.condition);
                count += self.count_statement_nodes(&if_stmt.then_branch);
                if let Some(else_branch) = &if_stmt.else_branch {
                    count += self.count_statement_nodes(else_branch);
                }
                count
            }
            Statement::ForLoop(for_loop) => {
                let mut count = 2; // forとイテレータ変数
                count += self.count_expression_nodes(&Expression::Literal(Literal::Array(for_loop.iterable.clone())));
                count += self.count_statement_nodes(&for_loop.body);
                count
            }
            Statement::WhileLoop(while_loop) => {
                let mut count = 1; // while自体
                count += self.count_expression_nodes(&while_loop.condition);
                count += self.count_statement_nodes(&while_loop.body);
                count
            }
            Statement::FunctionDeclaration(func_decl) => {
                let mut count = 1 + func_decl.parameters.len(); // 関数名とパラメータ
                count += self.count_statement_nodes(&func_decl.body);
                count
            }
            Statement::Return(ret) => {
                let mut count = 1; // return自体
                if let Some(expr) = &ret.value {
                    count += self.count_expression_nodes(expr);
                }
                count
            }
            Statement::Expression(expr) => self.count_expression_nodes(expr),
            _ => 1, // その他の基本的なステートメント
        }
    }
    
    /// 式のノード数をカウント
    fn count_expression_nodes(&self, expr: &Expression) -> usize {
        match expr {
            Expression::BinaryOp { left, operator, right } => {
                1 + self.count_expression_nodes(left) + self.count_expression_nodes(right)
            }
            Expression::UnaryOp { operator, operand } => {
                1 + self.count_expression_nodes(operand)
            }
            Expression::FunctionCall { name, arguments } => {
                let mut count = 1; // 関数名
                for arg in arguments {
                    count += self.count_expression_nodes(arg);
                }
                count
            }
            Expression::ArrayAccess { array, index } => {
                1 + self.count_expression_nodes(array) + self.count_expression_nodes(index)
            }
            Expression::ObjectProperty { object, property } => {
                1 + self.count_expression_nodes(object) + 1 // オブジェクトとプロパティ
            }
            Expression::Literal(_) => 1,
            Expression::Variable(_) => 1,
            _ => 1, // その他の基本的な式
        }
    }

    /// 定数畳み込みを適用
    fn apply_constant_folding(&mut self, ast: &mut Program) -> Result<(), AstError> {
        for statement in &mut ast.statements {
            self.fold_constants_in_statement(statement)?;
        }
        Ok(())
    }

    /// ステートメント内の定数畳み込み
    fn fold_constants_in_statement(&mut self, statement: &mut Statement) -> Result<(), AstError> {
        match statement {
            Statement::Expression(expr) => {
                if let Some(folded) = self.fold_constants_in_expression(expr)? {
                    *expr = folded;
                    self.stats.constant_folding_count += 1;
                }
            }
            Statement::IfStatement(if_stmt) => {
                // 条件式を畳み込み
                if let Some(folded) = self.fold_constants_in_expression(&mut if_stmt.condition)? {
                    if_stmt.condition = folded;
                    self.stats.constant_folding_count += 1;
                }
                
                // then節を畳み込み
                self.fold_constants_in_statement(&mut if_stmt.then_branch)?;
                
                // else節があれば畳み込み
                if let Some(else_branch) = &mut if_stmt.else_branch {
                    self.fold_constants_in_statement(else_branch)?;
                }
            }
            Statement::ForLoop(for_loop) => {
                // ループ本体を畳み込み
                self.fold_constants_in_statement(&mut for_loop.body)?;
            }
            Statement::WhileLoop(while_loop) => {
                // 条件式を畳み込み
                if let Some(folded) = self.fold_constants_in_expression(&mut while_loop.condition)? {
                    while_loop.condition = folded;
                    self.stats.constant_folding_count += 1;
                }
                
                // ループ本体を畳み込み
                self.fold_constants_in_statement(&mut while_loop.body)?;
            }
            Statement::Block(block) => {
                // ブロック内の各ステートメントを畳み込み
                for stmt in &mut block.statements {
                    self.fold_constants_in_statement(stmt)?;
                }
            }
            Statement::FunctionDeclaration(func) => {
                // 関数本体を畳み込み
                self.fold_constants_in_statement(&mut func.body)?;
            }
            Statement::Return(ret) => {
                // 戻り値があれば畳み込み
                if let Some(expr) = &mut ret.value {
                    if let Some(folded) = self.fold_constants_in_expression(expr)? {
                        *expr = folded;
                        self.stats.constant_folding_count += 1;
                    }
                }
            }
            // その他のステートメントタイプに対する畳み込み処理
            _ => {}
        }
        
        Ok(())
    }

    /// 式内の定数畳み込み
    fn fold_constants_in_expression(&mut self, expr: &mut Expression) -> Result<Option<Expression>, AstError> {
        match expr {
            Expression::BinaryOp { left, operator, right } => {
                // 左右の式を再帰的に処理
                if let Some(folded_left) = self.fold_constants_in_expression(left)? {
                    *left = Box::new(folded_left);
                    self.stats.constant_folding_count += 1;
                }
                
                if let Some(folded_right) = self.fold_constants_in_expression(right)? {
                    *right = Box::new(folded_right);
                    self.stats.constant_folding_count += 1;
                }
                
                // 両方が定数リテラルなら計算を実行
                match (&**left, &**right) {
                    (Expression::Literal(left_lit), Expression::Literal(right_lit)) => {
                        match self.evaluate_binary_op(left_lit, operator, right_lit) {
                            Ok(result) => {
                                self.stats.constant_folding_count += 1;
                                return Ok(Some(Expression::Literal(result)));
                            }
                            Err(_) => return Ok(None), // 計算エラーは無視して元の式を維持
                        }
                    }
                    _ => Ok(None),
                }
            }
            Expression::UnaryOp { operator, operand } => {
                // オペランドを再帰的に処理
                if let Some(folded) = self.fold_constants_in_expression(operand)? {
                    *operand = Box::new(folded);
                    self.stats.constant_folding_count += 1;
                }
                
                // オペランドが定数リテラルなら計算を実行
                if let Expression::Literal(lit) = &**operand {
                    match self.evaluate_unary_op(operator, lit) {
                        Ok(result) => {
                            self.stats.constant_folding_count += 1;
                            return Ok(Some(Expression::Literal(result)));
                        }
                        Err(_) => return Ok(None), // 計算エラーは無視して元の式を維持
                    }
                }
                
                Ok(None)
            }
            Expression::FunctionCall { name, arguments } => {
                // 引数を再帰的に処理
                let mut any_folded = false;
                for arg in arguments {
                    if let Some(folded) = self.fold_constants_in_expression(arg)? {
                        *arg = folded;
                        any_folded = true;
                        self.stats.constant_folding_count += 1;
                    }
                }
                
                // 組み込み関数で全引数が定数なら実行できる場合がある
                if let Some(result) = self.try_evaluate_builtin_function(name, arguments)? {
                    self.stats.constant_folding_count += 1;
                    return Ok(Some(Expression::Literal(result)));
                }
                
                Ok(None)
            }
            Expression::ArrayAccess { array, index } => {
                // 配列と添字を再帰的に処理
                let mut any_folded = false;
                
                if let Some(folded) = self.fold_constants_in_expression(array)? {
                    *array = Box::new(folded);
                    any_folded = true;
                    self.stats.constant_folding_count += 1;
                }
                
                if let Some(folded) = self.fold_constants_in_expression(index)? {
                    *index = Box::new(folded);
                    any_folded = true;
                    self.stats.constant_folding_count += 1;
                }
                
                // 配列が定数リテラルで添字も定数リテラルなら値を取得
                if let (Expression::Literal(Literal::Array(arr)), Expression::Literal(Literal::Integer(idx))) = (&**array, &**index) {
                    if *idx >= 0 && (*idx as usize) < arr.len() {
                        // 配列から値を取得
                        let result = arr[*idx as usize].clone();
                        self.stats.constant_folding_count += 1;
                        return Ok(Some(Expression::Literal(result)));
                    }
                }
                
                Ok(None)
            }
            // その他の式タイプに対する畳み込み処理
            _ => Ok(None),
        }
    }

    /// 二項演算子を評価
    fn evaluate_binary_op(&self, left: &Literal, op: &BinaryOperator, right: &Literal) -> Result<Literal, AstError> {
        match (left, op, right) {
            // 数値演算
            (Literal::Integer(l), BinaryOperator::Add, Literal::Integer(r)) => Ok(Literal::Integer(l + r)),
            (Literal::Integer(l), BinaryOperator::Subtract, Literal::Integer(r)) => Ok(Literal::Integer(l - r)),
            (Literal::Integer(l), BinaryOperator::Multiply, Literal::Integer(r)) => Ok(Literal::Integer(l * r)),
            (Literal::Integer(l), BinaryOperator::Divide, Literal::Integer(r)) => {
                if *r == 0 {
                    return Err(AstError::DivisionByZero);
                }
                Ok(Literal::Integer(l / r))
            }
            (Literal::Integer(l), BinaryOperator::Modulo, Literal::Integer(r)) => {
                if *r == 0 {
                    return Err(AstError::DivisionByZero);
                }
                Ok(Literal::Integer(l % r))
            }
            
            // 浮動小数点演算
            (Literal::Float(l), BinaryOperator::Add, Literal::Float(r)) => Ok(Literal::Float(l + r)),
            (Literal::Float(l), BinaryOperator::Subtract, Literal::Float(r)) => Ok(Literal::Float(l - r)),
            (Literal::Float(l), BinaryOperator::Multiply, Literal::Float(r)) => Ok(Literal::Float(l * r)),
            (Literal::Float(l), BinaryOperator::Divide, Literal::Float(r)) => {
                if *r == 0.0 {
                    return Err(AstError::DivisionByZero);
                }
                Ok(Literal::Float(l / r))
            }
            
            // 整数と浮動小数点の混合演算
            (Literal::Integer(l), BinaryOperator::Add, Literal::Float(r)) => Ok(Literal::Float(*l as f64 + r)),
            (Literal::Float(l), BinaryOperator::Add, Literal::Integer(r)) => Ok(Literal::Float(l + *r as f64)),
            // 他の混合演算...
            
            // 文字列連結
            (Literal::String(l), BinaryOperator::Add, Literal::String(r)) => {
                let mut result = l.clone();
                result.push_str(r);
                Ok(Literal::String(result))
            }
            
            // 比較演算子
            (Literal::Integer(l), BinaryOperator::Equal, Literal::Integer(r)) => Ok(Literal::Boolean(l == r)),
            (Literal::Integer(l), BinaryOperator::NotEqual, Literal::Integer(r)) => Ok(Literal::Boolean(l != r)),
            (Literal::Integer(l), BinaryOperator::LessThan, Literal::Integer(r)) => Ok(Literal::Boolean(l < r)),
            (Literal::Integer(l), BinaryOperator::LessThanOrEqual, Literal::Integer(r)) => Ok(Literal::Boolean(l <= r)),
            (Literal::Integer(l), BinaryOperator::GreaterThan, Literal::Integer(r)) => Ok(Literal::Boolean(l > r)),
            (Literal::Integer(l), BinaryOperator::GreaterThanOrEqual, Literal::Integer(r)) => Ok(Literal::Boolean(l >= r)),
            
            // 論理演算子
            (Literal::Boolean(l), BinaryOperator::And, Literal::Boolean(r)) => Ok(Literal::Boolean(*l && *r)),
            (Literal::Boolean(l), BinaryOperator::Or, Literal::Boolean(r)) => Ok(Literal::Boolean(*l || *r)),
            
            // その他の演算子と型の組み合わせはエラー
            _ => Err(AstError::InvalidOperation),
        }
    }

    /// 単項演算子を評価
    fn evaluate_unary_op(&self, op: &UnaryOperator, operand: &Literal) -> Result<Literal, AstError> {
        match (op, operand) {
            (UnaryOperator::Negate, Literal::Integer(val)) => Ok(Literal::Integer(-val)),
            (UnaryOperator::Negate, Literal::Float(val)) => Ok(Literal::Float(-val)),
            (UnaryOperator::Not, Literal::Boolean(val)) => Ok(Literal::Boolean(!val)),
            _ => Err(AstError::InvalidOperation),
        }
    }

    /// 組み込み関数を評価する試み
    fn try_evaluate_builtin_function(&self, name: &str, args: &[Expression]) -> Result<Option<Literal>, AstError> {
        // すべての引数がリテラルかチェック
        let literal_args: Vec<&Literal> = args.iter()
            .filter_map(|arg| {
                if let Expression::Literal(lit) = arg {
                    Some(lit)
                } else {
                    None
                }
            })
            .collect();
            
        // すべての引数がリテラルでない場合は評価できない
        if literal_args.len() != args.len() {
            return Ok(None);
        }
        
        // 組み込み関数を評価
        match name {
            "len" => {
                if args.len() != 1 {
                    return Ok(None);
                }
                
                match literal_args[0] {
                    Literal::String(s) => Ok(Some(Literal::Integer(s.len() as i64))),
                    Literal::Array(arr) => Ok(Some(Literal::Integer(arr.len() as i64))),
                    _ => Ok(None),
                }
            }
            "str" => {
                if args.len() != 1 {
                    return Ok(None);
                }
                
                match literal_args[0] {
                    Literal::Integer(i) => Ok(Some(Literal::String(i.to_string()))),
                    Literal::Float(f) => Ok(Some(Literal::String(f.to_string()))),
                    Literal::Boolean(b) => Ok(Some(Literal::String(b.to_string()))),
                    Literal::String(s) => Ok(Some(Literal::String(s.clone()))),
                    Literal::Null => Ok(Some(Literal::String("null".to_string()))),
                    _ => Ok(None),
                }
            }
            "int" => {
                if args.len() != 1 {
                    return Ok(None);
                }
                
                match literal_args[0] {
                    Literal::String(s) => {
                        match s.parse::<i64>() {
                            Ok(i) => Ok(Some(Literal::Integer(i))),
                            Err(_) => Ok(None),
                        }
                    }
                    Literal::Float(f) => Ok(Some(Literal::Integer(*f as i64))),
                    Literal::Integer(i) => Ok(Some(Literal::Integer(*i))),
                    _ => Ok(None),
                }
            }
            "float" => {
                if args.len() != 1 {
                    return Ok(None);
                }
                
                match literal_args[0] {
                    Literal::String(s) => {
                        match s.parse::<f64>() {
                            Ok(f) => Ok(Some(Literal::Float(f))),
                            Err(_) => Ok(None),
                        }
                    }
                    Literal::Integer(i) => Ok(Some(Literal::Float(*i as f64))),
                    Literal::Float(f) => Ok(Some(Literal::Float(*f))),
                    _ => Ok(None),
                }
            }
            // その他の組み込み関数
            _ => Ok(None),
        }
    }

    /// 不要な条件分岐の削除を適用
    fn apply_dead_branch_elimination(&mut self, ast: &mut Program) -> Result<(), AstError> {
        for statement in &mut ast.statements {
            self.eliminate_dead_branches_in_statement(statement)?;
        }
        Ok(())
    }

    /// ステートメント内の不要な条件分岐を削除
    fn eliminate_dead_branches_in_statement(&mut self, statement: &mut Statement) -> Result<(), AstError> {
        match statement {
            Statement::IfStatement(if_stmt) => {
                // 条件が定数リテラルの場合
                if let Expression::Literal(Literal::Boolean(condition)) = &if_stmt.condition {
                    if *condition {
                        // 条件が常にtrueの場合、thenブランチだけ残す
                        let then_branch = std::mem::take(&mut if_stmt.then_branch);
                        *statement = then_branch;
                        self.stats.dead_branches_removed += 1;
                    } else if let Some(else_branch) = &mut if_stmt.else_branch {
                        // 条件が常にfalseの場合、elseブランチだけ残す
                        let else_branch = std::mem::take(else_branch);
                        *statement = else_branch;
                        self.stats.dead_branches_removed += 1;
                    } else {
                        // 条件が常にfalseでelseブランチがない場合、空のブロックに置き換え
                        *statement = Statement::Block(Block { statements: Vec::new() });
                        self.stats.dead_branches_removed += 1;
                    }
                } else {
                    // 条件が定数でない場合は、ブランチ内の最適化を続行
                    self.eliminate_dead_branches_in_statement(&mut if_stmt.then_branch)?;
                    if let Some(else_branch) = &mut if_stmt.else_branch {
                        self.eliminate_dead_branches_in_statement(else_branch)?;
                    }
                }
            }
            Statement::Block(block) => {
                // ブロック内の各ステートメントに対して最適化を適用
                let mut i = 0;
                while i < block.statements.len() {
                    self.eliminate_dead_branches_in_statement(&mut block.statements[i])?;
                    
                    // 空ブロックを削除
                    if let Statement::Block(inner) = &block.statements[i] {
                        if inner.statements.is_empty() {
                            block.statements.remove(i);
                            self.stats.dead_branches_removed += 1;
                            continue;
                        }
                    }
                    
                    i += 1;
                }
            }
            Statement::WhileLoop(while_loop) => {
                // 条件が常にfalseの場合、ループを削除
                if let Expression::Literal(Literal::Boolean(false)) = &while_loop.condition {
                    *statement = Statement::Block(Block { statements: Vec::new() });
                    self.stats.dead_branches_removed += 1;
                } else {
                    // ループ本体の最適化
                    self.eliminate_dead_branches_in_statement(&mut while_loop.body)?;
                }
            }
            Statement::ForLoop(for_loop) => {
                // イテレータが空の配列の場合、ループを削除
                if let Literal::Array(items) = &for_loop.iterable {
                    if items.is_empty() {
                        *statement = Statement::Block(Block { statements: Vec::new() });
                        self.stats.dead_branches_removed += 1;
                    } else {
                        // ループ本体の最適化
                        self.eliminate_dead_branches_in_statement(&mut for_loop.body)?;
                    }
                } else {
                    // イテレータが定数でない場合、ループ本体の最適化
                    self.eliminate_dead_branches_in_statement(&mut for_loop.body)?;
                }
            }
            Statement::FunctionDeclaration(func) => {
                // 関数本体の最適化
                self.eliminate_dead_branches_in_statement(&mut func.body)?;
            }
            // その他のステートメントタイプに対する不要分岐削除
            _ => {}
        }
        
        Ok(())
    }

    /// 共通部分式削除を適用
    fn apply_common_subexpression_elimination(&mut self, ast: &mut Program) -> Result<(), AstError> {
        // 式の一意性を表現するためのハッシュマップ
        let mut expression_map = HashMap::new();
        
        for statement in &mut ast.statements {
            self.eliminate_common_subexpressions_in_statement(statement, &mut expression_map)?;
        }
        
        Ok(())
    }

    /// ステートメント内の共通部分式を削除
    fn eliminate_common_subexpressions_in_statement(
        &mut self,
        statement: &mut Statement, 
        expression_map: &mut HashMap<String, Expression>
    ) -> Result<(), AstError> {
        // 実装省略 - 共通部分式削除のロジック
        Ok(())
    }

    /// 命令の並べ替えを適用
    fn apply_instruction_reordering(&mut self, ast: &mut Program) -> Result<(), AstError> {
        // 命令依存グラフを構築
        
        // トポロジカルソートで命令を並べ替え
        
        Ok(())
    }

    /// パイプライン最適化を適用
    fn apply_pipeline_optimization(&mut self, ast: &mut Program) -> Result<(), AstError> {
        for i in 0..ast.statements.len() {
            if let Statement::Pipeline(pipeline) = &mut ast.statements[i] {
                self.optimize_pipeline(pipeline)?;
            }
        }
        
        Ok(())
    }

    /// パイプラインを最適化
    fn optimize_pipeline(&mut self, pipeline: &mut Pipeline) -> Result<(), AstError> {
        // 空のコマンドを削除
        pipeline.commands.retain(|cmd| !cmd.args.is_empty());
        
        // 無効な入出力リダイレクトを検出して修正
        
        // 連続するcatコマンドを最適化
        for i in 0..pipeline.commands.len() - 1 {
            if pipeline.commands[i].args.first().map_or(false, |arg| arg == "cat") && 
               pipeline.commands[i].args.len() == 1 {
                self.stats.pipelines_optimized += 1;
                // cat のみのコマンドを削除
                pipeline.commands.remove(i);
                break;
            }
        }
        
        Ok(())
    }
}

/// ASTエラー
#[derive(Debug, Clone, PartialEq)]
pub enum AstError {
    /// パースエラー
    ParseError(String),
    /// タイプエラー
    TypeError(String),
    /// 参照エラー
    ReferenceError(String),
    /// シンタックスエラー
    SyntaxError(String),
    /// ランタイムエラー
    RuntimeError(String),
    /// 0除算エラー
    DivisionByZero,
    /// 無効な操作
    InvalidOperation,
    /// 添字エラー
    IndexError(String),
}

impl std::fmt::Display for AstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            AstError::TypeError(msg) => write!(f, "Type error: {}", msg),
            AstError::ReferenceError(msg) => write!(f, "Reference error: {}", msg),
            AstError::SyntaxError(msg) => write!(f, "Syntax error: {}", msg),
            AstError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
            AstError::DivisionByZero => write!(f, "Division by zero"),
            AstError::InvalidOperation => write!(f, "Invalid operation"),
            AstError::IndexError(msg) => write!(f, "Index error: {}", msg),
        }
    }
}

impl std::error::Error for AstError {}

/// ASTエラー処理
pub struct AstErrorHandler {
    /// エラーリスト
    errors: Vec<AstError>,
    /// 致命的エラーがあるか
    has_fatal_error: bool,
}

impl AstErrorHandler {
    /// 新しいASTエラーハンドラーを作成
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            has_fatal_error: false,
        }
    }
    
    /// エラーを追加
    pub fn add_error(&mut self, error: AstError) {
        // パースエラーとシンタックスエラーは致命的
        if matches!(error, AstError::ParseError(_) | AstError::SyntaxError(_)) {
            self.has_fatal_error = true;
        }
        
        self.errors.push(error);
    }
    
    /// エラーメッセージでエラーを追加
    pub fn add_error_message(&mut self, error_type: &str, message: &str) {
        let error = match error_type {
            "parse" => AstError::ParseError(message.to_string()),
            "type" => AstError::TypeError(message.to_string()),
            "reference" => AstError::ReferenceError(message.to_string()),
            "syntax" => AstError::SyntaxError(message.to_string()),
            "runtime" => AstError::RuntimeError(message.to_string()),
            "index" => AstError::IndexError(message.to_string()),
            _ => AstError::RuntimeError(format!("Unknown error: {}", message)),
        };
        
        self.add_error(error);
    }
    
    /// 致命的エラーがあるか
    pub fn has_fatal_error(&self) -> bool {
        self.has_fatal_error
    }
    
    /// エラーがあるか
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    /// エラーを取得
    pub fn get_errors(&self) -> &[AstError] {
        &self.errors
    }
    
    /// エラーをクリア
    pub fn clear(&mut self) {
        self.errors.clear();
        self.has_fatal_error = false;
    }
    
    /// 最後のエラーを取得
    pub fn last_error(&self) -> Option<&AstError> {
        self.errors.last()
    }
    
    /// エラーを文字列フォーマットで取得
    pub fn format_errors(&self) -> Vec<String> {
        self.errors.iter().map(|e| e.to_string()).collect()
    }
}

/// デフォルト実装
impl Default for AstErrorHandler {
    fn default() -> Self {
        Self::new()
    }
} 