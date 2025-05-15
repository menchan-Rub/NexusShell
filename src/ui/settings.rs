// src/ui/settings.rs - NexusShellの設定管理

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

/// アプリケーション設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// アプリケーション名
    pub app_name: String,
    
    /// 現在選択されているテーマ
    pub theme: String,
    
    /// ウィンドウ設定
    pub window: WindowSettings,
    
    /// ターミナル設定
    pub terminal: TerminalSettings,
    
    /// キーバインド設定
    pub keybindings: KeybindingSettings,
    
    /// フォント設定
    pub font: FontSettings,
    
    /// パフォーマンス設定
    pub performance: PerformanceSettings,
}

/// ウィンドウ設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowSettings {
    /// 初期ウィンドウ幅
    pub width: u16,
    
    /// 初期ウィンドウ高さ
    pub height: u16,
    
    /// 初期ウィンドウX位置
    pub x: Option<i16>,
    
    /// 初期ウィンドウY位置
    pub y: Option<i16>,
    
    /// 最大化状態で起動
    pub start_maximized: bool,
    
    /// ウィンドウタイトル
    pub title: String,
    
    /// タイトルバーを表示するか
    pub show_title_bar: bool,
    
    /// フルスクリーンモード
    pub fullscreen: bool,
}

/// ターミナル設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalSettings {
    /// スクロールバックバッファの行数
    pub scrollback_lines: usize,
    
    /// カーソル形状（block, underline, bar）
    pub cursor_shape: String,
    
    /// カーソル点滅
    pub cursor_blink: bool,
    
    /// コピー時に自動的に選択範囲をクリップボードにコピー
    pub copy_on_select: bool,
    
    /// マウス操作を有効化
    pub enable_mouse: bool,
}

/// キーバインド設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeybindingSettings {
    /// 新規タブ
    pub new_tab: String,
    
    /// タブ切り替え（次）
    pub next_tab: String,
    
    /// タブ切り替え（前）
    pub prev_tab: String,
    
    /// タブを閉じる
    pub close_tab: String,
    
    /// 水平分割
    pub split_horizontal: String,
    
    /// 垂直分割
    pub split_vertical: String,
    
    /// ペイン移動
    pub focus_next_pane: String,
    
    /// 前のペインに移動
    pub focus_prev_pane: String,
    
    /// コピー
    pub copy: String,
    
    /// ペースト
    pub paste: String,
}

/// フォント設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FontSettings {
    /// フォント名
    pub family: String,
    
    /// フォントサイズ
    pub size: f32,
    
    /// フォントの線の太さ（normal, bold）
    pub weight: String,
    
    /// テキストの色
    pub color: String,
    
    /// リガチャを使用するか
    pub ligatures: bool,
}

/// パフォーマンス設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceSettings {
    /// フレームレート（FPS）
    pub fps_limit: u8,
    
    /// GPUアクセラレーションを使用するか
    pub gpu_acceleration: bool,
    
    /// アニメーション速度（0-10）
    pub animation_speed: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            app_name: "NexusShell".to_string(),
            theme: "Dark".to_string(),
            window: WindowSettings::default(),
            terminal: TerminalSettings::default(),
            keybindings: KeybindingSettings::default(),
            font: FontSettings::default(),
            performance: PerformanceSettings::default(),
        }
    }
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: 120,
            height: 30,
            x: None,
            y: None,
            start_maximized: false,
            title: "NexusShell".to_string(),
            show_title_bar: true,
            fullscreen: false,
        }
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            scrollback_lines: 10000,
            cursor_shape: "block".to_string(),
            cursor_blink: true,
            copy_on_select: false,
            enable_mouse: true,
        }
    }
}

impl Default for KeybindingSettings {
    fn default() -> Self {
        Self {
            new_tab: "Ctrl+T".to_string(),
            next_tab: "Ctrl+Tab".to_string(),
            prev_tab: "Ctrl+Shift+Tab".to_string(),
            close_tab: "Ctrl+W".to_string(),
            split_horizontal: "Ctrl+\\".to_string(),
            split_vertical: "Ctrl+-".to_string(),
            focus_next_pane: "Alt+Right".to_string(),
            focus_prev_pane: "Alt+Left".to_string(),
            copy: "Ctrl+C".to_string(),
            paste: "Ctrl+V".to_string(),
        }
    }
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            family: "Consolas, 'Courier New', monospace".to_string(),
            size: 12.0,
            weight: "normal".to_string(),
            color: "#FFFFFF".to_string(),
            ligatures: false,
        }
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            fps_limit: 60,
            gpu_acceleration: true,
            animation_speed: 5,
        }
    }
}

impl Settings {
    /// 設定を読み込む
    pub fn load() -> Result<Self, io::Error> {
        let config_path = Self::get_config_path()?;
        
        if !config_path.exists() {
            let default_settings = Self::default();
            default_settings.save()?;
            return Ok(default_settings);
        }
        
        let content = fs::read_to_string(&config_path)?;
        let settings: Self = toml::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        Ok(settings)
    }
    
    /// 設定を保存
    pub fn save(&self) -> Result<(), io::Error> {
        let config_path = Self::get_config_path()?;
        
        // 親ディレクトリが存在しない場合は作成
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let toml = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        fs::write(&config_path, toml)
    }
    
    /// 設定ファイルパスを取得
    fn get_config_path() -> Result<PathBuf, io::Error> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "ホームディレクトリが見つかりません"))?;
        
        #[cfg(target_os = "windows")]
        let config_dir = home_dir.join("AppData").join("Roaming").join("NexusShell");
        
        #[cfg(not(target_os = "windows"))]
        let config_dir = home_dir.join(".config").join("nexusshell");
        
        Ok(config_dir.join("config.toml"))
    }
} 