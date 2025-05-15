// completer.rs
// NexusShellのコマンド補完エンジン
// 高度で文脈を意識した補完機能を提供

use crate::{
    AstNode, Token, TokenKind, Span, ParserContext, ParserError, Result,
    lexer::{NexusLexer, CompletionSuggestion, CompletionType},
    parser::RecursiveDescentParser,
    grammar::GrammarManager
};

use std::collections::{HashMap, HashSet, BTreeMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use log::{debug, trace, info, warn};

/// 補完の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    /// コマンド補完
    Command,
    /// ファイルパス補完
    FilePath,
    /// 変数補完
    Variable,
    /// オプション/フラグ補完
    Option,
    /// 引数補完（コマンド固有）
    Argument,
    /// 構文補完（if/for/whileなど）
    Syntax,
    /// カスタム補完（プラグイン提供）
    Custom(String),
}

/// 補完結果
#[derive(Debug, Clone)]
pub struct CompletionResult {
    /// 補完候補リスト
    pub suggestions: Vec<CompletionSuggestion>,
    /// 補完を適用する位置
    pub replace_range: (usize, usize),
    /// 補完の種類
    pub kind: CompletionKind,
    /// 補完コンテキスト
    pub context: String,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl CompletionResult {
    /// 新しい補完結果を作成
    pub fn new(
        suggestions: Vec<CompletionSuggestion>,
        replace_range: (usize, usize),
        kind: CompletionKind,
    ) -> Self {
        Self {
            suggestions,
            replace_range,
            kind,
            context: String::new(),
            metadata: HashMap::new(),
        }
    }

    /// コンテキストを設定
    pub fn with_context(mut self, context: &str) -> Self {
        self.context = context.to_string();
        self
    }

    /// メタデータを追加
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// 候補が存在するかどうか
    pub fn has_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
    }

    /// 候補を優先度順にソート
    pub fn sort_by_priority(&mut self) {
        self.suggestions.sort_by_key(|s| s.priority);
    }

    /// 候補を関連度順にソート
    pub fn sort_by_relevance(&mut self, query: &str) {
        self.suggestions.sort_by(|a, b| {
            // 前方一致を優先
            let a_starts = a.text.starts_with(query);
            let b_starts = b.text.starts_with(query);
            
            if a_starts && !b_starts {
                return std::cmp::Ordering::Less;
            }
            if !a_starts && b_starts {
                return std::cmp::Ordering::Greater;
            }
            
            // 次に部分一致
            let a_contains = a.text.contains(query);
            let b_contains = b.text.contains(query);
            
            if a_contains && !b_contains {
                return std::cmp::Ordering::Less;
            }
            if !a_contains && b_contains {
                return std::cmp::Ordering::Greater;
            }
            
            // 最後に優先度
            a.priority.cmp(&b.priority)
        });
    }
}

/// 補完コンテキスト
#[derive(Debug, Clone)]
pub struct CompletionContext {
    /// 入力テキスト
    pub input: String,
    /// カーソル位置
    pub cursor_position: usize,
    /// 環境変数
    pub env_variables: HashMap<String, String>,
    /// 利用可能なコマンド
    pub available_commands: Vec<String>,
    /// コマンド別のオプション定義
    pub command_options: HashMap<String, Vec<String>>,
    /// コマンド履歴
    pub command_history: Vec<String>,
    /// パス検索キャッシュ
    pub path_cache: Option<Arc<RwLock<HashMap<String, Vec<String>>>>>,
}

impl CompletionContext {
    /// 新しい補完コンテキストを作成
    pub fn new(input: &str, cursor_position: usize) -> Self {
        Self {
            input: input.to_string(),
            cursor_position,
            env_variables: HashMap::new(),
            available_commands: Vec::new(),
            command_options: HashMap::new(),
            command_history: Vec::new(),
            path_cache: None,
        }
    }

    /// 環境変数を設定
    pub fn with_env_variables(mut self, env_variables: HashMap<String, String>) -> Self {
        self.env_variables = env_variables;
        self
    }

    /// 利用可能なコマンドを設定
    pub fn with_available_commands(mut self, available_commands: Vec<String>) -> Self {
        self.available_commands = available_commands;
        self
    }

