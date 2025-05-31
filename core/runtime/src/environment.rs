/*!
# 高性能環境変数管理モジュール

シェルの環境変数を管理する最先端の実装を提供します。
スコープベースの変数管理、名前空間機能、変更の監視とトレースなど
高度な機能を備えています。
*/

use anyhow::{Result, anyhow, Context};
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn, trace};
use std::time::Instant;
use dirs;

/// 環境変数の変更イベント
#[derive(Debug, Clone)]
pub enum EnvEvent {
    /// 変数の設定
    Set {
        /// 変数名
        name: String,
        /// 新しい値
        value: String,
        /// 前の値（設定されていなかった場合はNone）
        previous: Option<String>,
        /// 名前空間
        namespace: Option<String>,
    },
    /// 変数の削除
    Unset {
        /// 変数名
        name: String,
        /// 前の値
        previous: Option<String>,
        /// 名前空間
        namespace: Option<String>,
    },
    /// すべての変数のクリア
    Clear {
        /// 名前空間
        namespace: Option<String>,
    },
    /// 名前空間の追加
    AddNamespace {
        /// 名前空間名
        name: String,
    },
    /// 名前空間の削除
    RemoveNamespace {
        /// 名前空間名
        name: String,
    },
}

/// 環境変数の変更をサブスクライブするリスナー
#[derive(Debug, Clone)]
pub struct EnvListener {
    /// 受信チャネル
    receiver: broadcast::Receiver<EnvEvent>,
}

impl EnvListener {
    /// 次のイベントを受信
    pub async fn next(&mut self) -> Option<EnvEvent> {
        self.receiver.recv().await.ok()
    }
    
    /// 受信チャネルをクローン
    pub fn clone_receiver(&self) -> broadcast::Receiver<EnvEvent> {
        self.receiver.resubscribe()
    }
}

/// 環境変数マネージャー
#[derive(Debug)]
pub struct EnvManager {
    /// グローバル変数
    global_vars: Arc<RwLock<HashMap<String, String>>>,
    /// 名前空間変数
    namespace_vars: Arc<DashMap<String, HashMap<String, String>>>,
    /// 変更通知チャネル
    event_sender: broadcast::Sender<EnvEvent>,
    /// 変数の優先順位
    priority_order: Arc<RwLock<Vec<String>>>,
    /// エクスポートされた変数
    exported_vars: Arc<RwLock<HashSet<String>>>,
    /// キャッシュ
    cache: Arc<DashMap<String, (String, Instant)>>,
    /// キャッシュの有効期間（ミリ秒）
    cache_ttl_ms: u64,
}

impl EnvManager {
    /// 新しい環境変数マネージャーを作成
    pub fn new() -> Self {
        // 現在のプロセスから環境変数を初期化
        let mut global_vars = HashMap::new();
        for (key, value) in std::env::vars() {
            global_vars.insert(key, value);
        }
        
        let (sender, _) = broadcast::channel(100);
        
        Self {
            global_vars: Arc::new(RwLock::new(global_vars)),
            namespace_vars: Arc::new(DashMap::new()),
            event_sender: sender,
            priority_order: Arc::new(RwLock::new(Vec::new())),
            exported_vars: Arc::new(RwLock::new(HashSet::new())),
            cache: Arc::new(DashMap::new()),
            cache_ttl_ms: 5000, // デフォルト5秒
        }
    }
    
    /// リスナーを作成
    pub fn create_listener(&self) -> EnvListener {
        EnvListener {
            receiver: self.event_sender.subscribe(),
        }
    }
    
