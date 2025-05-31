// src/ui/panes.rs - NexusShellのペイン管理

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseButton};
use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Span, Spans as TuiSpans},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::collections::{VecDeque, HashMap};
use std::path::PathBuf;
use tokio::sync::mpsc;
use crate::ui::theme::ThemeManager;

// コアモジュールを使用するためのインポート
use nexusshell_executor::Executor;
use nexusshell_runtime::{Runtime, ExecutionResult};

// 定数定義（コンパイル時に決定してメモリを節約）
const MAX_CONTENT_LINES: usize = 1000; // スクロールバックバッファの最大行数
const MAX_COMMAND_HISTORY: usize = 100; // コマンド履歴の最大数
const MAX_PANES: usize = 8; // 画面に表示可能な最大ペイン数

/// ペイン管理
pub struct PaneManager {
    panes: Vec<Pane>,
    active_index: usize,
    // 追加: シェルランタイム
    runtime: Option<Arc<Runtime>>,
    // 追加: エグゼキュータ
    executor: Option<Arc<Executor>>,
    // 追加: 実行結果送信チャネル
    result_sender: Option<mpsc::Sender<ExecutionResult>>,
}

/// ターミナルペイン
pub struct Pane {
    id: usize,
    content: VecDeque<String>, // VecDequeを使用して効率的なFIFOキューに
    title: String,
    command_history: VecDeque<String>, // 同様にVecDequeを使用
    current_input: String,
    cursor_position: usize,
    scroll_offset: usize,
    created_at: Instant,
    process: Option<std::process::Child>,
    is_focused: bool,
    // 追加: 作業ディレクトリ
    working_dir: PathBuf,
    // 追加: 環境変数
    env_vars: HashMap<String, String>,
    // 追加: 最後の終了コード
    last_exit_code: Option<i32>,
    // 追加: バックグラウンドジョブ
    background_jobs: Vec<JobInfo>,
    // 追加: 最後の描画領域
    last_render_area: Option<Rect>,
}

/// ジョブ情報
struct JobInfo {
    id: usize,
    command: String,
    pid: Option<u32>,
}

