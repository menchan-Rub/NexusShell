use crate::{
    AstNode, Error, Result, Span, TokenKind, ParserContext, ParserError,
    RedirectionKind, PipelineKind
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::fmt;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use dashmap::DashMap;
use rayon::prelude::*;
use uuid::Uuid;

/// セマンティック解析ステージの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticStage {
    /// 変数解決ステージ
    VariableResolution,
    /// パス検証ステージ
    PathValidation,
    /// コマンド検証ステージ
    CommandValidation,
    /// 型チェックステージ
    TypeCheck,
    /// コンテキスト分析ステージ
    ContextAnalysis,
    /// データフロー分析ステージ
    DataFlowAnalysis,
    /// リソース使用分析ステージ
    ResourceUsageAnalysis,
    /// 副作用分析ステージ
    SideEffectAnalysis,
    /// 並列化可能性分析ステージ
    ParallelizabilityAnalysis,
    /// 静的最適化ステージ 
    StaticOptimization,
    /// セキュリティ分析ステージ
    SecurityAnalysis,
}

/// セマンティック解析器のトレイト
pub trait Analyzer {
    /// ASTノードの意味解析を実行
    fn analyze(&mut self, node: &AstNode) -> Result<AstNode>;
    
    /// 指定したステージのみを実行
    fn analyze_stage(&mut self, node: &AstNode, stage: SemanticStage) -> Result<AstNode>;
    
    /// 複数のステージを指定して実行
    fn analyze_stages(&mut self, node: &AstNode, stages: &[SemanticStage]) -> Result<AstNode>;
    
    /// 非同期で解析を実行
    async fn analyze_async(&mut self, node: &AstNode) -> Result<AstNode>;
    
    /// 並列解析を実行
    fn analyze_parallel(&mut self, node: &AstNode) -> Result<AstNode>;
}

/// 環境変数のスコープ
#[derive(Debug, Clone)]
pub struct Environment {
    /// 変数マップ
    variables: HashMap<String, String>,
    /// 親スコープ
    parent: Option<Box<Environment>>,
}

impl Environment {
    /// 新しい環境を作成
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            parent: None,
        }
    }
    
    /// 親スコープを持つ新しい環境を作成
    pub fn with_parent(parent: Environment) -> Self {
        Self {
            variables: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }
    
    /// 変数を設定
    pub fn set(&mut self, name: &str, value: &str) {
        self.variables.insert(name.to_string(), value.to_string());
    }
    
    /// 変数を取得
    pub fn get(&self, name: &str) -> Option<String> {
        if let Some(value) = self.variables.get(name) {
            Some(value.clone())
        } else if let Some(parent) = &self.parent {
            parent.get(name)
        } else {
            None
        }
    }
    
    /// 現在のスコープに変数が存在するか確認
    pub fn has_local(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }
    
    /// 現在のスコープと親スコープを含めて変数が存在するか確認
    pub fn has(&self, name: &str) -> bool {
        self.has_local(name) || self.parent.as_ref().map_or(false, |p| p.has(name))
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

/// セマンティック解析の設定
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// パス検証を有効にするかどうか
    pub enable_path_validation: bool,
    /// コマンド検証を有効にするかどうか
    pub enable_command_validation: bool,
    /// 型チェックを有効にするかどうか
    pub enable_type_check: bool,
    /// フロー解析を有効にするかどうか
    pub enable_flow_analysis: bool,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            enable_path_validation: true,
            enable_command_validation: true,
            enable_type_check: true,
            enable_flow_analysis: true,
        }
    }
}

/// 意味解析の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAnalysisType {
    /// 変数参照の検証
    VariableReference,
    /// コマンド存在確認
    CommandExists,
    /// タイプチェック
    TypeCheck,
    /// 実行コンテキスト解析
    ExecutionContext,
    /// データフロー解析
    DataFlow,
    /// 副作用解析
    SideEffect,
    /// リソース使用解析
    ResourceUsage,
}

/// 意味解析設定
#[derive(Debug, Clone)]
pub struct SemanticConfig {
    /// 有効化する解析
    pub enabled_analyses: HashSet<SemanticAnalysisType>,
    /// 警告を表示するか
    pub show_warnings: bool,
    /// 詳細な情報を表示するか
    pub verbose: bool,
    /// 環境変数の検証を行うか
    pub validate_env_vars: bool,
    /// コマンドの存在を検証するか
    pub validate_commands: bool,
    /// リダイレクトの検証を行うか
    pub validate_redirections: bool,
    /// パイプ接続の検証を行うか
    pub validate_pipes: bool,
    /// 最適化提案を生成するか
    pub generate_optimizations: bool,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        let mut enabled = HashSet::new();
        enabled.insert(SemanticAnalysisType::VariableReference);
        enabled.insert(SemanticAnalysisType::CommandExists);
        
        Self {
            enabled_analyses: enabled,
            show_warnings: true,
            verbose: false,
            validate_env_vars: true,
            validate_commands: true,
            validate_redirections: true,
            validate_pipes: true,
            generate_optimizations: true,
        }
    }
}

/// 意味解析結果の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticResultKind {
    /// エラー
    Error,
    /// 警告
    Warning,
    /// 情報
    Info,
    /// 最適化提案
    Optimization,
}

impl fmt::Display for SemanticResultKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => write!(f, "エラー"),
            Self::Warning => write!(f, "警告"),
            Self::Info => write!(f, "情報"),
            Self::Optimization => write!(f, "最適化"),
        }
    }
}

/// 意味解析の結果
#[derive(Debug, Clone)]
pub struct SemanticResult {
    /// 結果の種類
    pub kind: SemanticResultKind,
    /// メッセージ
    pub message: String,
    /// 位置情報
    pub span: Span,
    /// 解析の種類
    pub analysis_type: SemanticAnalysisType,
    /// 関連するノード
    pub node: Option<AstNode>,
    /// 修正方法
    pub fixes: Vec<String>,
}

impl SemanticResult {
    /// エラー結果を作成
    pub fn error(message: impl Into<String>, span: Span, analysis_type: SemanticAnalysisType) -> Self {
        Self {
            kind: SemanticResultKind::Error,
            message: message.into(),
            span,
            analysis_type,
            node: None,
            fixes: Vec::new(),
        }
    }
    
    /// 警告結果を作成
    pub fn warning(message: impl Into<String>, span: Span, analysis_type: SemanticAnalysisType) -> Self {
        Self {
            kind: SemanticResultKind::Warning,
            message: message.into(),
            span,
            analysis_type,
            node: None,
            fixes: Vec::new(),
        }
    }
    
    /// 情報結果を作成
    pub fn info(message: impl Into<String>, span: Span, analysis_type: SemanticAnalysisType) -> Self {
        Self {
            kind: SemanticResultKind::Info,
            message: message.into(),
            span,
            analysis_type,
            node: None,
            fixes: Vec::new(),
        }
    }
    
    /// 最適化提案を作成
    pub fn optimization(message: impl Into<String>, span: Span, analysis_type: SemanticAnalysisType) -> Self {
        Self {
            kind: SemanticResultKind::Optimization,
            message: message.into(),
            span,
            analysis_type,
            node: None,
            fixes: Vec::new(),
        }
    }
    
    /// 関連ノードを設定
    pub fn with_node(mut self, node: AstNode) -> Self {
        self.node = Some(node);
        self
    }
    
    /// 修正方法を追加
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fixes.push(fix.into());
        self
    }
    
    /// 複数の修正方法を追加
    pub fn with_fixes(mut self, fixes: Vec<String>) -> Self {
        self.fixes.extend(fixes);
        self
    }
}

/// コンテキスト情報のタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextType {
    /// コマンド実行コンテキスト
    Command,
    /// パイプライン実行コンテキスト
    Pipeline,
    /// 条件分岐コンテキスト
    Conditional,
    /// ループコンテキスト
    Loop,
    /// ブロックコンテキスト
    Block,
    /// スクリプトコンテキスト
    Script,
}

/// セマンティックコンテキスト
#[derive(Debug, Clone)]
pub struct SemanticContext {
    /// コンテキストのタイプ
    pub context_type: ContextType,
    /// コンテキストの深さ（ネスト）
    pub depth: usize,
    /// 親コンテキスト
    pub parent: Option<Arc<SemanticContext>>,
    /// コンテキストに関連するシンボル
    pub symbols: HashMap<String, SymbolInfo>,
    /// コンテキスト固有のプロパティ
    pub properties: HashMap<String, String>,
    /// コンテキストに関連するスパン
    pub span: Span,
}

impl SemanticContext {
    /// 新しいコンテキストを作成
    pub fn new(context_type: ContextType, span: Span) -> Self {
        Self {
            context_type,
            depth: 0,
            parent: None,
            symbols: HashMap::new(),
            properties: HashMap::new(),
            span,
        }
    }
    
    /// 親コンテキストから子コンテキストを作成
    pub fn with_parent(context_type: ContextType, span: Span, parent: Arc<SemanticContext>) -> Self {
        Self {
            context_type,
            depth: parent.depth + 1,
            parent: Some(parent),
            symbols: HashMap::new(),
            properties: HashMap::new(),
            span,
        }
    }
    