    /// 変数を設定
    pub async fn set(&self, name: &str, value: &str, namespace: Option<&str>) -> Result<()> {
        // 名前の検証
        if name.is_empty() {
            return Err(anyhow!("環境変数名が空です"));
        }
        
        // 変数名が有効な形式かをチェック
        if !Self::is_valid_var_name(name) {
            return Err(anyhow!("無効な環境変数名: {}", name));
        }
        
        // キャッシュをクリア
        self.cache.remove(name);
        
        match namespace {
            Some(ns) => {
                // 名前空間内の変数を設定
                let previous = if let Some(mut ns_vars) = self.namespace_vars.get_mut(ns) {
                    ns_vars.insert(name.to_string(), value.to_string())
                } else {
                    // 名前空間が存在しない場合は作成
                    let mut vars = HashMap::new();
                    vars.insert(name.to_string(), value.to_string());
                    self.namespace_vars.insert(ns.to_string(), vars);
                    None
                };
                
                // イベントを発行
                let _ = self.event_sender.send(EnvEvent::Set {
                    name: name.to_string(),
                    value: value.to_string(),
                    previous,
                    namespace: Some(ns.to_string()),
                });
                
                debug!("名前空間 {} に環境変数 {} = {} を設定しました", ns, name, value);
            },
            None => {
                // グローバル変数を設定
                let mut globals = self.global_vars.write().await;
                let previous = globals.insert(name.to_string(), value.to_string());
                
                // イベントを発行
                let _ = self.event_sender.send(EnvEvent::Set {
                    name: name.to_string(),
                    value: value.to_string(),
                    previous,
                    namespace: None,
                });
                
                // エクスポート済みの場合は実際の環境変数も設定
                let exported = {
                    let exported_vars = self.exported_vars.read().await;
                    exported_vars.contains(name)
                };
                
                if exported {
                    std::env::set_var(name, value);
                }
                
                debug!("グローバル環境変数 {} = {} を設定しました", name, value);
            }
        }
        
        Ok(())
    }
    
    /// 変数を取得
    pub async fn get(&self, name: &str) -> Option<String> {
        // キャッシュをチェック
        if let Some(cached) = self.cache.get(name) {
            let now = Instant::now();
            let cache_age = now.duration_since(cached.1).as_millis();
            
            if cache_age < self.cache_ttl_ms as u128 {
                trace!("環境変数キャッシュヒット: {} = {}", name, cached.0);
                return Some(cached.0.clone());
            }
        }
        
        // 優先順位に従って名前空間から変数を検索
        let priorities = self.priority_order.read().await.clone();
        for ns in &priorities {
            if let Some(ns_vars) = self.namespace_vars.get(ns) {
                if let Some(value) = ns_vars.get(name) {
                    // キャッシュに追加
                    self.cache.insert(name.to_string(), (value.clone(), Instant::now()));
                    return Some(value.clone());
                }
            }
        }
        
        // グローバル変数から検索
        let globals = self.global_vars.read().await;
        if let Some(value) = globals.get(name) {
            // キャッシュに追加
            self.cache.insert(name.to_string(), (value.clone(), Instant::now()));
            return Some(value.clone());
        }
        
        None
    }
    
    /// 変数の存在を確認
    pub async fn exists(&self, name: &str) -> bool {
        self.get(name).await.is_some()
    }
    