// スレッドセーフなカウンター
static PANE_ID_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl PaneManager {
    /// 新しいペインマネージャーを作成
    pub fn new() -> Self {
        let mut manager = Self {
            panes: Vec::with_capacity(MAX_PANES), // 最大数に基づいて容量を確保
            active_index: 0,
            runtime: None,
            executor: None,
            result_sender: None,
        };
        
        // 初期ペインを作成
        manager.add_pane();
        
        manager
    }
    
    /// シェルランタイムを設定
    pub fn set_runtime(&mut self, runtime: Arc<Runtime>) {
        self.runtime = Some(runtime);
    }
    
    /// エグゼキュータを設定
    pub fn set_executor(&mut self, executor: Arc<Executor>) {
        self.executor = Some(executor);
    }
    
    /// 実行結果チャネルを設定
    pub fn set_result_channel(&mut self, sender: mpsc::Sender<ExecutionResult>) {
        self.result_sender = Some(sender);
    }
    
    /// 作業ディレクトリを設定
    pub fn set_working_directory(&mut self, dir: &str) {
        if self.panes.is_empty() {
            return;
        }
        
        let pane = &mut self.panes[self.active_index];
        pane.working_dir = PathBuf::from(dir);
        
        // ディレクトリ変更をコンテンツに表示
        pane.content.push_back(format!("作業ディレクトリを変更: {}", dir));
    }
    
    /// 環境変数を設定
    pub fn set_env_var(&mut self, key: &str, value: &str) {
        if self.panes.is_empty() {
            return;
        }
        
        let pane = &mut self.panes[self.active_index];
        pane.env_vars.insert(key.to_string(), value.to_string());
    }
    
    /// 新しいペインを追加
    pub fn add_pane(&mut self) -> usize {
        // 最大数を超えないようにチェック
        if self.panes.len() >= MAX_PANES {
            return self.active_index;
        }
        
        let id = PANE_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        let mut content = VecDeque::with_capacity(MAX_CONTENT_LINES / 10); // 初期容量を小さく設定
        content.push_back("NexusShell v0.1.0".to_string());
        content.push_back("ようこそ！".to_string());
        
        // 作業ディレクトリをホームディレクトリに初期化
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        
        let pane = Pane {
            id,
            content,
            title: format!("ペイン #{}", id + 1),
            command_history: VecDeque::with_capacity(20), // 初期容量を小さく
            current_input: String::with_capacity(80), // 一般的なコマンド長に十分な容量
            cursor_position: 0,
            scroll_offset: 0,
            created_at: Instant::now(),
            process: None,
            is_focused: false,
            working_dir: home_dir,
            env_vars: HashMap::new(),
            last_exit_code: None,
            background_jobs: Vec::new(),
            last_render_area: None,
        };
        
        self.panes.push(pane);
        let index = self.panes.len() - 1;
        self.set_active_pane(index);
        
        index
    }
    
    /// 水平分割
    pub fn split_horizontal(&mut self) {
        self.add_pane();
    }
    
    /// 垂直分割
    pub fn split_vertical(&mut self) {
        self.add_pane();
    }
    
    /// アクティブペインを設定
    pub fn set_active_pane(&mut self, index: usize) {
        if index >= self.panes.len() {
            return;
        }
        
        // 以前のアクティブペインをフォーカス解除
        if self.active_index < self.panes.len() {
            self.panes[self.active_index].is_focused = false;
        }
        
        self.active_index = index;
        self.panes[self.active_index].is_focused = true;
    }
    
    /// 次のペインにフォーカス
    pub fn focus_next(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        
        let next_index = (self.active_index + 1) % self.panes.len();
        self.set_active_pane(next_index);
    }
    
    /// 前のペインにフォーカス
    pub fn focus_prev(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        
        let prev_index = if self.active_index == 0 {
            self.panes.len() - 1
        } else {
            self.active_index - 1
        };
        
        self.set_active_pane(prev_index);
    }
    
    /// マウスイベント処理
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        // マウスクリックでペイン選択
        if let MouseEvent::Down(MouseButton::Left, x, y, _) = event {
            // クリック位置からどのペインがクリックされたかを判定
            if let Some(pane_index) = self.find_pane_at(x, y) {
                // すでにアクティブなペインの場合は何もしない
                if pane_index == self.active_index {
                    return;
                }
                
                // ペインをアクティブにする
                self.set_active_pane(pane_index);
            }
        }
        // 現在アクティブなペインにイベントを転送
        if !self.panes.is_empty() {
            let pane = &mut self.panes[self.active_index];
            pane.handle_mouse_event(event);
        }
    }
    
    /// 指定された座標にあるペインのインデックスを取得
    fn find_pane_at(&self, x: u16, y: u16) -> Option<usize> {
        // 各ペインの領域を確認し、座標が含まれるペインを返す
        for (index, pane) in self.panes.iter().enumerate() {
            if let Some(area) = pane.last_render_area {
                // ペインの表示領域内かどうかを判定
                if x >= area.x && x < area.x + area.width &&
                   y >= area.y && y < area.y + area.height {
                    return Some(index);
                }
            }
        }
        None
    }
    
    /// キー入力処理
    pub fn send_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if self.panes.is_empty() {
            return;
        }
        
        let pane = &mut self.panes[self.active_index];
        
        match code {
            KeyCode::Char(c) => {
                pane.current_input.insert(pane.cursor_position, c);
                pane.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if pane.cursor_position > 0 {
                    pane.current_input.remove(pane.cursor_position - 1);
                    pane.cursor_position -= 1;
                }
            }
            KeyCode::Delete => {
                if pane.cursor_position < pane.current_input.len() {
                    pane.current_input.remove(pane.cursor_position);
                }
            }
            KeyCode::Left => {
                if pane.cursor_position > 0 {
                    pane.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if pane.cursor_position < pane.current_input.len() {
                    pane.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                pane.cursor_position = 0;
            }
            KeyCode::End => {
                pane.cursor_position = pane.current_input.len();
            }
            KeyCode::Enter => {
                self.execute_command_internal(self.active_index);
            }
            KeyCode::Up => {
                // コマンド履歴をさかのぼる
                if !pane.command_history.is_empty() {
                    if let Some(cmd) = pane.command_history.back() {
                        pane.current_input = cmd.clone();
                        pane.cursor_position = pane.current_input.len();
                    }
                }
            }
            KeyCode::Down => {
                // コマンド履歴を進める
                pane.current_input.clear();
                pane.cursor_position = 0;
            }
            // ページスクロール
            KeyCode::PageUp => {
                if pane.content.len() > pane.scroll_offset + 10 {
                    pane.scroll_offset += 10;
                } else {
                    pane.scroll_offset = pane.content.len();
                }
            }
            KeyCode::PageDown => {
                if pane.scroll_offset > 10 {
                    pane.scroll_offset -= 10;
                } else {
                    pane.scroll_offset = 0;
                }
            }
            // Tab補完（未実装）
            KeyCode::Tab => {
                // タブ補完を行う
            }
            _ => {}
        }
    }
    
    /// コマンド実行（外部から呼び出し可能）
    pub async fn execute_command(&mut self, command: &str) -> bool {
        if self.panes.is_empty() {
            return false;
        }
        
        let pane_index = self.active_index;
        let pane = &mut self.panes[pane_index];
        
        // コマンド入力を設定して実行
        pane.current_input = command.to_string();
        pane.cursor_position = pane.current_input.len();
        
        self.execute_command_internal(pane_index);
        
        // 結果が出るまで少し待機
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        true
    }
    
    /// コマンド実行（内部実装）
    fn execute_command_internal(&mut self, pane_index: usize) {
        if pane_index >= self.panes.len() {
            return;
        }
        
        let pane = &mut self.panes[pane_index];
        let command = pane.current_input.clone();
        
        if command.is_empty() {
            pane.content.push_back("> ".to_string());
        } else {
            // 作業ディレクトリを表示
            let wd_display = pane.working_dir.to_string_lossy();
            pane.content.push_back(format!("[{}]> {}", wd_display, command));
            
            // コマンド履歴に追加（最大数を超えないように）
            pane.command_history.push_back(command.clone());
            if pane.command_history.len() > MAX_COMMAND_HISTORY {
                pane.command_history.pop_front();
            }
            
            // 内部コマンド処理
            if command == "clear" || command == "cls" {
                pane.content.clear();
            } else if command == "exit" || command == "quit" {
                pane.content.push_back("NexusShellを終了するには、Ctrl+Qを押してください".to_string());
            } else if command.starts_with("cd ") {
                // cd コマンドの処理
                let path = command[3..].trim();
                let new_path = if path.starts_with('~') {
                    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                    if path.len() > 1 {
                        home.join(&path[2..])
                    } else {
                        home
                    }
                } else {
                    PathBuf::from(path)
                };
                
                let target_path = if new_path.is_absolute() {
                    new_path
                } else {
                    pane.working_dir.join(new_path)
                };
                
                if target_path.exists() && target_path.is_dir() {
                    pane.working_dir = target_path;
                    pane.content.push_back(format!("作業ディレクトリを変更: {}", pane.working_dir.to_string_lossy()));
                } else {
                    pane.content.push_back(format!("エラー: ディレクトリが存在しません: {}", new_path.to_string_lossy()));
                }
            } else if command == "pwd" {
                // 現在のディレクトリを表示
                pane.content.push_back(format!("{}", pane.working_dir.to_string_lossy()));
            } else if command == "history" {
                // コマンド履歴を表示
                pane.content.push_back("コマンド履歴:".to_string());
                for (i, cmd) in pane.command_history.iter().enumerate() {
                    pane.content.push_back(format!("{}: {}", i + 1, cmd));
                }
            } else if command == "jobs" {
                // バックグラウンドジョブを表示
                if pane.background_jobs.is_empty() {
                    pane.content.push_back("バックグラウンドジョブはありません".to_string());
                } else {
                    pane.content.push_back("バックグラウンドジョブ:".to_string());
                    for job in &pane.background_jobs {
                        pane.content.push_back(format!("[{}] {}", job.id, job.command));
                    }
                }
            } else if let Some(runtime) = &self.runtime {
                // ランタイムを使ってコマンドを実行
                let runtime_clone = runtime.clone();
                let command_clone = command.clone();
                let working_dir_clone = pane.working_dir.clone();
                let env_vars_clone = pane.env_vars.clone();
                
                // 結果チャンネル
                let result_sender = self.result_sender.clone();
                
                let pane_clone = unsafe { &mut *(pane as *mut Pane) };
                
                // 結果処理用のクロージャ
                let handle_result = move |result: Result<nexusshell_runtime::ExecutionResult, anyhow::Error>| {
                    match result {
                        Ok(exec_result) => {
                            // 実行結果を表示
                            if let Some(output) = exec_result.output {
                                let output_str = String::from_utf8_lossy(&output);
                                for line in output_str.lines() {
                                    pane_clone.content.push_back(line.to_string());
                                }
                            }
                            
                            // エラー出力を表示
                            if let Some(error) = exec_result.error {
                                let error_str = String::from_utf8_lossy(&error);
                                for line in error_str.lines() {
                                    pane_clone.content.push_back(format!("エラー: {}", line));
                                }
                            }
                            
                            // 終了コードを保存
                            pane_clone.last_exit_code = exec_result.exit_code;
                            
                            // スクロールオフセットをリセット
                            pane_clone.scroll_offset = 0;
                            
                            // 結果を送信
                            if let Some(sender) = &result_sender {
                                let _ = sender.try_send(exec_result);
                            }
                        },
                        Err(e) => {
                            // エラーメッセージを表示
                            pane_clone.content.push_back(format!("コマンド実行エラー: {}", e));
                            pane_clone.last_exit_code = Some(1);
                        }
                    }
                    
                    // 入力をクリア
                    pane_clone.current_input.clear();
                    pane_clone.cursor_position = 0;
                };
                
                // バックグラウンド実行かどうかをチェック
                let is_background = command.ends_with(" &");
                let command_to_run = if is_background {
                    command[..command.len() - 2].trim().to_string()
                } else {
                    command.clone()
                };
                
                if is_background {
                    // バックグラウンドで実行
                    let job_id = pane.background_jobs.len() + 1;
                    pane.background_jobs.push(JobInfo {
                        id: job_id,
                        command: command_to_run.clone(),
                        pid: None,
                    });
                    
                    pane.content.push_back(format!("[{}] バックグラウンドで実行中: {}", job_id, command_to_run));
                    
                    // 入力をクリア
                    pane.current_input.clear();
                    pane.cursor_position = 0;
                    
                    // Tokioランタイムでコマンドを実行
                    tokio::spawn(async move {
                        let result = runtime_clone.execute_command(&command_to_run).await;
                        handle_result(result);
                    });
                } else {
                    // 同期実行
                    // 入力をクリア
                    pane.current_input.clear();
                    pane.cursor_position = 0;
                    
                    // Tokioランタイムでコマンドを実行
                    tokio::spawn(async move {
                        let result = runtime_clone.execute_command(&command_to_run).await;
                        handle_result(result);
                    });
                }
            } else {
                // フォールバック実行（ランタイムがない場合）
                self.fallback_command_execution(pane_index, &command);
            }
        }
    }
    
    /// フォールバックコマンド実行（ランタイムがない場合）
    fn fallback_command_execution(&mut self, pane_index: usize, command: &str) {
        let pane = &mut self.panes[pane_index];
        
        // 単純なコマンド実行（Windows/Linux対応）
        #[cfg(windows)]
        let mut cmd = Command::new("cmd");
        #[cfg(windows)]
        cmd.args(&["/C", command]);
        
        #[cfg(not(windows))]
        let mut cmd = Command::new("sh");
        #[cfg(not(windows))]
        cmd.args(&["-c", command]);
        
        // 作業ディレクトリを設定
        cmd.current_dir(&pane.working_dir);
        
        // 環境変数を設定
        for (key, value) in &pane.env_vars {
            cmd.env(key, value);
        }
        
        // 標準出力と標準エラーを取得
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        match cmd.output() {
            Ok(output) => {
                // 標準出力を表示
                if !output.stdout.is_empty() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        pane.content.push_back(line.to_string());
                    }
                }
                
                // 標準エラーを表示
                if !output.stderr.is_empty() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    for line in stderr.lines() {
                        pane.content.push_back(format!("エラー: {}", line));
                    }
                }
                
                // 終了コードを保存
                pane.last_exit_code = output.status.code();
            }
            Err(e) => {
                pane.content.push_back(format!("コマンド実行エラー: {}", e));
                pane.last_exit_code = Some(1);
            }
        }
        
        // バッファが大きすぎる場合は古い行を削除
        while pane.content.len() > MAX_CONTENT_LINES {
            pane.content.pop_front();
        }
    }
    
    /// UI描画
    pub fn render<B: Backend>(&mut self, frame: &mut Frame<B>, areas: &[Rect], theme_manager: &ThemeManager) {
        for (i, area) in areas.iter().enumerate() {
            if i >= self.panes.len() {
                break;
            }
            
            let pane = &mut self.panes[i];
            
            // 描画領域を保存
            pane.last_render_area = Some(*area);
            
            // ペインのタイトルにプロンプト情報を追加
            let title = format!("{} [{}]", 
                pane.title, 
                pane.working_dir.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("~")
            );
            
            // ペインのスタイルを決定
            let border_style = if pane.is_focused {
                theme_manager.get_styles().pane_active
            } else {
                theme_manager.get_styles().pane_inactive
            };
            
            let block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style);
            
            // コンテンツ領域を計算（ボーダーの内側）
            let inner_area = block.inner(*area);
            
            // コンテンツの表示行数を計算
            let content_height = inner_area.height as usize;
            
            // スクロール制限
            let mut scroll_offset = pane.scroll_offset;
            let max_scroll = if pane.content.len() > content_height {
                pane.content.len() - content_height
            } else {
                0
            };
            if scroll_offset > max_scroll {
                scroll_offset = max_scroll;
            }
            
            // 表示行を準備
            let displayed_lines: Vec<TuiSpans> = pane.content.iter()
                .skip(pane.content.len().saturating_sub(content_height + scroll_offset))
                .take(content_height.saturating_sub(1)) // プロンプト行のためのスペースを確保
                .map(|line| {
                    TuiSpans::from(Span::raw(line))
                })
                .collect();
            
            // プロンプト行を追加
            let mut all_lines = displayed_lines;
            let prompt_line = format!("$ {}", pane.current_input);
            let cursor_pos = 2 + pane.cursor_position; // $とスペースを考慮
            
            // カーソル位置が画面外にならないように
            let visible_prompt = if prompt_line.len() > inner_area.width as usize {
                let start = if cursor_pos > inner_area.width as usize {
                    cursor_pos - inner_area.width as usize + 1
                } else {
                    0
                };
                &prompt_line[start..]
            } else {
                &prompt_line
            };
            
            all_lines.push(TuiSpans::from(Span::raw(visible_prompt)));
            
            // コンテンツを描画
            let content = Paragraph::new(all_lines).block(block);
            frame.render_widget(content, *area);
            
            // カーソルを描画（アクティブなペインのみ）
            if pane.is_focused {
                // カーソル位置が見えている場合のみ表示
                if cursor_pos < inner_area.width as usize {
                    frame.set_cursor(
                        inner_area.x + cursor_pos as u16,
                        inner_area.y + inner_area.height - 1 // 最後の行
                    );
                }
            }
        }
    }
}

