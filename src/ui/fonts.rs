// src/ui/fonts.rs - NexusShellのフォント管理

use std::collections::HashMap;
use std::sync::Arc;
use crate::ui::settings::Settings;

/// フォントスタイル
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FontStyle {
    /// 標準
    Regular,
    /// 太字
    Bold,
    /// イタリック
    Italic,
    /// 太字イタリック
    BoldItalic,
}

/// フォント管理
pub struct FontManager {
    settings: Arc<Settings>,
    fonts: HashMap<String, FontInfo>,
    current_font: String,
}

/// フォント情報
#[derive(Clone, Debug)]
pub struct FontInfo {
    /// フォント名
    pub name: String,
    /// ファミリー名
    pub family: String,
    /// モノスペースフォントかどうか
    pub monospace: bool,
    /// 利用可能なスタイル
    pub available_styles: Vec<FontStyle>,
    /// 使用可能なサイズ範囲
    pub size_range: (f32, f32),
}

impl FontManager {
    /// 新しいフォントマネージャーを作成
    pub fn new(settings: Arc<Settings>) -> Self {
        let mut manager = Self {
            settings,
            fonts: HashMap::new(),
            current_font: "Consolas".to_string(),
        };
        
        // システムで利用可能なフォントをロード
        manager.load_system_fonts();
        
        manager
    }
    
    /// システムフォントを読み込む
    fn load_system_fonts(&mut self) {
        // 実際の実装では、OSごとのフォントAPIを使用してシステムフォントを列挙
        // ここではサンプルデータのみ提供
        
        let monospace_fonts = vec![
            ("Consolas", "Consolas"),
            ("Courier New", "Courier"),
            ("DejaVu Sans Mono", "DejaVu"),
            ("Fira Code", "Fira"),
            ("JetBrains Mono", "JetBrains"),
            ("Source Code Pro", "Source Code"),
            ("Ubuntu Mono", "Ubuntu"),
            ("Cascadia Code", "Cascadia"),
            ("Menlo", "Menlo"),
            ("SF Mono", "SF"),
        ];
        
        for (name, family) in monospace_fonts {
            self.fonts.insert(name.to_string(), FontInfo {
                name: name.to_string(),
                family: family.to_string(),
                monospace: true,
                available_styles: vec![
                    FontStyle::Regular,
                    FontStyle::Bold,
                    FontStyle::Italic,
                    FontStyle::BoldItalic,
                ],
                size_range: (6.0, 72.0),
            });
        }
    }
    
    /// 現在のフォント名を取得
    pub fn current_font_name(&self) -> &str {
        &self.current_font
    }
    
    /// 現在のフォント情報を取得
    pub fn current_font_info(&self) -> Option<&FontInfo> {
        self.fonts.get(&self.current_font)
    }
    
    /// フォントを変更
    pub fn set_font(&mut self, name: &str) -> bool {
        if self.fonts.contains_key(name) {
            self.current_font = name.to_string();
            true
        } else {
            false
        }
    }
    
    /// 利用可能なモノスペースフォント一覧を取得
    pub fn get_monospace_fonts(&self) -> Vec<&str> {
        self.fonts.iter()
            .filter(|(_, info)| info.monospace)
            .map(|(name, _)| name.as_str())
            .collect()
    }
    
    /// フォントサイズを変更
    pub fn set_font_size(&mut self, size: f32) -> f32 {
        let mut settings = self.settings.as_ref().clone();
        
        // サイズ範囲を制限
        let new_size = size.max(6.0).min(72.0);
        settings.font.size = new_size;
        
        // 設定を保存（エラーは無視）
        let _ = settings.save();
        
        new_size
    }
    
    /// フォントサイズを大きくする
    pub fn increase_font_size(&mut self) -> f32 {
        let current_size = self.settings.as_ref().font.size;
        let new_size = if current_size >= 36.0 {
            current_size + 4.0
        } else if current_size >= 24.0 {
            current_size + 2.0
        } else {
            current_size + 1.0
        };
        
        self.set_font_size(new_size)
    }
    
    /// フォントサイズを小さくする
    pub fn decrease_font_size(&mut self) -> f32 {
        let current_size = self.settings.as_ref().font.size;
        let new_size = if current_size > 36.0 {
            current_size - 4.0
        } else if current_size > 24.0 {
            current_size - 2.0
        } else {
            current_size - 1.0
        };
        
        self.set_font_size(new_size)
    }
    
    /// フォントのレンダリング設定を取得
    pub fn get_font_config(&self) -> FontRenderConfig {
        FontRenderConfig {
            family: self.current_font.clone(),
            size: self.settings.as_ref().font.size,
            bold: self.settings.as_ref().font.weight == "bold",
            ligatures: self.settings.as_ref().font.ligatures,
        }
    }
}

/// フォントのレンダリング設定
#[derive(Clone, Debug)]
pub struct FontRenderConfig {
    /// フォントファミリー
    pub family: String,
    /// フォントサイズ
    pub size: f32,
    /// 太字かどうか
    pub bold: bool,
    /// リガチャを使用するか
    pub ligatures: bool,
}

impl Default for FontRenderConfig {
    fn default() -> Self {
        Self {
            family: "Consolas".to_string(),
            size: 12.0,
            bold: false,
            ligatures: false,
        }
    }
} 