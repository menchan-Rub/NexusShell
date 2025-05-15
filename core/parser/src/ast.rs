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
    #[error("型エラー: 期待: {expected}, 実際: {actual}")]
    TypeError { expected: String, actual: String },
    
    /// 構文エラー
    #[error("構文エラー: {message}")]
    SyntaxError { message: String, location: SourceLocation },
    
    /// 名前解決エラー
    #[error("名前解決エラー: {0}")]
    NameResolutionError(String),
    
    /// 検証エラー
    #[error("検証エラー: {0}")]
    ValidationError(String),
    
    /// その他のエラー
    #[error("ASTエラー: {0}")]
    Other(String),
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
        // TODO: AST最適化の実装
        Ok(())
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