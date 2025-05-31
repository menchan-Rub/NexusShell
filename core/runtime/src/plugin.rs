/*!
# プラグインモジュール

シェルの拡張機能を管理する高性能プラグインシステムを提供します。
動的ローディングとホットリロード機能を備えています。
*/

use anyhow::Result;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use crate::execution::{ExecutionContext, ExecutionResult};
use std::fs;
use uuid::Uuid;
use libloading;
use std::ffi::CStr;

/// プラグインインターフェース
#[async_trait]
pub trait Plugin: Send + Sync {
    /// プラグイン名
    fn name(&self) -> &str;
    
    /// プラグインの説明
    fn description(&self) -> &str;
    
    /// プラグインのバージョン
    fn version(&self) -> &str;
    
    /// プラグインの初期化
    async fn initialize(&self) -> Result<()>;
    
    /// プラグインの終了処理
    async fn shutdown(&self) -> Result<()>;
    
    /// プラグインの依存関係を取得（オプショナル）
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }
    
    /// プラグインのメタデータを取得
    fn metadata(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// プラグインハンドラーの結果
#[derive(Debug, Clone)]
pub struct PluginCommandResult {
    /// 終了コード
    pub exit_code: i32,
    /// 標準出力
    pub stdout: Vec<u8>,
    /// 標準エラー出力
    pub stderr: Vec<u8>,
    /// 追加メタデータ
    pub metadata: HashMap<String, String>,
}

impl PluginCommandResult {
    /// 成功結果を作成
    pub fn success(stdout: Vec<u8>) -> Self {
        Self {
            exit_code: 0,
            stdout,
            stderr: Vec::new(),
            metadata: HashMap::new(),
        }
    }
    
    /// エラー結果を作成
    pub fn error(stderr: Vec<u8>, code: i32) -> Self {
        Self {
            exit_code: code,
            stdout: Vec::new(),
            stderr,
            metadata: HashMap::new(),
        }
    }
    
    /// メタデータを追加
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// 実行フックインターフェース
#[async_trait]
pub trait ExecutionHook: Send + Sync {
    /// フック名
    fn name(&self) -> &str;
    
    /// コマンド実行前フック
    async fn before_execution(&self, command: &str, args: &[String], context: &ExecutionContext) -> Result<()>;
    
    /// コマンド実行後フック
    async fn after_execution(&self, command: &str, args: &[String], context: &ExecutionContext, result: &ExecutionResult) -> Result<()>;
}

/// コマンドハンドラーインターフェース
#[async_trait]
pub trait CommandHandler: Send + Sync {
    /// コマンド名
    fn command_name(&self) -> &str;
    
    /// コマンドの説明
    fn description(&self) -> &str;
    
    /// コマンドのヘルプ
    fn help(&self) -> String {
        format!("{}: {}", self.command_name(), self.description())
    }
    
    /// エイリアス（別名）
    fn aliases(&self) -> Vec<String> {
        Vec::new()
    }
    
    /// コマンドの実行
    async fn execute(&self, args: Vec<String>, context: ExecutionContext) -> Result<PluginCommandResult>;
}

/// プラグインのメタデータ
#[derive(Debug, Clone)]
struct PluginMetadata {
    /// 読み込み時間
    loaded_at: Instant,
    /// 最終アクセス時間
    last_accessed: Instant,
    /// 使用回数
    usage_count: usize,
    /// 依存関係
    dependencies: HashSet<String>,
    /// 追加メタデータ
    metadata: HashMap<String, String>,
}

/// プラグインマネージャ
pub struct PluginManager {
    /// 読み込まれたプラグイン
    plugins: RwLock<HashMap<String, (Arc<dyn Plugin>, PluginMetadata)>>,
    /// コマンドハンドラー (コマンド名 -> ハンドラー)
    command_handlers: RwLock<HashMap<String, Arc<dyn CommandHandler>>>,
    /// エイリアスマップ (エイリアス -> 元のコマンド名)
    command_aliases: RwLock<HashMap<String, String>>,
    /// 実行フック
    execution_hooks: RwLock<Vec<Arc<dyn ExecutionHook>>>,
    /// プラグインの検索パス
    plugin_paths: RwLock<Vec<PathBuf>>,
    /// コマンドハンドラーキャッシュ
    command_cache: dashmap::DashMap<String, Arc<dyn CommandHandler>>,
}

impl fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugins", &format!("<RwLock<{} plugins>>", self.plugins.try_read().map(|p| p.len()).unwrap_or(0)))
            .field("command_handlers", &format!("<RwLock<{} handlers>>", self.command_handlers.try_read().map(|h| h.len()).unwrap_or(0)))
            .field("command_aliases", &format!("<RwLock<{} aliases>>", self.command_aliases.try_read().map(|a| a.len()).unwrap_or(0)))
            .field("execution_hooks", &format!("<RwLock<{} hooks>>", self.execution_hooks.try_read().map(|h| h.len()).unwrap_or(0)))
            .field("plugin_paths", &format!("<RwLock<{} paths>>", self.plugin_paths.try_read().map(|p| p.len()).unwrap_or(0)))
            .field("command_cache", &format!("<DashMap<{} entries>>", self.command_cache.len()))
            .finish()
    }
}

