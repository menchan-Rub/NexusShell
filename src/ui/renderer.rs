// src/ui/renderer.rs - NexusShellのレンダリングエンジン

use tui::{
    backend::Backend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    Frame,
};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::collections::HashMap;
use crate::ui::fonts::FontRenderConfig;
use crate::ui::theme::ThemeManager;
use crate::ui::animations::AnimationManager;

const MAX_FPS_SAMPLES: usize = 60; // FPS履歴サンプル数を制限

/// レンダラー
pub struct Renderer {
    fps_counter: FpsCounter,
    animation_manager: AnimationManager,
    last_render_time: Instant,
    frame_count: u64,
    
    // レンダリング最適化用
    delta_time: Duration,
    render_stats: RenderStats,
    
    // 描画キャッシュ
    text_cache: TextCache,
}

/// FPSカウンター（メモリ使用量を最適化）
struct FpsCounter {
    frames: [Instant; MAX_FPS_SAMPLES], // 固定サイズ配列を使用
    frame_count: usize,
    last_fps: f32,
    last_update: Instant,
}

/// レンダリング統計
#[derive(Default)]
struct RenderStats {
    draw_calls: usize,
    text_chars: usize,
    rectangles: usize,
    frames_since_last_gc: usize,
}

/// テキスト描画キャッシュ
struct TextCache {
    lines: HashMap<u64, Arc<String>>,
    max_size: usize,
}

impl TextCache {
    fn new(max_size: usize) -> Self {
        Self {
            lines: HashMap::with_capacity(max_size),
            max_size,
        }
    }
    
    fn get_or_insert(&mut self, text: &str) -> Arc<String> {
        let hash = self.hash_text(text);
        
        if let Some(cached) = self.lines.get(&hash) {
            return cached.clone();
        }
        
        // キャッシュが一杯なら古いエントリを削除
        if self.lines.len() >= self.max_size {
            if let Some(oldest_key) = self.lines.keys().next().cloned() {
                self.lines.remove(&oldest_key);
            }
        }
        
        let new_entry = Arc::new(text.to_string());
        self.lines.insert(hash, new_entry.clone());
        new_entry
    }
    
    // 単純なハッシュ関数（高速化のため）
    fn hash_text(&self, text: &str) -> u64 {
        let mut hash: u64 = 5381;
        for c in text.chars() {
            hash = ((hash << 5) + hash) + c as u64;
        }
        hash
    }
    
    // キャッシュをクリア
    fn clear(&mut self) {
        self.lines.clear();
    }
}

impl Renderer {
    /// 新しいレンダラーを作成
    pub fn new() -> Self {
        Self {
            fps_counter: FpsCounter::new(),
            animation_manager: AnimationManager::new(),
            last_render_time: Instant::now(),
            frame_count: 0,
            delta_time: Duration::from_millis(16),
            render_stats: RenderStats::default(),
            text_cache: TextCache::new(1000), // キャッシュサイズ
        }
    }
    
    /// フレーム描画前の準備
    pub fn begin_frame<B: Backend>(&mut self, frame: &mut Frame<B>) {
        // フレームカウンター更新
        self.frame_count += 1;
        let now = Instant::now();
        self.fps_counter.add_frame(now);
        
        // 前フレームからの経過時間
        self.delta_time = now.duration_since(self.last_render_time);
        self.last_render_time = now;
        
        // アニメーション更新
        self.animation_manager.update(self.delta_time);
        
        // 統計情報リセット
        self.render_stats.draw_calls = 0;
        self.render_stats.text_chars = 0;
        self.render_stats.rectangles = 0;
        self.render_stats.frames_since_last_gc += 1;
        
        // 定期的にキャッシュをクリア（メモリ節約）
        if self.render_stats.frames_since_last_gc > 300 { // 5秒ごと（60FPS想定）
            self.text_cache.clear();
            self.render_stats.frames_since_last_gc = 0;
        }
    }
    
    /// フレーム描画後の処理
    pub fn end_frame(&mut self) {
        // フレーム完了時の処理
    }
    
    /// テキスト描画
    pub fn draw_text<B: Backend>(
        &mut self,
        frame: &mut Frame<B>,
        x: u16,
        y: u16,
        text: &str,
        style: Style,
        area: Rect,
    ) {
        self.render_stats.draw_calls += 1;
        
        // 表示領域外なら描画しない（早期リターン）
        if y < area.y || y >= area.y + area.height || x >= area.x + area.width {
            return;
        }
        
        // キャッシュから文字列を取得（頻繁に使われる文字列の場合メモリ使用量を削減）
        let cached_text = if text.len() > 3 {
            self.text_cache.get_or_insert(text)
        } else {
            // 短すぎる文字列はキャッシュしない
            Arc::new(text.to_string())
        };
        
        // 表示幅を計算
        let text_len = cached_text.len() as u16;
        
        // 表示範囲に収まる部分だけを描画
        if x < area.x + area.width {
            // 表示開始位置
            let start_x = if x < area.x { 
                (area.x - x) as usize
            } else { 
                0 
            };
            
            // 可視幅
            let visible_width = if x < area.x {
                area.width
            } else {
                (area.x + area.width - x).min(text_len)
            };
            
            // 表示する文字数がある場合のみ処理
            if visible_width > 0 && start_x < cached_text.len() {
                // 範囲内のテキストを抽出
                let end_x = (start_x + visible_width as usize).min(cached_text.len());
                let visible_text = &cached_text[start_x..end_x];
                
                self.render_stats.text_chars += visible_text.len();
                
                // バッファに直接書き込み
                let buffer = frame.buffer_mut();
                let mut cursor = x.max(area.x);
                
                for c in visible_text.chars() {
                    if cursor >= area.x + area.width {
                        break;
                    }
                    
                    buffer.get_mut(cursor, y)
                        .set_symbol(c.to_string())
                        .set_style(style);
                    
                    cursor += 1;
                }
            }
        }
    }
    
