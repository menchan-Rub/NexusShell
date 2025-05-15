// src/ui/theme.rs - テーマとカラースキーム管理
use tui::style::{Color, Style, Modifier};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::collections::HashMap;
use super::settings::Settings;

/// カラースキーム定義
#[derive(Clone, Debug)]
pub struct ColorScheme {
    pub background: Color,
    pub foreground: Color,
    pub selection: Color,
    pub cursor: Color,
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,
    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_magenta: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        // デフォルトのダークテーマ
        Self {
            background: Color::Rgb(30, 30, 30),
            foreground: Color::Rgb(220, 220, 220),
            selection: Color::Rgb(70, 70, 70),
            cursor: Color::Rgb(200, 200, 200),
            black: Color::Rgb(0, 0, 0),
            red: Color::Rgb(205, 49, 49),
            green: Color::Rgb(13, 188, 121),
            yellow: Color::Rgb(229, 229, 16),
            blue: Color::Rgb(36, 114, 200),
            magenta: Color::Rgb(188, 63, 188),
            cyan: Color::Rgb(17, 168, 205),
            white: Color::Rgb(229, 229, 229),
            bright_black: Color::Rgb(102, 102, 102),
            bright_red: Color::Rgb(241, 76, 76),
            bright_green: Color::Rgb(35, 209, 139),
            bright_yellow: Color::Rgb(245, 245, 67),
            bright_blue: Color::Rgb(59, 142, 234),
            bright_magenta: Color::Rgb(214, 112, 214),
            bright_cyan: Color::Rgb(41, 184, 219),
            bright_white: Color::Rgb(229, 229, 229),
        }
    }
}

/// テーマスタイル定義
pub struct ThemeStyles {
    pub tab_active: Style,
    pub tab_inactive: Style,
    pub border_active: Style,
    pub border_inactive: Style,
    pub command_input: Style,
    pub command_output: Style,
    pub status_bar: Style,
    pub status_bar_key: Style,
    pub error: Style,
    pub warning: Style,
    pub success: Style,
    pub hint: Style,
}

/// テーマ管理
pub struct ThemeManager {
    settings: Arc<Settings>,
    pub current_theme: String,
    themes: HashMap<String, ColorScheme>,
    // スタイルキャッシュ
    cached_styles: Option<ThemeStyles>,
}

impl ThemeManager {
    /// 新しいテーママネージャーを作成
    pub fn new(settings: Arc<Settings>) -> Self {
        // テーマをHashMapに格納（Vector より効率的な検索）
        let mut themes = HashMap::new();
        themes.insert("Dark".to_string(), ColorScheme::default());
        themes.insert("Light".to_string(), Self::light_theme());
        themes.insert("Dracula".to_string(), Self::dracula_theme());
        themes.insert("Nord".to_string(), Self::nord_theme());
        themes.insert("Solarized Dark".to_string(), Self::solarized_dark());
        themes.insert("Solarized Light".to_string(), Self::solarized_light());
        themes.insert("Tokyo Night".to_string(), Self::tokyo_night_theme());
        themes.insert("Monokai".to_string(), Self::monokai_theme());
        
        let current_theme = settings.theme.clone();
        
        Self {
            settings,
            current_theme,
            themes,
            cached_styles: None,
        }
    }
    
    /// 現在のテーマの配色スキームを取得
    pub fn current_scheme(&self) -> &ColorScheme {
        self.themes.get(&self.current_theme)
            .unwrap_or_else(|| self.themes.get("Dark").unwrap())
    }
    
    /// テーマを切り替え
    pub fn switch_theme(&mut self, name: &str) {
        if self.themes.contains_key(name) {
            self.current_theme = name.to_string();
            self.cached_styles = None; // キャッシュを無効化
        }
    }
    
    /// キャッシュに新しいスタイルを保存（mutableな参照があるときのみ呼び出し可能）
    pub fn update_cache(&mut self, styles: ThemeStyles) {
        self.cached_styles = Some(styles);
    }
    
    /// スタイルを生成
    pub fn get_styles(&self) -> ThemeStyles {
        // キャッシュがあればそれを返す
        if let Some(ref styles) = self.cached_styles {
            return styles.clone();
        }
        
        let scheme = self.current_scheme();
        
        let styles = ThemeStyles {
            tab_active: Style::default()
                .fg(scheme.foreground)
                .bg(scheme.selection)
                .add_modifier(Modifier::BOLD),
            tab_inactive: Style::default()
                .fg(scheme.foreground),
            border_active: Style::default()
                .fg(scheme.cyan),
            border_inactive: Style::default()
                .fg(scheme.bright_black),
            command_input: Style::default()
                .fg(scheme.foreground),
            command_output: Style::default()
                .fg(scheme.bright_white),
            status_bar: Style::default()
                .bg(scheme.blue)
                .fg(scheme.bright_white),
            status_bar_key: Style::default()
                .bg(scheme.blue)
                .fg(scheme.yellow)
                .add_modifier(Modifier::BOLD),
            error: Style::default()
                .fg(scheme.red)
                .add_modifier(Modifier::BOLD),
            warning: Style::default()
                .fg(scheme.yellow),
            success: Style::default()
                .fg(scheme.green),
            hint: Style::default()
                .fg(scheme.cyan),
        };
        
        styles
    }
    
