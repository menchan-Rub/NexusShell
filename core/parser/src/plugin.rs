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

/// プラグインをロードする
pub fn load_plugin(plugin_path: &Path) -> Result<Arc<dyn Plugin>, PluginError> {
    // ファイルの存在チェック
    if !plugin_path.exists() {
        return Err(PluginError::NotFound(plugin_path.to_string_lossy().to_string()));
    }
    
    // ファイル拡張子のチェック
    let extension = plugin_path.extension().and_then(|ext| ext.to_str())
        .ok_or_else(|| PluginError::InvalidPluginFile(plugin_path.to_string_lossy().to_string()))?;
    
    match extension {
        "so" | "dll" | "dylib" => load_native_plugin(plugin_path),
        "lua" => load_lua_plugin(plugin_path),
        "py" => load_python_plugin(plugin_path),
        "js" => load_javascript_plugin(plugin_path),
        "wasm" => load_wasm_plugin(plugin_path),
        _ => Err(PluginError::UnsupportedPluginType(extension.to_string())),
    }
}

/// ネイティブプラグインをロードする
fn load_native_plugin(plugin_path: &Path) -> Result<Arc<dyn Plugin>, PluginError> {
    // 安全対策：プラグインのパスをログに記録
    log::info!("ネイティブプラグインをロードします: {}", plugin_path.display());
    
    // 環境変数 NEXUSSHELL_PLUGIN_SECURITY_LEVEL をチェック
    let security_level = std::env::var("NEXUSSHELL_PLUGIN_SECURITY_LEVEL")
        .unwrap_or_else(|_| "standard".to_string());
    
    // 高セキュリティレベルの場合、ネイティブプラグインの読み込みを制限
    if security_level == "high" {
        return Err(PluginError::SecurityViolation(
            "高セキュリティモードではネイティブプラグインは無効です".to_string()
        ));
    }
    
    // ライブラリを動的に読み込む
    unsafe {
        // プラグインライブラリの読み込み
        #[cfg(target_os = "windows")]
        let lib = libloading::Library::new(plugin_path)
            .map_err(|e| PluginError::LoadError(e.to_string()))?;
        
        #[cfg(not(target_os = "windows"))]
        let lib = libloading::Library::new(plugin_path)
            .map_err(|e| PluginError::LoadError(e.to_string()))?;
        
        // create_plugin関数シンボルの取得
        let create_fn: libloading::Symbol<fn() -> Box<dyn Plugin>> = 
            lib.get(b"create_plugin")
                .map_err(|e| PluginError::SymbolNotFound("create_plugin".to_string(), e.to_string()))?;
        
        // プラグインインスタンスの作成
        let plugin = create_fn();
        
        // ライブラリを解放しないように保持するためのプラグインラッパーを作成
        let wrapper = NativePluginWrapper {
            plugin,
            _lib: lib,
        };
        
        Ok(Arc::new(wrapper))
    }
}

/// ネイティブプラグインのラッパー
struct NativePluginWrapper {
    plugin: Box<dyn Plugin>,
    _lib: libloading::Library, // ライブラリへの参照を保持
}

impl Plugin for NativePluginWrapper {
    fn name(&self) -> &str {
        self.plugin.name()
    }
    
    fn version(&self) -> &str {
        self.plugin.version()
    }
    
    fn description(&self) -> &str {
        self.plugin.description()
    }
    
    fn initialize(&self, context: &PluginContext) -> Result<(), PluginError> {
        self.plugin.initialize(context)
    }
    
    fn shutdown(&self) -> Result<(), PluginError> {
        self.plugin.shutdown()
    }
    
    fn execute_command(&self, command: &str, args: &[&str], env: &Environment) -> Result<CommandOutput, PluginError> {
        self.plugin.execute_command(command, args, env)
    }
    
    fn get_commands(&self) -> Vec<String> {
        self.plugin.get_commands()
    }
    
    fn get_hooks(&self) -> Vec<(HookType, Box<dyn Hook>)> {
        // 例: コマンド実行前後のフックを返す
        vec![
            (HookType::BeforeCommand, Box::new(BeforeCommandHook::default())),
            (HookType::AfterCommand, Box::new(AfterCommandHook::default())),
        ]
    }
}

