use crate::{
    AstNode, Error, Result, Span, TokenKind, ParserContext, ParserError,
    RedirectionKind, PipelineKind
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::fmt;
use std::sync::Arc;

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
}

/// セマンティック解析器のトレイト
pub trait Analyzer {
    /// ASTノードの意味解析を実行
    fn analyze(&mut self, node: &AstNode) -> Result<AstNode>;
    
    /// 指定したステージのみを実行
    fn analyze_stage(&mut self, node: &AstNode, stage: SemanticStage) -> Result<AstNode>;
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
    /// 定義位置
    defined_at: Span,
    /// 参照位置
    references: Vec<Span>,
    /// スコープ
    scope: String,
    /// アトリビュート（追加情報）
    attributes: HashMap<String, String>,
}

/// シンボルの種類
#[derive(Debug, Clone, PartialEq, Eq)]
enum SymbolKind {
    /// 変数
    Variable,
    /// 関数
    Function,
    /// エイリアス
    Alias,
    /// 引数
    Argument,
    /// 環境変数
    Environment,
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