    /// シンボルを追加
    pub fn add_symbol(&mut self, symbol: SymbolInfo) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }
    
    /// シンボルを取得
    pub fn get_symbol(&self, name: &str) -> Option<&SymbolInfo> {
        // まず現在のコンテキストから検索
        if let Some(symbol) = self.symbols.get(name) {
            return Some(symbol);
        }
        
        // 親コンテキストを辿って検索
        let mut current = self.parent.as_ref();
        while let Some(parent) = current {
            if let Some(symbol) = parent.symbols.get(name) {
                return Some(symbol);
            }
            current = parent.parent.as_ref();
        }
        
        None
    }
    
    /// プロパティを設定
    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.properties.insert(key.into(), value.into());
    }
    
    /// プロパティを取得
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }
    
    /// 最も近い指定タイプの親コンテキストを検索
    pub fn find_parent_of_type(&self, context_type: ContextType) -> Option<Arc<SemanticContext>> {
        if self.context_type == context_type {
            return None; // 自分自身は返さない
        }
        
        let mut current = self.parent.as_ref();
        while let Some(parent) = current {
            if parent.context_type == context_type {
                return Some(parent.clone());
            }
            current = parent.parent.as_ref();
        }
        
        None
    }
    
    /// 最上位（ルート）コンテキストを取得
    pub fn get_root_context(&self) -> Arc<SemanticContext> {
        let mut current_opt = self.parent.clone();
        let mut current = match &current_opt {
            Some(ctx) => ctx.clone(),
            None => return Arc::new(self.clone()), // 親がなければ自分自身がルート
        };
        
        while let Some(parent) = &current.parent {
            current = parent.clone();
        }
        
        current
    }
}

/// コンテキスト分析マネージャー
#[derive(Debug)]
pub struct ContextAnalyzer {
    /// 現在のコンテキスト
    current_context: Option<Arc<SemanticContext>>,
    /// コンテキストスタック
    context_stack: Vec<Arc<SemanticContext>>,
    /// グローバルコンテキスト
    global_context: Arc<SemanticContext>,
    /// コンテキスト間の関係マップ
    context_relations: HashMap<usize, Vec<usize>>,
    /// コンテキストID割り当て
    context_id_counter: usize,
    /// コンテキストIDマップ
    context_id_map: HashMap<Arc<SemanticContext>, usize>,
}

impl ContextAnalyzer {
    /// 新しいコンテキスト分析マネージャーを作成
    pub fn new() -> Self {
        let global = Arc::new(SemanticContext::new(ContextType::Script, Span::default()));
        Self {
            current_context: Some(global.clone()),
            context_stack: vec![global.clone()],
            global_context: global,
            context_relations: HashMap::new(),
            context_id_counter: 0,
            context_id_map: HashMap::new(),
        }
    }
    
    /// 新しいコンテキストを作成して現在のコンテキストにする
    pub fn push_context(&mut self, context_type: ContextType, span: Span) {
        let parent = match &self.current_context {
            Some(ctx) => ctx.clone(),
            None => self.global_context.clone(),
        };
        
        let new_context = Arc::new(SemanticContext::with_parent(context_type, span, parent));
        
        // コンテキストIDを割り当て
        let parent_id = self.get_context_id(&parent);
        let new_id = self.assign_context_id(&new_context);
        
        // 関係を記録
        if let Some(relations) = self.context_relations.get_mut(&parent_id) {
            relations.push(new_id);
        } else {
            self.context_relations.insert(parent_id, vec![new_id]);
        }
        
        self.context_stack.push(new_context.clone());
        self.current_context = Some(new_context);
    }
    
    /// 現在のコンテキストを取り出し、1つ前のコンテキストに戻る
    pub fn pop_context(&mut self) -> Option<Arc<SemanticContext>> {
        if self.context_stack.len() <= 1 {
            return None; // グローバルコンテキストは取り出さない
        }
        
        let popped = self.context_stack.pop();
        self.current_context = self.context_stack.last().cloned();
        
        popped
    }
    
    /// 現在のコンテキストを取得
    pub fn current_context(&self) -> Option<Arc<SemanticContext>> {
        self.current_context.clone()
    }
    
    /// グローバルコンテキストを取得
    pub fn global_context(&self) -> Arc<SemanticContext> {
        self.global_context.clone()
    }
    
    /// コンテキストIDを取得または割り当て
    fn get_context_id(&mut self, context: &Arc<SemanticContext>) -> usize {
        if let Some(id) = self.context_id_map.get(context) {
            *id
        } else {
            self.assign_context_id(context)
        }
    }
    
    /// 新しいコンテキストIDを割り当て
    fn assign_context_id(&mut self, context: &Arc<SemanticContext>) -> usize {
        let id = self.context_id_counter;
        self.context_id_counter += 1;
        self.context_id_map.insert(context.clone(), id);
        id
    }
    
    /// ASTを分析してコンテキスト情報を構築
    pub fn analyze_ast(&mut self, ast: &AstNode) -> Result<()> {
        match ast {
            AstNode::Command { name, arguments, redirections, span } => {
                // コマンドコンテキストを作成
                self.push_context(ContextType::Command, span.clone());
                
                if let Some(ctx) = self.current_context() {
                    // 可変な参照を取得するためにArcを解除（安全な方法）
                    let ctx_ptr = Arc::as_ptr(&ctx);
                    let ctx_mut = unsafe { &mut *(ctx_ptr as *mut SemanticContext) };
                    
                    // コマンド情報を設定
                    ctx_mut.set_property("command_name", name.clone());
                    ctx_mut.set_property("arg_count", arguments.len().to_string());
                    ctx_mut.set_property("redirect_count", redirections.len().to_string());
                }
                
                // 引数とリダイレクションを分析
                for arg in arguments {
                    self.analyze_ast(arg)?;
                }
                
                for redirect in redirections {
                    self.analyze_ast(redirect)?;
                }
                
                // コンテキストをポップ
                self.pop_context();
            },
            AstNode::Pipeline { commands, kind, span } => {
                // パイプラインコンテキストを作成
                self.push_context(ContextType::Pipeline, span.clone());
                
                if let Some(ctx) = self.current_context() {
                    let ctx_ptr = Arc::as_ptr(&ctx);
                    let ctx_mut = unsafe { &mut *(ctx_ptr as *mut SemanticContext) };
                    
                    // パイプライン情報を設定
                    ctx_mut.set_property("command_count", commands.len().to_string());
                    ctx_mut.set_property("pipeline_kind", format!("{:?}", kind));
                }
                
                // コマンドを分析
                for cmd in commands {
                    self.analyze_ast(cmd)?;
                }
                
                // コンテキストをポップ
                self.pop_context();
            },
            // 他のノードタイプも同様に実装
            _ => {
                // その他のノードは現在のコンテキストで処理
            }
        }
        
        Ok(())
    }
    
    /// コンテキスト関係をダンプ（デバッグ用）
    pub fn dump_context_relations(&self) -> String {
        let mut result = String::new();
        result.push_str("コンテキスト関係:\n");
        
        for (parent_id, children) in &self.context_relations {
            result.push_str(&format!("Parent {}: ", parent_id));
            for child_id in children {
                result.push_str(&format!("{} ", child_id));
            }
            result.push('\n');
        }
        
        result
    }
}

/// シンボル情報
#[derive(Debug, Clone)]
struct SymbolInfo {
    /// シンボル名
    name: String,
    /// シンボルの種類
    kind: SymbolKind,
    /// 型情報
    shell_type: ShellType,
    /// 定義位置
    defined_at: Span,
    /// 参照位置
    references: Vec<Span>,
    /// スコープID
    scope_id: String,
    /// 属性（メタデータ）
    attributes: HashMap<String, String>,
    /// 定数値（定数の場合）
    constant_value: Option<String>,
    /// ドキュメンテーション
    documentation: Option<String>,
    /// 変数が初期化済みかどうか
    initialized: bool,
}

/// シンボルの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    /// ローカル変数
    LocalVariable,
    /// グローバル変数
    GlobalVariable,
    /// エクスポートされた変数
    ExportedVariable,
    /// 定数
    Constant,
    /// 関数
    Function,
    /// エイリアス
    Alias,
    /// コマンド
    Command,
    /// 引数
    Argument,
    /// 環境変数
    EnvironmentVariable,
    /// パラメータ
    Parameter,
    /// 戻り値
    ReturnValue,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalVariable => write!(f, "ローカル変数"),
            Self::GlobalVariable => write!(f, "グローバル変数"),
            Self::ExportedVariable => write!(f, "エクスポート変数"),
            Self::Constant => write!(f, "定数"),
            Self::Function => write!(f, "関数"),
            Self::Alias => write!(f, "エイリアス"),
            Self::Command => write!(f, "コマンド"),
            Self::Argument => write!(f, "引数"),
            Self::EnvironmentVariable => write!(f, "環境変数"),
            Self::Parameter => write!(f, "パラメータ"),
            Self::ReturnValue => write!(f, "戻り値"),
        }
    }
}

/// 新しい型システム - シェル値の型
#[derive(Debug, Clone, PartialEq)]
pub enum ShellType {
    /// 文字列型
    String,
    /// 整数型
    Integer,
    /// 浮動小数点型
    Float,
    /// 真偽値型
    Boolean,
    /// 配列型
    Array(Box<ShellType>),
    /// マップ型
    Map(Box<ShellType>, Box<ShellType>),
    /// パス型
    Path,
    /// コマンド型
    Command,
    /// 関数型
    Function(Vec<ShellType>, Box<ShellType>),
    /// ストリーム型
    Stream(Box<ShellType>),
    /// ファイルディスクリプタ型
    FileDescriptor,
    /// プロセスID型
    ProcessId,
    /// ジョブID型
    JobId,
    /// 正規表現型
    Regex,
    /// 日付時刻型
    DateTime,
    /// 任意型（型推論に使用）
    Any,
    /// 未知型（エラー状態）
    Unknown,
    /// ユニオン型（複数の型の可能性がある）
    Union(Vec<ShellType>),
    /// オプション型（値があるかないか）
    Option(Box<ShellType>),
    /// 結果型（成功または失敗）
    Result(Box<ShellType>, Box<ShellType>),
}