    /// コマンドオプションを設定
    pub fn with_command_options(mut self, command_options: HashMap<String, Vec<String>>) -> Self {
        self.command_options = command_options;
        self
    }

    /// コマンド履歴を設定
    pub fn with_command_history(mut self, command_history: Vec<String>) -> Self {
        self.command_history = command_history;
        self
    }

    /// パスキャッシュを設定
    pub fn with_path_cache(mut self, path_cache: Arc<RwLock<HashMap<String, Vec<String>>>>) -> Self {
        self.path_cache = Some(path_cache);
        self
    }

    /// カーソル位置のトークンを取得
    pub fn get_token_at_cursor(&self) -> Option<(String, usize, usize)> {
        let input = &self.input;
        let cursor = self.cursor_position;
        
        if cursor > input.len() {
            return None;
        }
        
        // カーソル位置の単語を抽出
        let mut start = cursor;
        while start > 0 && !input[start-1..start].chars().next().unwrap().is_whitespace() {
            start -= 1;
        }
        
        let mut end = cursor;
        while end < input.len() && !input[end..end+1].chars().next().unwrap().is_whitespace() {
            end += 1;
        }
        
        if start < end {
            Some((input[start..end].to_string(), start, end))
        } else {
            None
        }
    }

    /// カーソル前のコマンド名を取得
    pub fn get_current_command(&self) -> Option<String> {
        let input = &self.input[..self.cursor_position.min(self.input.len())];
        let words: Vec<&str> = input.trim().split_whitespace().collect();
        
        if words.is_empty() {
            None
        } else {
            Some(words[0].to_string())
        }
    }
}

/// 補完エンジン
#[derive(Debug)]
pub struct Completer {
    /// 文法マネージャー
    grammar_manager: Arc<GrammarManager>,
    /// パスキャッシュ
    path_cache: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// コマンドメタデータ
    command_metadata: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// プラグイン補完ハンドラー
    plugin_handlers: Vec<Box<dyn CompletionHandler>>,
    /// AI補完モデル（オプション）
    ai_completer: Option<Box<dyn AiCompleter>>,
    /// キャッシュ有効期限（秒）
    cache_ttl: u64,
    /// 最大候補数
    max_suggestions: usize,
}

impl Completer {
    /// 新しい補完エンジンを作成
    pub fn new() -> Self {
        Self {
            grammar_manager: Arc::new(GrammarManager::new()),
            path_cache: Arc::new(RwLock::new(HashMap::new())),
            command_metadata: Arc::new(RwLock::new(HashMap::new())),
            plugin_handlers: Vec::new(),
            ai_completer: None,
            cache_ttl: 300, // 5分
            max_suggestions: 100,
        }
    }

    /// 文法マネージャーを設定
    pub fn with_grammar_manager(mut self, grammar_manager: Arc<GrammarManager>) -> Self {
        self.grammar_manager = grammar_manager;
        self
    }

    /// 補完ハンドラーを追加
    pub fn add_completion_handler(&mut self, handler: Box<dyn CompletionHandler>) {
        self.plugin_handlers.push(handler);
    }

    /// AI補完モデルを設定
    pub fn set_ai_completer(&mut self, completer: Box<dyn AiCompleter>) {
        self.ai_completer = Some(completer);
    }

