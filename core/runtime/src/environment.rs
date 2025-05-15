/*!
# 環境変数モジュール

シェル環境の環境変数を管理するモジュールです。システム環境変数と
シェル固有の環境変数を統合的に管理し、変数の設定・取得・削除などの
機能を提供します。
*/

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// 環境変数の変更を通知するためのリスナー
pub trait EnvironmentListener: Send + Sync {
    /// 環境変数が設定されたときに呼び出される
    fn on_set(&self, name: &str, value: &str);
    
    /// 環境変数が削除されたときに呼び出される
    fn on_unset(&self, name: &str);
}

/// 環境変数管理クラス
pub struct Environment {
    /// シェル固有の環境変数
    variables: DashMap<String, String>,
    
    /// 環境変数のリスナー
    listeners: RwLock<Vec<Arc<dyn EnvironmentListener>>>,
    
    /// パス区切り文字（Windowsは`;`、それ以外は`:`）
    path_separator: char,
}

impl Environment {
    /// 新しい環境変数マネージャを作成
    pub fn new() -> Self {
        // システムの環境変数を初期値として使用
        let system_vars: HashMap<String, String> = env::vars().collect();
        let variables = DashMap::new();
        
        // システム環境変数をインポート
        for (key, value) in system_vars {
            variables.insert(key, value);
        }
        
        // パス区切り文字を決定
        #[cfg(windows)]
        let path_separator = ';';
        #[cfg(not(windows))]
        let path_separator = ':';
        
        Self {
            variables,
            listeners: RwLock::new(Vec::new()),
            path_separator,
        }
    }
    
    /// 環境変数の値を取得
    pub fn get(&self, name: &str) -> Option<String> {
        self.variables.get(name).map(|value| value.clone())
    }
    
    /// 環境変数が存在するかを確認
    pub fn contains(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }
    
    /// 環境変数を設定
    pub async fn set(&self, name: &str, value: &str) {
        // 環境変数を更新
        self.variables.insert(name.to_string(), value.to_string());
        
        // リスナーに通知
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_set(name, value);
        }
        
