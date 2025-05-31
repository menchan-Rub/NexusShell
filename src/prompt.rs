use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use colored::Colorize;
use chrono::Local;
use anyhow::Result;
use std::process::Command;
use console::measure_text_width;
use std::io::{self, Write};

use crate::config::PromptConfig;

/// プロンプトマネージャー
pub struct PromptManager {
    /// 設定
    config: PromptConfig,
    /// 前回のコマンド実行時間
    last_command_time: Option<Duration>,
    /// 前回のコマンド実行開始時刻
    last_command_start: Option<Instant>,
    /// キャッシュされたGitステータス
    cached_git_status: Option<(String, Instant)>,
    /// キャッシュの有効期間（秒）
    cache_ttl: u64,
}

impl PromptManager {
    /// 新しいプロンプトマネージャーを作成します
    pub fn new(config: &PromptConfig) -> Self {
        Self {
            config: config.clone(),
            last_command_time: None,
            last_command_start: None,
            cached_git_status: None,
            cache_ttl: 2, // 2秒間キャッシュ有効
        }
    }
    
    /// プロンプトを取得します
    pub fn get_prompt(&self) -> String {
        let prompt = self.format_prompt(&self.config.format);
        
        // 右側プロンプトがある場合は処理
        if let Some(right_format) = &self.config.right_format {
            let right_prompt = self.format_prompt(right_format);
            let term_width = term_size::dimensions().map(|(w,_)| w).unwrap_or(80);
            let left_width = measure_text_width(&prompt);
            let right_width = measure_text_width(&right_prompt);
            let space = if term_width > left_width + right_width {
                term_width - left_width - right_width
            } else { 1 };
            let mut stdout = io::stdout();
            write!(stdout, "{}{}{}", prompt, " ".repeat(space), right_prompt.dimmed()).unwrap();
            stdout.flush().unwrap();
            String::new() // すでに表示済みなので空文字返す
        } else {
            prompt
        }
    }
    
    /// プロンプトをフォーマットします
    fn format_prompt(&self, format_str: &str) -> String {
        let mut result = format_str.to_string();
        
        // 基本的な変数の置換
        if let Ok(username) = env::var("USER") {
            result = result.replace("\\u", &username);
        }
        
        if let Ok(hostname) = hostname::get() {
            let host = hostname.to_string_lossy();
            result = result.replace("\\h", &host);
        }
        
        // 現在のディレクトリ
        if let Ok(cwd) = env::current_dir() {
            let full_path = cwd.to_string_lossy();
            result = result.replace("\\w", &full_path);
            
            // ディレクトリ名のみ
            if let Some(dir_name) = cwd.file_name() {
                result = result.replace("\\W", &dir_name.to_string_lossy());
            }
        }
        
        // ホームディレクトリからの相対パス
        if let (Ok(cwd), Some(home)) = (env::current_dir(), dirs::home_dir()) {
            if let Ok(rel_path) = cwd.strip_prefix(&home) {
                let path = format!("~/{}", rel_path.to_string_lossy());
                result = result.replace("\\$HOME", &path);
            }
        }
        
        // 日時
        let now = Local::now();
        result = result.replace("\\t", &now.format("%H:%M:%S").to_string());
        result = result.replace("\\d", &now.format("%Y-%m-%d").to_string());
        result = result.replace("\\D{%H:%M:%S}", &now.format("%H:%M:%S").to_string());
        
        // Gitステータス
        if self.config.show_git_status {
            let git_status = self.get_git_status();
            result = result.replace("\\git", &git_status);
        }
        
        // 実行時間
        if self.config.show_execution_time {
            if let Some(duration) = self.last_command_time {
                let time_str = format_duration(duration);
                result = result.replace("\\duration", &time_str);
            } else {
                result = result.replace("\\duration", "");
            }
        }
        
        // プロンプトスタイルに応じた整形
        match self.config.style {
            crate::config::PromptStyle::Standard => {
                // 標準スタイルはそのまま
            },
            crate::config::PromptStyle::Powerline => {
                // パワーラインスタイル
                result = self.format_powerline_prompt(&result);
            },
            crate::config::PromptStyle::Minimal => {
                // 最小限スタイル
                result = "> ".to_string();
            },
            crate::config::PromptStyle::Custom(ref _custom) => {
                // カスタムスタイル
                // 実装は省略
            }
        }
        
        result
    }
    
    /// パワーラインスタイルでプロンプトをフォーマットします
    fn format_powerline_prompt(&self, prompt: &str) -> String {
        // 簡易的な実装
        let segments: Vec<&str> = prompt.split_whitespace().collect();
        let mut result = String::new();
        
        for (i, segment) in segments.iter().enumerate() {
            // 色分け
            let colored = match i % 5 {
                0 => segment.bright_blue(),
                1 => segment.bright_green(),
                2 => segment.bright_yellow(),
                3 => segment.bright_magenta(),
                _ => segment.bright_cyan(),
            };
            
            // セグメント間の区切り
            if i > 0 {
                result.push_str(" > ");
            }
            
            result.push_str(&colored.to_string());
        }
        
        result
    }
    
    /// Gitステータスを取得します
    fn get_git_status(&self) -> String {
        // キャッシュチェック
        if let Some((status, time)) = &self.cached_git_status {
            if time.elapsed().as_secs() < self.cache_ttl {
                return status.clone();
            }
        }
        
        // Gitリポジトリかどうかをチェック
        let output = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();
        
        if let Ok(output) = output {
            if output.status.success() {
                // ブランチ名の取得
                if let Ok(branch_output) = Command::new("git")
                    .args(["symbolic-ref", "--short", "HEAD"])
                    .output() {
                    
                    if branch_output.status.success() {
                        let branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
                        
                        // 変更有無の確認
                        let is_clean = Command::new("git")
                            .args(["diff", "--quiet", "HEAD"])
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);
                        
                        let status = if is_clean {
                            format!("({}) ", branch.green())
                        } else {
                            format!("({} *) ", branch.red())
                        };
                        
                        // キャッシュを更新
                        let cached = (status.clone(), Instant::now());
                        let _ = std::mem::replace(&mut self.cached_git_status, Some(cached));
                        
                        return status;
                    }
                }
            }
        }
        
        // Gitリポジトリでない場合は空文字列
        String::new()
    }
    
    /// コマンド実行開始を記録します
    pub fn record_command_start(&mut self) {
        self.last_command_start = Some(Instant::now());
    }
    
    /// コマンド実行終了を記録します
    pub fn record_command_end(&mut self) {
        if let Some(start) = self.last_command_start {
            self.last_command_time = Some(start.elapsed());
        }
    }
}

/// 持続時間を人が読みやすい形式でフォーマットします
fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    
    if total_seconds < 1 {
        let ms = duration.subsec_millis();
        format!("{}ms", ms)
    } else if total_seconds < 60 {
        format!("{}s", total_seconds)
    } else if total_seconds < 3600 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}m {}s", minutes, seconds)
    } else {
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        format!("{}h {}m", hours, minutes)
    }
}

/// プロンプトインターフェース
pub trait Prompt {
    /// プロンプト文字列を取得します
    fn get_prompt(&self) -> String;
    /// コマンド実行前に呼び出されます
    fn before_command(&mut self);
    /// コマンド実行後に呼び出されます
    fn after_command(&mut self, exit_code: i32);
} 