    /// 補完候補を生成
    pub fn complete(&self, context: &CompletionContext) -> CompletionResult {
        let start_time = Instant::now();
        
        // カーソル位置のトークンを取得
        let (current_word, start, end) = match context.get_token_at_cursor() {
            Some(token) => token,
            None => {
                return CompletionResult::new(
                    Vec::new(),
                    (context.cursor_position, context.cursor_position),
                    CompletionKind::Command,
                );
            }
        };
        
        // カーソル前のテキストを解析して補完コンテキストを決定
        let lexer = NexusLexer::new(&context.input);
        let completion_type = self.determine_completion_type(context, &current_word);
        
        // 補完の種類に応じた候補を生成
        let suggestions = match completion_type {
            CompletionKind::Command => self.complete_command(context, &current_word),
            CompletionKind::FilePath => self.complete_file_path(context, &current_word),
            CompletionKind::Variable => self.complete_variable(context, &current_word),
            CompletionKind::Option => self.complete_option(context, &current_word),
            CompletionKind::Argument => self.complete_argument(context, &current_word),
            CompletionKind::Syntax => self.complete_syntax(context, &current_word),
            CompletionKind::Custom(plugin_name) => self.complete_with_plugin(context, &current_word, &plugin_name),
        };
        
        // プラグインからの補完候補を追加
        let mut all_suggestions = suggestions;
        for handler in &self.plugin_handlers {
            if handler.can_handle(completion_type.clone(), &current_word) {
                let plugin_suggestions = handler.get_suggestions(context, &current_word);
                all_suggestions.extend(plugin_suggestions);
            }
        }
        
        // AI補完を試みる（設定されている場合）
        if let Some(ai_completer) = &self.ai_completer {
            if completion_type == CompletionKind::Command || completion_type == CompletionKind::Argument {
                let ai_suggestions = ai_completer.complete(context, &current_word, completion_type.clone());
                all_suggestions.extend(ai_suggestions);
            }
        }
        
        // 候補が多すぎる場合は制限
        if all_suggestions.len() > self.max_suggestions {
            all_suggestions.truncate(self.max_suggestions);
        }
        
        // 補完結果を作成して返す
        let mut result = CompletionResult::new(
            all_suggestions,
            (start, end),
            completion_type,
        );
        
        result.with_context(&context.input)
              .with_metadata("execution_time_ms", &format!("{}", start_time.elapsed().as_millis()));
        
        result
    }

    /// 補完の種類を判定
    fn determine_completion_type(&self, context: &CompletionContext, current_word: &str) -> CompletionKind {
        // 補完の種類を判定するロジック
        if current_word.starts_with("$") {
            return CompletionKind::Variable;
        }
        
        if current_word.starts_with("-") {
            return CompletionKind::Option;
        }
        
        if current_word.starts_with("./") || current_word.starts_with("/") || current_word.contains("/") {
            return CompletionKind::FilePath;
        }
        
        // カーソル前のテキストを解析
        let input_before_cursor = &context.input[..context.cursor_position.min(context.input.len())];
        let words: Vec<&str> = input_before_cursor.trim().split_whitespace().collect();
        
        if words.is_empty() || (words.len() == 1 && current_word == words[0]) {
            // 入力の最初の単語はコマンド
            return CompletionKind::Command;
        }
        
        // 構文キーワードの判定
        let syntax_keywords = ["if", "then", "else", "fi", "for", "while", "until", "do", "done", "case", "esac"];
        if syntax_keywords.contains(&current_word) || current_word.is_empty() && words.last().map_or(false, |w| syntax_keywords.contains(w)) {
            return CompletionKind::Syntax;
        }
        
        // コマンド引数の判定
        CompletionKind::Argument
    }

    /// コマンド補完
    fn complete_command(&self, context: &CompletionContext, current_word: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 利用可能なコマンドから候補を生成
        for cmd in &context.available_commands {
            if cmd.starts_with(current_word) || current_word.is_empty() {
                suggestions.push(CompletionSuggestion::new(
                    cmd.clone(),
                    CompletionType::Command(cmd.clone()),
                    None,
                    1
                ));
            }
        }
        
        // コマンド履歴からも候補を追加
        for cmd in &context.command_history {
            let first_word = cmd.split_whitespace().next().unwrap_or("");
            if first_word.starts_with(current_word) && !suggestions.iter().any(|s| s.text == first_word) {
                suggestions.push(CompletionSuggestion::new(
                    first_word.to_string(),
                    CompletionType::Command(first_word.to_string()),
                    Some("(履歴)".to_string()),
                    2
                ));
            }
        }
        
        suggestions
    }