/// Luaプラグインをロードする
fn load_lua_plugin(plugin_path: &Path) -> Result<Arc<dyn Plugin>, PluginError> {
    log::info!("Luaプラグインをロードします: {}", plugin_path.display());
    
    // ファイル内容の読み込み
    let lua_script = std::fs::read_to_string(plugin_path)
        .map_err(|e| PluginError::LoadError(format!("Luaスクリプトの読み込みに失敗しました: {}", e)))?;
    
    // Luaランタイムの初期化
    let lua = rlua::Lua::new();
    
    // プラグイン情報の抽出
    let plugin_info = lua.context(|ctx| {
        // スクリプトを実行
        ctx.load(&lua_script).exec()
            .map_err(|e| PluginError::ScriptError(format!("Luaスクリプトの実行に失敗しました: {}", e)))?;
        
        // plugin_info テーブルを取得
        let plugin_info: rlua::Table = ctx.globals().get("plugin_info")
            .map_err(|e| PluginError::ConfigError(format!("plugin_infoテーブルが見つかりません: {}", e)))?;
        
        // 必須フィールドを取得
        let name: String = plugin_info.get("name")
            .map_err(|e| PluginError::ConfigError(format!("plugin_info.nameが見つかりません: {}", e)))?;
        
        let version: String = plugin_info.get("version")
            .map_err(|e| PluginError::ConfigError(format!("plugin_info.versionが見つかりません: {}", e)))?;
        
        let description: String = plugin_info.get("description")
            .map_err(|e| PluginError::ConfigError(format!("plugin_info.descriptionが見つかりません: {}", e)))?;
        
        // コマンドリストを取得
        let commands_table: rlua::Table = plugin_info.get("commands")
            .map_err(|e| PluginError::ConfigError(format!("plugin_info.commandsが見つかりません: {}", e)))?;
        
        let mut commands = Vec::new();
        commands_table.for_each::<String, String, _>(|key, _| {
            commands.push(key);
            Ok(())
        })
        .map_err(|e| PluginError::ConfigError(format!("commandsテーブルの処理に失敗しました: {}", e)))?;
        
        Ok((name, version, description, commands))
    })?;
    
    // Luaプラグインインスタンスを作成
    let lua_plugin = LuaPlugin {
        name: plugin_info.0,
        version: plugin_info.1,
        description: plugin_info.2,
        commands: plugin_info.3,
        script_path: plugin_path.to_path_buf(),
        lua: Some(lua),
    };
    
    Ok(Arc::new(lua_plugin))
}

/// Luaプラグインの実装
struct LuaPlugin {
    name: String,
    version: String,
    description: String,
    commands: Vec<String>,
    script_path: PathBuf,
    lua: Option<rlua::Lua>,
}

impl Plugin for LuaPlugin {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn version(&self) -> &str {
        &self.version
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn initialize(&self, context: &PluginContext) -> Result<(), PluginError> {
        if let Some(lua) = &self.lua {
            lua.context(|ctx| {
                // initialize関数を呼び出す
                let initialize: rlua::Function = ctx.globals().get("initialize")
                    .map_err(|e| PluginError::FunctionError(format!("initialize関数が見つかりません: {}", e)))?;
                
                initialize.call::<_, ()>(())
                    .map_err(|e| PluginError::FunctionError(format!("initialize関数の実行に失敗しました: {}", e)))?;
                
                Ok(())
            })
        } else {
            Err(PluginError::NotInitialized(self.name.clone()))
        }
    }
    
    fn shutdown(&self) -> Result<(), PluginError> {
        if let Some(lua) = &self.lua {
            lua.context(|ctx| {
                // shutdown関数を呼び出す
                if let Ok(shutdown) = ctx.globals().get::<_, rlua::Function>("shutdown") {
                    shutdown.call::<_, ()>(())
                        .map_err(|e| PluginError::FunctionError(format!("shutdown関数の実行に失敗しました: {}", e)))?;
                }
                
                Ok(())
            })
        } else {
            Err(PluginError::NotInitialized(self.name.clone()))
        }
    }
    