        debug!("環境変数を設定: {}={}", name, value);
    }
    
    /// 環境変数を削除
    pub async fn unset(&self, name: &str) -> bool {
        // 環境変数を削除
        let removed = self.variables.remove(name).is_some();
        
        // リスナーに通知
        if removed {
            let listeners = self.listeners.read().await;
            for listener in listeners.iter() {
                listener.on_unset(name);
            }
            
            debug!("環境変数を削除: {}", name);
        }
        
        removed
    }
    
    /// 全ての環境変数を取得
    pub fn get_all(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        
        for item in self.variables.iter() {
            result.insert(item.key().clone(), item.value().clone());
        }
        
        result
    }
    
    /// PATHに新しいディレクトリを追加
    pub async fn add_to_path(&self, dir: &Path) -> Result<()> {
        if !dir.is_dir() {
            return Err(anyhow!("指定されたパスはディレクトリではありません: {:?}", dir));
        }
        
        let path_var = if cfg!(windows) { "Path" } else { "PATH" };
        let current_path = self.get(path_var).unwrap_or_default();
        
        // ディレクトリを文字列に変換
        let dir_str = dir.to_str()
            .ok_or_else(|| anyhow!("パスを文字列に変換できません: {:?}", dir))?;
        
        // 新しいPATHを構築
        let paths: Vec<&str> = current_path.split(self.path_separator).collect();
        
        // すでに含まれているか確認
        if paths.contains(&dir_str) {
            return Ok(());
        }
        
        // 新しいパスを構築
        let mut new_paths = paths.clone();
        new_paths.push(dir_str);
        let new_path = new_paths.join(&self.path_separator.to_string());
        
        // 環境変数を更新
        self.set(path_var, &new_path).await;
        
        Ok(())
    }
    
    /// PATHからディレクトリを削除
    pub async fn remove_from_path(&self, dir: &Path) -> Result<()> {
        let path_var = if cfg!(windows) { "Path" } else { "PATH" };
        let current_path = self.get(path_var).unwrap_or_default();
        
        // ディレクトリを文字列に変換
        let dir_str = dir.to_str()
            .ok_or_else(|| anyhow!("パスを文字列に変換できません: {:?}", dir))?;
        
        // パスを分割
        let paths: Vec<&str> = current_path.split(self.path_separator).collect();
        
        // 指定されたディレクトリを除外
        let new_paths: Vec<&str> = paths.into_iter()
            .filter(|&p| p != dir_str)
            .collect();
        
        // 新しいパスを構築
        let new_path = new_paths.join(&self.path_separator.to_string());
        
        // 環境変数を更新
        self.set(path_var, &new_path).await;
        
        Ok(())
    }
    
    /// PATHの内容を取得
    pub fn get_path(&self) -> Vec<PathBuf> {
        let path_var = if cfg!(windows) { "Path" } else { "PATH" };
        let path_value = self.get(path_var).unwrap_or_default();
        
        path_value.split(self.path_separator)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect()
    }
    
    /// 環境変数リスナーを追加
    pub async fn add_listener(&self, listener: Arc<dyn EnvironmentListener>) {
        let mut listeners = self.listeners.write().await;
        listeners.push(listener);
    }
    
    /// 環境変数リスナーを削除
    pub async fn remove_listener(&self, listener: &Arc<dyn EnvironmentListener>) {
        let mut listeners = self.listeners.write().await;
        
        // ポインタ比較でリスナーを検索し削除
        if let Some(index) = listeners.iter().position(|l| Arc::ptr_eq(l, listener)) {
            listeners.remove(index);
        }
    }
    
    /// ホームディレクトリのパスを取得
    pub fn get_home_dir(&self) -> Option<PathBuf> {
        #[cfg(windows)]
        {
            self.get("USERPROFILE").map(PathBuf::from)
        }
        
        #[cfg(not(windows))]
        {
            self.get("HOME").map(PathBuf::from)
        }
    }
    
    /// ホームディレクトリを基準にしたパスを絶対パスに変換
    pub fn expand_tilde(&self, path: &str) -> PathBuf {
        if path.starts_with("~") {
            if let Some(home) = self.get_home_dir() {
                if path.len() == 1 {
                    return home;
                } else if path.starts_with("~/") || path.starts_with("~\\") {
                    let path_remainder = &path[2..];
                    return home.join(path_remainder);
                }
            }
        }
        
        PathBuf::from(path)
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct TestListener {
        set_count: AtomicUsize,
        unset_count: AtomicUsize,
    }
    
    impl EnvironmentListener for TestListener {
        fn on_set(&self, _name: &str, _value: &str) {
            self.set_count.fetch_add(1, Ordering::SeqCst);
        }
        
        fn on_unset(&self, _name: &str) {
            self.unset_count.fetch_add(1, Ordering::SeqCst);
        }
    }
    
    #[tokio::test]
    async fn test_environment_basics() {
        let env = Environment::new();
        
        // 変数設定と取得
        env.set("TEST_VAR", "test_value").await;
        assert_eq!(env.get("TEST_VAR"), Some("test_value".to_string()));
        
        // 変数削除
        assert!(env.unset("TEST_VAR").await);
        assert_eq!(env.get("TEST_VAR"), None);
        
        // 存在しない変数の削除
        assert!(!env.unset("NON_EXISTENT").await);
    }
    
    #[tokio::test]
    async fn test_environment_listeners() {
        let env = Environment::new();
        
        let listener = Arc::new(TestListener {
            set_count: AtomicUsize::new(0),
            unset_count: AtomicUsize::new(0),
        });
        
        env.add_listener(listener.clone()).await;
        
        // 変数設定と削除
        env.set("TEST_VAR", "test_value").await;
        env.unset("TEST_VAR").await;
        
        assert_eq!(listener.set_count.load(Ordering::SeqCst), 1);
        assert_eq!(listener.unset_count.load(Ordering::SeqCst), 1);
        
        // リスナー削除
        env.remove_listener(&listener).await;
        
        env.set("TEST_VAR2", "test_value").await;
        assert_eq!(listener.set_count.load(Ordering::SeqCst), 1); // 変化なし
    }
    
    #[tokio::test]
    async fn test_path_manipulation() {
        let env = Environment::new();
        let test_dir = PathBuf::from("/tmp/test_dir");
        
        // PATHに追加
        env.set("PATH", "/usr/bin:/bin").await;
        env.add_to_path(&test_dir).await.unwrap();
        
        let paths = env.get_path();
        assert!(paths.contains(&test_dir));
        
        // PATHから削除
        env.remove_from_path(&test_dir).await.unwrap();
        let paths = env.get_path();
        assert!(!paths.contains(&test_dir));
    }
} 