impl Pane {
    /// マウスイベントを処理
    pub fn handle_mouse_event(&mut self, event: MouseEvent) {
        match event {
            MouseEvent::Down(MouseButton::Left, x, y, _) => {
                // ペイン内の座標に変換
                if let Some(area) = self.last_render_area {
                    let relative_x = x.saturating_sub(area.x);
                    let relative_y = y.saturating_sub(area.y);
                    
                    // 入力部分のクリック処理
                    let input_area_y = area.height.saturating_sub(2);
                    if relative_y == input_area_y {
                        // 現在の入力行上でのクリック
                        let prompt_len = 2; // "> " のサイズ
                        if relative_x >= prompt_len {
                            let content_x = relative_x.saturating_sub(prompt_len);
                            // 文字位置の計算（簡易実装、マルチバイト文字の対応は省略）
                            self.cursor_position = content_x.min(self.current_input.len() as u16) as usize;
                        }
                    } else if relative_y < input_area_y {
                        // コンテンツ部分でのスクロール操作（クリックした位置に応じてスクロール）
                        let content_lines = self.content.len();
                        let visible_lines = area.height.saturating_sub(3) as usize;
                        
                        if content_lines > visible_lines {
                            let scroll_ratio = relative_y as f32 / input_area_y as f32;
                            let scroll_position = ((content_lines - visible_lines) as f32 * scroll_ratio) as usize;
                            self.scroll_offset = (content_lines - visible_lines).saturating_sub(scroll_position);
                        }
                    }
                }
            },
            MouseEvent::ScrollDown(_, _, _) => {
                // 下にスクロール（スクロールオフセットを減らす）
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            },
            MouseEvent::ScrollUp(_, _, _) => {
                // 上にスクロール（スクロールオフセットを増やす）
                if self.scroll_offset < self.content.len() {
                    self.scroll_offset += 1;
                }
            },
            _ => {} // その他のマウスイベントは無視
        }
    }
} 