    /// ファイルパス補完
    fn complete_file_path(&self, context: &CompletionContext, current_word: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // パスの展開
        let path_str = if current_word.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                let home_str = home.to_string_lossy();
                current_word.replace("~", &home_str)
            } else {
                current_word.to_string()
            }
        } else {
            current_word.to_string()
        };
        
        // ディレクトリとファイル名を分離
        let (dir_path, file_prefix) = if let Some(last_slash) = path_str.rfind('/') {
            let (dir, file) = path_str.split_at(last_slash + 1);
            (dir.to_string(), file.to_string())
        } else {
            ("./".to_string(), path_str)
        };
        
        // キャッシュをチェック
        let mut use_cache = false;
        if let Some(cache) = &context.path_cache {
            if let Ok(cache_read) = cache.read() {
                if let Some(entries) = cache_read.get(&dir_path) {
                    for entry in entries {
                        if entry.starts_with(&file_prefix) {
                            let full_path = format!("{}{}", dir_path, entry);
                            suggestions.push(CompletionSuggestion::new(
                                full_path.clone(),
                                CompletionType::FilePath(full_path),
                                None,
                                1
                            ));
                        }
                    }
                    use_cache = true;
                }
            }
        }
        
        // キャッシュにない場合はファイルシステムを検索
        if !use_cache {
            if let Ok(entries) = std::fs::read_dir(Path::new(&dir_path)) {
                let mut dir_entries = Vec::new();
                
                for entry in entries.filter_map(Result::ok) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&file_prefix) {
                        let full_path = format!("{}{}", dir_path, name);
                        
                        // ディレクトリの場合は末尾にスラッシュを追加
                        let (display_name, is_dir) = if entry.path().is_dir() {
                            (format!("{}/", name), true)
                        } else {
                            (name.clone(), false)
                        };
                        
                        suggestions.push(CompletionSuggestion::new(
                            full_path.clone(),
                            CompletionType::FilePath(full_path),
                            Some(if is_dir { "(ディレクトリ)".to_string() } else { "".to_string() }),
                            if is_dir { 1 } else { 2 }
                        ));
                        
                        dir_entries.push(name);
                    }
                }
                
                // キャッシュを更新
                if let Some(cache) = &self.path_cache {
                    if let Ok(mut cache_write) = cache.write() {
                        cache_write.insert(dir_path, dir_entries);
                    }
                }
            }
        }
        
        suggestions
    }

    /// 変数補完
    fn complete_variable(&self, context: &CompletionContext, current_word: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 変数名の抽出（$または${から始まる場合）
        let var_prefix = if current_word.starts_with("${") {
            &current_word[2..]
        } else if current_word.starts_with("$") {
            &current_word[1..]
        } else {
            current_word
        };
        
        // 環境変数からの補完
        for (name, value) in &context.env_variables {
            if name.starts_with(var_prefix) {
                let completion = if current_word.starts_with("${") {
                    format!("${{{}}}", name)
                } else {
                    format!("${}", name)
                };
                
                suggestions.push(CompletionSuggestion::new(
                    completion,
                    CompletionType::Variable(name.clone()),
                    Some(value.clone()),
                    1
                ));
            }
        }
        
        suggestions
    }

    /// オプション補完
    fn complete_option(&self, context: &CompletionContext, current_word: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 現在のコマンドを取得
        if let Some(cmd) = context.get_current_command() {
            // コマンド固有のオプションを取得
            if let Some(options) = context.command_options.get(&cmd) {
                for opt in options {
                    if opt.starts_with(current_word) {
                        suggestions.push(CompletionSuggestion::new(
                            opt.clone(),
                            CompletionType::Flag(opt.clone()),
                            None,
                            1
                        ));
                    }
                }
            }
        }
        
        suggestions
    }

    /// 引数補完
    fn complete_argument(&self, context: &CompletionContext, current_word: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // コマンド固有の引数補完を試みる
        if let Some(cmd) = context.get_current_command() {
            // コマンド固有の補完ロジック
            match cmd.as_str() {
                "cd" => {
                    // cdコマンドならディレクトリのみ補完
                    return self.complete_file_path(context, current_word).into_iter()
                        .filter(|s| s.text.ends_with("/"))
                        .collect();
                },
                "git" => {
                    // gitコマンドのサブコマンド補完
                    let words: Vec<&str> = context.input.trim().split_whitespace().collect();
                    if words.len() == 2 && words[0] == "git" {
                        let git_commands = ["add", "commit", "push", "pull", "status", "branch", "checkout", "merge"];
                        for cmd in git_commands.iter() {
                            if cmd.starts_with(current_word) {
                                suggestions.push(CompletionSuggestion::new(
                                    cmd.to_string(),
                                    CompletionType::Argument(cmd.to_string()),
                                    None,
                                    1
                                ));
                            }
                        }
                        return suggestions;
                    }
                },
                _ => {}
            }
        }
        
        // デフォルトはファイルパス補完
        if suggestions.is_empty() {
            suggestions = self.complete_file_path(context, current_word);
        }
        
        suggestions
    }

    /// 構文補完
    fn complete_syntax(&self, context: &CompletionContext, current_word: &str) -> Vec<CompletionSuggestion> {
        let mut suggestions = Vec::new();
        
        // 基本的な構文キーワード
        let syntax_keywords = [
            "if", "then", "else", "elif", "fi",
            "for", "while", "until", "do", "done",
            "case", "esac", "in",
            "function", "return",
            "break", "continue",
            "true", "false"
        ];
        
        for keyword in syntax_keywords.iter() {
            if keyword.starts_with(current_word) {
                suggestions.push(CompletionSuggestion::new(
                    keyword.to_string(),
                    CompletionType::Syntax(keyword.to_string()),
                    None,
                    1
                ));
            }
        }
        
        // 一般的な構文テンプレート
        let templates = [
            ("if", "if [ $condition ]; then\n  \nfi"),
            ("for", "for item in $items; do\n  \ndone"),
            ("while", "while [ $condition ]; do\n  \ndone"),
            ("function", "function name() {\n  \n}")
        ];
        
        if current_word.is_empty() {
            for (name, template) in templates.iter() {
                suggestions.push(CompletionSuggestion::new(
                    template.to_string(),
                    CompletionType::Syntax(name.to_string()),
                    Some(format!("{}テンプレート", name)),
                    10
                ));
            }
        }
        
        suggestions
    }

    /// プラグインによる補完
    fn complete_with_plugin(&self, context: &CompletionContext, current_word: &str, plugin_name: &str) -> Vec<CompletionSuggestion> {
        // プラグイン名に一致するハンドラーを検索
        for handler in &self.plugin_handlers {
            if handler.name() == plugin_name {
                return handler.get_suggestions(context, current_word);
            }
        }
        
        Vec::new()
    }
}