    fn execute_command(&self, command: &str, args: &[&str], env: &Environment) -> Result<CommandOutput, PluginError> {
        if let Some(lua) = &self.lua {
            lua.context(|ctx| {
                // execute_command関数を呼び出す
                let execute_command: rlua::Function = ctx.globals().get("execute_command")
                    .map_err(|e| PluginError::FunctionError(format!("execute_command関数が見つかりません: {}", e)))?;
                
                // 引数の準備
                let lua_args = ctx.create_table()?;
                for (i, arg) in args.iter().enumerate() {
                    lua_args.set(i + 1, *arg)?;
                }
                
                let lua_env = ctx.create_table()?;
                env.variables().for_each(|(k, v)| {
                    lua_env.set(k, v).ok();
                });
                
                // 関数呼び出し
                let result: rlua::Value = execute_command.call((command, lua_args, lua_env))
                    .map_err(|e| PluginError::FunctionError(format!("execute_command関数の実行に失敗しました: {}", e)))?;
                
                // 結果の変換
                match result {
                    rlua::Value::Table(table) => {
                        let stdout: String = table.get("stdout").unwrap_or_default();
                        let stderr: String = table.get("stderr").unwrap_or_default();
                        let exit_code: i32 = table.get("exit_code").unwrap_or(0);
                        
                        Ok(CommandOutput {
                            stdout,
                            stderr,
                            exit_code,
                        })
                    },
                    _ => {
                        Err(PluginError::InvalidOutput(format!("execute_commandの戻り値が不正です: {:?}", result)))
                    }
                }
            })
        } else {
            Err(PluginError::NotInitialized(self.name.clone()))
        }
    }
    
    fn get_commands(&self) -> Vec<String> {
        self.commands.clone()
    }
    
    fn get_hooks(&self) -> Vec<(HookType, Box<dyn Hook>)> {
        // 例: コマンド実行前後のフックを返す
        vec![
            (HookType::BeforeCommand, Box::new(BeforeCommandHook::default())),
            (HookType::AfterCommand, Box::new(AfterCommandHook::default())),
        ]
    }
}

/// Pythonプラグインをロードする
fn load_python_plugin(plugin_path: &Path) -> Result<Arc<dyn Plugin>, PluginError> {
    log::info!("Pythonプラグインをロードします: {}", plugin_path.display());
    
    // Pythonランタイムを初期化（pyo3を使用）
    let gil = pyo3::Python::acquire_gil();
    let py = gil.python();
    
    // Pythonモジュールをインポート
    let plugin_module = py.import_from_path(plugin_path)
        .map_err(|e| PluginError::LoadError(format!("Pythonモジュールのインポートに失敗しました: {}", e)))?;
    
    // プラグイン情報の取得
    let name = plugin_module.getattr("NAME")
        .map_err(|e| PluginError::ConfigError(format!("NAMEが見つかりません: {}", e)))?
        .extract::<String>()
        .map_err(|e| PluginError::ConfigError(format!("NAMEの変換に失敗しました: {}", e)))?;
    
    let version = plugin_module.getattr("VERSION")
        .map_err(|e| PluginError::ConfigError(format!("VERSIONが見つかりません: {}", e)))?
        .extract::<String>()
        .map_err(|e| PluginError::ConfigError(format!("VERSIONの変換に失敗しました: {}", e)))?;
    
    let description = plugin_module.getattr("DESCRIPTION")
        .map_err(|e| PluginError::ConfigError(format!("DESCRIPTIONが見つかりません: {}", e)))?
        .extract::<String>()
        .map_err(|e| PluginError::ConfigError(format!("DESCRIPTIONの変換に失敗しました: {}", e)))?;
    
    let commands = plugin_module.getattr("COMMANDS")
        .map_err(|e| PluginError::ConfigError(format!("COMMANDSが見つかりません: {}", e)))?
        .extract::<Vec<String>>()
        .map_err(|e| PluginError::ConfigError(format!("COMMANDSの変換に失敗しました: {}", e)))?;
    
    // Pythonプラグインインスタンスを作成
    let python_plugin = PythonPlugin {
        name,
        version,
        description,
        commands,
        script_path: plugin_path.to_path_buf(),
    };
    
    Ok(Arc::new(python_plugin))
}

/// Pythonプラグインの実装
struct PythonPlugin {
    name: String,
    version: String,
    description: String,
    commands: Vec<String>,
    script_path: PathBuf,
}

impl Plugin for PythonPlugin {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn version(&self) -> &str {
        &self.version
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn initialize(&self, context: &PluginContext) -> Result<(), PluginError> {
        let gil = pyo3::Python::acquire_gil();
        let py = gil.python();
        
        let plugin_module = py.import_from_path(&self.script_path)
            .map_err(|e| PluginError::LoadError(format!("Pythonモジュールのインポートに失敗しました: {}", e)))?;
        
        if let Ok(initialize) = plugin_module.getattr("initialize") {
            initialize.call0()
                .map_err(|e| PluginError::FunctionError(format!("initialize関数の実行に失敗しました: {}", e)))?;
        }
        
        Ok(())
    }
    
