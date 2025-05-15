// src/ui/tabs.rs - NexusShellのタブ管理

use crossterm::event::{KeyCode, KeyModifiers, MouseEvent};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans as TuiSpans},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::ui::theme::ThemeManager;
use crate::ui::layout::{LayoutManager, LayoutType};
use crate::ui::panes::PaneManager;

// 外部コアモジュールのインポート
use nexusshell_executor::Executor;
use nexusshell_runtime::{Runtime, ExecutionResult};

/// タブ管理
pub struct TabManager {
    tabs: Vec<Tab>,
    active_index: usize,
    layout_manager: LayoutManager,
    /// シェルランタイム参照
    runtime: Option<Arc<Runtime>>,
    /// エグゼキュータ参照
    executor: Option<Arc<Executor>>,
    /// 実行結果送信チャネル
    result_sender: Option<mpsc::Sender<ExecutionResult>>,
}

/// タブ
pub struct Tab {
    title: String,
    pane_manager: PaneManager,
    layout_type: LayoutType,
    /// タブ固有の作業ディレクトリ
    working_dir: Option<String>,
    /// タブ固有の環境変数
    env_vars: HashMap<String, String>,
}

impl TabManager {
    /// 新しいタブマネージャーを作成
    pub fn new() -> Self {
        let mut manager = Self {
            tabs: Vec::new(),
            active_index: 0,
            layout_manager: LayoutManager::new(),
            runtime: None,
            executor: None,
            result_sender: None,
        };
        
        // 初期タブを作成
        manager.add_tab("Shell 1");
        
        manager
    }
    
    /// シェルランタイムを登録
    pub fn register_runtime(&mut self, runtime: Arc<Runtime>) {
        self.runtime = Some(runtime);
        
        // 既存のタブにもランタイムを設定
        for tab in &mut self.tabs {
            tab.pane_manager.set_runtime(runtime.clone());
        }
    }
    
    /// エグゼキュータを登録
    pub fn register_executor(&mut self, executor: Arc<Executor>) {
        self.executor = Some(executor);
        
        // 既存のタブにもエグゼキュータを設定
        for tab in &mut self.tabs {
            tab.pane_manager.set_executor(executor.clone());
        }
    }
    
    /// 実行結果チャネルを登録
    pub fn register_result_channel(&mut self, sender: mpsc::Sender<ExecutionResult>) {
        self.result_sender = Some(sender);
        
        // 既存のタブにも結果チャネルを設定
        for tab in &mut self.tabs {
            tab.pane_manager.set_result_channel(sender.clone());
        }
    }
    
    /// 新しいタブを追加
    pub fn add_tab(&mut self, title: &str) {
        let mut tab = Tab {
            title: title.to_string(),
            pane_manager: PaneManager::new(),
            layout_type: LayoutType::Full,
            working_dir: None,
            env_vars: HashMap::new(),
        };
        
        // ランタイムを設定
        if let Some(rt) = &self.runtime {
            tab.pane_manager.set_runtime(rt.clone());
        }
        
        // エグゼキュータを設定
        if let Some(exec) = &self.executor {
            tab.pane_manager.set_executor(exec.clone());
        }
        
        // 結果チャネルを設定
        if let Some(sender) = &self.result_sender {
            tab.pane_manager.set_result_channel(sender.clone());
        }
        
        self.tabs.push(tab);
        self.active_index = self.tabs.len() - 1;
    }
    
    /// タブ数を取得
    pub fn get_tab_count(&self) -> usize {
        self.tabs.len()
    }
    
