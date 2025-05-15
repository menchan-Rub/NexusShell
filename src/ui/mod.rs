// src/ui/mod.rs - NexusShellのUIモジュール
pub mod renderer;
pub mod theme;
pub mod layout;
pub mod fonts;
pub mod tabs;
pub mod panes;
pub mod animations;
pub mod settings;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use tui::backend::CrosstermBackend;
use tui::Terminal;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::cell::RefCell;

/// NexusShellのターミナルUI管理
pub struct NexusTerminal {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    theme_manager: theme::ThemeManager,
    layout_manager: layout::LayoutManager,
    tab_manager: tabs::TabManager,
    settings: Arc<settings::Settings>,
    running: bool,
    last_render: Instant,
    frame_times: RefCell<Vec<Duration>>, // パフォーマンス測定用
    frame_count: usize,
    renderer: renderer::Renderer,
}

impl NexusTerminal {
    /// 新しいNexusTerminalインスタンスを作成
    pub fn new() -> Result<Self, io::Error> {
        // クロスタームバックエンドを初期化
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
        
        // ターミナル初期化
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        
        // 設定を読み込み
        let settings = match settings::Settings::load() {
            Ok(s) => Arc::new(s),
            Err(_) => Arc::new(settings::Settings::default()),
        };
        
        // 各種マネージャー初期化
        let theme_manager = theme::ThemeManager::new(settings.clone());
        let layout_manager = layout::LayoutManager::new();
        let tab_manager = tabs::TabManager::new();
        let renderer = renderer::Renderer::new();
        
        Ok(Self {
            terminal,
            theme_manager,
            layout_manager,
            tab_manager,
            settings,
            running: false,
            last_render: Instant::now(),
            frame_times: RefCell::new(Vec::with_capacity(100)),
            frame_count: 0,
            renderer,
        })
    }
    
    /// ターミナルUIのメインループを実行
    pub fn run(&mut self) -> Result<(), io::Error> {
        self.running = true;
        
        // 初期レンダリング
        self.render()?;
        
        // メインループ
        while self.running {
            // フレームレート制限（設定から取得）
            let fps_limit = self.settings.performance.fps_limit as u64;
            let target_frame_time = Duration::from_micros(1_000_000 / fps_limit);
            let elapsed = self.last_render.elapsed();
            
            if elapsed < target_frame_time {
                // CPU使用率を下げるためにスリープ
                let sleep_time = target_frame_time - elapsed;
                std::thread::sleep(sleep_time);
            }
            
            // イベント処理（タイムアウト付き）
            if crossterm::event::poll(Duration::from_millis(1))? {
                self.handle_event(event::read()?)?;
            }
            
            // フレーム描画
            let render_start = Instant::now();
            self.render()?;
            let render_time = render_start.elapsed();
            
            // パフォーマンス測定
            self.frame_count += 1;
            self.frame_times.borrow_mut().push(render_time);
            if self.frame_times.borrow().len() > 100 {
                self.frame_times.borrow_mut().remove(0);
            }
            
            self.last_render = Instant::now();
        }
        
        // 終了時の後片付け
        self.cleanup()?;
        
        Ok(())
    }
    
    /// イベント処理
    fn handle_event(&mut self, event: Event) -> Result<(), io::Error> {
        match event {
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                match (code, modifiers) {
                    // 終了
                    (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                        self.running = false;
                    },
                    // 新しいタブ
                    (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                        self.tab_manager.add_tab(&format!("Shell {}", self.tab_manager.get_tab_count() + 1));
                    },
                    // タブ切り替え
                    (KeyCode::Tab, KeyModifiers::CONTROL) => {
                        self.tab_manager.next_tab();
                    },
                    (KeyCode::BackTab, KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                        self.tab_manager.prev_tab();
                    },
                    // ペイン分割（水平）
                    (KeyCode::Char('\\'), KeyModifiers::CONTROL) => {
                        self.tab_manager.split_horizontal();
                    },
                    // ペイン分割（垂直）
                    (KeyCode::Char('-'), KeyModifiers::CONTROL) => {
                        self.tab_manager.split_vertical();
                    },
                    // ペイン間移動
                    (KeyCode::Right, KeyModifiers::ALT) => {
                        self.tab_manager.focus_next_pane();
                    },
                    (KeyCode::Left, KeyModifiers::ALT) => {
                        self.tab_manager.focus_prev_pane();
                    },
                    // テーマ切り替え
                    (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                        let themes = ["Dark", "Light", "Dracula", "Nord", "Solarized Dark", "Tokyo Night"];
                        let current = &self.theme_manager.current_theme;
                        let idx = themes.iter().position(|t| t == current).unwrap_or(0);
                        let next = (idx + 1) % themes.len();
                        self.theme_manager.switch_theme(themes[next]);
                        
                        // テーマ切り替え後にスタイルを再生成してキャッシュを更新
                        let styles = self.theme_manager.get_styles();
                        self.theme_manager.update_cache(styles);
                    },
                    // アクティブペインに入力を転送
                    _ => {
                        self.tab_manager.send_key_to_active_pane(code, modifiers);
                    },
                }
            },
            Event::Mouse(mouse_event) => {
                self.tab_manager.handle_mouse(mouse_event);
            },
            Event::Resize(width, height) => {
                // 画面サイズが変わった場合にキャッシュを無効化
                self.layout_manager.invalidate_cache();
                self.terminal.resize(tui::layout::Rect::new(0, 0, width, height))?;
            },
        }
        
        Ok(())
    }
    
    /// UIをレンダリング
    fn render(&mut self) -> Result<(), io::Error> {
        self.terminal.draw(|frame| {
            // フレーム描画前の準備
            self.renderer.begin_frame(frame);
            
            // 画面全体を取得
            let size = frame.size();
            
            // レイアウトを取得
            let layout = self.layout_manager.get_layout(size);
            
            // タブバーとタブコンテンツを描画
            self.tab_manager.render(frame, layout, &self.theme_manager);
            
            // FPS表示（デバッグモード時のみ）
            #[cfg(debug_assertions)]
            {
                let fps = self.calculate_fps();
                let fps_text = format!("{:.1} FPS", fps);
                self.renderer.draw_text(
                    frame,
                    size.width - fps_text.len() as u16 - 1,
                    size.height - 1,
                    &fps_text,
                    tui::style::Style::default().fg(tui::style::Color::Yellow),
                    size,
                );
            }
            
            // フレーム描画後の処理
            self.renderer.end_frame();
        })?;
        
        Ok(())
    }
    
    /// FPS計算
    fn calculate_fps(&self) -> f32 {
        let times = self.frame_times.borrow();
        if times.is_empty() {
            return 0.0;
        }
        
        let total: Duration = times.iter().sum();
        let avg_frame_time = total.as_secs_f32() / times.len() as f32;
        
        if avg_frame_time > 0.0 {
            1.0 / avg_frame_time
        } else {
            0.0
        }
    }
    
    /// 終了時のクリーンアップ
    fn cleanup(&mut self) -> Result<(), io::Error> {
        // 設定を保存
        if let Err(e) = self.settings.save() {
            eprintln!("Failed to save settings: {}", e);
        }
        
        // ターミナル状態を復元
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        )?;
        
        Ok(())
    }
} 