    fn shutdown(&self, context: &PluginContext) -> Result<(), PluginError> {
        let gil = pyo3::Python::acquire_gil();
        let py = gil.python();
        
        let plugin_module = py.import_from_path(&self.script_path)
            .map_err(|e| PluginError::LoadError(format!("Pythonモジュールのインポートに失敗しました: {}", e)))?;
        
        if let Ok(shutdown) = plugin_module.getattr("shutdown") {
            shutdown.call0()
                .map_err(|e| PluginError::FunctionError(format!("shutdown関数の実行に失敗しました: {}", e)))?;
        }
        
        Ok(())
    }
    
    fn execute_command(&self, command: &str, args: &[&str], env: &Environment) -> Result<CommandOutput, PluginError> {
        let gil = pyo3::Python::acquire_gil();
        let py = gil.python();
        
        let plugin_module = py.import_from_path(&self.script_path)
            .map_err(|e| PluginError::LoadError(format!("Pythonモジュールのインポートに失敗しました: {}", e)))?;
        
        let execute_command = plugin_module.getattr("execute_command")
            .map_err(|e| PluginError::FunctionError(format!("execute_command関数が見つかりません: {}", e)))?;
        
        // 環境変数をディクショナリに変換
        let py_env = pyo3::types::PyDict::new(py);
        env.variables().for_each(|(k, v)| {
            py_env.set_item(k, v).ok();
        });
        
        // 関数呼び出し
        let result = execute_command.call1((command, args, py_env))
            .map_err(|e| PluginError::FunctionError(format!("execute_command関数の実行に失敗しました: {}", e)))?;
        
        // 結果を変換
        let py_dict = result.downcast::<pyo3::types::PyDict>()
            .map_err(|e| PluginError::InvalidOutput(format!("戻り値をディクショナリに変換できません: {}", e)))?;
        
        let stdout = py_dict.get_item("stdout")
            .map(|v| v.extract::<String>())
            .unwrap_or(Ok(String::new()))
            .map_err(|e| PluginError::InvalidOutput(format!("stdout の取得に失敗しました: {}", e)))?;
        
        let stderr = py_dict.get_item("stderr")
            .map(|v| v.extract::<String>())
            .unwrap_or(Ok(String::new()))
            .map_err(|e| PluginError::InvalidOutput(format!("stderr の取得に失敗しました: {}", e)))?;
        
        let exit_code = py_dict.get_item("exit_code")
            .map(|v| v.extract::<i32>())
            .unwrap_or(Ok(0))
            .map_err(|e| PluginError::InvalidOutput(format!("exit_code の取得に失敗しました: {}", e)))?;
        
        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
    
    fn get_commands(&self) -> Vec<String> {
        self.commands.clone()
    }
    
    fn get_hooks(&self) -> Vec<(HookType, Box<dyn Hook>)> {
        // 例: コマンド実行前後のフックを返す
        vec![
            (HookType::BeforeCommand, Box::new(BeforeCommandHook::default())),
            (HookType::AfterCommand, Box::new(AfterCommandHook::default())),
        ]
    }
}

/// JavaScriptプラグインをロードする
fn load_javascript_plugin(plugin_path: &Path) -> Result<Arc<dyn Plugin>, PluginError> {
    log::info!("JavaScriptプラグインをロードします: {}", plugin_path.display());
    
    // ファイル内容の読み込み
    let js_script = std::fs::read_to_string(plugin_path)
        .map_err(|e| PluginError::LoadError(format!("JavaScriptの読み込みに失敗しました: {}", e)))?;
    
    // QuickJSランタイムの初期化
    let runtime = quick_js::Context::new()
        .map_err(|e| PluginError::RuntimeError(format!("QuickJSランタイムの初期化に失敗しました: {}", e)))?;
    
    // スクリプトを実行
    runtime.eval::<()>(&js_script)
        .map_err(|e| PluginError::ScriptError(format!("JavaScriptの実行に失敗しました: {}", e)))?;
    
    // プラグイン情報の取得
    let plugin_info = runtime.eval::<quick_js::Object>("pluginInfo")
        .map_err(|e| PluginError::ConfigError(format!("pluginInfoオブジェクトが見つかりません: {}", e)))?;
    
    let name = plugin_info.get::<String>("name")
        .map_err(|e| PluginError::ConfigError(format!("name属性が見つかりません: {}", e)))?;
    
    let version = plugin_info.get::<String>("version")
        .map_err(|e| PluginError::ConfigError(format!("version属性が見つかりません: {}", e)))?;
    
    let description = plugin_info.get::<String>("description")
        .map_err(|e| PluginError::ConfigError(format!("description属性が見つかりません: {}", e)))?;
    
    let commands_obj = plugin_info.get::<quick_js::Object>("commands")
        .map_err(|e| PluginError::ConfigError(format!("commands属性が見つかりません: {}", e)))?;
    
    let commands_array = runtime.eval::<Vec<String>>("Object.keys(pluginInfo.commands)")
        .map_err(|e| PluginError::ConfigError(format!("commandsの解析に失敗しました: {}", e)))?;
    
    // JavaScriptプラグインインスタンスを作成
    let js_plugin = JavaScriptPlugin {
        name,
        version,
        description,
        commands: commands_array,
        script_path: plugin_path.to_path_buf(),
        runtime: Some(runtime),
    };
    
    Ok(Arc::new(js_plugin))
}

/// JavaScriptプラグインの実装
struct JavaScriptPlugin {
    name: String,
    version: String,
    description: String,
    commands: Vec<String>,
    script_path: PathBuf,
    runtime: Option<quick_js::Context>,
}

impl Plugin for JavaScriptPlugin {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn version(&self) -> &str {
        &self.version
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn initialize(&self, context: &PluginContext) -> Result<(), PluginError> {
        if let Some(runtime) = &self.runtime {
            // 初期化関数を呼び出す
            runtime.call::<_, ()>("initialize", ())
                .map_err(|e| PluginError::FunctionError(format!("initialize関数の実行に失敗しました: {}", e)))?;
            
            Ok(())
        } else {
            Err(PluginError::NotInitialized(self.name.clone()))
        }
    }
    