    /// 変数を削除
    pub async fn unset(&self, name: &str, namespace: Option<&str>) -> Result<bool> {
        // キャッシュをクリア
        self.cache.remove(name);
        
        match namespace {
            Some(ns) => {
                // 名前空間内の変数を削除
                let mut removed = false;
                let previous = if let Some(mut ns_vars) = self.namespace_vars.get_mut(ns) {
                    let previous = ns_vars.remove(name);
                    removed = previous.is_some();
                    previous
                } else {
                    None
                };
                
                // イベントを発行
                if removed {
                    let _ = self.event_sender.send(EnvEvent::Unset {
                        name: name.to_string(),
                        previous,
                        namespace: Some(ns.to_string()),
                    });
                    
                    debug!("名前空間 {} から環境変数 {} を削除しました", ns, name);
                }
                
                Ok(removed)
            },
            None => {
                // グローバル変数を削除
                let mut globals = self.global_vars.write().await;
                let previous = globals.remove(name);
                
                if previous.is_some() {
                    // イベントを発行
                    let _ = self.event_sender.send(EnvEvent::Unset {
                        name: name.to_string(),
                        previous,
                        namespace: None,
                    });
                    
                    // エクスポート済みリストからも削除
                    let mut exported_vars = self.exported_vars.write().await;
                    exported_vars.remove(name);
                    
                    // 実際の環境変数も削除
                    std::env::remove_var(name);
                    
                    debug!("グローバル環境変数 {} を削除しました", name);
                    
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }
    
    /// 名前空間を追加
    pub async fn add_namespace(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow!("名前空間名が空です"));
        }
        
        // 名前空間が存在しない場合のみ追加
        if !self.namespace_vars.contains_key(name) {
            self.namespace_vars.insert(name.to_string(), HashMap::new());
            
            // 優先順位リストの先頭に追加
            let mut priorities = self.priority_order.write().await;
            priorities.insert(0, name.to_string());
            
            // イベントを発行
            let _ = self.event_sender.send(EnvEvent::AddNamespace {
                name: name.to_string(),
            });
            
            debug!("名前空間 {} を追加しました", name);
        }
        
        Ok(())
    }
    
    /// 名前空間を削除
    pub async fn remove_namespace(&self, name: &str) -> Result<bool> {
        // 名前空間を削除
        let removed = self.namespace_vars.remove(name).is_some();
        
        if removed {
            // 優先順位リストからも削除
            let mut priorities = self.priority_order.write().await;
            priorities.retain(|ns| ns != name);
            
            // イベントを発行
            let _ = self.event_sender.send(EnvEvent::RemoveNamespace {
                name: name.to_string(),
            });
            
            // キャッシュをクリア
            self.cache.clear();
            
            debug!("名前空間 {} を削除しました", name);
        }
        
        Ok(removed)
    }
    
    /// 名前空間の優先順位を設定
    pub async fn set_namespace_priority(&self, order: &[String]) -> Result<()> {
        // 全ての名前空間が存在するか検証
        for ns in order {
            if !self.namespace_vars.contains_key(ns) {
                return Err(anyhow!("名前空間 {} が存在しません", ns));
            }
        }
        
        // 優先順位を更新
        let mut priorities = self.priority_order.write().await;
        *priorities = order.to_vec();
        
        // キャッシュをクリア
        self.cache.clear();
        
        debug!("名前空間の優先順位を更新しました: {:?}", order);
        
        Ok(())
    }
    
    /// 変数をエクスポート
    pub async fn export(&self, name: &str) -> Result<()> {
        // グローバル変数を確認
        let globals = self.global_vars.read().await;
        
        if let Some(value) = globals.get(name) {
            // エクスポート済みリストに追加
            let mut exported_vars = self.exported_vars.write().await;
            exported_vars.insert(name.to_string());
            
            // 実際の環境変数に設定
            std::env::set_var(name, value);
            
            debug!("環境変数 {} をエクスポートしました", name);
            
            Ok(())
        } else {
            Err(anyhow!("環境変数 {} が存在しません", name))
        }
    }
    
    /// すべての変数をクリア
    pub async fn clear(&self, namespace: Option<&str>) -> Result<()> {
        // キャッシュをクリア
        self.cache.clear();
        
        match namespace {
            Some(ns) => {
                // 名前空間内の変数をクリア
                if let Some(mut ns_vars) = self.namespace_vars.get_mut(ns) {
                    ns_vars.clear();
                    
                    // イベントを発行
                    let _ = self.event_sender.send(EnvEvent::Clear {
                        namespace: Some(ns.to_string()),
                    });
                    
                    debug!("名前空間 {} の環境変数をクリアしました", ns);
                }
            },
            None => {
                // グローバル変数をクリア
                let mut globals = self.global_vars.write().await;
                globals.clear();
                
                // エクスポート済みリストもクリア
                let mut exported_vars = self.exported_vars.write().await;
                
                // エクスポートされた環境変数を削除
                for var in exported_vars.iter() {
                    std::env::remove_var(var);
                }
                
                exported_vars.clear();
                
                // イベントを発行
                let _ = self.event_sender.send(EnvEvent::Clear {
                    namespace: None,
                });
                
                debug!("グローバル環境変数をクリアしました");
            }
        }
        
        Ok(())
    }
    
    /// PATHを追加
    pub async fn add_to_path(&self, path: &Path, prepend: bool) -> Result<()> {
        // 現在のPATHを取得
        let current_path = match self.get("PATH").await {
            Some(p) => p,
            None => String::new(),
        };
        
        let path_str = path.to_string_lossy().to_string();
        
        // パス区切り文字
        #[cfg(target_os = "windows")]
        let separator = ";";
        #[cfg(not(target_os = "windows"))]
        let separator = ":";
        
        // パス要素を分割
        let mut path_elements: Vec<String> = current_path
            .split(separator)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        // すでに含まれているか確認
        if !path_elements.contains(&path_str) {
            // 先頭または末尾に追加
            if prepend {
                path_elements.insert(0, path_str);
            } else {
                path_elements.push(path_str);
            }
            
            // 新しいPATHを作成
            let new_path = path_elements.join(separator);
            
            // PATHを更新
            self.set("PATH", &new_path, None).await?;
            
            // PATHをエクスポート
            self.export("PATH").await?;
            
            debug!("PATH を更新しました: {}", new_path);
        }
        
        Ok(())
    }
    
    /// 変数を一括設定
    pub async fn set_many(&self, vars: &HashMap<String, String>, namespace: Option<&str>) -> Result<()> {
        for (name, value) in vars {
            self.set(name, value, namespace).await?;
        }
        
        Ok(())
    }
    
    /// スクリプトから変数をロード
    pub async fn load_from_file(&self, path: &Path, namespace: Option<&str>) -> Result<usize> {
        // ファイルを読み込む
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("ファイルの読み込みに失敗: {:?}", path))?;
        
        let mut count = 0;
        
        // 各行を処理
        for line in content.lines() {
            let line = line.trim();
            
            // コメントや空行をスキップ
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            // 環境変数の定義を解析
            if let Some(idx) = line.find('=') {
                let name = line[..idx].trim();
                let value = line[idx+1..].trim();
                
                // クォートがある場合は除去
                let value = Self::unquote(value);
                
                if !name.is_empty() {
                    // 変数を設定
                    if let Ok(()) = self.set(name, &value, namespace).await {
                        count += 1;
                    }
                }
            }
        }
        
        debug!("ファイル {:?} から {} 個の環境変数をロードしました", path, count);
        
        Ok(count)
    }
    
