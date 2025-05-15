/*!
# プラグインモジュール

シェルの拡張機能を管理するプラグインシステムを提供します。
*/

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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
}

/// プラグインマネージャ
pub struct PluginManager {
    /// 読み込まれたプラグイン
    plugins: HashMap<String, Arc<dyn Plugin>>,
}

impl PluginManager {
    /// 新しいプラグインマネージャを作成
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }
    
    /// プラグインを登録
    pub fn register_plugin(&mut self, plugin: Arc<dyn Plugin>) -> Result<()> {
        let name = plugin.name().to_string();
        self.plugins.insert(name, plugin);
        Ok(())
    }
    
    /// プラグインを取得
    pub fn get_plugin(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.get(name).cloned()
    }
    
    /// 全てのプラグインを取得
    pub fn get_all_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.plugins.values().cloned().collect()
    }
    
    /// プラグインディレクトリから読み込み（将来的に実装）
    pub async fn load_from_directory(&mut self, _directory: &Path) -> Result<()> {
        // 将来的に実装
        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}