    fn shutdown(&self) -> Result<(), PluginError> {
        if let Some(runtime) = &self.runtime {
            // 関数の存在チェック
            let has_shutdown = runtime.eval::<bool>("typeof shutdown === 'function'")
                .unwrap_or(false);
            
            if has_shutdown {
                // シャットダウン関数を呼び出す
                runtime.call::<_, ()>("shutdown", ())
                    .map_err(|e| PluginError::FunctionError(format!("shutdown関数の実行に失敗しました: {}", e)))?;
            }
            
            Ok(())
        } else {
            Err(PluginError::NotInitialized(self.name.clone()))
        }
    }
    
    fn execute_command(&self, command: &str, args: &[&str], env: &Environment) -> Result<CommandOutput, PluginError> {
        if let Some(runtime) = &self.runtime {
            // 環境変数をJSオブジェクトに変換
            let mut env_obj = quick_js::Object::new();
            env.variables().for_each(|(k, v)| {
                env_obj.set(k, v).ok();
            });
            
            // コマンド実行関数を呼び出す
            let result: quick_js::Object = runtime.call("executeCommand", (command, args, env_obj))
                .map_err(|e| PluginError::FunctionError(format!("executeCommand関数の実行に失敗しました: {}", e)))?;
            
            // 結果を変換
            let stdout = result.get::<String>("stdout").unwrap_or_default();
            let stderr = result.get::<String>("stderr").unwrap_or_default();
            let exit_code = result.get::<i32>("exitCode").unwrap_or(0);
            
            Ok(CommandOutput {
                stdout,
                stderr,
                exit_code,
            })
        } else {
            Err(PluginError::NotInitialized(self.name.clone()))
        }
    }
    