    /// キャッシュのTTLを設定
    pub fn set_cache_ttl(&mut self, ms: u64) {
        self.cache_ttl_ms = ms;
        debug!("環境変数キャッシュのTTLを {} ミリ秒に設定しました", ms);
    }
    
    /// キャッシュをクリア
    pub fn clear_cache(&self) {
        self.cache.clear();
        debug!("環境変数キャッシュをクリアしました");
    }
    
    /// 変数名が有効かチェック
    fn is_valid_var_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        
        // 最初の文字はアルファベットまたはアンダースコア
        let first_char = name.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return false;
        }
        
        // 残りの文字はアルファベット、数字、アンダースコア
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
    
    /// 文字列からクォートを除去
    fn unquote(s: &str) -> String {
        let s = s.trim();
        
        if (s.starts_with('"') && s.ends_with('"')) ||
           (s.starts_with('\'') && s.ends_with('\'')) {
            s[1..s.len()-1].to_string()
        } else {
            s.to_string()
        }
    }

    pub fn export_var(&mut self, name: &str, value: &str) {
        // プロセス内
        self.global_vars.write().await.insert(name.to_string(), value.to_string());
        // OS環境変数
        std::env::set_var(name, value);
        // 親シェル通知用（CommandResult等で返却する設計を想定）
        self.exported_vars.write().await.insert(name.to_string(), value.to_string());
        // 設定ファイルにも永続化
        if let Some(home) = dirs::home_dir() {
            let env_path = home.join(".nexusshell/env");
            let _ = std::fs::write(&env_path, self.global_vars.read().await.iter().map(|(k,v)| format!("{}={}",k,v)).collect::<Vec<_>>().join("\n"));
        }
    }
    pub fn unset_var(&mut self, name: &str) {
        self.global_vars.write().await.remove(name);
        std::env::remove_var(name);
        self.exported_vars.write().await.remove(name);
        if let Some(home) = dirs::home_dir() {
            let env_path = home.join(".nexusshell/env");
            let _ = std::fs::write(&env_path, self.global_vars.read().await.iter().map(|(k,v)| format!("{}={}",k,v)).collect::<Vec<_>>().join("\n"));
        }
    }
}