impl ShellType {
    /// 型の互換性をチェック
    pub fn is_compatible_with(&self, other: &ShellType) -> bool {
        match (self, other) {
            (ShellType::Any, _) | (_, ShellType::Any) => true,
            (ShellType::Unknown, _) | (_, ShellType::Unknown) => false,
            (ShellType::String, ShellType::String) => true,
            (ShellType::Integer, ShellType::Integer) => true,
            (ShellType::Float, ShellType::Float) => true,
            (ShellType::Boolean, ShellType::Boolean) => true,
            (ShellType::Path, ShellType::Path) => true,
            (ShellType::Integer, ShellType::Float) | (ShellType::Float, ShellType::Integer) => true,
            (ShellType::String, ShellType::Path) | (ShellType::Path, ShellType::String) => true,
            (ShellType::Array(t1), ShellType::Array(t2)) => t1.is_compatible_with(t2),
            (ShellType::Map(k1, v1), ShellType::Map(k2, v2)) => 
                k1.is_compatible_with(k2) && v1.is_compatible_with(v2),
            (ShellType::Function(p1, r1), ShellType::Function(p2, r2)) => {
                if p1.len() != p2.len() {
                    return false;
                }
                
                for (param1, param2) in p1.iter().zip(p2.iter()) {
                    if !param1.is_compatible_with(param2) {
                        return false;
                    }
                }
                
                r1.is_compatible_with(r2)
            },
            (ShellType::Stream(t1), ShellType::Stream(t2)) => t1.is_compatible_with(t2),
            (ShellType::Option(t1), ShellType::Option(t2)) => t1.is_compatible_with(t2),
            (ShellType::Result(ok1, err1), ShellType::Result(ok2, err2)) => 
                ok1.is_compatible_with(ok2) && err1.is_compatible_with(err2),
            (ShellType::Union(types1), _) => types1.iter().any(|t| t.is_compatible_with(other)),
            (_, ShellType::Union(types2)) => types2.iter().any(|t| self.is_compatible_with(t)),
            _ => false,
        }
    }
    
    /// 型を結合（ユニオン型の作成）
    pub fn union_with(&self, other: &ShellType) -> ShellType {
        if self.is_compatible_with(other) {
            // 互換性がある場合は、より一般的な型を返す
            self.generalize(other)
        } else {
            // 互換性がない場合はユニオン型を作成
            match (self, other) {
                (ShellType::Union(types1), ShellType::Union(types2)) => {
                    let mut types = types1.clone();
                    for t in types2 {
                        if !types.contains(t) {
                            types.push(t.clone());
                        }
                    }
                    ShellType::Union(types)
                },
                (ShellType::Union(types), other_type) | (other_type, ShellType::Union(types)) => {
                    let mut new_types = types.clone();
                    if !new_types.contains(other_type) {
                        new_types.push(other_type.clone());
                    }
                    ShellType::Union(new_types)
                },
                (t1, t2) => ShellType::Union(vec![t1.clone(), t2.clone()]),
            }
        }
    }
    
    /// 型の一般化（より広い型に変換）
    pub fn generalize(&self, other: &ShellType) -> ShellType {
        match (self, other) {
            (ShellType::Any, _) => ShellType::Any,
            (_, ShellType::Any) => ShellType::Any,
            (ShellType::Unknown, other) => other.clone(),
            (other, ShellType::Unknown) => other.clone(),
            (ShellType::Integer, ShellType::Float) | (ShellType::Float, ShellType::Integer) => ShellType::Float,
            (ShellType::String, ShellType::Path) | (ShellType::Path, ShellType::String) => ShellType::String,
            (ShellType::Option(t1), ShellType::Option(t2)) => 
                ShellType::Option(Box::new(t1.generalize(t2))),
            (ShellType::Array(t1), ShellType::Array(t2)) => 
                ShellType::Array(Box::new(t1.generalize(t2))),
            (ShellType::Stream(t1), ShellType::Stream(t2)) => 
                ShellType::Stream(Box::new(t1.generalize(t2))),
            (t1, t2) if t1 == t2 => t1.clone(),
            _ => ShellType::Union(vec![self.clone(), other.clone()]),
        }
    }
    
    /// 型の具体化（より具体的な型に変換）
    pub fn concretize(&self) -> ShellType {
        match self {
            ShellType::Any => ShellType::String, // デフォルトは文字列型
            ShellType::Union(types) if !types.is_empty() => types[0].clone(),
            ShellType::Option(inner) => inner.concretize(),
            ShellType::Result(ok, _) => ok.concretize(),
            _ => self.clone(),
        }
    }
    
    /// 型変換が可能かどうかをチェック
    pub fn can_convert_to(&self, target: &ShellType) -> bool {
        match (self, target) {
            (_, ShellType::Any) => true,
            (ShellType::Any, _) => true,
            (ShellType::String, ShellType::Integer) => true,
            (ShellType::String, ShellType::Float) => true,
            (ShellType::String, ShellType::Boolean) => true,
            (ShellType::String, ShellType::Path) => true,
            (ShellType::String, ShellType::Regex) => true,
            (ShellType::String, ShellType::DateTime) => true,
            (ShellType::Integer, ShellType::String) => true,
            (ShellType::Integer, ShellType::Float) => true,
            (ShellType::Integer, ShellType::Boolean) => true,
            (ShellType::Float, ShellType::String) => true,
            (ShellType::Float, ShellType::Integer) => true,
            (ShellType::Boolean, ShellType::String) => true,
            (ShellType::Boolean, ShellType::Integer) => true,
            (ShellType::Path, ShellType::String) => true,
            (ShellType::DateTime, ShellType::String) => true,
            (ShellType::Array(_), ShellType::String) => true,
            (ShellType::Map(_, _), ShellType::String) => true,
            (s, t) => s.is_compatible_with(t),
        }
    }
}

impl fmt::Display for ShellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellType::String => write!(f, "string"),
            ShellType::Integer => write!(f, "integer"),
            ShellType::Float => write!(f, "float"),
            ShellType::Boolean => write!(f, "boolean"),
            ShellType::Array(t) => write!(f, "array<{}>", t),
            ShellType::Map(k, v) => write!(f, "map<{}, {}>", k, v),
            ShellType::Path => write!(f, "path"),
            ShellType::Command => write!(f, "command"),
            ShellType::Function(params, ret) => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", ret)
            },
            ShellType::Stream(t) => write!(f, "stream<{}>", t),
            ShellType::FileDescriptor => write!(f, "fd"),
            ShellType::ProcessId => write!(f, "pid"),
            ShellType::JobId => write!(f, "jobid"),
            ShellType::Regex => write!(f, "regex"),
            ShellType::DateTime => write!(f, "datetime"),
            ShellType::Any => write!(f, "any"),
            ShellType::Unknown => write!(f, "unknown"),
            ShellType::Union(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            },
            ShellType::Option(t) => write!(f, "option<{}>", t),
            ShellType::Result(ok, err) => write!(f, "result<{}, {}>", ok, err),
        }
    }
}

/// 高度なシンボルテーブル
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// シンボルマップ（名前 -> シンボル情報）
    symbols: HashMap<String, SymbolInfo>,
    /// 親スコープ
    parent: Option<Arc<RwLock<SymbolTable>>>,
    /// スコープID
    scope_id: String,
    /// スコープ名
    scope_name: String,
    /// スコープの開始位置
    scope_span: Span,
}

impl SymbolTable {
    /// 新しいシンボルテーブルを作成
    pub fn new(name: &str, span: Span) -> Self {
        let scope_id = Uuid::new_v4().to_string();
        Self {
            symbols: HashMap::new(),
            parent: None,
            scope_id,
            scope_name: name.to_string(),
            scope_span: span,
        }
    }
    
    /// 親スコープを持つ新しいシンボルテーブルを作成
    pub fn with_parent(name: &str, span: Span, parent: Arc<RwLock<SymbolTable>>) -> Self {
        let scope_id = Uuid::new_v4().to_string();
        Self {
            symbols: HashMap::new(),
            parent: Some(parent),
            scope_id,
            scope_name: name.to_string(),
            scope_span: span,
        }
    }
    
    /// シンボルを定義
    pub fn define(&mut self, symbol: SymbolInfo) -> Result<()> {
        let name = symbol.name.clone();
        if self.symbols.contains_key(&name) {
            return Err(ParserError::SemanticError(
                format!("シンボル '{}'は既に定義されています", name),
                symbol.defined_at,
            ));
        }
        
        self.symbols.insert(name, symbol);
        Ok(())
    }
    
    /// シンボルを更新
    pub fn update(&mut self, symbol: SymbolInfo) -> bool {
        let name = symbol.name.clone();
        if self.symbols.contains_key(&name) {
            self.symbols.insert(name, symbol);
            true
        } else {
            false
        }
    }
    
    /// シンボルを参照
    pub fn reference(&mut self, name: &str, usage_span: Span) -> Result<()> {
        if let Some(symbol) = self.symbols.get_mut(name) {
            symbol.references.push(usage_span);
            Ok(())
        } else if let Some(parent) = &self.parent {
            let mut parent = parent.write().unwrap();
            parent.reference(name, usage_span)
        } else {
            Err(ParserError::SemanticError(
                format!("未定義のシンボル '{}'を参照しています", name),
                usage_span,
            ))
        }
    }
    
    /// シンボルを検索
    pub fn lookup(&self, name: &str) -> Option<SymbolInfo> {
        if let Some(symbol) = self.symbols.get(name) {
            Some(symbol.clone())
        } else if let Some(parent) = &self.parent {
            let parent = parent.read().unwrap();
            parent.lookup(name)
        } else {
            None
        }
    }
    
    /// このスコープで定義されたシンボルのみを検索
    pub fn lookup_local(&self, name: &str) -> Option<SymbolInfo> {
        self.symbols.get(name).cloned()
    }
    
    /// すべてのシンボルを取得
    pub fn get_all_symbols(&self) -> Vec<SymbolInfo> {
        self.symbols.values().cloned().collect()
    }
    
    /// 未使用のシンボルを検索
    pub fn find_unused_symbols(&self) -> Vec<SymbolInfo> {
        self.symbols.values()
            .filter(|s| s.references.is_empty() && s.kind != SymbolKind::ExportedVariable)
            .cloned()
            .collect()
    }
}