/// 補完ハンドラー trait
pub trait CompletionHandler: Send + Sync {
    /// ハンドラー名
    fn name(&self) -> &str;
    
    /// この補完ハンドラーが対応可能か判定
    fn can_handle(&self, kind: CompletionKind, word: &str) -> bool;
    
    /// 補完候補を取得
    fn get_suggestions(&self, context: &CompletionContext, word: &str) -> Vec<CompletionSuggestion>;
}

/// AI補完 trait
pub trait AiCompleter: Send + Sync {
    /// AI補完を実行
    fn complete(&self, context: &CompletionContext, word: &str, kind: CompletionKind) -> Vec<CompletionSuggestion>;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_command_completion() {
        let completer = Completer::new();
        let mut context = CompletionContext::new("g", 1);
        context.available_commands = vec![
            "git".to_string(),
            "grep".to_string(),
            "gcc".to_string(),
            "go".to_string(),
        ];
        
        let result = completer.complete(&context);
        
        assert_eq!(result.kind, CompletionKind::Command);
        assert_eq!(result.suggestions.len(), 4);
        assert!(result.suggestions.iter().any(|s| s.text == "git"));
        assert!(result.suggestions.iter().any(|s| s.text == "grep"));
        assert!(result.suggestions.iter().any(|s| s.text == "gcc"));
        assert!(result.suggestions.iter().any(|s| s.text == "go"));
    }
    
    #[test]
    fn test_variable_completion() {
        let completer = Completer::new();
        let mut context = CompletionContext::new("echo $HO", 8);
        context.env_variables = [
            ("HOME".to_string(), "/home/user".to_string()),
            ("HOSTNAME".to_string(), "localhost".to_string()),
            ("HOST".to_string(), "localhost".to_string()),
        ].iter().cloned().collect();
        
        let result = completer.complete(&context);
        
        assert_eq!(result.kind, CompletionKind::Variable);
        assert_eq!(result.suggestions.len(), 2);
        assert!(result.suggestions.iter().any(|s| s.text == "$HOME"));
        assert!(result.suggestions.iter().any(|s| s.text == "$HOSTNAME"));
    }
} 