impl Default for EnvManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_set_get() {
        let env = EnvManager::new();
        
        // 変数を設定
        env.set("TEST_VAR", "test_value", None).await.unwrap();
        
        // 変数を取得
        let value = env.get("TEST_VAR").await.unwrap();
        assert_eq!(value, "test_value");
    }
    
    #[tokio::test]
    async fn test_namespace() {
        let env = EnvManager::new();
        
        // 名前空間を追加
        env.add_namespace("test_ns").await.unwrap();
        
        // 名前空間に変数を設定
        env.set("NS_VAR", "ns_value", Some("test_ns")).await.unwrap();
        
        // グローバルに同名の変数を設定
        env.set("NS_VAR", "global_value", None).await.unwrap();
        
        // 優先順位により名前空間の値が取得される
        let value = env.get("NS_VAR").await.unwrap();
        assert_eq!(value, "ns_value");
        
        // 名前空間を削除
        env.remove_namespace("test_ns").await.unwrap();
        
        // グローバルの値が取得される
        let value = env.get("NS_VAR").await.unwrap();
        assert_eq!(value, "global_value");
    }
    
    #[tokio::test]
    async fn test_events() {
        let env = EnvManager::new();
        let mut listener = env.create_listener();
        
        // バックグラウンドでイベントを監視
        let events_task = tokio::spawn(async move {
            let mut events = Vec::new();
            for _ in 0..3 {
                if let Some(event) = listener.next().await {
                    events.push(event);
                }
            }
            events
        });
        
        // 変数を設定
        env.set("EVENT_TEST", "value1", None).await.unwrap();
        
        // 値を変更
        env.set("EVENT_TEST", "value2", None).await.unwrap();
        
        // 変数を削除
        env.unset("EVENT_TEST", None).await.unwrap();
        
        // イベントを取得
        let events = events_task.await.unwrap();
        
        // イベントのタイプを確認
        assert_eq!(events.len(), 3);
        
        match &events[0] {
            EnvEvent::Set { name, value, .. } => {
                assert_eq!(name, "EVENT_TEST");
                assert_eq!(value, "value1");
            },
            _ => panic!("Unexpected event type"),
        }
        
        match &events[1] {
            EnvEvent::Set { name, value, previous, .. } => {
                assert_eq!(name, "EVENT_TEST");
                assert_eq!(value, "value2");
                assert_eq!(previous, &Some("value1".to_string()));
            },
            _ => panic!("Unexpected event type"),
        }
        
        match &events[2] {
            EnvEvent::Unset { name, previous, .. } => {
                assert_eq!(name, "EVENT_TEST");
                assert_eq!(previous, &Some("value2".to_string()));
            },
            _ => panic!("Unexpected event type"),
        }
    }
    
    #[tokio::test]
    async fn test_export() {
        let env = EnvManager::new();
        
        // テスト用のユニークな変数名を作成
        let var_name = format!("TEST_EXPORT_{}", std::process::id());
        
        // 変数を設定
        env.set(&var_name, "exported_value", None).await.unwrap();
        
        // 変数をエクスポート
        env.export(&var_name).await.unwrap();
        
        // 実際の環境変数を確認
        let actual_value = std::env::var(&var_name).unwrap();
        assert_eq!(actual_value, "exported_value");
        
        // クリーンアップ
        env.unset(&var_name, None).await.unwrap();
    }
} 