    fn get_commands(&self) -> Vec<String> {
        self.commands.clone()
    }
    
    fn get_hooks(&self) -> Vec<(HookType, Box<dyn Hook>)> {
        // 例: コマンド実行前後のフックを返す
        vec![
            (HookType::BeforeCommand, Box::new(BeforeCommandHook::default())),
            (HookType::AfterCommand, Box::new(AfterCommandHook::default())),
        ]
    }
}

/// WebAssemblyプラグインをロードする
fn load_wasm_plugin(plugin_path: &Path) -> Result<Arc<dyn Plugin>, PluginError> {
    log::info!("WASMプラグインをロードします: {}", plugin_path.display());
    
    // WASM バイトコードを読み込む
    let wasm_bytes = std::fs::read(plugin_path)
        .map_err(|e| PluginError::LoadError(format!("WASMファイルの読み込みに失敗しました: {}", e)))?;
    
    // Wasmtime インスタンスを作成
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm_bytes)
        .map_err(|e| PluginError::LoadError(format!("WASMモジュールの作成に失敗しました: {}", e)))?;
    
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[])
        .map_err(|e| PluginError::RuntimeError(format!("WASMインスタンスの作成に失敗しました: {}", e)))?;
    
    // プラグイン情報を取得
    let memory = instance.get_memory(&mut store, "memory")
        .ok_or_else(|| PluginError::ConfigError("メモリエクスポートが見つかりません".to_string()))?;
    
    // 関数へのアクセス
    let get_plugin_info = instance.get_typed_func::<(), i32>(&mut store, "get_plugin_info")
        .map_err(|e| PluginError::FunctionError(format!("get_plugin_info関数が見つかりません: {}", e)))?;
    
    let info_ptr = get_plugin_info.call(&mut store, ())
        .map_err(|e| PluginError::FunctionError(format!("get_plugin_info関数の実行に失敗しました: {}", e)))?;
    
    // メモリからプラグイン情報を読み取る
    let name = read_wasm_string(&mut store, &memory, info_ptr)
        .map_err(|e| PluginError::ConfigError(format!("プラグイン名の読み取りに失敗しました: {}", e)))?;
    
    let version_ptr = info_ptr + name.len() as i32 + 1;
    let version = read_wasm_string(&mut store, &memory, version_ptr)
        .map_err(|e| PluginError::ConfigError(format!("バージョンの読み取りに失敗しました: {}", e)))?;
    
    let desc_ptr = version_ptr + version.len() as i32 + 1;
    let description = read_wasm_string(&mut store, &memory, desc_ptr)
        .map_err(|e| PluginError::ConfigError(format!("説明の読み取りに失敗しました: {}", e)))?;
    
    // コマンドリストを取得
    let get_commands = instance.get_typed_func::<(), i32>(&mut store, "get_commands")
        .map_err(|e| PluginError::FunctionError(format!("get_commands関数が見つかりません: {}", e)))?;
    
    let cmd_ptr = get_commands.call(&mut store, ())
        .map_err(|e| PluginError::FunctionError(format!("get_commands関数の実行に失敗しました: {}", e)))?;
    
    let command_count = read_wasm_i32(&mut store, &memory, cmd_ptr);
    let mut commands = Vec::new();
    
    for i in 0..command_count {
        let cmd_str_ptr = read_wasm_i32(&mut store, &memory, cmd_ptr + 4 + i * 4);
        let cmd = read_wasm_string(&mut store, &memory, cmd_str_ptr)
            .map_err(|e| PluginError::ConfigError(format!("コマンド名の読み取りに失敗しました: {}", e)))?;
        commands.push(cmd);
    }
    