/// シンボル情報
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// シンボル名
    pub name: String,
    /// シンボルの種類
    pub kind: SymbolKind,
    /// 型情報
    pub shell_type: ShellType,
    /// 定義位置
    pub defined_at: Span,
    /// 参照位置
    pub references: Vec<Span>,
    /// スコープID
    pub scope_id: String,
    /// 属性（メタデータ）
    pub attributes: HashMap<String, String>,
    /// 定数値（定数の場合）
    pub constant_value: Option<String>,
    /// ドキュメンテーション
    pub documentation: Option<String>,
    /// 変数が初期化済みかどうか
    pub initialized: bool,
}

impl SymbolInfo {
    /// 新しいシンボル情報を作成
    pub fn new(
        name: &str,
        kind: SymbolKind,
        shell_type: ShellType,
        defined_at: Span,
        scope_id: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            kind,
            shell_type,
            defined_at,
            references: Vec::new(),
            scope_id: scope_id.to_string(),
            attributes: HashMap::new(),
            constant_value: None,
            documentation: None,
            initialized: false,
        }
    }
    
    /// 属性を追加
    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }
    
    /// 定数値を設定
    pub fn with_constant_value(mut self, value: &str) -> Self {
        self.constant_value = Some(value.to_string());
        self
    }
    
    /// ドキュメンテーションを設定
    pub fn with_documentation(mut self, docs: &str) -> Self {
        self.documentation = Some(docs.to_string());
        self
    }
    
    /// 初期化済みとしてマーク
    pub fn mark_initialized(mut self) -> Self {
        self.initialized = true;
        self
    }
    
    /// シンボルが使用されているかどうか
    pub fn is_used(&self) -> bool {
        !self.references.is_empty()
    }
}

/// シンボルの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    /// ローカル変数
    LocalVariable,
    /// グローバル変数
    GlobalVariable,
    /// エクスポートされた変数
    ExportedVariable,
    /// 定数
    Constant,
    /// 関数
    Function,
    /// エイリアス
    Alias,
    /// コマンド
    Command,
    /// 引数
    Argument,
    /// 環境変数
    EnvironmentVariable,
    /// パラメータ
    Parameter,
    /// 戻り値
    ReturnValue,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalVariable => write!(f, "ローカル変数"),
            Self::GlobalVariable => write!(f, "グローバル変数"),
            Self::ExportedVariable => write!(f, "エクスポート変数"),
            Self::Constant => write!(f, "定数"),
            Self::Function => write!(f, "関数"),
            Self::Alias => write!(f, "エイリアス"),
            Self::Command => write!(f, "コマンド"),
            Self::Argument => write!(f, "引数"),
            Self::EnvironmentVariable => write!(f, "環境変数"),
            Self::Parameter => write!(f, "パラメータ"),
            Self::ReturnValue => write!(f, "戻り値"),
        }
    }
}

/// コマンド情報
#[derive(Debug, Clone)]
struct CommandInfo {
    /// コマンド名
    name: String,
    /// 最小引数数
    min_args: usize,
    /// 最大引数数（None は無制限）
    max_args: Option<usize>,
    /// サポートされるオプション
    options: HashSet<String>,
    /// 競合するオプションのグループ
    conflicting_options: Vec<Vec<String>>,
    /// カスタムバリデーション関数
    validator: Option<Arc<dyn Fn(&[AstNode]) -> Vec<SemanticResult> + Send + Sync>>,
}

/// レーベンシュタイン距離（編集距離）を計算
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    if s1 == s2 { return 0; }
    
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    
    let mut dp = vec![vec![0; s2_chars.len() + 1]; s1_chars.len() + 1];
    
    for i in 0..=s1_chars.len() {
        dp[i][0] = i;
    }
    
    for j in 0..=s2_chars.len() {
        dp[0][j] = j;
    }
    
    for i in 1..=s1_chars.len() {
        for j in 1..=s2_chars.len() {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
            
            dp[i][j] = (dp[i - 1][j] + 1)          // 削除
                .min(dp[i][j - 1] + 1)              // 挿入
                .min(dp[i - 1][j - 1] + cost);      // 置換
        }
    }
    
    dp[s1_chars.len()][s2_chars.len()]
}

/// 意味解析のテスト
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_analyzer_command_exists() {
        // TODO: テストケースを追加
    }
    
    #[test]
    fn test_analyzer_variable_reference() {
        // TODO: テストケースを追加
    }
    
    #[test]
    fn test_analyzer_redirection() {
        // TODO: テストケースを追加
    }
    
    #[test]
    fn test_analyzer_pipeline_optimization() {
        // TODO: テストケースを追加
    }
}

/// 意味解析を行うためのモジュール
/// ASTを解析し、意味的なエラーや警告を検出する
pub struct SemanticAnalyzer {
    /// 利用可能なコマンドのリスト
    available_commands: HashSet<String>,
    
    /// コマンドの引数パターン (コマンド名 -> 期待される引数パターン)
    command_arg_patterns: HashMap<String, Vec<ArgPattern>>,
    
    /// コマンドのフラグ情報 (コマンド名 -> フラグ情報)
    command_flags: HashMap<String, HashMap<String, FlagInfo>>,
    
    /// 検出されたエラーと警告のリスト
    errors: Vec<ParserError>,
    
    /// 変数定義の追跡 (変数名 -> 定義位置)
    variable_definitions: HashMap<String, Span>,
    
    /// 変数使用の追跡 (変数名 -> 使用位置のリスト)
    variable_usages: HashMap<String, Vec<Span>>,
}

/// 引数パターンを表す構造体
#[derive(Debug, Clone)]
pub struct ArgPattern {
    /// パターン名（説明用）
    pub name: String,
    
    /// 最小引数数
    pub min_args: usize,
    
    /// 最大引数数（Noneは無制限）
    pub max_args: Option<usize>,
    
    /// 引数の型の制約のリスト
    pub arg_constraints: Vec<ArgConstraint>,
}

/// 引数の制約を表すenum
#[derive(Debug, Clone)]
pub enum ArgConstraint {
    /// 任意の文字列
    Any,
    
    /// ファイルパス（存在するファイル）
    ExistingFile,
    
    /// ディレクトリパス（存在するディレクトリ）
    ExistingDirectory,
    
    /// ファイルパスまたはディレクトリパス（存在するか否かは問わない）
    Path,
    
    /// 数値
    Number,
    
    /// 列挙型（許可される値のリスト）
    Enum(Vec<String>),
    
    /// 正規表現パターン
    Pattern(String),
}

/// フラグ情報を表す構造体
#[derive(Debug, Clone)]
pub struct FlagInfo {
    /// フラグの短い形式（例: -f）
    pub short_form: Option<String>,
    
    /// フラグの長い形式（例: --file）
    pub long_form: Option<String>,
    
    /// フラグの説明
    pub description: String,
    
    /// フラグが引数を必要とするか
    pub requires_arg: bool,
    
    /// フラグの引数の制約
    pub arg_constraint: Option<ArgConstraint>,
}

impl SemanticAnalyzer {
    /// 新しいSemanticAnalyzerインスタンスを作成
    pub fn new() -> Self {
        let mut analyzer = Self {
            available_commands: HashSet::new(),
            command_arg_patterns: HashMap::new(),
            command_flags: HashMap::new(),
            errors: Vec::new(),
            variable_definitions: HashMap::new(),
            variable_usages: HashMap::new(),
        };
        
        // 基本的なシェルコマンドを登録
        analyzer.register_basic_commands();
        
        analyzer
    }
    
    /// 基本的なシェルコマンドと引数パターン、フラグを登録
    fn register_basic_commands(&mut self) {
        // 利用可能なコマンドを登録
        let basic_commands = [
            "cd", "ls", "pwd", "echo", "cat", "grep", "find", "mkdir", "rm", "cp", "mv",
            "touch", "chmod", "chown", "ps", "kill", "top", "man", "wget", "curl", "ssh",
            "scp", "git", "tar", "zip", "unzip", "sudo", "apt", "yum", "dnf", "brew",
        ];
        
        for cmd in basic_commands.iter() {
            self.available_commands.insert(cmd.to_string());
        }
        
        // cdコマンドの引数パターンを登録
        self.command_arg_patterns.insert(
            "cd".to_string(),
            vec![
                ArgPattern {
                    name: "ホームディレクトリに移動".to_string(),
                    min_args: 0,
                    max_args: Some(0),
                    arg_constraints: vec![],
                },
                ArgPattern {
                    name: "指定ディレクトリに移動".to_string(),
                    min_args: 1,
                    max_args: Some(1),
                    arg_constraints: vec![ArgConstraint::Path],
                },
            ],
        );
        
        // lsコマンドのフラグを登録
        let mut ls_flags = HashMap::new();
        ls_flags.insert(
            "l".to_string(),
            FlagInfo {
                short_form: Some("-l".to_string()),
                long_form: Some("--long".to_string()),
                description: "詳細形式でファイル情報を表示".to_string(),
                requires_arg: false,
                arg_constraint: None,
            },
        );
        ls_flags.insert(
            "a".to_string(),
            FlagInfo {
                short_form: Some("-a".to_string()),
                long_form: Some("--all".to_string()),
                description: "隠しファイルを含む全てのファイルを表示".to_string(),
                requires_arg: false,
                arg_constraint: None,
            },
        );
        
        self.command_flags.insert("ls".to_string(), ls_flags);
        
        // 他のコマンドの引数パターンとフラグも同様に登録
    }
    
    /// ASTを意味解析し、エラーや警告を検出
    pub fn analyze(&mut self, ast: &Node) -> Vec<ParserError> {
        self.errors.clear();
        self.variable_definitions.clear();
        self.variable_usages.clear();
        
        self.visit_node(ast);
        
        // 未定義変数の使用をチェック
        self.check_undefined_variables();
        
        // 未使用変数の警告
        self.check_unused_variables();
        
        self.errors.clone()
    }
    