    /// 矩形描画
    pub fn draw_rect<B: Backend>(
        &mut self,
        frame: &mut Frame<B>,
        rect: Rect,
        style: Style,
    ) {
        self.render_stats.draw_calls += 1;
        self.render_stats.rectangles += 1;
        
        // 最小サイズ以下なら描画しない
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        
        // 矩形全体を指定スタイルで塗りつぶす（連続メモリアクセスに最適化）
        let buffer = frame.buffer_mut();
        
        // 効率的な行単位処理
        for y in rect.y..(rect.y + rect.height) {
            for x in rect.x..(rect.x + rect.width) {
                buffer.get_mut(x, y).set_style(style);
            }
        }
    }
    
    /// 境界線描画
    pub fn draw_border<B: Backend>(
        &mut self,
        frame: &mut Frame<B>,
        rect: Rect,
        style: Style,
    ) {
        self.render_stats.draw_calls += 1;
        
        // 最小サイズ以下なら描画しない
        if rect.width < 2 || rect.height < 2 {
            return;
        }
        
        let buffer = frame.buffer_mut();
        
        // 上下の横線
        for x in rect.x..(rect.x + rect.width) {
            buffer.get_mut(x, rect.y).set_symbol("─").set_style(style);
            buffer.get_mut(x, rect.y + rect.height - 1).set_symbol("─").set_style(style);
        }
        
        // 左右の縦線
        for y in rect.y..(rect.y + rect.height) {
            buffer.get_mut(rect.x, y).set_symbol("│").set_style(style);
            buffer.get_mut(rect.x + rect.width - 1, y).set_symbol("│").set_style(style);
        }
        
        // 四隅
        buffer.get_mut(rect.x, rect.y).set_symbol("┌").set_style(style);
        buffer.get_mut(rect.x + rect.width - 1, rect.y).set_symbol("┐").set_style(style);
        buffer.get_mut(rect.x, rect.y + rect.height - 1).set_symbol("└").set_style(style);
        buffer.get_mut(rect.x + rect.width - 1, rect.y + rect.height - 1).set_symbol("┘").set_style(style);
    }
    
    /// 現在のFPS取得
    pub fn get_fps(&self) -> f32 {
        self.fps_counter.get_fps()
    }
    
    /// 描画統計を取得
    pub fn get_stats(&self) -> (usize, usize, usize) {
        (
            self.render_stats.draw_calls,
            self.render_stats.text_chars,
            self.render_stats.rectangles
        )
    }
    
    /// アニメーションマネージャー取得
    pub fn animation_manager(&mut self) -> &mut AnimationManager {
        &mut self.animation_manager
    }
}

impl FpsCounter {
    /// 新しいFPSカウンターを作成
    fn new() -> Self {
        // 配列を現在時刻で初期化
        let now = Instant::now();
        let mut frames = [now; MAX_FPS_SAMPLES];
        
        Self {
            frames,
            frame_count: 0,
            last_fps: 0.0,
            last_update: now,
        }
    }
    
    /// フレームを追加
    fn add_frame(&mut self, time: Instant) {
        // 配列を循環バッファとして使用
        self.frames[self.frame_count % MAX_FPS_SAMPLES] = time;
        self.frame_count += 1;
        
        // 0.5秒ごとにFPSを更新
        if time.duration_since(self.last_update) >= Duration::from_millis(500) {
            // 直近のフレーム数を計算
            let oldest_index = if self.frame_count >= MAX_FPS_SAMPLES {
                (self.frame_count + 1) % MAX_FPS_SAMPLES
            } else {
                0
            };
            
            let oldest_time = self.frames[oldest_index];
            let elapsed = time.duration_since(oldest_time);
            
            if elapsed.as_secs_f32() > 0.0 {
                let counted_frames = self.frame_count.min(MAX_FPS_SAMPLES) as f32;
                self.last_fps = counted_frames / elapsed.as_secs_f32();
            }
            
            self.last_update = time;
        }
    }
    
    /// FPS取得
    fn get_fps(&self) -> f32 {
        self.last_fps
    }
} 