    // 以下、各種テーマの定義
    
    /// ライトテーマ
    fn light_theme() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(240, 240, 240),
            foreground: Color::Rgb(30, 30, 30),
            selection: Color::Rgb(200, 200, 250),
            cursor: Color::Rgb(50, 50, 50),
            black: Color::Rgb(28, 28, 28),
            red: Color::Rgb(215, 0, 0),
            green: Color::Rgb(0, 135, 0),
            yellow: Color::Rgb(215, 135, 0),
            blue: Color::Rgb(0, 80, 215),
            magenta: Color::Rgb(135, 0, 175),
            cyan: Color::Rgb(0, 150, 170),
            white: Color::Rgb(188, 188, 188),
            bright_black: Color::Rgb(85, 85, 85),
            bright_red: Color::Rgb(255, 85, 85),
            bright_green: Color::Rgb(85, 215, 85),
            bright_yellow: Color::Rgb(255, 215, 85),
            bright_blue: Color::Rgb(85, 170, 255),
            bright_magenta: Color::Rgb(215, 85, 215),
            bright_cyan: Color::Rgb(85, 215, 215),
            bright_white: Color::Rgb(255, 255, 255),
        }
    }
    
    /// Draculaテーマ
    fn dracula_theme() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(40, 42, 54),
            foreground: Color::Rgb(248, 248, 242),
            selection: Color::Rgb(68, 71, 90),
            cursor: Color::Rgb(255, 255, 255),
            black: Color::Rgb(0, 0, 0),
            red: Color::Rgb(255, 85, 85),
            green: Color::Rgb(80, 250, 123),
            yellow: Color::Rgb(241, 250, 140),
            blue: Color::Rgb(189, 147, 249),
            magenta: Color::Rgb(255, 121, 198),
            cyan: Color::Rgb(139, 233, 253),
            white: Color::Rgb(229, 229, 229),
            bright_black: Color::Rgb(102, 102, 102),
            bright_red: Color::Rgb(255, 110, 110),
            bright_green: Color::Rgb(105, 255, 148),
            bright_yellow: Color::Rgb(255, 255, 165),
            bright_blue: Color::Rgb(214, 172, 255),
            bright_magenta: Color::Rgb(255, 146, 223),
            bright_cyan: Color::Rgb(164, 255, 255),
            bright_white: Color::Rgb(255, 255, 255),
        }
    }
    
    /// Nordテーマ
    fn nord_theme() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(46, 52, 64),
            foreground: Color::Rgb(229, 233, 240),
            selection: Color::Rgb(67, 76, 94),
            cursor: Color::Rgb(229, 233, 240),
            black: Color::Rgb(59, 66, 82),
            red: Color::Rgb(191, 97, 106),
            green: Color::Rgb(163, 190, 140),
            yellow: Color::Rgb(235, 203, 139),
            blue: Color::Rgb(129, 161, 193),
            magenta: Color::Rgb(180, 142, 173),
            cyan: Color::Rgb(136, 192, 208),
            white: Color::Rgb(229, 233, 240),
            bright_black: Color::Rgb(76, 86, 106),
            bright_red: Color::Rgb(191, 97, 106),
            bright_green: Color::Rgb(163, 190, 140),
            bright_yellow: Color::Rgb(235, 203, 139),
            bright_blue: Color::Rgb(129, 161, 193),
            bright_magenta: Color::Rgb(180, 142, 173),
            bright_cyan: Color::Rgb(143, 188, 187),
            bright_white: Color::Rgb(236, 239, 244),
        }
    }
    
    /// Solarized Darkテーマ
    fn solarized_dark() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(0, 43, 54),
            foreground: Color::Rgb(131, 148, 150),
            selection: Color::Rgb(7, 54, 66),
            cursor: Color::Rgb(131, 148, 150),
            black: Color::Rgb(7, 54, 66),
            red: Color::Rgb(220, 50, 47),
            green: Color::Rgb(133, 153, 0),
            yellow: Color::Rgb(181, 137, 0),
            blue: Color::Rgb(38, 139, 210),
            magenta: Color::Rgb(211, 54, 130),
            cyan: Color::Rgb(42, 161, 152),
            white: Color::Rgb(238, 232, 213),
            bright_black: Color::Rgb(0, 43, 54),
            bright_red: Color::Rgb(203, 75, 22),
            bright_green: Color::Rgb(88, 110, 117),
            bright_yellow: Color::Rgb(101, 123, 131),
            bright_blue: Color::Rgb(131, 148, 150),
            bright_magenta: Color::Rgb(108, 113, 196),
            bright_cyan: Color::Rgb(147, 161, 161),
            bright_white: Color::Rgb(253, 246, 227),
        }
    }
    
    /// Solarized Lightテーマ
    fn solarized_light() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(253, 246, 227),
            foreground: Color::Rgb(101, 123, 131),
            selection: Color::Rgb(238, 232, 213),
            cursor: Color::Rgb(101, 123, 131),
            black: Color::Rgb(7, 54, 66),
            red: Color::Rgb(220, 50, 47),
            green: Color::Rgb(133, 153, 0),
            yellow: Color::Rgb(181, 137, 0),
            blue: Color::Rgb(38, 139, 210),
            magenta: Color::Rgb(211, 54, 130),
            cyan: Color::Rgb(42, 161, 152),
            white: Color::Rgb(238, 232, 213),
            bright_black: Color::Rgb(0, 43, 54),
            bright_red: Color::Rgb(203, 75, 22),
            bright_green: Color::Rgb(88, 110, 117),
            bright_yellow: Color::Rgb(101, 123, 131),
            bright_blue: Color::Rgb(131, 148, 150),
            bright_magenta: Color::Rgb(108, 113, 196),
            bright_cyan: Color::Rgb(147, 161, 161),
            bright_white: Color::Rgb(253, 246, 227),
        }
    }
    
    /// Tokyo Nightテーマ
    fn tokyo_night_theme() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(26, 27, 38),
            foreground: Color::Rgb(169, 177, 214),
            selection: Color::Rgb(33, 34, 44),
            cursor: Color::Rgb(169, 177, 214),
            black: Color::Rgb(32, 32, 44),
            red: Color::Rgb(247, 118, 142),
            green: Color::Rgb(158, 206, 106),
            yellow: Color::Rgb(224, 175, 104),
            blue: Color::Rgb(122, 162, 247),
            magenta: Color::Rgb(187, 154, 247),
            cyan: Color::Rgb(125, 207, 255),
            white: Color::Rgb(169, 177, 214),
            bright_black: Color::Rgb(68, 75, 106),
            bright_red: Color::Rgb(247, 118, 142),
            bright_green: Color::Rgb(158, 206, 106),
            bright_yellow: Color::Rgb(224, 175, 104),
            bright_blue: Color::Rgb(122, 162, 247),
            bright_magenta: Color::Rgb(187, 154, 247),
            bright_cyan: Color::Rgb(125, 207, 255),
            bright_white: Color::Rgb(192, 202, 245),
        }
    }
    
    /// Monokaiテーマ
    fn monokai_theme() -> ColorScheme {
        ColorScheme {
            background: Color::Rgb(39, 40, 34),
            foreground: Color::Rgb(248, 248, 242),
            selection: Color::Rgb(73, 72, 62),
            cursor: Color::Rgb(253, 151, 31),
            black: Color::Rgb(39, 40, 34),
            red: Color::Rgb(249, 38, 114),
            green: Color::Rgb(166, 226, 46),
            yellow: Color::Rgb(244, 191, 117),
            blue: Color::Rgb(102, 217, 239),
            magenta: Color::Rgb(174, 129, 255),
            cyan: Color::Rgb(161, 239, 228),
            white: Color::Rgb(248, 248, 242),
            bright_black: Color::Rgb(117, 113, 94),
            bright_red: Color::Rgb(249, 38, 114),
            bright_green: Color::Rgb(166, 226, 46),
            bright_yellow: Color::Rgb(244, 191, 117),
            bright_blue: Color::Rgb(102, 217, 239),
            bright_magenta: Color::Rgb(174, 129, 255),
            bright_cyan: Color::Rgb(161, 239, 228),
            bright_white: Color::Rgb(249, 248, 245),
        }
    }
}

impl Clone for ThemeManager {
    fn clone(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            current_theme: self.current_theme.clone(),
            themes: self.themes.clone(),
            cached_styles: self.cached_styles.clone(),
        }
    }
}

// テーマスタイルのクローン実装
impl Clone for ThemeStyles {
    fn clone(&self) -> Self {
        Self {
            tab_active: self.tab_active,
            tab_inactive: self.tab_inactive,
            border_active: self.border_active,
            border_inactive: self.border_inactive,
            command_input: self.command_input,
            command_output: self.command_output,
            status_bar: self.status_bar,
            status_bar_key: self.status_bar_key,
            error: self.error,
            warning: self.warning,
            success: self.success,
            hint: self.hint,
        }
    }
} 