    /// ノードを再帰的に訪問
    fn visit_node(&mut self, node: &Node) {
        match &node.kind {
            NodeKind::Command(cmd) => self.analyze_command(cmd, node.span),
            NodeKind::Pipeline(pipeline) => self.analyze_pipeline(pipeline, node.span),
            NodeKind::Redirection(redirection) => self.analyze_redirection(redirection, node.span),
            NodeKind::Assignment(name, value) => self.analyze_assignment(name, value, node.span),
            NodeKind::Conditional(condition, then_branch, else_branch) => {
                self.visit_node(condition);
                self.visit_node(then_branch);
                if let Some(else_node) = else_branch {
                    self.visit_node(else_node);
                }
            },
            NodeKind::Loop(condition, body) => {
                self.visit_node(condition);
                self.visit_node(body);
            },
            NodeKind::Block(statements) => {
                for stmt in statements {
                    self.visit_node(stmt);
                }
            },
            // その他のノード種類に対する処理
            _ => {},
        }
    }
    
    /// コマンドを分析
    fn analyze_command(&mut self, cmd: &Command, span: Span) {
        let command_name = &cmd.name;
        
        // コマンドの存在チェック
        if !self.available_commands.contains(command_name) {
            self.errors.push(ParserError::UnknownCommand {
                span,
                command: command_name.clone(),
            });
            return;
        }
        
        // 引数の数と型をチェック
        if let Some(patterns) = self.command_arg_patterns.get(command_name) {
            let mut pattern_matched = false;
            
            for pattern in patterns {
                if cmd.args.len() >= pattern.min_args && 
                   (pattern.max_args.is_none() || cmd.args.len() <= pattern.max_args.unwrap()) {
                    // 引数の制約をチェック
                    let mut constraint_violated = false;
                    
                    for (i, arg) in cmd.args.iter().enumerate() {
                        if i < pattern.arg_constraints.len() {
                            if !self.check_arg_constraint(&pattern.arg_constraints[i], arg) {
                                constraint_violated = true;
                                break;
                            }
                        }
                    }
                    
                    if !constraint_violated {
                        pattern_matched = true;
                        break;
                    }
                }
            }
            
            if !pattern_matched {
                self.errors.push(ParserError::InvalidCommandUsage {
                    span,
                    command: command_name.clone(),
                    message: format!("{}コマンドの引数が正しくありません", command_name),
                });
            }
        }
        
        // フラグをチェック
        if let Some(flags_info) = self.command_flags.get(command_name) {
            for flag in &cmd.flags {
                let flag_name = if flag.name.starts_with("--") {
                    flag.name[2..].to_string()
                } else if flag.name.starts_with('-') {
                    flag.name[1..].to_string()
                } else {
                    flag.name.clone()
                };
                
                let mut flag_found = false;
                
                for (_, flag_info) in flags_info {
                    if (flag_info.short_form.as_ref().map_or(false, |s| s == &flag.name)) ||
                       (flag_info.long_form.as_ref().map_or(false, |l| l == &flag.name)) {
                        flag_found = true;
                        
                        // フラグに引数が必要かチェック
                        if flag_info.requires_arg && flag.value.is_none() {
                            self.errors.push(ParserError::MissingFlagArgument {
                                span: flag.span,
                                flag: flag.name.clone(),
                            });
                        } else if !flag_info.requires_arg && flag.value.is_some() {
                            self.errors.push(ParserError::UnexpectedFlagArgument {
                                span: flag.span,
                                flag: flag.name.clone(),
                            });
                        }
                        
                        // フラグの引数の制約をチェック
                        if let (Some(constraint), Some(value)) = (&flag_info.arg_constraint, &flag.value) {
                            if !self.check_arg_constraint(constraint, value) {
                                self.errors.push(ParserError::InvalidFlagArgument {
                                    span: flag.span,
                                    flag: flag.name.clone(),
                                    message: format!("フラグ{}の引数が無効です", flag.name),
                                });
                            }
                        }
                        
                        break;
                    }
                }
                
                if !flag_found {
                    self.errors.push(ParserError::UnknownFlag {
                        span: flag.span,
                        flag: flag.name.clone(),
                        command: command_name.clone(),
                    });
                }
            }
        }
        
        // 変数の使用を追跡
        for arg in &cmd.args {
            self.check_variable_references(arg);
        }
        
        for flag in &cmd.flags {
            if let Some(value) = &flag.value {
                self.check_variable_references(value);
            }
        }
    }
    
    /// パイプラインを分析
    fn analyze_pipeline(&mut self, pipeline: &Pipeline, span: Span) {
        for cmd in &pipeline.commands {
            self.visit_node(cmd);
        }
        
        // パイプラインの最後のコマンドがリダイレクト出力を持つ場合の最適化提案
        if let Some(last_cmd) = pipeline.commands.last() {
            if let NodeKind::Command(cmd) = &last_cmd.kind {
                for redir in &cmd.redirections {
                    if matches!(redir.operator, TokenKind::RedirectOut | TokenKind::RedirectAppend) {
                        // 最適化提案のヒントを追加
                        self.errors.push(ParserError::OptimizationHint {
                            span: redir.span,
                            message: "パイプラインの最後のコマンドでリダイレクト出力を使用しています。パイプライン全体のリダイレクトを検討してください。".to_string(),
                        });
                    }
                }
            }
        }
    }
    
    /// リダイレクションを分析
    fn analyze_redirection(&mut self, redirection: &Redirection, span: Span) {
        // リダイレクト先のファイル名をチェック
        self.check_variable_references(&redirection.target);
        
        // リダイレクトの種類に応じた分析
        match redirection.operator {
            TokenKind::RedirectIn => {
                // 入力リダイレクトの場合、ファイルが存在するべき
                // 実際の環境では、ファイルシステムにアクセスして存在確認を行う
            },
            TokenKind::RedirectOut => {
                // 出力リダイレクトの場合、書き込み権限をチェック
            },
            TokenKind::RedirectAppend => {
                // 追記リダイレクトの場合、ファイルが存在して書き込み可能かチェック
            },
            TokenKind::RedirectErr => {
                // エラー出力リダイレクトの場合
            },
            _ => {
                self.errors.push(ParserError::InvalidRedirection {
                    span,
                    message: "無効なリダイレクト操作です".to_string(),
                });
            }
        }
    }
    
    /// 変数代入を分析
    fn analyze_assignment(&mut self, name: &str, value: &str, span: Span) {
        // 変数名の妥当性をチェック
        if !Self::is_valid_variable_name(name) {
            self.errors.push(ParserError::InvalidVariableName {
                span,
                name: name.to_string(),
            });
        }
        
        // 変数の定義を登録
        self.variable_definitions.insert(name.to_string(), span);
        
        // 値に含まれる変数参照をチェック
        self.check_variable_references(value);
    }
    
    /// 変数名が有効かどうかをチェック
    fn is_valid_variable_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        
        let first_char = name.chars().next().unwrap();
        if !first_char.is_alphabetic() && first_char != '_' {
            return false;
        }
        
