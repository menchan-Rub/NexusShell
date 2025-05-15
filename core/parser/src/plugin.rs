// plugin.rs
// NexusShellのパーサープラグインシステム
// 外部プラグインによるパーサー機能の拡張が可能

use crate::{
    AstNode, Token, TokenKind, Span, ParserContext, ParserError, Result,
    parser::RecursiveDescentParser,
    lexer::NexusLexer,
    grammar::GrammarManager,
    completer::{CompletionContext, CompletionSuggestion, CompletionKind, CompletionResult},
    predictor::{PredictionResult, PredictionKind}
};

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, Mutex};
use std::path::{Path, PathBuf};
use std::any::Any;
use log::{debug, trace, info, warn, error};
use async_trait::async_trait;

/// プラグインの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginType {
    /// 字句解析拡張
    Lexer,
    /// 構文解析拡張
    Parser,
    /// 文法拡張
    Grammar,
    /// 補完拡張
    Completion,
    /// 予測拡張
    Prediction,
    /// セマンティクス拡張
    Semantics,
    /// 変換拡張（AST変換など）
    Transform,
}

impl std::fmt::Display for PluginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginType::Lexer => write!(f, "Lexer"),
            PluginType::Parser => write!(f, "Parser"),
            PluginType::Grammar => write!(f, "Grammar"),
            PluginType::Completion => write!(f, "Completion"),
            PluginType::Prediction => write!(f, "Prediction"),
            PluginType::Semantics => write!(f, "Semantics"),
            PluginType::Transform => write!(f, "Transform"),
        }
    }
}

/// プラグインのメタデータ
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// プラグインID
    pub id: String,
    /// プラグインの名前
    pub name: String,
    /// プラグインのバージョン
    pub version: String,
    /// プラグインの説明
    pub description: String,
    /// プラグインの作者
    pub author: String,
    /// プラグインの種類
    pub plugin_type: PluginType,
    /// プラグインの依存関係
    pub dependencies: Vec<String>,
    /// プラグインの設定スキーマ
    pub config_schema: Option<String>,
    /// ライセンス
    pub license: String,
    /// 追加のメタデータ
    pub extra: HashMap<String, String>,
}

impl PluginMetadata {
    /// 新しいプラグインメタデータを作成
    pub fn new(
        id: &str,
        name: &str,
        version: &str,
        description: &str,
        author: &str,
        plugin_type: PluginType,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            author: author.to_string(),
            plugin_type,
            dependencies: Vec::new(),
            config_schema: None,
            license: "MIT".to_string(),
            extra: HashMap::new(),
        }
    }

    /// 依存関係を追加
    pub fn with_dependency(mut self, dependency_id: &str) -> Self {
        self.dependencies.push(dependency_id.to_string());
        self
    }

    /// 設定スキーマを追加
    pub fn with_config_schema(mut self, schema: &str) -> Self {
        self.config_schema = Some(schema.to_string());
        self
    }

    /// ライセンスを設定
    pub fn with_license(mut self, license: &str) -> Self {
        self.license = license.to_string();
        self
    }

    /// 追加のメタデータを設定
    pub fn with_extra(mut self, key: &str, value: &str) -> Self {
        self.extra.insert(key.to_string(), value.to_string());
        self
    }
}

/// プラグイン設定
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// プラグインID
    pub plugin_id: String,
    /// 有効/無効
    pub enabled: bool,
    /// 優先度（低いほど先に実行）
    pub priority: i32,
    /// 設定値
    pub settings: HashMap<String, serde_json::Value>,
}

impl PluginConfig {
    /// 新しいプラグイン設定を作成
    pub fn new(plugin_id: &str) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            enabled: true,
            priority: 100,
            settings: HashMap::new(),
        }
    }

    /// 設定の有効/無効を設定
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 優先度を設定
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// 設定値を追加
    pub fn with_setting<T: serde::Serialize>(mut self, key: &str, value: T) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.settings.insert(key.to_string(), json_value);
        }
        self
    }

    /// 設定値を取得
    pub fn get_setting<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.settings.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 文字列設定値を取得
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.settings.get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// 数値設定値を取得
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.settings.get(key)
            .and_then(|v| v.as_f64())
    }

    /// 真偽値設定値を取得
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.settings.get(key)
            .and_then(|v| v.as_bool())
    }
}