impl PluginManager {
    /// 新しいプラグインマネージャを作成
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            command_handlers: RwLock::new(HashMap::new()),
            command_aliases: RwLock::new(HashMap::new()),
            execution_hooks: RwLock::new(Vec::new()),
            plugin_paths: RwLock::new(Vec::new()),
            command_cache: dashmap::DashMap::new(),
        }
    }
    
    /// プラグインを登録
    pub async fn register_plugin(&self, plugin: Arc<dyn Plugin>) -> Result<()> {
        let name = plugin.name().to_string();
        
        // 初期化を実行
        plugin.initialize().await?;
        
        // メタデータを作成
        let metadata = PluginMetadata {
            loaded_at: Instant::now(),
            last_accessed: Instant::now(),
            usage_count: 0,
            dependencies: plugin.dependencies().into_iter().collect(),
            metadata: plugin.metadata(),
        };
        
        // プラグインを追加
        let mut plugins = self.plugins.write().await;
        plugins.insert(name, (plugin, metadata));
        
        Ok(())
    }
    
    /// プラグインを取得
    pub async fn get_plugin(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        let mut plugins = self.plugins.write().await;
        
        if let Some((plugin, metadata)) = plugins.get_mut(name) {
            // アクセス情報を更新
            metadata.last_accessed = Instant::now();
            metadata.usage_count += 1;
            
            return Some(plugin.clone());
        }
        
        None
    }
    
    /// 全てのプラグインを取得
    pub async fn get_all_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|(plugin, _)| plugin.clone()).collect()
    }
    
    /// プラグインディレクトリから読み込み
    pub async fn load_from_directory(&self, directory: &Path) -> Result<usize> {
        // プラグインパスに追加
        {
            let mut paths = self.plugin_paths.write().await;
            if !paths.contains(&directory.to_path_buf()) {
                paths.push(directory.to_path_buf());
            }
        }
        
        // プラグイン読み込みロジックを実装
        let mut loaded_count = 0;
        
        // ディレクトリが存在することを確認
        if !directory.exists() || !directory.is_dir() {
            debug!("プラグインディレクトリが存在しないか、ディレクトリではありません: {:?}", directory);
            return Ok(0);
        }
        
        // プラグインファイルをディレクトリから探す
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(e) => {
                debug!("プラグインディレクトリの読み取りに失敗しました: {:?} - {}", directory, e);
                return Err(anyhow::anyhow!("ディレクトリの読み取りに失敗: {}", e));
            }
        };
        
        // 各エントリを処理
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                
                // ライブラリファイルを探す（.so, .dll, .dylib）
                if path.is_file() {
                    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    
                    #[cfg(target_os = "windows")]
                    let is_plugin_file = extension.eq_ignore_ascii_case("dll");
                    
                    #[cfg(target_os = "macos")]
                    let is_plugin_file = extension.eq_ignore_ascii_case("dylib");
                    
                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    let is_plugin_file = extension.eq_ignore_ascii_case("so");
                    
                    if is_plugin_file {
                        debug!("プラグインファイルを発見: {:?}", path);
                        
                        // プラグインをロード
                        match self.load_plugin_from_file(&path).await {
                            Ok(plugin_name) => {
                                debug!("プラグインをロードしました: {} - {:?}", plugin_name, path);
                                loaded_count += 1;
                            }
                            Err(e) => {
                                debug!("プラグインのロードに失敗しました: {:?} - {}", path, e);
                                // エラーをログに記録するが、続行する
                            }
                        }
                    }
                } else if path.is_dir() {
                    // サブディレクトリの場合も再帰的に探索
                    match self.load_from_directory(&path).await {
                        Ok(count) => {
                            loaded_count += count;
                        }
                        Err(e) => {
                            debug!("サブディレクトリからのプラグイン読み込みに失敗: {:?} - {}", path, e);
                            // エラーをログに記録するが、続行する
                        }
                    }
                }
            }
        }
        
        debug!("プラグインディレクトリから読み込み完了: {:?}, ロード済みプラグイン数: {}", directory, loaded_count);
        Ok(loaded_count)
    }
    
    /// プラグインファイルを読み込む
    async fn load_plugin_from_file(&self, path: &Path) -> Result<String> {
        debug!("プラグインファイルをロード: {:?}", path);
        
        // 動的ライブラリを読み込む
        let lib = match unsafe { libloading::Library::new(path) } {
            Ok(lib) => lib,
            Err(e) => {
                return Err(anyhow::anyhow!("プラグインライブラリの読み込みに失敗: {}", e));
            }
        };
        
        // プラグイン初期化関数を探す
        let init_fn: libloading::Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = match unsafe {
            lib.get(b"plugin_create")
        } {
            Ok(func) => func,
            Err(e) => {
                return Err(anyhow::anyhow!("プラグイン初期化関数が見つかりません: {}", e));
            }
        };
        
        // プラグインインスタンスを作成
        let plugin_instance = unsafe {
            let raw_plugin = init_fn();
            if raw_plugin.is_null() {
                return Err(anyhow::anyhow!("プラグインの作成に失敗"));
            }
            Arc::from_raw(raw_plugin)
        };
        
        // プラグイン名を取得
        let plugin_name = plugin_instance.name().to_string();
        
        // プラグインメタデータを作成
        let metadata = PluginMetadata {
            loaded_at: Instant::now(),
            last_accessed: Instant::now(),
            usage_count: 0,
            dependencies: plugin_instance.dependencies().into_iter().collect(),
            metadata: plugin_instance.metadata(),
        };
        
        // プラグインを初期化
        if let Err(e) = plugin_instance.initialize().await {
            return Err(anyhow::anyhow!("プラグインの初期化に失敗: {}", e));
        }
        
        // プラグインを登録
        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(plugin_name.clone(), (plugin_instance, metadata));
        }
        
        Ok(plugin_name)
    }
    
    /// コマンドハンドラーを登録
    pub async fn register_command_handler(&self, handler: Arc<dyn CommandHandler>) -> Result<()> {
        let command = handler.command_name().to_string();
        
        // メインコマンドを登録
        {
            let mut handlers = self.command_handlers.write().await;
            handlers.insert(command.clone(), handler.clone());
        }
        
        // コマンドキャッシュに追加
        self.command_cache.insert(command.clone(), handler.clone());
        
        // エイリアスを登録
        let aliases = handler.aliases();
        if !aliases.is_empty() {
            let mut alias_map = self.command_aliases.write().await;
            for alias in aliases {
                alias_map.insert(alias, command.clone());
                // キャッシュにもエイリアスを追加
                self.command_cache.insert(alias, handler.clone());
            }
        }
        
        Ok(())
    }
    
    /// コマンドハンドラーが存在するか確認
    pub async fn has_command_handler(&self, command: &str) -> bool {
        // まずキャッシュをチェック（最も高速）
        if self.command_cache.contains_key(command) {
            return true;
        }
        
        // キャッシュになければ通常の検索
        let handlers = self.command_handlers.read().await;
        if handlers.contains_key(command) {
            return true;
        }
        
        // エイリアスをチェック
        let aliases = self.command_aliases.read().await;
        if let Some(main_command) = aliases.get(command) {
            let has_command = handlers.contains_key(main_command);
            // キャッシュ更新
            if has_command {
                if let Some(handler) = handlers.get(main_command) {
                    self.command_cache.insert(command.to_string(), handler.clone());
                }
            }
            return has_command;
        }
        
        false
    }
    
    /// コマンドを実行
    pub async fn execute_command(&self, command: &str, args: Vec<String>, context: ExecutionContext) -> Result<PluginCommandResult> {
        // キャッシュからハンドラーを取得（高速パス）
        if let Some(handler) = self.command_cache.get(command) {
            return handler.execute(args, context).await;
        }
        
        // キャッシュにない場合は通常の検索ルート
        let handler = {
            let handlers = self.command_handlers.read().await;
            
            if let Some(handler) = handlers.get(command) {
                handler.clone()
            } else {
                // エイリアスを確認
                let aliases = self.command_aliases.read().await;
                if let Some(main_command) = aliases.get(command) {
                    if let Some(handler) = handlers.get(main_command) {
                        // キャッシュを更新
                        self.command_cache.insert(command.to_string(), handler.clone());
                        handler.clone()
                    } else {
                        return Err(anyhow::anyhow!("コマンドハンドラーが見つかりません: {}", command));
                    }
                } else {
                    return Err(anyhow::anyhow!("コマンドハンドラーが見つかりません: {}", command));
                }
            }
        };
        
        handler.execute(args, context).await
    }
    
    /// 実行フックを登録
    pub async fn register_execution_hook(&self, hook: Arc<dyn ExecutionHook>) {
        let mut hooks = self.execution_hooks.write().await;
        hooks.push(hook);
    }
    
    /// 実行フックが存在するか確認
    pub async fn has_execution_hooks(&self) -> bool {
        let hooks = self.execution_hooks.read().await;
        !hooks.is_empty()
    }
    
    /// コマンド実行前のフック処理
    pub async fn before_command_execution(
        &self,
        command: &str,
        args: &[String],
        context: &ExecutionContext,
    ) -> Result<()> {
        let hooks = self.execution_hooks.read().await;
        for hook in hooks.iter() {
            hook.before_execution(command, args, context).await?;
        }
        Ok(())
    }
    
    /// コマンド実行後のフック処理
    pub async fn after_command_execution(
        &self,
        command: &str,
        args: &[String],
        context: &ExecutionContext,
        result: &ExecutionResult,
    ) -> Result<()> {
        let hooks = self.execution_hooks.read().await;
        for hook in hooks.iter() {
            hook.after_execution(command, args, context, result).await?;
        }
        Ok(())
    }
    
    /// プラグインをアンロード
    pub async fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        
        if let Some((plugin, _)) = plugins.remove(name) {
            // シャットダウン処理を実行
            plugin.shutdown().await?;
            debug!("プラグインをアンロードしました: {}", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("プラグインが見つかりません: {}", name))
        }
    }
    
    /// コマンドハンドラーを削除
    pub async fn unregister_command_handler(&self, command: &str) -> bool {
        let mut removed = false;
        
        // メインハンドラーを削除
        {
            let mut handlers = self.command_handlers.write().await;
            removed = handlers.remove(command).is_some();
        }
        
        // キャッシュから削除
        self.command_cache.remove(command);
        
        // エイリアスからも削除
        {
            let mut aliases = self.command_aliases.write().await;
            let alias_keys: Vec<String> = aliases.iter()
                .filter(|(_, cmd)| *cmd == command)
                .map(|(alias, _)| alias.clone())
                .collect();
            
            for alias in alias_keys {
                aliases.remove(&alias);
                self.command_cache.remove(&alias);
            }
        }
        
        removed
    }
    
    /// キャッシュを最適化
    pub async fn optimize_cache(&self) {
        // 未使用のキャッシュエントリを削除
        let handlers = self.command_handlers.read().await;
        let aliases = self.command_aliases.read().await;
        
        let valid_commands: HashSet<String> = handlers.keys().cloned().collect();
        let valid_aliases: HashSet<String> = aliases.keys().cloned().collect();
        
        // キャッシュ内の無効なエントリを削除
        let cached_keys: Vec<String> = self.command_cache.iter().map(|entry| entry.key().clone()).collect();
        
        for key in cached_keys {
            if !valid_commands.contains(&key) && !valid_aliases.contains(&key) {
                self.command_cache.remove(&key);
            }
        }
    }
    
    /// プラグイン統計情報を取得
    pub async fn get_plugin_stats(&self) -> HashMap<String, PluginStats> {
        let plugins = self.plugins.read().await;
        let mut stats = HashMap::new();
        
        for (name, (plugin, metadata)) in plugins.iter() {
            stats.insert(name.clone(), PluginStats {
                name: name.clone(),
                version: plugin.version().to_string(),
                loaded_at: metadata.loaded_at,
                last_accessed: metadata.last_accessed,
                usage_count: metadata.usage_count,
                uptime: metadata.loaded_at.elapsed(),
            });
        }
        
        stats
    }
}

/// プラグイン統計情報
#[derive(Debug, Clone)]
pub struct PluginStats {
    /// プラグイン名
    pub name: String,
    /// バージョン
    pub version: String,
    /// 読み込み時間
    pub loaded_at: Instant,
    /// 最終アクセス時間
    pub last_accessed: Instant,
    /// 使用回数
    pub usage_count: usize,
    /// 稼働時間
    pub uptime: Duration,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}