        name.chars().all(|c| c.is_alphanumeric() || c == '_')
    }
    
    /// 文字列内の変数参照をチェック
    fn check_variable_references(&mut self, text: &str) {
        let mut pos = 0;
        
        while let Some(dollar_pos) = text[pos..].find('$') {
            let var_start = pos + dollar_pos;
            pos = var_start + 1;
            
            if pos >= text.len() {
                continue;
            }
            
            // ${var} 形式の変数
            if text.chars().nth(pos) == Some('{') {
                if let Some(end_brace) = text[pos..].find('}') {
                    let var_name = &text[pos+1..pos+end_brace];
                    let var_span = Span::new(var_start as u32, (pos + end_brace + 1) as u32);
                    
                    self.variable_usages
                        .entry(var_name.to_string())
                        .or_insert_with(Vec::new)
                        .push(var_span);
                    
                    pos += end_brace + 1;
                }
            } 
            // $var 形式の変数
            else if text.chars().nth(pos).map_or(false, |c| c.is_alphabetic() || c == '_') {
                let var_end = text[pos..].find(|c: char| !c.is_alphanumeric() && c != '_')
                    .map_or(text.len(), |i| pos + i);
                
                let var_name = &text[pos..var_end];
                let var_span = Span::new(var_start as u32, var_end as u32);
                
                self.variable_usages
                    .entry(var_name.to_string())
                    .or_insert_with(Vec::new)
                    .push(var_span);
                
                pos = var_end;
            }
        }
    }
    
    /// 未定義変数の使用をチェック
    fn check_undefined_variables(&mut self) {
        for (var_name, usages) in &self.variable_usages {
            if !self.variable_definitions.contains_key(var_name) && !Self::is_environment_variable(var_name) {
                for span in usages {
                    self.errors.push(ParserError::UndefinedVariable {
                        span: *span,
                        name: var_name.clone(),
                    });
                }
            }
        }
    }
    
    /// 環境変数かどうかをチェック（単純化のため一部の一般的な環境変数のみ）
    fn is_environment_variable(name: &str) -> bool {
        let common_env_vars = [
            "PATH", "HOME", "USER", "SHELL", "PWD", "OLDPWD", "TERM", "LANG",
            "LC_ALL", "DISPLAY", "EDITOR", "VISUAL", "PAGER", "TZ", "HOSTNAME",
        ];
        
        common_env_vars.contains(&name)
    }
    
    /// 未使用変数をチェック
    fn check_unused_variables(&mut self) {
        for (var_name, def_span) in &self.variable_definitions {
            if !self.variable_usages.contains_key(var_name) {
                self.errors.push(ParserError::UnusedVariable {
                    span: *def_span,
                    name: var_name.clone(),
                });
            }
        }
    }
    
    /// 引数が制約を満たすかチェック
    fn check_arg_constraint(&self, constraint: &ArgConstraint, arg: &str) -> bool {
        match constraint {
            ArgConstraint::Any => true,
            ArgConstraint::ExistingFile => {
                // 実際の環境ではファイルシステムにアクセスして確認
                // ここではシミュレーションとして一部の拡張子を持つものを有効とする
                arg.ends_with(".txt") || arg.ends_with(".log") || arg.ends_with(".rs")
            },
            ArgConstraint::ExistingDirectory => {
                // 実際の環境ではファイルシステムにアクセスして確認
                // ここではシミュレーションとして単純なパスパターンを有効とする
                arg == "." || arg == ".." || arg.starts_with("/") || !arg.contains(".")
            },
            ArgConstraint::Path => true, // すべての文字列をパスとして許可
            ArgConstraint::Number => arg.parse::<f64>().is_ok(),
            ArgConstraint::Enum(values) => values.contains(&arg.to_string()),
            ArgConstraint::Pattern(pattern) => {
                // 簡易的な実装として、単純な前方一致や後方一致をチェック
                if pattern.starts_with('*') {
                    arg.ends_with(&pattern[1..])
                } else if pattern.ends_with('*') {
                    arg.starts_with(&pattern[..pattern.len()-1])
                } else {
                    arg == pattern
                }
            },
        }
    }
    
    /// 現在の解析状態に基づいてコード補完候補を生成
    pub fn generate_completions(&self, partial_input: &str, position: usize) -> Vec<CompletionItem> {
        let mut completions = Vec::new();
        
        // コマンド名の補完
        if position == 0 || partial_input[..position].trim().is_empty() {
            for cmd in &self.available_commands {
                if cmd.starts_with(partial_input) {
                    completions.push(CompletionItem {
                        label: cmd.clone(),
                        kind: CompletionItemKind::Command,
                        detail: Some("コマンド".to_string()),
                        documentation: None,
                    });
                }
            }
            return completions;
        }
        
        // コマンドの引数やフラグの補完
        let words: Vec<&str> = partial_input[..position].split_whitespace().collect();
        if let Some(cmd_name) = words.first() {
            if self.available_commands.contains(&cmd_name.to_string()) {
                // フラグの補完
                if let Some(flags_info) = self.command_flags.get(&cmd_name.to_string()) {
                    let current_word = if position > 0 && partial_input.chars().nth(position - 1) != Some(' ') {
                        words.last().unwrap_or(&"")
                    } else {
                        ""
                    };
                    
                    if current_word.starts_with('-') {
                        for (_, flag_info) in flags_info {
                            if let Some(short) = &flag_info.short_form {
                                if short.starts_with(current_word) {
                                    completions.push(CompletionItem {
                                        label: short.clone(),
                                        kind: CompletionItemKind::Flag,
                                        detail: Some(flag_info.description.clone()),
                                        documentation: None,
                                    });
                                }
                            }
                            
                            if let Some(long) = &flag_info.long_form {
                                if long.starts_with(current_word) {
                                    completions.push(CompletionItem {
                                        label: long.clone(),
                                        kind: CompletionItemKind::Flag,
                                        detail: Some(flag_info.description.clone()),
                                        documentation: None,
                                    });
                                }
                            }
                        }
                    }
                }
                
                // 特定のコマンドに対する引数の補完
                match *cmd_name {
                    "cd" => {
                        // ディレクトリ補完（実際の環境ではファイルシステムから取得）
                        let dirs = ["home", "usr", "var", "etc", "opt"];
                        for dir in dirs.iter() {
                            completions.push(CompletionItem {
                                label: dir.to_string(),
                                kind: CompletionItemKind::Directory,
                                detail: Some("ディレクトリ".to_string()),
                                documentation: None,
                            });
                        }
                    },
                    "ls" => {
                        // ディレクトリとファイルの補完
                    },
                    // 他のコマンドに対する特殊な補完
                    _ => {},
                }
            }
        }
        
        completions
    }
}

/// コード補完アイテムを表す構造体
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// 表示ラベル
    pub label: String,
    
    /// 補完アイテムの種類
    pub kind: CompletionItemKind,
    
    /// 詳細情報
    pub detail: Option<String>,
    
    /// ドキュメント
    pub documentation: Option<String>,
}

/// 補完アイテムの種類を表すenum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionItemKind {
    Command,
    Flag,
    Argument,
    Variable,
    File,
    Directory,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::DefaultParser;
    
    #[test]
    fn test_semantic_analysis_unknown_command() {
        let input = "unknown_cmd arg1 arg2";
        let parser = DefaultParser::new();
        let ast = parser.parse(input).unwrap();
        
        let mut analyzer = SemanticAnalyzer::new();
        let errors = analyzer.analyze(&ast);
        
        assert!(!errors.is_empty());
        match &errors[0] {
            ParserError::UnknownCommand { command, .. } => {
                assert_eq!(command, "unknown_cmd");
            },
            _ => panic!("予期しないエラー種類: {:?}", errors[0]),
        }
    }
    
    #[test]
    fn test_semantic_analysis_valid_command() {
        let input = "cd /home";
        let parser = DefaultParser::new();
        let ast = parser.parse(input).unwrap();
        
        let mut analyzer = SemanticAnalyzer::new();
        let errors = analyzer.analyze(&ast);
        
        assert!(errors.is_empty());
    }
    
    #[test]
    fn test_semantic_analysis_invalid_command_usage() {
        let input = "cd /home /usr /var";  // cdは1つの引数しか受け付けない
        let parser = DefaultParser::new();
        let ast = parser.parse(input).unwrap();
        
        let mut analyzer = SemanticAnalyzer::new();
        let errors = analyzer.analyze(&ast);
        
        assert!(!errors.is_empty());
        match &errors[0] {
            ParserError::InvalidCommandUsage { command, .. } => {
                assert_eq!(command, "cd");
            },
            _ => panic!("予期しないエラー種類: {:?}", errors[0]),
        }
    }
    
    #[test]
    fn test_undefined_variable() {
        let input = "echo $UNDEFINED_VAR";
        let parser = DefaultParser::new();
        let ast = parser.parse(input).unwrap();
        
        let mut analyzer = SemanticAnalyzer::new();
        let errors = analyzer.analyze(&ast);
        
        assert!(!errors.is_empty());
        match &errors[0] {
            ParserError::UndefinedVariable { name, .. } => {
                assert_eq!(name, "UNDEFINED_VAR");
            },
            _ => panic!("予期しないエラー種類: {:?}", errors[0]),
        }
    }
    
    #[test]
    fn test_unused_variable() {
        let input = "VAR=value\necho hello";
        let parser = DefaultParser::new();
        let ast = parser.parse(input).unwrap();
        
        let mut analyzer = SemanticAnalyzer::new();
        let errors = analyzer.analyze(&ast);
        
        assert!(!errors.is_empty());
        match &errors[0] {
            ParserError::UnusedVariable { name, .. } => {
                assert_eq!(name, "VAR");
            },
            _ => panic!("予期しないエラー種類: {:?}", errors[0]),
        }
    }
} 

/// 高度なデータフロー解析
#[derive(Debug)]
pub struct DataFlowAnalyzer {
    /// 現在の解析中のコンテキスト
    current_context: Option<Arc<SemanticContext>>,
    /// シンボルテーブル
    symbol_table: Arc<RwLock<SymbolTable>>,
    /// 変数定義マップ (変数名 -> 定義ノード)
    definitions: HashMap<String, Arc<AstNode>>,
    /// 変数使用マップ (変数名 -> 使用ノードリスト)
    uses: HashMap<String, Vec<Arc<AstNode>>>,
    /// ノード間のデータフロー関係 (ノードID -> 依存ノードIDリスト)
    flow_edges: HashMap<usize, Vec<usize>>,
    /// ノードIDカウンター
    node_id_counter: usize,
    /// ノードIDマップ (AstNode -> ID)
    node_id_map: HashMap<usize, usize>,
}

impl DataFlowAnalyzer {
    /// 新しいデータフロー解析器を作成
    pub fn new(symbol_table: Arc<RwLock<SymbolTable>>) -> Self {
        Self {
            current_context: None,
            symbol_table,
            definitions: HashMap::new(),
            uses: HashMap::new(),
            flow_edges: HashMap::new(),
            node_id_counter: 0,
            node_id_map: HashMap::new(),
        }
    }
    
    /// ノードにIDを割り当て
    fn assign_node_id(&mut self, node: &AstNode) -> usize {
        let node_ptr = node as *const AstNode as usize;
        
        if let Some(id) = self.node_id_map.get(&node_ptr) {
            return *id;
        }
        
        let id = self.node_id_counter;
        self.node_id_counter += 1;
        self.node_id_map.insert(node_ptr, id);
        id
    }
    
    /// データフロー解析を実行
    pub fn analyze(&mut self, node: &AstNode) -> Result<()> {
        // 変数定義と使用を収集
        self.collect_definitions_and_uses(node)?;
        
        // データフロー関係を構築
        self.build_flow_graph()?;
        
        Ok(())
    }
    