/// プラグインコンテキスト
#[derive(Debug)]
pub struct PluginContext {
    /// プラグインID
    pub plugin_id: String,
    /// プラグイン設定
    pub config: PluginConfig,
    /// 共有データ（プラグイン間）
    pub shared_data: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
    /// ログ機能
    pub logger: Arc<dyn PluginLogger>,
    /// 文法マネージャー参照
    pub grammar_manager: Arc<GrammarManager>,
}

impl PluginContext {
    /// 新しいプラグインコンテキストを作成
    pub fn new(
        plugin_id: &str,
        config: PluginConfig,
        shared_data: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
        logger: Arc<dyn PluginLogger>,
        grammar_manager: Arc<GrammarManager>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            config,
            shared_data,
            logger,
            grammar_manager,
        }
    }

    /// 共有データを設定
    pub fn set_shared_data<T: 'static + Send + Sync>(&self, key: &str, value: T) -> Result<()> {
        let mut data = self.shared_data.write().map_err(|_| {
            ParserError::PluginError(format!("共有データへのアクセスに失敗しました: {}", key))
        })?;
        
        data.insert(key.to_string(), Box::new(value));
        Ok(())
    }

    /// 共有データを取得
    pub fn get_shared_data<T: 'static + Clone>(&self, key: &str) -> Option<T> {
        if let Ok(data) = self.shared_data.read() {
            if let Some(value) = data.get(key) {
                if let Some(typed_value) = value.downcast_ref::<T>() {
                    return Some(typed_value.clone());
                }
            }
        }
        None
    }

    /// デバッグログを出力
    pub fn debug(&self, message: &str) {
        self.logger.debug(&self.plugin_id, message);
    }

    /// 情報ログを出力
    pub fn info(&self, message: &str) {
        self.logger.info(&self.plugin_id, message);
    }

    /// 警告ログを出力
    pub fn warn(&self, message: &str) {
        self.logger.warn(&self.plugin_id, message);
    }

    /// エラーログを出力
    pub fn error(&self, message: &str) {
        self.logger.error(&self.plugin_id, message);
    }
}

/// プラグインロガーインターフェース
#[async_trait]
pub trait PluginLogger: Send + Sync {
    /// デバッグログを出力
    fn debug(&self, plugin_id: &str, message: &str);
    
    /// 情報ログを出力
    fn info(&self, plugin_id: &str, message: &str);
    
    /// 警告ログを出力
    fn warn(&self, plugin_id: &str, message: &str);
    
    /// エラーログを出力
    fn error(&self, plugin_id: &str, message: &str);
}

/// 字句解析プラグインインターフェース
#[async_trait]
pub trait LexerPlugin: Send + Sync {
    /// プラグインのメタデータを取得
    fn metadata(&self) -> PluginMetadata;
    
    /// プラグインを初期化
    async fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    
    /// カスタムトークンを処理
    async fn process_token(&self, input: &str, position: usize) -> Option<(TokenKind, String, usize)>;
    
    /// トークン列を後処理
    async fn post_process(&self, tokens: Vec<Token>) -> Vec<Token>;
    
    /// プラグインをクリーンアップ
    async fn cleanup(&mut self) -> Result<()>;
}

/// 構文解析プラグインインターフェース
#[async_trait]
pub trait ParserPlugin: Send + Sync {
    /// プラグインのメタデータを取得
    fn metadata(&self) -> PluginMetadata;
    
    /// プラグインを初期化
    async fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    
    /// カスタムノードを解析
    async fn parse_node(&self, parser: &mut RecursiveDescentParser, context: &mut ParserContext) -> Option<Result<AstNode>>;
    
    /// AST全体を後処理
    async fn post_process(&self, ast: AstNode) -> AstNode;
    
    /// プラグインをクリーンアップ
    async fn cleanup(&mut self) -> Result<()>;
}

/// 文法拡張プラグインインターフェース
#[async_trait]
pub trait GrammarPlugin: Send + Sync {
    /// プラグインのメタデータを取得
    fn metadata(&self) -> PluginMetadata;
    