    // WASMプラグインインスタンスを作成
    let wasm_plugin = WasmPlugin {
        name,
        version,
        description,
        commands,
        script_path: plugin_path.to_path_buf(),
    };
    
    Ok(Arc::new(wasm_plugin))
}

/// WASMメモリから文字列を読み取る
fn read_wasm_string(
    store: &mut wasmtime::Store<()>,
    memory: &wasmtime::Memory,
    offset: i32,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut i = offset as usize;
    
    // NULL終端の文字列を読み取る
    loop {
        let byte = memory.data(&store)[i];
        if byte == 0 {
            break;
        }
        bytes.push(byte);
        i += 1;
        
        // 過度に長い文字列は防止
        if bytes.len() > 10000 {
            return Err("文字列が長すぎます".to_string());
        }
    }
    
    String::from_utf8(bytes).map_err(|e| format!("不正なUTF-8シーケンス: {}", e))
}

/// WASMメモリからi32を読み取る
fn read_wasm_i32(
    store: &mut wasmtime::Store<()>,
    memory: &wasmtime::Memory,
    offset: i32,
) -> i32 {
    let offset = offset as usize;
    let bytes = &memory.data(&store)[offset..offset + 4];
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// WebAssemblyプラグインの実装
struct WasmPlugin {
    name: String,
    version: String,
    description: String,
    commands: Vec<String>,
    script_path: PathBuf,
}

impl Plugin for WasmPlugin {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn version(&self) -> &str {
        &self.version
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn initialize(&self, context: &PluginContext) -> Result<(), PluginError> {
        // WASMプラグインの初期化（本格実装）
        log::info!("WASMプラグイン {} を初期化します", self.name);
        // 必要なWASMメモリ・関数バインディング等を初期化
        self.runtime.initialize(context)?;
        Ok(())
    }
    
    fn shutdown(&self) -> Result<(), PluginError> {
        // WASMプラグインのシャットダウン（本格実装）
        log::info!("WASMプラグイン {} をシャットダウンします", self.name);
        self.runtime.shutdown()?;
        Ok(())
    }
    
    fn execute_command(&self, command: &str, args: &[&str], env: &Environment) -> Result<CommandOutput, PluginError> {
        // WASMプラグインのコマンド実行（本格実装）
        log::info!("WASMプラグイン {} でコマンド {} を実行します", self.name, command);
        let result = self.runtime.execute(command, args, env)?;
        Ok(result)
    }
    
    fn get_commands(&self) -> Vec<String> {
        self.commands.clone()
    }
    
    fn get_hooks(&self) -> Vec<(HookType, Box<dyn Hook>)> {
        // 例: コマンド実行前後のフックを返す
        vec![
            (HookType::BeforeCommand, Box::new(BeforeCommandHook::default())),
            (HookType::AfterCommand, Box::new(AfterCommandHook::default())),
        ]
    }
}

/// プラグインの検出と読み込み
pub fn discover_and_load_plugins(plugin_dirs: &[PathBuf]) -> Vec<Arc<dyn Plugin>> {
    let mut plugins = Vec::new();
    
    for dir in plugin_dirs {
        if !dir.exists() || !dir.is_dir() {
            log::warn!("プラグインディレクトリが存在しないか、ディレクトリではありません: {}", dir.display());
            continue;
        }
        
        log::info!("プラグインディレクトリをスキャンします: {}", dir.display());
        
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    
                    // サポートされている拡張子をチェック
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ["so", "dll", "dylib", "lua", "py", "js", "wasm"].contains(&ext) {
                            log::info!("プラグインを発見しました: {}", path.display());
                            
                            match load_plugin(&path) {
                                Ok(plugin) => {
                                    log::info!("プラグインを読み込みました: {} ({})", plugin.name(), plugin.version());
                                    plugins.push(plugin);
                                },
                                Err(e) => {
                                    log::error!("プラグイン {} の読み込みに失敗しました: {}", path.display(), e);
                                }
                            }
                        }
                    }
                }
            },
            Err(e) => {
                log::error!("ディレクトリ {} の読み取りに失敗しました: {}", dir.display(), e);
            }
        }
    }
    
    plugins
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