    /// 変数定義と使用を収集
    fn collect_definitions_and_uses(&mut self, node: &AstNode) -> Result<()> {
        match node {
            AstNode::VariableAssignment { name, value, export, span } => {
                let node_id = self.assign_node_id(node);
                let value_id = self.assign_node_id(value);
                
                // フローエッジを追加（値から代入へ）
                self.add_flow_edge(value_id, node_id);
                
                // 定義を記録
                self.definitions.insert(name.clone(), Arc::new(node.clone()));
                
                // シンボルテーブルに変数を追加
                let kind = if *export {
                    SymbolKind::ExportedVariable
                } else {
                    SymbolKind::LocalVariable
                };
                
                let mut symbol_table = self.symbol_table.write().unwrap();
                let symbol = SymbolInfo::new(
                    name,
                    kind,
                    ShellType::Unknown, // 型推論は後で行う
                    *span,
                    &symbol_table.scope_id,
                ).mark_initialized();
                
                symbol_table.define(symbol)?;
                
                // 値のノードを再帰的に処理
                self.collect_definitions_and_uses(value)?;
            },
            
            AstNode::VariableReference { name, default_value, span } => {
                let node_id = self.assign_node_id(node);
                
                // 使用を記録
                self.uses.entry(name.clone())
                    .or_insert_with(Vec::new)
                    .push(Arc::new(node.clone()));
                
                // シンボルテーブルで参照をマーク
                let mut symbol_table = self.symbol_table.write().unwrap();
                symbol_table.reference(name, *span)?;
                
                // デフォルト値があれば処理
                if let Some(default) = default_value {
                    let default_id = self.assign_node_id(default);
                    self.add_flow_edge(default_id, node_id);
                    self.collect_definitions_and_uses(default)?;
                }
            },
            
            AstNode::Command { name, arguments, redirections, span } => {
                let node_id = self.assign_node_id(node);
                
                // 引数を処理
                for arg in arguments {
                    let arg_id = self.assign_node_id(arg);
                    self.add_flow_edge(arg_id, node_id);
                    self.collect_definitions_and_uses(arg)?;
                }
                
                // リダイレクションを処理
                for redir in redirections {
                    let redir_id = self.assign_node_id(redir);
                    self.add_flow_edge(redir_id, node_id);
                    self.collect_definitions_and_uses(redir)?;
                }
            },
            
            AstNode::Pipeline { commands, kind, span } => {
                let node_id = self.assign_node_id(node);
                
                // パイプラインのコマンドを処理（順序が重要）
                for cmd in commands {
                    let cmd_id = self.assign_node_id(cmd);
                    self.add_flow_edge(cmd_id, node_id);
                    self.collect_definitions_and_uses(cmd)?;
                }
            },
            
            // 他のノード型も同様に処理
            // ...
            
            _ => {
                // 子ノードを持つ可能性のある他のノード型を再帰的に処理
                for child in node.children() {
                    self.collect_definitions_and_uses(child)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// フローエッジを追加
    fn add_flow_edge(&mut self, from: usize, to: usize) {
        self.flow_edges.entry(from)
            .or_insert_with(Vec::new)
            .push(to);
    }
    
    /// データフローグラフを構築
    fn build_flow_graph(&mut self) -> Result<()> {
        // 変数の定義から使用へのエッジを追加
        for (var_name, def_node) in &self.definitions {
            let def_id = self.node_id_map[&(Arc::as_ptr(def_node) as usize)];
            
            if let Some(uses) = self.uses.get(var_name) {
                for use_node in uses {
                    let use_id = self.node_id_map[&(Arc::as_ptr(use_node) as usize)];
                    self.add_flow_edge(def_id, use_id);
                }
            }
        }
        
        Ok(())
    }
    
    /// 到達可能な定義を検索
    pub fn find_reaching_definitions(&self, node: &AstNode) -> Vec<Arc<AstNode>> {
        let node_ptr = node as *const AstNode as usize;
        
        if let Some(node_id) = self.node_id_map.get(&node_ptr) {
            let mut result = Vec::new();
            let mut visited = HashSet::new();
            self.dfs_reaching_definitions(*node_id, &mut result, &mut visited);
            result
        } else {
            Vec::new()
        }
    }
    
    /// 到達可能な定義のDFS探索
    fn dfs_reaching_definitions(&self, node_id: usize, result: &mut Vec<Arc<AstNode>>, visited: &mut HashSet<usize>) {
        if visited.contains(&node_id) {
            return;
        }
        
        visited.insert(node_id);
        
        // このノードが変数定義なら結果に追加
        for (var_name, def_node) in &self.definitions {
            let def_id = self.node_id_map[&(Arc::as_ptr(def_node) as usize)];
            if def_id == node_id {
                result.push(def_node.clone());
                break;
            }
        }
        
        // 前任者を探索
        for (pred, succs) in &self.flow_edges {
            if succs.contains(&node_id) {
                self.dfs_reaching_definitions(*pred, result, visited);
            }
        }
    }
    
    /// 未使用の変数を検出
    pub fn find_unused_variables(&self) -> Vec<String> {
        let mut unused = Vec::new();
        
        for (var_name, def_node) in &self.definitions {
            if !self.uses.contains_key(var_name) {
                // 変数が一度も使用されていない
                unused.push(var_name.clone());
            }
        }
        
        unused
    }
    
    /// ライブ変数解析
    pub fn analyze_live_variables(&self) -> HashMap<usize, HashSet<String>> {
        let mut live_out = HashMap::new();
        let mut changed = true;
        
        // 初期化
        for node_id in self.node_id_map.values() {
            live_out.insert(*node_id, HashSet::new());
        }
        
        // 不動点に達するまで繰り返し
        while changed {
            changed = false;
            
            for (node_id, _) in &self.node_id_map {
                let node_id = *node_id;
                let old_live_out = live_out.get(&node_id).unwrap().clone();
                
                // 後続ノードのライブ変数を集める
                let mut new_live_out = HashSet::new();
                if let Some(successors) = self.flow_edges.get(&node_id) {
                    for succ in successors {
                        let succ_live_in = self.compute_live_in(*succ, &live_out);
                        new_live_out.extend(succ_live_in);
                    }
                }
                
                if new_live_out != old_live_out {
                    live_out.insert(node_id, new_live_out);
                    changed = true;
                }
            }
        }
        
        live_out
    }
    
    /// ノードのライブイン変数を計算
    fn compute_live_in(&self, node_id: usize, live_out: &HashMap<usize, HashSet<String>>) -> HashSet<String> {
        let mut live_in = live_out.get(&node_id).cloned().unwrap_or_default();
        
        // このノードで定義された変数を削除
        for (var_name, def_node) in &self.definitions {
            let def_id = self.node_id_map[&(Arc::as_ptr(def_node) as usize)];
            if def_id == node_id {
                live_in.remove(var_name);
                break;
            }
        }
        
        // このノードで使用された変数を追加
        for (var_name, use_nodes) in &self.uses {
            for use_node in use_nodes {
                let use_id = self.node_id_map[&(Arc::as_ptr(use_node) as usize)];
                if use_id == node_id {
                    live_in.insert(var_name.clone());
                    break;
                }
            }
        }
        
        live_in
    }
}

/// 型推論エンジン
#[derive(Debug)]
pub struct TypeInferenceEngine {
    /// シンボルテーブル
    symbol_table: Arc<RwLock<SymbolTable>>,
    /// 型制約グラフ
    constraints: Vec<TypeConstraint>,
    /// ノードの型マップ (ノードID -> 型)
    node_types: HashMap<usize, ShellType>,
    /// ノードIDカウンター
    node_id_counter: usize,
    /// ノードIDマップ (AstNode -> ID)
    node_id_map: HashMap<usize, usize>,
    /// コマンドの戻り値型マップ
    command_return_types: HashMap<String, ShellType>,
    /// 型定義マップ
    type_definitions: HashMap<String, ShellType>,
}

/// 型制約
#[derive(Debug, Clone)]
pub enum TypeConstraint {
    /// 2つの型が等しい必要がある
    Equals(usize, usize),
    /// 左の型は右の型のサブタイプである必要がある
    Subtype(usize, usize),
    /// ノードの型を直接指定
    Direct(usize, ShellType),
    /// 型変換が必要
    Convert(usize, usize, ShellType),
}

impl TypeInferenceEngine {
    /// 新しい型推論エンジンを作成
    pub fn new(symbol_table: Arc<RwLock<SymbolTable>>) -> Self {
        let mut engine = Self {
            symbol_table,
            constraints: Vec::new(),
            node_types: HashMap::new(),
            node_id_counter: 0,
            node_id_map: HashMap::new(),
            command_return_types: HashMap::new(),
            type_definitions: HashMap::new(),
        };
        
        // 標準コマンドの戻り値型を登録
        engine.register_builtin_command_types();
        
        engine
    }
    
    /// 組み込みコマンドの型を登録
    fn register_builtin_command_types(&mut self) {
        // 一般的なコマンドの戻り値型
        self.command_return_types.insert("echo".to_string(), ShellType::Integer);
        self.command_return_types.insert("cd".to_string(), ShellType::Integer);
        self.command_return_types.insert("ls".to_string(), ShellType::Integer);
        self.command_return_types.insert("grep".to_string(), ShellType::Integer);
        self.command_return_types.insert("find".to_string(), ShellType::Integer);
        
        // ストリームを返すコマンド
        self.command_return_types.insert("cat".to_string(), ShellType::Stream(Box::new(ShellType::String)));
        self.command_return_types.insert("head".to_string(), ShellType::Stream(Box::new(ShellType::String)));
        self.command_return_types.insert("tail".to_string(), ShellType::Stream(Box::new(ShellType::String)));
        
        // 特殊な戻り値型
        self.command_return_types.insert("date".to_string(), ShellType::DateTime);
        self.command_return_types.insert("wc".to_string(), ShellType::Array(Box::new(ShellType::Integer)));
        self.command_return_types.insert("du".to_string(), ShellType::Array(Box::new(ShellType::Integer)));
    }
    
    /// ノードにIDを割り当て
    fn assign_node_id(&mut self, node: &AstNode) -> usize {
        let node_ptr = node as *const AstNode as usize;
        
        if let Some(id) = self.node_id_map.get(&node_ptr) {
            return *id;
        }
        
        let id = self.node_id_counter;
        self.node_id_counter += 1;
        self.node_id_map.insert(node_ptr, id);
        id
    }
    
    /// 型推論を実行
    pub fn infer(&mut self, node: &AstNode) -> Result<()> {
        // 制約を収集
        self.collect_constraints(node)?;
        
        // 制約を解決
        self.solve_constraints()?;
        
        // 型情報をシンボルテーブルに反映
        self.update_symbol_table()?;
        
        Ok(())
    }
    
    /// 制約を収集
    fn collect_constraints(&mut self, node: &AstNode) -> Result<usize> {
        let node_id = self.assign_node_id(node);
        
        match node {
            AstNode::Command { name, arguments, redirections, span } => {
                // コマンド自体の型
                self.add_direct_constraint(node_id, ShellType::Command);
                
                // 引数の制約を収集
                let mut arg_ids = Vec::new();
                for arg in arguments {
                    let arg_id = self.collect_constraints(arg)?;
                    arg_ids.push(arg_id);
                }
                
                // コマンドに応じた引数の型チェック
                if let Some(return_type) = self.command_return_types.get(name) {
                    // コマンドの戻り値型を設定
                    self.add_direct_constraint(node_id, return_type.clone());
                    
                    // TODO: コマンド固有の引数型チェック
                }
                
                // リダイレクションの制約を収集
                for redir in redirections {
                    self.collect_constraints(redir)?;
                }
            },
            
            AstNode::Argument { value, span } => {
                // 引数はデフォルトで文字列型
                self.add_direct_constraint(node_id, ShellType::String);
                
                // 数値や真偽値のリテラルの場合は型を推測
                if let Ok(int_val) = value.parse::<i64>() {
                    self.add_direct_constraint(node_id, ShellType::Integer);
                } else if let Ok(float_val) = value.parse::<f64>() {
                    self.add_direct_constraint(node_id, ShellType::Float);
                } else if value == "true" || value == "false" {
                    self.add_direct_constraint(node_id, ShellType::Boolean);
                } else if value.starts_with('/') || value.contains('/') {
                    // パスっぽい
                    self.add_direct_constraint(node_id, ShellType::Path);
                }
            },
            
            AstNode::VariableAssignment { name, value, export, span } => {
                // 値の型制約を収集
                let value_id = self.collect_constraints(value)?;
                
                // 変数の型は値の型と等しい
                self.add_equals_constraint(node_id, value_id);
            },
            
            AstNode::VariableReference { name, default_value, span } => {
                // 変数の型を参照
                let symbol_table = self.symbol_table.read().unwrap();
                if let Some(symbol) = symbol_table.lookup(name) {
                    if symbol.shell_type != ShellType::Unknown {
                        self.add_direct_constraint(node_id, symbol.shell_type.clone());
                    }
                }
                
                // デフォルト値がある場合
                if let Some(default) = default_value {
                    let default_id = self.collect_constraints(default)?;
                    
                    // デフォルト値の型は変数の型と互換性があるべき
                    self.add_subtype_constraint(default_id, node_id);
                }
            },
            
            AstNode::Pipeline { commands, kind, span } => {
                // パイプラインの最後のコマンドの戻り値型がパイプライン全体の型
                if !commands.is_empty() {
                    let last_cmd_id = self.collect_constraints(&commands[commands.len() - 1])?;
                    self.add_equals_constraint(node_id, last_cmd_id);
                }
                
                // 他のコマンドも処理
                for cmd in &commands[0..commands.len().saturating_sub(1)] {
                    self.collect_constraints(cmd)?;
                }
            },
            
            // 他のノード型についても同様に処理
            // ...
            
            _ => {
                // 子ノードを持つ可能性のある他のノード型を再帰的に処理
                for child in node.children() {
                    self.collect_constraints(child)?;
                }
                
                // デフォルトの型はAny
                self.add_direct_constraint(node_id, ShellType::Any);
            }
        }
        
        Ok(node_id)
    }
    
    /// 等価制約を追加
    fn add_equals_constraint(&mut self, node1_id: usize, node2_id: usize) {
        self.constraints.push(TypeConstraint::Equals(node1_id, node2_id));
    }
    
    /// サブタイプ制約を追加
    fn add_subtype_constraint(&mut self, subtype_id: usize, supertype_id: usize) {
        self.constraints.push(TypeConstraint::Subtype(subtype_id, supertype_id));
    }
    
    /// 直接型制約を追加
    fn add_direct_constraint(&mut self, node_id: usize, shell_type: ShellType) {
        self.constraints.push(TypeConstraint::Direct(node_id, shell_type));
    }
    
    /// 変換制約を追加
    fn add_convert_constraint(&mut self, from_id: usize, to_id: usize, target_type: ShellType) {
        self.constraints.push(TypeConstraint::Convert(from_id, to_id, target_type));
    }
    
    /// 制約を解決
    fn solve_constraints(&mut self) -> Result<()> {
        // 制約処理の最大反復回数
        const MAX_ITERATIONS: usize = 100;
        
        // 各ノードに初期型（Unknown）を割り当て
        for node_id in self.node_id_map.values() {
            self.node_types.insert(*node_id, ShellType::Unknown);
        }
        
        // 解決済み制約を記録
        let mut resolved = HashSet::new();
        let mut changed = true;
        let mut iteration = 0;
        
        // 制約が解決されなくなるか、最大反復回数に達するまで繰り返す
        while changed && iteration < MAX_ITERATIONS {
            changed = false;
            iteration += 1;
            
            for (i, constraint) in self.constraints.iter().enumerate() {
                if resolved.contains(&i) {
                    continue;
                }
                
                match constraint {
                    TypeConstraint::Direct(node_id, shell_type) => {
                        let current_type = self.node_types.get_mut(node_id).unwrap();
                        if *current_type == ShellType::Unknown {
                            *current_type = shell_type.clone();
                            changed = true;
                        } else {
                            // 既存の型と新しい型を統合
                            let combined_type = current_type.union_with(shell_type);
                            if *current_type != combined_type {
                                *current_type = combined_type;
                                changed = true;
                            }
                        }
                        resolved.insert(i);
                    },
                    
                    TypeConstraint::Equals(node1_id, node2_id) => {
                        let type1 = self.node_types.get(node1_id).unwrap().clone();
                        let type2 = self.node_types.get(node2_id).unwrap().clone();
                        
                        if type1 == ShellType::Unknown && type2 != ShellType::Unknown {
                            self.node_types.insert(*node1_id, type2);
                            changed = true;
                            resolved.insert(i);
                        } else if type2 == ShellType::Unknown && type1 != ShellType::Unknown {
                            self.node_types.insert(*node2_id, type1);
                            changed = true;
                            resolved.insert(i);
                        } else if type1 != ShellType::Unknown && type2 != ShellType::Unknown {
                            if type1.is_compatible_with(&type2) {
                                // 両方の型を一般化したものを使用
                                let generalized = type1.generalize(&type2);
                                self.node_types.insert(*node1_id, generalized.clone());
                                self.node_types.insert(*node2_id, generalized);
                                changed = true;
                            } else {
                                return Err(ParserError::SemanticError(
                                    format!("型の不一致: {} と {}", type1, type2),
                                    Span::default(), // TODO: 実際のスパンを取得
                                ));
                            }
                            resolved.insert(i);
                        }
                    },
                    
                    TypeConstraint::Subtype(sub_id, super_id) => {
                        let subtype = self.node_types.get(sub_id).unwrap().clone();
                        let supertype = self.node_types.get(super_id).unwrap().clone();
                        
                        if subtype != ShellType::Unknown && supertype != ShellType::Unknown {
                            if !subtype.is_compatible_with(&supertype) {
                                return Err(ParserError::SemanticError(
                                    format!("サブタイプ制約違反: {} は {} のサブタイプではありません", subtype, supertype),
                                    Span::default(), // TODO: 実際のスパンを取得
                                ));
                            }
                            resolved.insert(i);
                        }
                    },
                    
                    TypeConstraint::Convert(from_id, to_id, target_type) => {
                        let from_type = self.node_types.get(from_id).unwrap().clone();
                        
                        if from_type != ShellType::Unknown {
                            if !from_type.can_convert_to(target_type) {
                                return Err(ParserError::SemanticError(
                                    format!("型変換エラー: {} から {} への変換はサポートされていません", from_type, target_type),
                                    Span::default(), // TODO: 実際のスパンを取得
                                ));
                            }
                            self.node_types.insert(*to_id, target_type.clone());
                            changed = true;
                            resolved.insert(i);
                        }
                    },
                }
            }
        }
        
        // 未解決の制約が残っているかどうかをチェック
        let unresolved = self.constraints.len() - resolved.len();
        if unresolved > 0 {
            println!("警告: {}個の未解決の型制約が残っています", unresolved);
        }
        
        // 最後のパスで未知の型を具体化
        for (_, shell_type) in self.node_types.iter_mut() {
            if *shell_type == ShellType::Unknown {
                *shell_type = ShellType::Any;
            }
        }
        
        Ok(())
    }
    
    /// 型情報をシンボルテーブルに反映
    fn update_symbol_table(&self) -> Result<()> {
        let mut symbol_table = self.symbol_table.write().unwrap();
        
        for (node_ptr, node_id) in &self.node_id_map {
            // ノードポインタからAstNodeを逆引き
            // 注意: これは単純化のためのコード。実際はより安全な方法が必要
            let node_ptr = *node_ptr as *const AstNode;
            let node = unsafe { &*node_ptr };
            
            if let AstNode::VariableAssignment { name, .. } = node {
                if let Some(symbol) = symbol_table.lookup_local(name) {
                    let mut updated_symbol = symbol.clone();
                    if let Some(shell_type) = self.node_types.get(node_id) {
                        updated_symbol.shell_type = shell_type.clone();
                    }
                    symbol_table.update(updated_symbol);
                }
            }
        }
        
        Ok(())
    }
    
    /// ノードの型を取得
    pub fn get_node_type(&self, node: &AstNode) -> Option<ShellType> {
        let node_ptr = node as *const AstNode as usize;
        
        if let Some(node_id) = self.node_id_map.get(&node_ptr) {
            self.node_types.get(node_id).cloned()
        } else {
            None
        }
    }
}

// ... existing code ...