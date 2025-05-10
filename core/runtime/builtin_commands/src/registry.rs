use crate::BuiltinCommand;
use anyhow::{Result, anyhow};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// 組み込みコマンドのレジストリ
/// 
/// このレジストリは利用可能な全ての組み込みコマンドを管理し、
/// 名前によるコマンドの検索と実行を可能にします。
/// スレッド安全な設計になっており、並行アクセスが可能です。
#[derive(Default, Clone)]
pub struct CommandRegistry {
    /// コマンド名からコマンド実装へのマッピング
    commands: Arc<DashMap<String, Arc<dyn BuiltinCommand>>>,
    
    /// エイリアス名から実際のコマンド名へのマッピング
    aliases: Arc<DashMap<String, String>>,
}

impl CommandRegistry {
    /// 新しいコマンドレジストリを作成
    pub fn new() -> Self {
        Self {
            commands: Arc::new(DashMap::new()),
            aliases: Arc::new(DashMap::new()),
        }
    }

    /// コマンドをレジストリに登録
    pub fn register(&mut self, command: Box<dyn BuiltinCommand>) {
        let name = command.name().to_string();
        debug!("コマンド '{}' をレジストリに登録しています", name);
        
        // Arc<dyn BuiltinCommand>としてコマンドを保存
        self.commands.insert(name, Arc::from(command));
    }

    /// エイリアスを登録
    pub fn register_alias(&mut self, alias: &str, command_name: &str) -> Result<()> {
        // 元のコマンドが存在するか確認
        if !self.commands.contains_key(command_name) {
            return Err(anyhow!("コマンド '{}' が見つかりません", command_name));
        }
        
        debug!("エイリアス '{}' -> '{}' を登録しています", alias, command_name);
        self.aliases.insert(alias.to_string(), command_name.to_string());
        Ok(())
    }

    /// エイリアスを削除
    pub fn remove_alias(&mut self, alias: &str) -> bool {
        debug!("エイリアス '{}' を削除しています", alias);
        self.aliases.remove(alias).is_some()
    }

    /// コマンドをレジストリから削除
    pub fn unregister(&mut self, name: &str) -> bool {
        debug!("コマンド '{}' の登録を解除しています", name);
        
        // このコマンドを指すエイリアスを全て削除
        let aliases_to_remove: Vec<String> = self.aliases
            .iter()
            .filter_map(|entry| {
                if entry.value() == name {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();
        
        for alias in aliases_to_remove {
            self.aliases.remove(&alias);
        }
        
        // コマンド自体を削除
        self.commands.remove(name).is_some()
    }

    /// 名前またはエイリアスからコマンドを取得
    pub fn get_command(&self, name: &str) -> Option<Arc<dyn BuiltinCommand>> {
        // まずコマンドを直接検索
        if let Some(command) = self.commands.get(name) {
            return Some(command.clone());
        }
        
        // エイリアスをチェック
        if let Some(actual_name) = self.aliases.get(name) {
            if let Some(command) = self.commands.get(&*actual_name) {
                return Some(command.clone());
            }
        }
        
        None
    }

    /// レジストリ内の全てのコマンド名を取得
    pub fn list_commands(&self) -> Vec<String> {
        self.commands
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// レジストリ内の全てのエイリアスを取得
    pub fn list_aliases(&self) -> Vec<(String, String)> {
        self.aliases
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// レジストリをマージ
    pub fn merge(&mut self, other: &CommandRegistry) {
        // 他のレジストリからコマンドをコピー
        for entry in other.commands.iter() {
            self.commands.insert(entry.key().clone(), entry.value().clone());
        }
        
        // 他のレジストリからエイリアスをコピー
        for entry in other.aliases.iter() {
            self.aliases.insert(entry.key().clone(), entry.value().clone());
        }
    }

    /// 特定のプレフィックスで始まるコマンドを検索
    pub fn search_commands(&self, prefix: &str) -> Vec<String> {
        self.commands
            .iter()
            .filter_map(|entry| {
                if entry.key().starts_with(prefix) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// コマンドの詳細情報を取得
    pub fn get_command_details(&self, name: &str) -> Option<CommandDetails> {
        let command = self.get_command(name)?;
        
        // このコマンドを指すエイリアスを検索
        let aliases: Vec<String> = self.aliases
            .iter()
            .filter_map(|entry| {
                if entry.value() == name {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();
        
        Some(CommandDetails {
            name: name.to_string(),
            description: command.description().to_string(),
            usage: command.usage().to_string(),
            aliases,
        })
    }
}

/// コマンドの詳細情報を表すデータ構造体
#[derive(Debug, Clone)]
pub struct CommandDetails {
    /// コマンド名
    pub name: String,
    /// コマンドの説明
    pub description: String,
    /// 使用方法
    pub usage: String,
    /// コマンドのエイリアス
    pub aliases: Vec<String>,
} 