    /// 次のタブに切り替え
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_index = (self.active_index + 1) % self.tabs.len();
        }
    }
    
    /// 前のタブに切り替え
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_index = if self.active_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_index - 1
            };
        }
    }
    
    /// タブを閉じる
    pub fn close_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        
        if self.tabs.len() > 1 {
            self.tabs.remove(self.active_index);
            if self.active_index >= self.tabs.len() {
                self.active_index = self.tabs.len() - 1;
            }
        }
    }
    
    /// タブの名前を変更
    pub fn rename_tab(&mut self, title: &str) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.title = title.to_string();
    }
    
    /// タブの作業ディレクトリを設定
    pub fn set_tab_working_dir(&mut self, dir: String) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.working_dir = Some(dir);
        tab.pane_manager.set_working_directory(&dir);
    }
    
    /// タブの環境変数を設定
    pub fn set_tab_env_var(&mut self, key: &str, value: &str) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.env_vars.insert(key.to_string(), value.to_string());
        tab.pane_manager.set_env_var(key, value);
    }
    
    /// 水平分割を実行
    pub fn split_horizontal(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.layout_type = LayoutType::HorizontalSplit;
        tab.pane_manager.split_horizontal();
        
        // 新しいペインにもランタイムとエグゼキュータを設定
        if let Some(rt) = &self.runtime {
            tab.pane_manager.set_runtime(rt.clone());
        }
        
        if let Some(exec) = &self.executor {
            tab.pane_manager.set_executor(exec.clone());
        }
        
        if let Some(sender) = &self.result_sender {
            tab.pane_manager.set_result_channel(sender.clone());
        }
    }
    
    /// 垂直分割を実行
    pub fn split_vertical(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.layout_type = LayoutType::VerticalSplit;
        tab.pane_manager.split_vertical();
        
        // 新しいペインにもランタイムとエグゼキュータを設定
        if let Some(rt) = &self.runtime {
            tab.pane_manager.set_runtime(rt.clone());
        }
        
        if let Some(exec) = &self.executor {
            tab.pane_manager.set_executor(exec.clone());
        }
        
        if let Some(sender) = &self.result_sender {
            tab.pane_manager.set_result_channel(sender.clone());
        }
    }
    
    /// 次のペインにフォーカス
    pub fn focus_next_pane(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.pane_manager.focus_next();
    }
    
    /// 前のペインにフォーカス
    pub fn focus_prev_pane(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.pane_manager.focus_prev();
    }
    
    /// アクティブペインにキー入力を送信
    pub fn send_key_to_active_pane(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.pane_manager.send_key(code, modifiers);
    }
    
    /// マウスイベント処理
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        if self.tabs.is_empty() {
            return;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.pane_manager.handle_mouse(event);
    }
    
    /// コマンドを実行
    pub async fn execute_command(&mut self, command: &str) -> bool {
        if self.tabs.is_empty() || self.runtime.is_none() {
            return false;
        }
        
        let tab = &mut self.tabs[self.active_index];
        tab.pane_manager.execute_command(command).await
    }
    
    /// UI描画
    pub fn render<B: Backend>(&self, frame: &mut Frame<B>, layouts: super::layout::Layouts, theme_manager: &ThemeManager) {
        if self.tabs.is_empty() {
            return;
        }
        
        // タブバーを描画
        self.render_tab_bar(frame, layouts.tab_bar(), theme_manager);
        
        // アクティブタブのコンテンツを描画
        let tab = &self.tabs[self.active_index];
        let pane_layout = self.layout_manager.get_pane_layout(layouts.content(), tab.layout_type.clone());
        tab.pane_manager.render(frame, &pane_layout, theme_manager);
        
        // ステータスバーを描画
        self.render_status_bar(frame, layouts.status_bar(), theme_manager);
    }
    
    /// タブバー描画
    fn render_tab_bar<B: Backend>(&self, frame: &mut Frame<B>, area: Rect, theme_manager: &ThemeManager) {
        let titles: Vec<TuiSpans> = self.tabs
            .iter()
            .map(|t| {
                TuiSpans::from(vec![
                    Span::styled(format!(" {} ", t.title), Style::default())
                ])
            })
            .collect();
        
        let tabs = Tabs::new(titles)
            .select(self.active_index)
            .style(theme_manager.get_styles().tab_inactive)
            .highlight_style(theme_manager.get_styles().tab_active)
            .divider(Span::raw("|"));
        
        frame.render_widget(tabs, area);
    }
    
    /// ステータスバー描画
    fn render_status_bar<B: Backend>(&self, frame: &mut Frame<B>, area: Rect, theme_manager: &ThemeManager) {
        let styles = theme_manager.get_styles();
        
        // 作業ディレクトリを取得
        let working_dir = if !self.tabs.is_empty() {
            self.tabs[self.active_index].working_dir.clone().unwrap_or_else(|| "~".to_string())
        } else {
            "~".to_string()
        };
        
        // 左側のステータス情報
        let left_status = TuiSpans::from(vec![
            Span::styled(" ", styles.status_bar),
            Span::styled(&working_dir, styles.status_bar_highlight),
            Span::styled(" ", styles.status_bar),
        ]);
        
        // 右側のショートカットヘルプ
        let right_help = TuiSpans::from(vec![
            Span::styled("Ctrl+T", styles.status_bar_key),
            Span::styled(" 新規タブ ", styles.status_bar),
            Span::styled("Ctrl+Tab", styles.status_bar_key),
            Span::styled(" タブ切替 ", styles.status_bar),
            Span::styled("Ctrl+\\", styles.status_bar_key),
            Span::styled(" 水平分割 ", styles.status_bar),
            Span::styled("Ctrl+-", styles.status_bar_key),
            Span::styled(" 垂直分割 ", styles.status_bar),
            Span::styled("Ctrl+Q", styles.status_bar_key),
            Span::styled(" 終了", styles.status_bar),
        ]);
        
        // ステータスエリアを分割
        let status_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);
        
        // 左右のステータス情報を描画
        let left_para = Paragraph::new(left_status)
            .style(styles.status_bar);
        
        let right_para = Paragraph::new(right_help)
            .style(styles.status_bar);
        
        frame.render_widget(left_para, status_layout[0]);
        frame.render_widget(right_para, status_layout[1]);
    }
} 