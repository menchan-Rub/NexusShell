// src/ui/layout.rs - NexusShellのレイアウト管理

use tui::layout::{Constraint, Direction, Layout, Rect};
use std::collections::HashMap;

/// レイアウト管理
pub struct LayoutManager {
    split_ratio: f32,
    // レイアウトキャッシュ（サイズごとに結果をキャッシュ）
    layout_cache: HashMap<(u16, u16), Layouts>,
    pane_cache: HashMap<(u16, u16, LayoutType), Vec<Rect>>,
    cache_enabled: bool,
}

/// レイアウト種別
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum LayoutType {
    /// 画面全体
    Full,
    /// 水平分割
    HorizontalSplit,
    /// 垂直分割
    VerticalSplit,
    /// グリッド（2x2）
    Grid,
}

impl LayoutManager {
    /// 新しいレイアウトマネージャーを作成
    pub fn new() -> Self {
        Self {
            split_ratio: 0.5,
            layout_cache: HashMap::new(),
            pane_cache: HashMap::new(),
            cache_enabled: true,
        }
    }
    
    /// キャッシュを無効化
    pub fn invalidate_cache(&mut self) {
        self.layout_cache.clear();
        self.pane_cache.clear();
    }
    
    /// キャッシュの有効/無効を設定
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.cache_enabled = enabled;
        if !enabled {
            self.invalidate_cache();
        }
    }
    
    /// 分割比率を設定
    pub fn set_split_ratio(&mut self, ratio: f32) {
        let new_ratio = ratio.max(0.1).min(0.9);
        if (new_ratio - self.split_ratio).abs() > 0.01 {
            self.split_ratio = new_ratio;
            // 比率が変わったらキャッシュをクリア
            self.pane_cache.clear();
        }
    }
    
    /// 画面レイアウトを取得
    pub fn get_layout(&mut self, size: Rect) -> Layouts {
        // キャッシュが有効で、同じサイズのレイアウトがキャッシュにある場合はそれを返す
        if self.cache_enabled {
            let key = (size.width, size.height);
            if let Some(layout) = self.layout_cache.get(&key) {
                return layout.clone();
            }
        }
        
        // 新しいレイアウトを計算
        let layout = Layouts {
            // 全体を上部（タブバー）と下部（コンテンツ）に分割
            main: Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // タブバー用の高さ
                    Constraint::Min(0),    // メインコンテンツ
                    Constraint::Length(1), // ステータスバー
                ])
                .split(size),
        };
        
        // キャッシュに保存
        if self.cache_enabled {
            let key = (size.width, size.height);
            self.layout_cache.insert(key, layout.clone());
        }
        
        layout
    }
    
    /// ペイン分割レイアウトを取得
    pub fn get_pane_layout(&mut self, area: Rect, layout_type: LayoutType) -> Vec<Rect> {
        // キャッシュが有効で、同じ条件のレイアウトがキャッシュにある場合はそれを返す
        if self.cache_enabled {
            let key = (area.width, area.height, layout_type.clone());
            if let Some(layout) = self.pane_cache.get(&key) {
                return layout.clone();
            }
        }
        
        // 新しいレイアウトを計算
        let result = match layout_type {
            LayoutType::Full => vec![area],
            LayoutType::HorizontalSplit => {
                // 水平分割（上下）- 整数計算のみを使用
                let split_height = ((area.height as f32 * self.split_ratio) as u16).max(1);
                let bottom_height = area.height.saturating_sub(split_height).max(1);
                
                vec![
                    Rect::new(area.x, area.y, area.width, split_height),
                    Rect::new(area.x, area.y + split_height, area.width, bottom_height),
                ]
            },
            LayoutType::VerticalSplit => {
                // 垂直分割（左右）- 整数計算のみを使用
                let split_width = ((area.width as f32 * self.split_ratio) as u16).max(1);
                let right_width = area.width.saturating_sub(split_width).max(1);
                
                vec![
                    Rect::new(area.x, area.y, split_width, area.height),
                    Rect::new(area.x + split_width, area.y, right_width, area.height),
                ]
            },
            LayoutType::Grid => {
                // 2x2グリッド - 整数計算のみを使用
                let half_height = area.height / 2;
                let remainder_height = area.height % 2;
                let half_width = area.width / 2;
                let remainder_width = area.width % 2;
                
                vec![
                    // 左上
                    Rect::new(area.x, area.y, half_width, half_height),
                    // 右上
                    Rect::new(area.x + half_width, area.y, half_width + remainder_width, half_height),
                    // 左下
                    Rect::new(area.x, area.y + half_height, half_width, half_height + remainder_height),
                    // 右下
                    Rect::new(area.x + half_width, area.y + half_height, half_width + remainder_width, half_height + remainder_height),
                ]
            },
        };
        
        // キャッシュに保存
        if self.cache_enabled {
            let key = (area.width, area.height, layout_type);
            self.pane_cache.insert(key, result.clone());
        }
        
        result
    }
}

/// 画面レイアウト
#[derive(Clone)]
pub struct Layouts {
    /// メインレイアウト
    pub main: Vec<Rect>,
}

impl Layouts {
    /// タブバー領域を取得
    pub fn tab_bar(&self) -> Rect {
        self.main[0]
    }
    
    /// メインコンテンツ領域を取得
    pub fn content(&self) -> Rect {
        self.main[1]
    }
    
    /// ステータスバー領域を取得
    pub fn status_bar(&self) -> Rect {
        self.main[2]
    }
} 