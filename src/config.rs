use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};

/// プロンプト設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    /// プロンプトフォーマット
    pub format: String,
    /// 右側プロンプト
    pub right_format: Option<String>,
    /// プロンプトのスタイル
    pub style: PromptStyle,
    /// GitステータスをプロンプトJに表示するかどうか
    pub show_git_status: bool,
    /// 実行時間を表示するかどうか
    pub show_execution_time: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            format: "[\\u@\\h \\W]\\$ ".to_string(),
            right_format: None,
            style: PromptStyle::default(),
            show_git_status: true,
            show_execution_time: true,
        }
    }
}

/// プロンプトスタイル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromptStyle {
    /// 標準スタイル
    Standard,
    /// パワーラインスタイル
    Powerline,
    /// 最小限スタイル
    Minimal,
    /// カスタムスタイル
    Custom(String),
}

impl Default for PromptStyle {
    fn default() -> Self {
        Self::Standard
    }
}

/// 履歴設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// 履歴ファイルのパス
    pub file_path: Option<PathBuf>,
    /// 履歴のサイズ上限
    pub size_limit: usize,
    /// 重複を無視するかどうか
    pub ignore_duplicates: bool,
    /// スペースで始まるコマンドを無視するかどうか
    pub ignore_space: bool,
    /// タイムスタンプを記録するかどうか
    pub record_timestamps: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            file_path: dirs::home_dir().map(|h| h.join(".nexusshell_history")),
            size_limit: 1000,
            ignore_duplicates: true,
            ignore_space: true,
            record_timestamps: true,
        }
    }
}

/// プラグイン設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsConfig {
    /// プラグインディレクトリ
    pub directory: Option<PathBuf>,
    /// 有効なプラグイン
    pub enabled: Vec<String>,
    /// プラグインの設定
    pub settings: serde_json::Value,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            directory: dirs::data_dir().map(|d| d.join("nexusshell").join("plugins")),
            enabled: vec![],
            settings: serde_json::json!({}),
        }
    }
}

/// テーマ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// テーマ名
    pub name: String,
    /// 色設定
    pub colors: ColorConfig,
    /// ファイルごとの色設定
    pub file_colors: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            colors: ColorConfig::default(),
            file_colors: true,
        }
    }
}

/// 色設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    /// コマンド色
    pub command: String,
    /// エラー色
    pub error: String,
    /// 警告色
    pub warning: String,
    /// 情報色
    pub info: String,
    /// プロンプト色
    pub prompt: String,
    /// 出力色
    pub output: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            command: "green".to_string(),
            error: "red".to_string(),
            warning: "yellow".to_string(),
            info: "blue".to_string(),
            prompt: "cyan".to_string(),
            output: "white".to_string(),
        }
    }
}

/// シェル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// プロンプト設定
    pub prompt: PromptConfig,
    /// 履歴設定
    pub history: HistoryConfig,
    /// プラグイン設定
    pub plugins: PluginsConfig,
    /// テーマ設定
    pub theme: ThemeConfig,
    /// エラー時に実行を中止するかどうか
    pub stop_on_error: bool,
    /// 補完を有効にするかどうか
    pub completion_enabled: bool,
    /// エディタのパス
    pub editor: Option<String>,
    /// デフォルトのシェル
    pub default_shell: Option<String>,
    /// パス環境変数
    pub path: Option<Vec<String>>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            prompt: PromptConfig::default(),
            history: HistoryConfig::default(),
            plugins: PluginsConfig::default(),
            theme: ThemeConfig::default(),
            stop_on_error: true,
            completion_enabled: true,
            editor: std::env::var("EDITOR").ok().or_else(|| Some("nano".to_string())),
            default_shell: None,
            path: None,
        }
    }
}

impl ShellConfig {
    /// 設定ファイルからロードします
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("設定ファイルの読み込みに失敗しました: {:?}", path.as_ref()))?;
        
        let config: ShellConfig = toml::from_str(&content)
            .with_context(|| format!("設定ファイルのパースに失敗しました: {:?}", path.as_ref()))?;
        
        Ok(config)
    }
    
    /// 設定ファイルに保存します
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("設定のシリアライズに失敗しました")?;
        
        // ディレクトリが存在しない場合は作成
        if let Some(parent) = path.as_ref().parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("ディレクトリの作成に失敗しました: {:?}", parent))?;
            }
        }
        
        fs::write(path, content)
            .with_context(|| format!("設定ファイルの書き込みに失敗しました: {:?}", path.as_ref()))?;
        
        Ok(())
    }
} 