    /// プラグインを初期化
    async fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    
    /// カスタム文法ルールを提供
    async fn provide_grammar_rules(&self) -> Vec<String>;
    
    /// カスタム文法シンボルを登録
    async fn register_symbols(&self, manager: &mut GrammarManager) -> Result<()>;
    
    /// プラグインをクリーンアップ
    async fn cleanup(&mut self) -> Result<()>;
}

/// 補完プラグインインターフェース
#[async_trait]
pub trait CompletionPlugin: Send + Sync {
    /// プラグインのメタデータを取得
    fn metadata(&self) -> PluginMetadata;
    
    /// プラグインを初期化
    async fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    
    /// この補完プラグインが対応可能か判定
    async fn can_complete(&self, context: &CompletionContext, word: &str) -> bool;
    
    /// 補完候補を生成
    async fn generate_completions(&self, context: &CompletionContext, word: &str) -> Vec<CompletionSuggestion>;
    
    /// プラグインをクリーンアップ
    async fn cleanup(&mut self) -> Result<()>;
}

/// 予測プラグインインターフェース
#[async_trait]
pub trait PredictionPlugin: Send + Sync {
    /// プラグインのメタデータを取得
    fn metadata(&self) -> PluginMetadata;
    
    /// プラグインを初期化
    async fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    
    /// この予測プラグインが対応可能か判定
    async fn can_predict(&self, context: &CompletionContext) -> bool;
    
    /// 予測を生成
    async fn generate_predictions(&self, context: &CompletionContext) -> Vec<PredictionResult>;
    
    /// プラグインをクリーンアップ
    async fn cleanup(&mut self) -> Result<()>;
}

/// 意味解析プラグインインターフェース
#[async_trait]
pub trait SemanticsPlugin: Send + Sync {
    /// プラグインのメタデータを取得
    fn metadata(&self) -> PluginMetadata;
    
    /// プラグインを初期化
    async fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    
    /// ASTを意味解析
    async fn analyze(&self, ast: &AstNode) -> Result<Vec<ParserError>>;
    
    /// プラグインをクリーンアップ
    async fn cleanup(&mut self) -> Result<()>;
}

/// プラグインローダー
#[derive(Debug)]
pub struct PluginLoader {
    /// プラグイン検索パス
    search_paths: Vec<PathBuf>,
    /// ロードされたプラグイン
    loaded_plugins: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
    /// プラグイン設定
    plugin_configs: Arc<RwLock<HashMap<String, PluginConfig>>>,
    /// プラグインメタデータ
    plugin_metadata: Arc<RwLock<HashMap<String, PluginMetadata>>>,
    /// 共有データ
    shared_data: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
    /// プラグインロガー
    logger: Arc<dyn PluginLogger>,
    /// 文法マネージャー
    grammar_manager: Arc<GrammarManager>,
}

impl PluginLoader {
    /// 新しいプラグインローダーを作成
    pub fn new(logger: Arc<dyn PluginLogger>, grammar_manager: Arc<GrammarManager>) -> Self {
        Self {
            search_paths: Vec::new(),
            loaded_plugins: Arc::new(RwLock::new(HashMap::new())),
            plugin_configs: Arc::new(RwLock::new(HashMap::new())),
            plugin_metadata: Arc::new(RwLock::new(HashMap::new())),
            shared_data: Arc::new(RwLock::new(HashMap::new())),
            logger,
            grammar_manager,
        }
    }

    /// プラグイン検索パスを追加
    pub fn add_search_path(&mut self, path: &Path) -> &mut Self {
        self.search_paths.push(path.to_path_buf());
        self
    }

    /// プラグイン設定を登録
    pub fn register_config(&self, config: PluginConfig) -> Result<()> {
        let mut configs = self.plugin_configs.write().map_err(|_| {
            ParserError::PluginError("プラグイン設定の登録に失敗しました".to_string())
        })?;
        
        configs.insert(config.plugin_id.clone(), config);
        Ok(())
    }

    /// 組み込みプラグインを登録
    pub fn register_builtin_plugin<P: 'static + Send + Sync>(&self, plugin: P, metadata: PluginMetadata) -> Result<()> {
        let plugin_id = metadata.id.clone();
        
