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
    
    /// システムフォントをリロード（差分検知・キャッシュ活用）
    pub fn reload_system_fonts(&mut self) {
        let old_fonts = self.fonts.clone();
        self.fonts.clear();
        self.load_system_fonts();
        // 差分検知
        let added: Vec<_> = self.fonts.keys().filter(|k| !old_fonts.contains_key(*k)).cloned().collect();
        let removed: Vec<_> = old_fonts.keys().filter(|k| !self.fonts.contains_key(*k)).cloned().collect();
        if !added.is_empty() || !removed.is_empty() {
            debug!("フォントリストに差分: 追加={:?} 削除={:?}", added, removed);
        }
    }
    
    /// システムフォントを読み込む
    fn load_system_fonts(&mut self) {
        // 各OSのネイティブフォントAPIを使用してシステムフォントを完全列挙
        
        // OS固有の実装を呼び出す
        #[cfg(target_os = "windows")]
        self.load_windows_fonts();
        
        #[cfg(target_os = "macos")]
        self.load_macos_fonts();
        
        #[cfg(target_os = "linux")]
        self.load_linux_fonts();
        
        #[cfg(target_os = "freebsd")]
        self.load_bsd_fonts();
        
        #[cfg(target_os = "openbsd")]
        self.load_bsd_fonts();
        
        #[cfg(target_os = "netbsd")]
        self.load_bsd_fonts();
        
        #[cfg(target_os = "dragonfly")]
        self.load_bsd_fonts();
        
        #[cfg(target_os = "wsl")]
        self.load_linux_fonts(); // WSLはLinux扱い
        
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly", target_os = "wsl")))]
        self.load_fallback_fonts(); // サポート外OSではフォールバック

        // ファミリー名でフォントをグループ化
        let mut families = HashMap::new();
        for (name, info) in &self.fonts {
            families.entry(info.family.clone())
                .or_insert_with(Vec::new)
                .push(name.clone());
        }
        
        // ファミリーごとにフォントスタイル情報を更新
        for (family, members) in families {
            for name in members {
                if let Some(info) = self.fonts.get_mut(&name) {
                    info.available_styles = self.determine_font_styles(&family);
                }
            }
        }
        
        // フォントがない場合のフォールバック
        if self.fonts.is_empty() {
            error!("システムフォント列挙に失敗: フォールバックフォントを使用");
            self.add_fallback_fonts();
        }
        
        // 現在のフォントを設定
        self.set_default_font();
        
        debug!("{} フォントをロードしました", self.fonts.len());
    }
    
    /// Windows環境でのフォント列挙実装
    #[cfg(target_os = "windows")]
    fn load_windows_fonts(&mut self) {
        use std::ptr;
        use std::ffi::CString;
        use std::os::raw::c_void;
        use windows_sys::Win32::Graphics::Gdi::{
            EnumFontFamiliesExA, EnumFontFamiliesExW, LOGFONTA, LOGFONTW, 
            DEFAULT_CHARSET, OUT_TT_ONLY_PRECIS, CLIP_DEFAULT_PRECIS,
            ANSI_CHARSET, DEFAULT_PITCH, FF_DONTCARE, FF_MODERN, 
            ENUMLOGFONTEXA, ENUMLOGFONTEXW, FONTENUMPROCA, FONTENUMPROCW,
            DEVICE_FONTTYPE, RASTER_FONTTYPE, TRUETYPE_FONTTYPE
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetDC;
        use windows_sys::Win32::UI::WindowsAndMessaging::ReleaseDC;
        use windows_sys::Win32::Graphics::Gdi::GetDeviceCaps;
        use windows_sys::Win32::Graphics::Gdi::{HORZRES, VERTRES, LOGPIXELSX, LOGPIXELSY};
        use windows_sys::Win32::Foundation::LPARAM;
        
        debug!("Windows: フォント列挙を開始します");
        
        // システムのデバイスコンテキストを取得
        let hdc = unsafe { GetDC(0) };
        if hdc == 0 {
            error!("GetDC failed - フォールバックフォントを使用します");
            self.add_fallback_fonts();
            return;
        }
        
        // フォント列挙に使用するLOGFONT構造体 (ワイド文字版)
        let mut log_font: LOGFONTW = unsafe { std::mem::zeroed() };
        log_font.lfCharSet = DEFAULT_CHARSET as u8;
        log_font.lfOutPrecision = OUT_TT_ONLY_PRECIS as u8; // TrueTypeフォントのみ
        log_font.lfClipPrecision = CLIP_DEFAULT_PRECIS as u8;
        log_font.lfPitchAndFamily = DEFAULT_PITCH as u8 | FF_DONTCARE as u8;
        
        // フォント列挙コールバック関数 (ワイド文字版)
        extern "system" fn enum_font_callback_w(
            lpelfe: *const ENUMLOGFONTEXW,
            _lpntme: *const c_void,
            font_type: u32,
            lparam: LPARAM,
        ) -> i32 {
            unsafe {
                let font_manager = &mut *(lparam as *mut FontManager);
                let logfont = &(*lpelfe).elfLogFont;
                
                // フォント名を取得
                let face_name_raw = logfont.lfFaceName;
                let mut face_name = String::new();
                
                for &wide_char in face_name_raw.iter() {
                    if wide_char == 0 {
                        break;
                    }
                    if let Some(c) = std::char::from_u32(wide_char as u32) {
                        face_name.push(c);
                    }
                }
                
                if face_name.is_empty() {
                    return 1; // 次のフォントへ
                }
                
                // フォント情報を作成
                let is_truetype = (font_type & TRUETYPE_FONTTYPE) != 0;
                let is_monospace = (logfont.lfPitchAndFamily & 0x01) == 0; // FIXED_PITCH
                
                let mut family_name = face_name.clone();
                
                // フォントスタイルを判定
                let mut style = if logfont.lfItalic != 0 {
                    "Italic".to_string()
                } else {
                    "Regular".to_string()
                };
                
                if logfont.lfWeight >= 700 {
                    if style == "Regular" {
                        style = "Bold".to_string();
                    } else {
                        style = "BoldItalic".to_string();
                    }
                }
                
                // フォントファミリー名からスタイル指定を削除
                if family_name.ends_with(&format!(" {}", style)) {
                    family_name = family_name[..family_name.len() - style.len() - 1].to_string();
                } else if family_name.contains(&format!(" {} ", style)) {
                    // スタイルが中間にある場合も対応
                    if let Some(idx) = family_name.find(&format!(" {} ", style)) {
                        family_name = format!(
                            "{}{}",
                            &family_name[..idx],
                            &family_name[idx + style.len() + 2..]
                        );
                    }
                }
                
                // TrueTypeフォントのみを追加
                if is_truetype {
                    let font_info = FontInfo {
                        name: face_name.clone(),
                        family: family_name,
                        monospace: is_monospace,
                        available_styles: vec![], // 後で更新
                        size_range: (8.0, 72.0),  // デフォルト範囲
                    };
                    
                    // 重複チェック
                    if !font_manager.fonts.contains_key(&face_name) {
                        font_manager.fonts.insert(face_name, font_info);
                    }
                }
                
                1 // 列挙を続行
            }
        }
        
        // フォント列挙を実行
        let result = unsafe {
            EnumFontFamiliesExW(
                hdc,
                &mut log_font,
                Some(enum_font_callback_w),
                self as *mut _ as LPARAM,
                0,
            )
        };
        
        // DCを解放
        unsafe { ReleaseDC(0, hdc) };
        
        if result == 0 {
            error!("EnumFontFamiliesExW failed - フォールバックフォントを使用します");
            self.add_fallback_fonts();
        }
        
        debug!("Windows: {} フォントをロードしました", self.fonts.len());
        
        // フォントが少なすぎる場合はフォールバック
        if self.fonts.len() < 5 {
            debug!("フォント列挙が少なすぎるようです、フォールバックを追加します");
            self.add_fallback_fonts();
        }
    }
    
    /// macOS環境でのフォント列挙実装
    #[cfg(target_os = "macos")]
    fn load_macos_fonts(&mut self) {
        use core_foundation::array::{CFArray, CFArrayRef};
        use core_foundation::base::{CFType, TCFType};
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::{CFNumber, CFNumberRef};
        use core_foundation::string::{CFString, CFStringRef};
        use core_graphics::font::{CGFont, CGFontRef};
        use core_text::font::CTFont;
        use core_text::font_descriptor::{CTFontDescriptor, CTFontDescriptorCreateWithAttributes};
        use core_text::font_manager::CTFontManagerCopyAvailableFontFamilyNames;
        
        debug!("macOS: フォント列挙を開始します");
        
        unsafe {
            // 利用可能なフォントファミリー名を取得
            let names_array = CFArray::wrap_under_create_rule(CTFontManagerCopyAvailableFontFamilyNames());
            let count = names_array.len();
            
            debug!("macOSで{}個のフォントファミリーが見つかりました", count);
            
            // 各フォントファミリーを処理
            for i in 0..count {
                let family_name_ref = names_array.get(i);
                if let Some(family_name) = family_name_ref.downcast::<CFString>() {
                    let family_name_str = family_name.to_string();
                    
                    // フォントインスタンスを作成して詳細情報を取得
                    let cf_name = CFString::new(&family_name_str);
                    
                    // フォント記述子を作成
                    let font_size = 12.0;
                    let font = CTFont::new_from_name(&cf_name, font_size).ok();
                    
                    if let Some(font) = font {
                        // フォント情報を取得
                        let full_name = font.display_name().to_string();
                        let is_monospace = font.symbolic_traits().is_monospace();
                        
                        // 利用可能なスタイルのリスト
                        let styles = self.determine_font_styles(&family_name_str);
                        
                        // FontInfo構造体を作成
                        let font_info = FontInfo {
                            name: full_name.clone(),
                            family: family_name_str.clone(),
                            monospace: is_monospace,
                            available_styles: styles,
                            size_range: (8.0, 72.0), // デフォルト値
                        };
                        
                        // 重複チェック
                        if !self.fonts.contains_key(&full_name) {
                            self.fonts.insert(full_name, font_info);
                        }
                    }
                }
            }
        }
        
        debug!("macOS: {} フォントをロードしました", self.fonts.len());
        
        // フォントが少なすぎる場合はフォールバック
        if self.fonts.len() < 5 {
            debug!("フォント列挙が少なすぎるようです、フォールバックを追加します");
            self.add_fallback_fonts();
        }
    }
    
    /// Linux環境でのフォント列挙実装
    #[cfg(target_os = "linux")]
    fn load_linux_fonts(&mut self) {
        use std::path::{Path, PathBuf};
        use std::process::Command;
        use fontconfig::Fontconfig;
        use std::ffi::CStr;
        
        debug!("Linux: フォント列挙を開始します");
        
        // FontConfig APIを使用してシステムフォントを列挙
        match Fontconfig::new() {
            Ok(fc) => {
                // 全フォントのパターン検索
                match fc.fonts() {
                    Ok(fonts) => {
                        for font in fonts {
                            if let (Some(family), Some(style)) = (font.family(), font.style()) {
                                let family_name = family.to_string();
                                let style_name = style.to_string();
                                let full_name = format!("{} {}", family_name, style_name);
                                
                                // ファイルパスを取得
                                let file_path = font.path().map(|p| p.to_string_lossy().to_string());
                                
                                // モノスペースかどうかを確認
                                let is_monospace = font.spacing().map_or(false, |s| s == fontconfig::Spacing::Mono);
                                
                                // FontInfo構造体を作成
                                let font_info = FontInfo {
                                    name: full_name.clone(),
                                    family: family_name,
                                    monospace: is_monospace,
                                    available_styles: vec![], // 後で更新
                                    size_range: (8.0, 72.0),  // デフォルト範囲
                                };
                                
                                // 重複チェック
                                if !self.fonts.contains_key(&full_name) {
                                    self.fonts.insert(full_name, font_info);
                                }
                            }
                        }
                        
                        debug!("Linux: FontConfig API から {} フォントをロードしました", self.fonts.len());
                    },
                    Err(e) => {
                        error!("FontConfig フォント列挙エラー: {}", e);
                        // フォールバック: fc-listコマンドを試す
                        self.load_linux_fonts_fallback();
                    }
                }
            },
            Err(e) => {
                error!("FontConfig 初期化エラー: {}", e);
                // フォールバック: fc-listコマンドを試す
                self.load_linux_fonts_fallback();
            }
        }
        
        // フォントが少なすぎる場合はフォールバック
        if self.fonts.len() < 5 {
            debug!("フォント列挙が少なすぎるようです、フォールバックを追加します");
            self.add_fallback_fonts();
        }
    }
    
    #[cfg(target_os = "linux")]
    fn load_linux_fonts_fallback(&mut self) {
        // fc-listコマンドを使用してフォントを列挙するフォールバック
        debug!("fc-listを使用したフォールバック処理を実行");
        
        let output = std::process::Command::new("fc-list")
            .args(&["--format", "%{family}\\t%{style}\\t%{file}\\n"])
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                
                // 各行を処理
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        let family = parts[0].trim();
                        let style = parts[1].trim();
                        let file = parts[2].trim();
                        
                        // 名前が空でない場合のみ処理
                        if !family.is_empty() {
                            // モノスペースかどうかを名前から推測
                            let is_monospace = family.to_lowercase().contains("mono") || 
                                              family.to_lowercase().contains("courier") ||
                                              family.to_lowercase().contains("console");
                            
                            // フォントスタイルを判定
                            let font_style = if style.to_lowercase().contains("bold") && 
                                               style.to_lowercase().contains("italic") {
                                FontStyle::BoldItalic
                            } else if style.to_lowercase().contains("bold") {
                                FontStyle::Bold
                            } else if style.to_lowercase().contains("italic") {
                                FontStyle::Italic
                            } else {
                                FontStyle::Regular
                            };
                            
                            let name = format!("{} {}", family, style);
                            
                            // 既に存在するフォントの場合はスタイルを追加
                            if let Some(existing) = self.fonts.get_mut(&name) {
                                if !existing.available_styles.contains(&font_style) {
                                    existing.available_styles.push(font_style);
                                }
                            } else {
                                // 新しいフォント情報を追加
                                self.fonts.insert(name.clone(), FontInfo {
                                    name: name,
                                    family: family.to_string(),
                                    monospace: is_monospace,
                                    available_styles: vec![font_style],
                                    size_range: (6.0, 72.0),
                                });
                            }
                        }
                    }
                }
            },
            Ok(_) => {
                error!("fc-listコマンドの実行に失敗");
                self.add_fallback_fonts();
            },
            Err(e) => {
                error!("fc-listコマンドの実行時にエラー: {}", e);
                self.add_fallback_fonts();
            }
        }
    }
    
    /// BSD系のフォント列挙（fontconfigまたはfc-list利用）
    #[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly"))]
    fn load_bsd_fonts(&mut self) {
        // fontconfigが利用可能なら使う
        if let Ok(fc) = fontconfig::Fontconfig::new() {
            if let Ok(fonts) = fc.fonts() {
                for font in fonts {
                    if let (Some(family), Some(style)) = (font.family(), font.style()) {
                        let family_name = family.to_string();
                        let style_name = style.to_string();
                        let full_name = format!("{} {}", family_name, style_name);
                        let is_monospace = font.spacing().map_or(false, |s| s == fontconfig::Spacing::Mono);
                        let font_info = FontInfo {
                            name: full_name.clone(),
                            family: family_name,
                            monospace: is_monospace,
                            available_styles: vec![],
                            size_range: (8.0, 72.0),
                        };
                        if !self.fonts.contains_key(&full_name) {
                            self.fonts.insert(full_name, font_info);
                        }
                    }
                }
                debug!("BSD: fontconfig API から {} フォントをロード", self.fonts.len());
                return;
            }
        }
        // 失敗時はfc-listコマンド
        let output = std::process::Command::new("fc-list")
            .args(&["--format", "%{family}\\t%{style}\\t%{file}\\n"])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        let family = parts[0].trim();
                        let style = parts[1].trim();
                        let is_monospace = family.to_lowercase().contains("mono") || family.to_lowercase().contains("courier") || family.to_lowercase().contains("console");
                        let name = format!("{} {}", family, style);
                        let font_info = FontInfo {
                            name: name.clone(),
                            family: family.to_string(),
                            monospace: is_monospace,
                            available_styles: vec![],
                            size_range: (6.0, 72.0),
                        };
                        if !self.fonts.contains_key(&name) {
                            self.fonts.insert(name, font_info);
                        }
                    }
                }
                debug!("BSD: fc-list から {} フォントをロード", self.fonts.len());
            },
            Ok(_) | Err(_) => {
                error!("BSD: fc-listコマンドの実行に失敗、フォールバック");
                self.add_fallback_fonts();
            }
        }
    }
    
    /// すべてのプラットフォームで利用可能なフォールバックフォントを追加
    fn add_fallback_fonts(&mut self) {
        // 一般的なシステムフォントを手動で追加
        let common_fonts = [
            ("Arial", "Regular", "Normal"),
            ("Arial", "Bold", "Bold"),
            ("Arial", "Italic", "Normal"),
            ("Arial", "Bold Italic", "Bold"),
            ("Times New Roman", "Regular", "Normal"),
            ("Times New Roman", "Bold", "Bold"),
            ("Times New Roman", "Italic", "Normal"),
            ("Times New Roman", "Bold Italic", "Bold"),
            ("Courier New", "Regular", "Normal"),
            ("Courier New", "Bold", "Bold"),
            ("Courier New", "Italic", "Normal"),
            ("Courier New", "Bold Italic", "Bold"),
            ("Verdana", "Regular", "Normal"),
            ("Verdana", "Bold", "Bold"),
            ("Verdana", "Italic", "Normal"),
            ("Verdana", "Bold Italic", "Bold"),
            ("Georgia", "Regular", "Normal"),
            ("Georgia", "Bold", "Bold"),
            ("Georgia", "Italic", "Normal"),
            ("Georgia", "Bold Italic", "Bold"),
            ("Tahoma", "Regular", "Normal"),
            ("Tahoma", "Bold", "Bold"),
            ("Impact", "Regular", "Normal"),
            ("Trebuchet MS", "Regular", "Normal"),
            ("Trebuchet MS", "Bold", "Bold"),
            ("Trebuchet MS", "Italic", "Normal"),
            ("Trebuchet MS", "Bold Italic", "Bold"),
        ];
        
        for (name, style, weight) in common_fonts.iter() {
            let font_info = FontInfo {
                name: name.to_string(),
                style: style.to_string(),
                weight: weight.to_string(),
                file_path: None,
            };
            
            // 重複を避ける
            if !self.fonts.iter().any(|f| f.name == font_info.name && f.style == font_info.style) {
                self.fonts.insert(font_info.name, font_info);
            }
        }
        
        // モノスペースフォント（コード表示用）
        #[cfg(target_os = "windows")]
        {
            // Windows固有のフォント
            self.fonts.insert("Consolas".to_string(), FontInfo {
                name: "Consolas".to_string(),
                style: "Regular".to_string(),
                weight: "Normal".to_string(),
                file_path: None,
            });
        }
        
        #[cfg(target_os = "macos")]
        {
            // macOS固有のフォント
            self.fonts.insert("Menlo".to_string(), FontInfo {
                name: "Menlo".to_string(),
                style: "Regular".to_string(),
                weight: "Normal".to_string(),
                file_path: None,
            });
        }
        
        #[cfg(target_os = "linux")]
        {
            // Linux固有のフォント
            self.fonts.insert("DejaVu Sans Mono".to_string(), FontInfo {
                name: "DejaVu Sans Mono".to_string(),
                style: "Book".to_string(),
                weight: "Normal".to_string(),
                file_path: None,
            });
        }
        
        // クロスプラットフォームのフォント
        self.fonts.insert("Liberation Mono".to_string(), FontInfo {
            name: "Liberation Mono".to_string(),
            style: "Regular".to_string(),
            weight: "Normal".to_string(),
            file_path: None,
        });
        
        self.fonts.insert("Courier".to_string(), FontInfo {
            name: "Courier".to_string(),
            style: "Regular".to_string(),
            weight: "Normal".to_string(),
            file_path: None,
        });
        
        debug!("フォールバック: {} フォントを追加しました", self.fonts.len());
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

    fn determine_font_styles(&self, family: &str) -> Vec<FontStyle> {
        let mut styles = Vec::new();
        
        // ファミリー名で検索してスタイルを特定
        let family_fonts: Vec<_> = self.fonts.iter()
            .filter(|(_, info)| info.family == family)
            .collect();
        
        // 標準フォントは常に存在するとみなす
        styles.push(FontStyle::Regular);
        
        // 名前に基づいてスタイルを判定
        for (name, _) in family_fonts {
            let name_lower = name.to_lowercase();
            
            if name_lower.contains("bold") && name_lower.contains("italic") {
                styles.push(FontStyle::BoldItalic);
            } else if name_lower.contains("bold") {
                styles.push(FontStyle::Bold);
            } else if name_lower.contains("italic") || name_lower.contains("oblique") {
                styles.push(FontStyle::Italic);
            }
        }
        
        // 重複を除去
        styles.sort_by_key(|s| match s {
            FontStyle::Regular => 0,
            FontStyle::Bold => 1,
            FontStyle::Italic => 2,
            FontStyle::BoldItalic => 3,
        });
        styles.dedup();
        
        styles
    }

    fn set_default_font(&mut self) {
        // 設定から優先フォントを取得
        if let Some(preferred_font) = self.settings.get_string("font.default") {
            if self.fonts.contains_key(&preferred_font) {
                self.current_font = preferred_font;
                return;
            }
        }
        
        // モノスペースフォントを優先
        let default_monospace_fonts = [
            "Consolas", "Menlo", "DejaVu Sans Mono", "Liberation Mono", 
            "Courier New", "Monaco", "Ubuntu Mono", "Fira Code"
        ];
        
        for font in &default_monospace_fonts {
            if self.fonts.contains_key(*font) {
                self.current_font = font.to_string();
                return;
            }
        }
        
        // フォールバック：最初のフォントを選択
        if let Some(font) = self.fonts.keys().next() {
            self.current_font = font.clone();
        } else {
            // フォントが見つからない場合のデフォルト
            self.current_font = "Monospace".to_string();
            self.add_fallback_fonts();
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