        // メタデータを登録
        {
            let mut meta_map = self.plugin_metadata.write().map_err(|_| {
                ParserError::PluginError(format!("プラグインメタデータの登録に失敗しました: {}", plugin_id))
            })?;
            
            meta_map.insert(plugin_id.clone(), metadata);
        }
        
        // プラグインを登録
        {
            let mut plugins = self.loaded_plugins.write().map_err(|_| {
                ParserError::PluginError(format!("プラグインの登録に失敗しました: {}", plugin_id))
            })?;
            
            plugins.insert(plugin_id.clone(), Box::new(plugin));
        }
        
        // デフォルト設定を登録
        if !self.has_config(&plugin_id)? {
            self.register_config(PluginConfig::new(&plugin_id))?;
        }
        
        Ok(())
    }

    /// プラグイン設定が存在するか確認
    pub fn has_config(&self, plugin_id: &str) -> Result<bool> {
        let configs = self.plugin_configs.read().map_err(|_| {
            ParserError::PluginError("プラグイン設定の確認に失敗しました".to_string())
        })?;
        
        Ok(configs.contains_key(plugin_id))
    }

    /// プラグイン設定を取得
    pub fn get_config(&self, plugin_id: &str) -> Result<Option<PluginConfig>> {
        let configs = self.plugin_configs.read().map_err(|_| {
            ParserError::PluginError("プラグイン設定の取得に失敗しました".to_string())
        })?;
        
        Ok(configs.get(plugin_id).cloned())
    }

    /// プラグインメタデータを取得
    pub fn get_metadata(&self, plugin_id: &str) -> Result<Option<PluginMetadata>> {
        let metadata = self.plugin_metadata.read().map_err(|_| {
            ParserError::PluginError("プラグインメタデータの取得に失敗しました".to_string())
        })?;
        
        Ok(metadata.get(plugin_id).cloned())
    }

    /// 指定された種類のプラグインをすべて取得
    pub fn get_plugins_by_type(&self, plugin_type: PluginType) -> Result<Vec<String>> {
        let metadata = self.plugin_metadata.read().map_err(|_| {
            ParserError::PluginError("プラグインメタデータの取得に失敗しました".to_string())
        })?;
        
        Ok(metadata.iter()
            .filter(|(_, meta)| meta.plugin_type == plugin_type)
            .map(|(id, _)| id.clone())
            .collect())
    }

    /// 字句解析プラグインを取得
    pub fn get_lexer_plugin(&self, plugin_id: &str) -> Result<Option<Box<dyn LexerPlugin>>> {
        let plugins = self.loaded_plugins.read().map_err(|_| {
            ParserError::PluginError("プラグインの取得に失敗しました".to_string())
        })?;
        
        if let Some(plugin) = plugins.get(plugin_id) {
            if let Some(lexer_plugin) = plugin.downcast_ref::<Box<dyn LexerPlugin>>() {
                // 参照を複製して返す（Boxはクローン不可のため不可能）
                return Err(ParserError::NotImplemented(
                    "現在のプラグインシステムでは直接の型変換はサポートされていません".to_string()
                ));
            }
        }
        
        Ok(None)
    }

    /// 外部プラグインをロード
    pub async fn load_external_plugin(&self, path: &Path) -> Result<String> {
        // 外部プラグイン実装はまだサポートされていない
        Err(ParserError::NotImplemented(
            "外部プラグインのロードは現在サポートされていません".to_string()
        ))
    }

    /// プラグインコンテキストを作成
    pub fn create_context(&self, plugin_id: &str) -> Result<PluginContext> {
        // プラグイン設定を取得
        let config = self.get_config(plugin_id)?
            .ok_or_else(|| ParserError::PluginError(
                format!("プラグイン設定が見つかりません: {}", plugin_id)
            ))?;
        
        Ok(PluginContext::new(
            plugin_id,
            config,
            self.shared_data.clone(),
            self.logger.clone(),
            self.grammar_manager.clone(),
        ))
    }

    /// 全プラグインを初期化
    pub async fn initialize_all_plugins(&self) -> Result<()> {
        let plugin_ids = {
            let metadata = self.plugin_metadata.read().map_err(|_| {
                ParserError::PluginError("プラグインメタデータの取得に失敗しました".to_string())
            })?;
            
            metadata.keys().cloned().collect::<Vec<_>>()
        };
        
        for plugin_id in plugin_ids {
            // プラグイン設定を確認
            let config = match self.get_config(&plugin_id)? {
                Some(config) => config,
                None => continue,
            };
            
            // 無効なプラグインはスキップ
            if !config.enabled {
                continue;
            }
            
            // プラグインの種類によって初期化
            if let Some(meta) = self.get_metadata(&plugin_id)? {
                match meta.plugin_type {
                    PluginType::Lexer => {
                        // レキサープラグインを初期化
                        // ...実装...
                    },
                    PluginType::Parser => {
                        // パーサープラグインを初期化
                        // ...実装...
                    },
                    // その他のプラグイン種類も同様に初期化
                    _ => {}
                }
            }
        }
        
        Ok(())
    }
}

/// デフォルトプラグインロガー
pub struct DefaultPluginLogger;

#[async_trait]
impl PluginLogger for DefaultPluginLogger {
    fn debug(&self, plugin_id: &str, message: &str) {
        debug!("[Plugin:{}] {}", plugin_id, message);
    }
    
    fn info(&self, plugin_id: &str, message: &str) {
        info!("[Plugin:{}] {}", plugin_id, message);
    }
    
    fn warn(&self, plugin_id: &str, message: &str) {
        warn!("[Plugin:{}] {}", plugin_id, message);
    }
    
    fn error(&self, plugin_id: &str, message: &str) {
        error!("[Plugin:{}] {}", plugin_id, message);
    }
}

/// 組み込みの引用符トークン処理プラグイン例
pub struct QuotedStringLexerPlugin;

#[async_trait]
impl LexerPlugin for QuotedStringLexerPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "nexusshell.lexer.quoted-string",
            "引用符文字列プラグイン",
            "1.0.0",
            "複雑な引用符とエスケープシーケンスをサポート",
            "NexusShell Team",
            PluginType::Lexer,
        )
    }
    
    async fn initialize(&mut self, _context: &PluginContext) -> Result<()> {
        Ok(())
    }
    
    async fn process_token(&self, input: &str, position: usize) -> Option<(TokenKind, String, usize)> {
        // 位置が範囲外
        if position >= input.len() {
            return None;
        }
        
        let c = input.chars().nth(position)?;
        
        // 引用符を検出したら処理
        if c == '"' || c == '\'' || c == '`' {
            let quote_type = c;
            let mut result = String::new();
            let mut i = position + 1;
            let mut escaped = false;
            
            // 閉じる引用符を探す
            while i < input.len() {
                let current = input.chars().nth(i)?;
                
                if escaped {
                    // エスケープされた文字
                    match current {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        'v' => result.push('\u{000B}'), // 垂直タブ
                        'f' => result.push('\u{000C}'), // フォームフィード
                        '0' => result.push('\0'),
                        'x' => {
                            // 16進数エスケープ
                            if i + 2 < input.len() {
                                let hex = &input[i+1..i+3];
                                if let Ok(code) = u32::from_str_radix(hex, 16) {
                                    if let Some(ch) = std::char::from_u32(code) {
                                        result.push(ch);
                                    }
                                }
                                i += 2;
                            }
                        },
                        'u' => {
                            // Unicodeエスケープ
                            if i + 4 < input.len() && input.chars().nth(i+1)? == '{' {
                                // 閉じる括弧を探す
                                let mut end = i + 2;
                                while end < input.len() {
                                    if input.chars().nth(end)? == '}' {
                                        break;
                                    }
                                    end += 1;
                                }
                                
                                if end < input.len() {
                                    let hex = &input[i+2..end];
                                    if let Ok(code) = u32::from_str_radix(hex, 16) {
                                        if let Some(ch) = std::char::from_u32(code) {
                                            result.push(ch);
                                        }
                                    }
                                    i = end;
                                }
                            }
                        },
                        _ => result.push(current),
                    }
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote_type {
                    // 引用符が一致したら終了
                    let token_kind = match quote_type {
                        '"' => TokenKind::String,
                        '\'' => TokenKind::String,
                        '`' => TokenKind::Command,
                        _ => unreachable!(),
                    };
                    
                    return Some((token_kind, result, i + 1 - position));
                } else {
                    result.push(current);
                }
                
                i += 1;
            }
        }
        
        None
    }
    
    async fn post_process(&self, tokens: Vec<Token>) -> Vec<Token> {
        // トークン列の後処理はデフォルトで何もしない
        tokens
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

/// 組み込みのトークン色分けプラグイン例
pub struct SyntaxHighlightPlugin {
    color_map: HashMap<TokenKind, String>,
}

impl SyntaxHighlightPlugin {
    pub fn new() -> Self {
        let mut color_map = HashMap::new();
        
        // ANSI色コードでのマッピング
        color_map.insert(TokenKind::Command, "\x1b[1;32m".to_string());      // 太字緑
        color_map.insert(TokenKind::Argument, "\x1b[0;37m".to_string());     // 白
        color_map.insert(TokenKind::Option, "\x1b[1;34m".to_string());       // 太字青
        color_map.insert(TokenKind::Flag, "\x1b[1;36m".to_string());         // 太字シアン
        color_map.insert(TokenKind::Variable, "\x1b[1;33m".to_string());     // 太字黄
        color_map.insert(TokenKind::String, "\x1b[0;32m".to_string());       // 緑
        color_map.insert(TokenKind::Integer, "\x1b[0;35m".to_string());      // マゼンタ
        color_map.insert(TokenKind::Float, "\x1b[0;35m".to_string());        // マゼンタ
        color_map.insert(TokenKind::Boolean, "\x1b[1;35m".to_string());      // 太字マゼンタ
        color_map.insert(TokenKind::Pipe, "\x1b[1;31m".to_string());         // 太字赤
        color_map.insert(TokenKind::PipeParallel, "\x1b[1;31m".to_string()); // 太字赤
        
        Self { color_map }
    }

    /// テキストを色付け
    pub fn colorize(&self, text: &str, token_kind: &TokenKind) -> String {
        if let Some(color) = self.color_map.get(token_kind) {
            format!("{}{}\x1b[0m", color, text)
        } else {
            text.to_string()
        }
    }

    /// テキスト全体を色付け
    pub fn colorize_text(&self, text: &str, tokens: &[Token]) -> String {
        let mut result = String::new();
        let mut pos = 0;
        
        for token in tokens {
            // トークン前のテキストをそのまま追加
            if token.span.start > pos {
                result.push_str(&text[pos..token.span.start]);
            }
            
            // トークンを色付け
            let token_text = &text[token.span.start..token.span.end];
            result.push_str(&self.colorize(token_text, &token.kind));
            
            pos = token.span.end;
        }
        
        // 残りのテキストを追加
        if pos < text.len() {
            result.push_str(&text[pos..]);
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plugin_metadata() {
        let metadata = PluginMetadata::new(
            "test-plugin",
            "Test Plugin",
            "1.0.0",
            "A test plugin",
            "Test Author",
            PluginType::Lexer,
        );
        
        assert_eq!(metadata.id, "test-plugin");
        assert_eq!(metadata.name, "Test Plugin");
        assert_eq!(metadata.plugin_type, PluginType::Lexer);
    }
    
    #[test]
    fn test_plugin_config() {
        let config = PluginConfig::new("test-plugin")
            .with_enabled(true)
            .with_priority(50)
            .with_setting("max_tokens", 100)
            .with_setting("debug_mode", true);
        
        assert_eq!(config.plugin_id, "test-plugin");
        assert_eq!(config.enabled, true);
        assert_eq!(config.priority, 50);
        assert_eq!(config.get_number("max_tokens"), Some(100.0));
        assert_eq!(config.get_bool("debug_mode"), Some(true));
    }
    
    #[tokio::test]
    async fn test_quoted_string_plugin() {
        let mut plugin = QuotedStringLexerPlugin;
        
        let input = r#"echo "Hello \"World\""#;
        let result = plugin.process_token(input, 5).await;
        
        assert!(result.is_some());
        let (kind, value, len) = result.unwrap();
        assert_eq!(kind, TokenKind::String);
        assert_eq!(value, "Hello \"World\"");
        assert_eq!(len, 15);
    }
} 