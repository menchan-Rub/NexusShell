// grammar.rs - 世界最高水準の文法エンジン
//
// NexusShellの高度な構文解析を担当する中核コンポーネント。
// 柔軟なDSLとマクロで文法定義を可能にし、複雑なシェルスクリプト構文を効率的に解析します。

use crate::{
    AstNode, Error, Result, Span, TokenKind, ParserContext, ParserError,
    error_recovery::{ErrorRecoveryManager, RepairResult, RecoveryStrategy}
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use std::time::{Duration, Instant};
use rayon::prelude::*;

/// 文法ルールの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrammarRuleKind {
    /// 終端記号（トークン）
    Terminal(TokenKind),
    
    /// 非終端記号（他のルールの組み合わせ）
    NonTerminal(String),
    
    /// 連接（複数のルールを順に適用）
    Sequence(Vec<Box<GrammarRule>>),
    
    /// 選択（複数のルールのいずれかを適用）
    Choice(Vec<Box<GrammarRule>>),
    
    /// 繰り返し（0回以上）
    ZeroOrMore(Box<GrammarRule>),
    
    /// 繰り返し（1回以上）
    OneOrMore(Box<GrammarRule>),
    
    /// オプション（0または1回）
    Optional(Box<GrammarRule>),
    
    /// 否定先読み（指定パターンがない場合に成功）
    NegativeLookahead(Box<GrammarRule>),
    
    /// 肯定先読み（指定パターンがある場合に成功、消費しない）
    PositiveLookahead(Box<GrammarRule>),
    
    /// セマンティックアクション（解析後の処理）
    SemanticAction(SemanticActionFn),
    
    /// エラー回復ポイント（エラー発生時の回復方法を指定）
    ErrorRecoveryPoint(ErrorRecoveryStrategy),
    
    /// コンテキスト依存ルール（パーサーの状態に応じてルールを変更）
    ContextDependent(ContextDependentFn),
    
    /// カスタム解析器（完全なカスタム実装）
    Custom(CustomParserFn),
}

/// 文法ルール
#[derive(Debug, Clone)]
pub struct GrammarRule {
    /// ルールの種類
    pub kind: GrammarRuleKind,
    
    /// ルール名（デバッグや参照用）
    pub name: Option<String>,
    
    /// 優先度（競合時の選択に使用）
    pub priority: i32,
    
    /// メモ化キャッシュを使用するかどうか
    pub use_memoization: bool,
    
    /// 左再帰対応かどうか
    pub handle_left_recursion: bool,
    
    /// デバッグトレースを有効にするかどうか
    pub enable_trace: bool,
}

/// セマンティックアクション関数の型
pub type SemanticActionFn = Box<dyn Fn(&mut ParserContext, &AstNode) -> Result<AstNode> + Send + Sync>;

/// コンテキスト依存関数の型
pub type ContextDependentFn = Box<dyn Fn(&ParserContext) -> Box<GrammarRule> + Send + Sync>;

/// カスタム解析関数の型
pub type CustomParserFn = Box<dyn Fn(&mut ParserContext) -> Result<AstNode> + Send + Sync>;

/// エラー回復戦略
#[derive(Debug, Clone)]
pub enum ErrorRecoveryStrategy {
    /// 指定されたトークンまでスキップ
    SkipUntil(TokenKind),
    
    /// 指定されたトークンを挿入
    Insert(TokenKind),
    
    /// 指定された非終端記号まで同期
    SynchronizeToNonTerminal(String),
    
    /// カスタム回復ロジック
    Custom(Box<dyn Fn(&mut ParserContext, &Error) -> Result<RepairResult> + Send + Sync>),
}

/// 文法エンジン
#[derive(Debug)]
pub struct GrammarEngine {
    /// 名前付きルール定義のマップ
    rules: HashMap<String, Box<GrammarRule>>,
    
    /// 開始ルール名
    start_rule: String,
    
    /// メモ化キャッシュ
    memo_cache: Mutex<HashMap<MemoKey, MemoEntry>>,
    
    /// 左再帰検出セット
    left_recursion_set: Mutex<HashSet<String>>,
    
    /// エラー回復マネージャー
    error_recovery: Mutex<ErrorRecoveryManager>,
    
    /// トレースが有効かどうか
    trace_enabled: bool,
    
    /// トレース階層の深さ
    trace_depth: Mutex<usize>,
    
    /// トレースログ
    trace_log: Mutex<Vec<TraceEntry>>,
    
    /// パフォーマンス統計
    stats: Mutex<GrammarStats>,
}

/// メモ化キャッシュのキー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemoKey {
    /// ルール名
    rule_name: String,
    
    /// 入力位置
    position: usize,
}

/// メモ化キャッシュのエントリ
#[derive(Debug, Clone)]
struct MemoEntry {
    /// 解析結果
    result: Result<AstNode>,
    
    /// 入力の最終位置
    end_position: usize,
    
    /// 使用回数（統計用）
    usage_count: usize,
}

/// トレースエントリ
#[derive(Debug, Clone)]
struct TraceEntry {
    /// ルール名
    rule_name: String,
    
    /// 入力位置
    position: usize,
    
    /// 終了位置
    end_position: Option<usize>,
    
    /// 成功したかどうか
    success: bool,
    
    /// トレースの深さ
    depth: usize,
    
    /// 実行時間
    duration: Duration,
    
    /// 結果の概要
    result_summary: String,
}

/// 文法エンジンの統計情報
#[derive(Debug, Clone, Default)]
pub struct GrammarStats {
    /// ルール呼び出し回数
    pub rule_invocations: HashMap<String, usize>,
    
    /// ルール成功回数
    pub rule_successes: HashMap<String, usize>,
    
    /// ルール失敗回数
    pub rule_failures: HashMap<String, usize>,
    
    /// ルール実行時間
    pub rule_times: HashMap<String, Duration>,
    
    /// メモ化ヒット数
    pub memo_hits: usize,
    
    /// メモ化ミス数
    pub memo_misses: usize,
    
    /// エラー回復回数
    pub recovery_attempts: usize,
    
    /// エラー回復成功数
    pub recovery_successes: usize,
    
    /// 解析したトークン総数
    pub total_tokens_parsed: usize,
    
    /// 解析にかかった総時間
    pub total_parse_time: Duration,
}

impl GrammarRule {
    /// 新しい終端記号ルールを作成
    pub fn terminal(token_kind: TokenKind) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::Terminal(token_kind),
            name: None,
            priority: 0,
            use_memoization: false, // 終端記号は単純なのでメモ化不要
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// 新しい非終端記号ルールを作成
    pub fn non_terminal(name: &str) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::NonTerminal(name.to_string()),
            name: Some(name.to_string()),
            priority: 0,
            use_memoization: true,
            handle_left_recursion: true,
            enable_trace: false,
        })
    }
    
    /// 複数のルールを順番に適用する連接ルールを作成
    pub fn sequence(rules: Vec<Box<GrammarRule>>) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::Sequence(rules),
            name: None,
            priority: 0,
            use_memoization: true,
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// 複数のルールのいずれかを適用する選択ルールを作成
    pub fn choice(rules: Vec<Box<GrammarRule>>) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::Choice(rules),
            name: None,
            priority: 0,
            use_memoization: true,
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// ルールを0回以上繰り返す繰り返しルールを作成
    pub fn zero_or_more(rule: Box<GrammarRule>) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::ZeroOrMore(rule),
            name: None,
            priority: 0,
            use_memoization: true,
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// ルールを1回以上繰り返す繰り返しルールを作成
    pub fn one_or_more(rule: Box<GrammarRule>) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::OneOrMore(rule),
            name: None,
            priority: 0,
            use_memoization: true,
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// ルールをオプション（0または1回）とするルールを作成
    pub fn optional(rule: Box<GrammarRule>) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::Optional(rule),
            name: None,
            priority: 0,
            use_memoization: true,
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// セマンティックアクションを持つルールを作成
    pub fn action<F>(rule: Box<GrammarRule>, action: F) -> Box<Self>
    where
        F: Fn(&mut ParserContext, &AstNode) -> Result<AstNode> + Send + Sync + 'static,
    {
        Box::new(Self {
            kind: GrammarRuleKind::SemanticAction(Box::new(action)),
            name: None,
            priority: 0,
            use_memoization: true,
            handle_left_recursion: false,
            enable_trace: false,
        })
    }
    
    /// エラー回復ポイントを持つルールを作成
    pub fn with_recovery(rule: Box<GrammarRule>, strategy: ErrorRecoveryStrategy) -> Box<Self> {
        Box::new(Self {
            kind: GrammarRuleKind::ErrorRecoveryPoint(strategy),
            name: rule.name.clone(),
            priority: rule.priority,
            use_memoization: rule.use_memoization,
            handle_left_recursion: rule.handle_left_recursion,
            enable_trace: rule.enable_trace,
        })
    }
    
    /// ルールに名前を設定
    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }
    
    /// ルールの優先度を設定
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
    
    /// メモ化の使用を設定
    pub fn memoize(mut self, use_memoization: bool) -> Self {
        self.use_memoization = use_memoization;
        self
    }
    
    /// 左再帰対応を設定
    pub fn left_recursive(mut self, handle_left_recursion: bool) -> Self {
        self.handle_left_recursion = handle_left_recursion;
        self
    }
    
    /// トレースを有効化
    pub fn trace(mut self, enable_trace: bool) -> Self {
        self.enable_trace = enable_trace;
        self
    }
}

impl GrammarEngine {
    /// 新しい文法エンジンを作成
    pub fn new(start_rule: &str) -> Self {
        Self {
            rules: HashMap::new(),
            start_rule: start_rule.to_string(),
            memo_cache: Mutex::new(HashMap::new()),
            left_recursion_set: Mutex::new(HashSet::new()),
            error_recovery: Mutex::new(crate::error_recovery::create_error_recovery_manager()),
            trace_enabled: false,
            trace_depth: Mutex::new(0),
            trace_log: Mutex::new(Vec::new()),
            stats: Mutex::new(GrammarStats::default()),
        }
    }
    
    /// ルールを追加
    pub fn add_rule(&mut self, name: &str, rule: Box<GrammarRule>) {
        self.rules.insert(name.to_string(), rule);
    }
    
    /// トレースを有効化
    pub fn enable_trace(&mut self, enabled: bool) {
        self.trace_enabled = enabled;
    }
    
    /// 入力を解析
    pub fn parse(&self, ctx: &mut ParserContext) -> Result<AstNode> {
        // 解析開始時刻を記録
        let start_time = Instant::now();
        
        // メモ化キャッシュをクリア
        self.memo_cache.lock().clear();
        
        // 左再帰セットをクリア
        self.left_recursion_set.lock().clear();
        
        // トレースログをクリア
        if self.trace_enabled {
            *self.trace_depth.lock() = 0;
            self.trace_log.lock().clear();
        }
        
        // 開始ルールを取得
        let start_rule = self.rules.get(&self.start_rule)
            .ok_or_else(|| Error::new(
                format!("開始ルール '{}' が見つかりません", self.start_rule),
                Span::new(0, 0)
            ))?;
        
        // 解析を実行
        let result = self.apply_rule(ctx, start_rule);
        
        // 解析終了時刻を記録し統計を更新
        let parse_time = start_time.elapsed();
        let mut stats = self.stats.lock();
        stats.total_parse_time += parse_time;
        stats.total_tokens_parsed += ctx.tokens.len();
        
        // 結果を返す
        result
    }
    
    /// ルールを適用
    fn apply_rule(&self, ctx: &mut ParserContext, rule: &GrammarRule) -> Result<AstNode> {
        // トレース開始
        let start_time = Instant::now();
        let rule_name = rule.name.clone().unwrap_or_else(|| "匿名".to_string());
        let start_position = ctx.current;
        
        if self.trace_enabled && rule.enable_trace {
            let mut depth = self.trace_depth.lock();
            self.add_trace_entry(TraceEntry {
                rule_name: rule_name.clone(),
                position: start_position,
                end_position: None,
                success: false,
                depth: *depth,
                duration: Duration::from_secs(0),
                result_summary: "開始".to_string(),
            });
            *depth += 1;
        }
        
        // メモ化キャッシュをチェック
        if rule.use_memoization {
            let key = MemoKey {
                rule_name: rule_name.clone(),
                position: start_position,
            };
            
            let mut memo_cache = self.memo_cache.lock();
            if let Some(entry) = memo_cache.get_mut(&key) {
                // キャッシュヒット
                let mut stats = self.stats.lock();
                stats.memo_hits += 1;
                entry.usage_count += 1;
                
                // 位置を更新
                ctx.current = entry.end_position;
                
                if self.trace_enabled && rule.enable_trace {
                    let mut depth = self.trace_depth.lock();
                    *depth -= 1;
                    self.add_trace_entry(TraceEntry {
                        rule_name: rule_name.clone(),
                        position: start_position,
                        end_position: Some(entry.end_position),
                        success: entry.result.is_ok(),
                        depth: *depth,
                        duration: start_time.elapsed(),
                        result_summary: format!("メモ化キャッシュヒット: {:?}", entry.result.is_ok()),
                    });
                }
                
                return entry.result.clone();
            } else {
                // キャッシュミス
                let mut stats = self.stats.lock();
                stats.memo_misses += 1;
                drop(memo_cache);
            }
        }
        
        // 左再帰チェック
        if rule.handle_left_recursion {
            let mut left_recursion_set = self.left_recursion_set.lock();
            if left_recursion_set.contains(&rule_name) {
                // 左再帰検出、空の結果を返す
                drop(left_recursion_set);
                
                if self.trace_enabled && rule.enable_trace {
                    let mut depth = self.trace_depth.lock();
                    *depth -= 1;
                    self.add_trace_entry(TraceEntry {
                        rule_name: rule_name.clone(),
                        position: start_position,
                        end_position: Some(start_position),
                        success: false,
                        depth: *depth,
                        duration: start_time.elapsed(),
                        result_summary: "左再帰検出".to_string(),
                    });
                }
                
                return Err(Error::new(
                    format!("左再帰検出: ルール '{}'", rule_name),
                    Span::new(start_position, start_position)
                ));
            }
            
            // ルールを左再帰セットに追加
            left_recursion_set.insert(rule_name.clone());
            drop(left_recursion_set);
        }
        
        // ルール呼び出し統計を更新
        {
            let mut stats = self.stats.lock();
            *stats.rule_invocations.entry(rule_name.clone()).or_insert(0) += 1;
        }
        
        // ルールを適用
        let result = match &rule.kind {
            GrammarRuleKind::Terminal(token_kind) => {
                self.apply_terminal_rule(ctx, *token_kind)
            },
            
            GrammarRuleKind::NonTerminal(name) => {
                self.apply_non_terminal_rule(ctx, name)
            },
            
            GrammarRuleKind::Sequence(rules) => {
                self.apply_sequence_rule(ctx, rules)
            },
            
            GrammarRuleKind::Choice(rules) => {
                self.apply_choice_rule(ctx, rules)
            },
            
            GrammarRuleKind::ZeroOrMore(rule) => {
                self.apply_zero_or_more_rule(ctx, rule)
            },
            
            GrammarRuleKind::OneOrMore(rule) => {
                self.apply_one_or_more_rule(ctx, rule)
            },
            
            GrammarRuleKind::Optional(rule) => {
                self.apply_optional_rule(ctx, rule)
            },
            
            GrammarRuleKind::NegativeLookahead(rule) => {
                self.apply_negative_lookahead_rule(ctx, rule)
            },
            
            GrammarRuleKind::PositiveLookahead(rule) => {
                self.apply_positive_lookahead_rule(ctx, rule)
            },
            
            GrammarRuleKind::SemanticAction(action) => {
                // セマンティックアクションはルールなしで適用できない
                Err(Error::new(
                    "セマンティックアクションは単独で使用できません".to_string(),
                    Span::new(start_position, start_position)
                ))
            },
            
            GrammarRuleKind::ErrorRecoveryPoint(strategy) => {
                // エラー回復ポイントはルールなしで適用できない
                Err(Error::new(
                    "エラー回復ポイントは単独で使用できません".to_string(),
                    Span::new(start_position, start_position)
                ))
            },
            
            GrammarRuleKind::ContextDependent(context_fn) => {
                // コンテキスト依存ルールを評価
                let dynamic_rule = context_fn(ctx);
                self.apply_rule(ctx, &dynamic_rule)
            },
            
            GrammarRuleKind::Custom(parser_fn) => {
                // カスタム解析器を実行
                parser_fn(ctx)
            },
        };
        
        // 左再帰セットからルールを削除
        if rule.handle_left_recursion {
            let mut left_recursion_set = self.left_recursion_set.lock();
            left_recursion_set.remove(&rule_name);
        }
        
        // 結果に基づいて統計を更新
        {
            let mut stats = self.stats.lock();
            match &result {
                Ok(_) => {
                    *stats.rule_successes.entry(rule_name.clone()).or_insert(0) += 1;
                },
                Err(_) => {
                    *stats.rule_failures.entry(rule_name.clone()).or_insert(0) += 1;
                }
            }
            
            let elapsed = start_time.elapsed();
            *stats.rule_times.entry(rule_name.clone()).or_insert(Duration::from_secs(0)) += elapsed;
        }
        
        // メモ化キャッシュに結果を保存
        if rule.use_memoization {
            let key = MemoKey {
                rule_name: rule_name.clone(),
                position: start_position,
            };
            
            let entry = MemoEntry {
                result: result.clone(),
                end_position: ctx.current,
                usage_count: 1,
            };
            
            let mut memo_cache = self.memo_cache.lock();
            memo_cache.insert(key, entry);
        }
        
        // トレース終了
        if self.trace_enabled && rule.enable_trace {
            let mut depth = self.trace_depth.lock();
            *depth -= 1;
            self.add_trace_entry(TraceEntry {
                rule_name: rule_name.clone(),
                position: start_position,
                end_position: Some(ctx.current),
                success: result.is_ok(),
                depth: *depth,
                duration: start_time.elapsed(),
                result_summary: format!("{:?}", result),
            });
        }
        
        result
    }
    
    /// トレースエントリを追加
    fn add_trace_entry(&self, entry: TraceEntry) {
        if self.trace_enabled {
            let mut trace_log = self.trace_log.lock();
            trace_log.push(entry);
        }
    }
    
    /// 終端記号ルールを適用
    fn apply_terminal_rule(&self, ctx: &mut ParserContext, token_kind: TokenKind) -> Result<AstNode> {
        if ctx.current >= ctx.tokens.len() {
            return Err(Error::new(
                format!("予期せぬ入力の終わり、期待: {:?}", token_kind),
                Span::new(ctx.current, ctx.current)
            ));
        }
        
        let token = &ctx.tokens[ctx.current];
        if token.kind == token_kind {
            // トークンが一致した場合
            let span = token.span.clone();
            ctx.current += 1;
            
            // 単純なASTノードを作成
            Ok(AstNode::Terminal {
                token_kind,
                lexeme: token.lexeme.clone(),
                span,
            })
        } else {
            // トークンが一致しなかった場合
            Err(Error::new(
                format!("構文エラー: 期待されたトークンは {:?} でしたが、実際には {:?} でした。", token_kind, token.kind),
                token.span.clone(),
                Some(format!("ソース: {} (行: {}, 列: {})", token.lexeme, token.span.line, token.span.column))
            ))
        }
    }
    
    /// 非終端記号ルールを適用
    fn apply_non_terminal_rule(&self, ctx: &mut ParserContext, name: &str) -> Result<AstNode> {
        // ルールを取得
        let rule = self.rules.get(name).ok_or_else(|| {
            Error::new(
                format!("未定義のルール: {}", name),
                Span::new(ctx.current, ctx.current)
            )
        })?;
        
        // ルールを適用
        let start_position = ctx.current;
        let result = self.apply_rule(ctx, rule);
        
        match result {
            Ok(node) => {
                // 非終端記号のノードを作成
                Ok(AstNode::NonTerminal {
                    name: name.to_string(),
                    children: vec![node],
                    span: Span::new(start_position, ctx.current),
                })
            },
            Err(e) => Err(e),
        }
    }
    
    /// 連接ルールを適用
    fn apply_sequence_rule(&self, ctx: &mut ParserContext, rules: &[Box<GrammarRule>]) -> Result<AstNode> {
        let start_position = ctx.current;
        let mut children = Vec::with_capacity(rules.len());
        
        // 各ルールを順に適用
        for rule in rules {
            match self.apply_rule(ctx, rule) {
                Ok(node) => {
                    children.push(node);
                },
                Err(e) => {
                    // エラーが発生した場合、全体が失敗
                    ctx.current = start_position; // 位置を元に戻す
                    return Err(e);
                }
            }
        }
        
        // 連接ノードを作成
        Ok(AstNode::Sequence {
            children,
            span: Span::new(start_position, ctx.current),
        })
    }
    
    /// 選択ルールを適用
    fn apply_choice_rule(&self, ctx: &mut ParserContext, rules: &[Box<GrammarRule>]) -> Result<AstNode> {
        let start_position = ctx.current;
        let mut last_error = None;
        
        // 優先度でソート
        let mut sorted_rules: Vec<&Box<GrammarRule>> = rules.iter().collect();
        sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        // いずれかのルールが成功するまで試行
        for rule in sorted_rules {
            ctx.current = start_position; // 位置をリセット
            match self.apply_rule(ctx, rule) {
                Ok(node) => {
                    return Ok(AstNode::Choice {
                        value: Box::new(node),
                        span: Span::new(start_position, ctx.current),
                    });
                },
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }
        
        // すべてのルールが失敗した場合
        ctx.current = start_position; // 位置を元に戻す
        Err(last_error.unwrap_or_else(|| {
            Error::new(
                "選択ルールに候補がありません".to_string(),
                Span::new(start_position, start_position)
            )
        }))
    }
    
    /// ゼロ回以上の繰り返しルールを適用
    fn apply_zero_or_more_rule(&self, ctx: &mut ParserContext, rule: &GrammarRule) -> Result<AstNode> {
        let start_position = ctx.current;
        let mut children = Vec::new();
        
        // ルールを繰り返し適用
        loop {
            let current_position = ctx.current;
            match self.apply_rule(ctx, rule) {
                Ok(node) => {
                    // 無限ループを防止（位置が進まない場合）
                    if ctx.current == current_position {
                        break;
                    }
                    children.push(node);
                },
                Err(_) => {
                    // エラーは繰り返しの終了条件
                    ctx.current = current_position; // エラー時の位置に戻す
                    break;
                }
            }
        }
        
        // 繰り返しノードを作成
        Ok(AstNode::Repetition {
            children,
            span: Span::new(start_position, ctx.current),
        })
    }
    
    /// 1回以上の繰り返しルールを適用
    fn apply_one_or_more_rule(&self, ctx: &mut ParserContext, rule: &GrammarRule) -> Result<AstNode> {
        let start_position = ctx.current;
        let mut children = Vec::new();
        
        // 最初の適用（必須）
        match self.apply_rule(ctx, rule) {
            Ok(node) => {
                children.push(node);
            },
            Err(e) => {
                ctx.current = start_position; // 位置を元に戻す
                return Err(e);
            }
        }
        
        // 残りは0回以上
        loop {
            let current_position = ctx.current;
            match self.apply_rule(ctx, rule) {
                Ok(node) => {
                    // 無限ループを防止
                    if ctx.current == current_position {
                        break;
                    }
                    children.push(node);
                },
                Err(_) => {
                    // エラーは繰り返しの終了条件
                    ctx.current = current_position;
                    break;
                }
            }
        }
        
        // 繰り返しノードを作成
        Ok(AstNode::Repetition {
            children,
            span: Span::new(start_position, ctx.current),
        })
    }
    
    /// オプションルールを適用
    fn apply_optional_rule(&self, ctx: &mut ParserContext, rule: &GrammarRule) -> Result<AstNode> {
        let start_position = ctx.current;
        
        match self.apply_rule(ctx, rule) {
            Ok(node) => {
                // オプションノードを作成（値あり）
                Ok(AstNode::Optional {
                    value: Some(Box::new(node)),
                    span: Span::new(start_position, ctx.current),
                })
            },
            Err(_) => {
                // 失敗しても問題なし（オプションなので）
                ctx.current = start_position; // 位置を元に戻す
                
                // オプションノードを作成（値なし）
                Ok(AstNode::Optional {
                    value: None,
                    span: Span::new(start_position, start_position),
                })
            }
        }
    }
    
    /// 否定先読みルールを適用
    fn apply_negative_lookahead_rule(&self, ctx: &mut ParserContext, rule: &GrammarRule) -> Result<AstNode> {
        let start_position = ctx.current;
        
        match self.apply_rule(ctx, rule) {
            Ok(_) => {
                // ルールが成功した場合、否定先読みは失敗
                ctx.current = start_position; // 位置を元に戻す
                Err(Error::new(
                    "否定先読みに一致しました".to_string(),
                    Span::new(start_position, start_position)
                ))
            },
            Err(_) => {
                // ルールが失敗した場合、否定先読みは成功
                ctx.current = start_position; // 位置を元に戻す（消費しない）
                
                // 空のノードを作成
                Ok(AstNode::Empty {
                    span: Span::new(start_position, start_position),
                })
            }
        }
    }
    
    /// 肯定先読みルールを適用
    fn apply_positive_lookahead_rule(&self, ctx: &mut ParserContext, rule: &GrammarRule) -> Result<AstNode> {
        let start_position = ctx.current;
        
        match self.apply_rule(ctx, rule) {
            Ok(_) => {
                // ルールが成功した場合、肯定先読みも成功
                ctx.current = start_position; // 位置を元に戻す（消費しない）
                
                // 空のノードを作成
                Ok(AstNode::Empty {
                    span: Span::new(start_position, start_position),
                })
            },
            Err(e) => {
                // ルールが失敗した場合、肯定先読みも失敗
                ctx.current = start_position; // 位置を元に戻す
                Err(e)
            }
        }
    }
    
    /// 統計情報を取得
    pub fn get_statistics(&self) -> GrammarStats {
        self.stats.lock().clone()
    }
    
    /// トレースログを取得
    pub fn get_trace_log(&self) -> Vec<String> {
        if !self.trace_enabled {
            return vec!["トレースが無効です".to_string()];
        }
        
        let trace_log = self.trace_log.lock();
        trace_log.iter().map(|entry| {
            let indent = "  ".repeat(entry.depth);
            let status = if entry.success { "✓" } else { "✗" };
            let position_info = match entry.end_position {
                Some(end) => format!("{}→{}", entry.position, end),
                None => format!("{}", entry.position),
            };
            
            format!("{}{} {} [{}] ({:.2?}): {}", 
                indent, status, entry.rule_name, position_info, 
                entry.duration, entry.result_summary)
        }).collect()
    }
    
    /// 特定のルールに対して左再帰を検出
    pub fn detect_left_recursion(&self, rule_name: &str) -> bool {
        if let Some(rule) = self.rules.get(rule_name) {
            self.check_left_recursion(rule, &mut HashSet::new())
        } else {
            false
        }
    }
    
    /// 左再帰をチェック
    fn check_left_recursion(&self, rule: &GrammarRule, visited: &mut HashSet<String>) -> bool {
        match &rule.kind {
            GrammarRuleKind::NonTerminal(name) => {
                // 既に訪問したルールなら左再帰
                if visited.contains(name) {
                    return true;
                }
                
                // ルールをVisitedに追加
                if let Some(rule_name) = &rule.name {
                    visited.insert(rule_name.clone());
                }
                
                // 非終端記号のルールをチェック
                if let Some(sub_rule) = self.rules.get(name) {
                    let result = self.check_left_recursion(sub_rule, visited);
                    visited.remove(name);
                    result
                } else {
                    false
                }
            },
            GrammarRuleKind::Sequence(rules) => {
                // 先頭のルールだけチェック（左再帰の定義）
                if let Some(first_rule) = rules.first() {
                    self.check_left_recursion(first_rule, visited)
                } else {
                    false
                }
            },
            GrammarRuleKind::Choice(rules) => {
                // いずれかのルールが左再帰を含むか
                rules.iter().any(|r| self.check_left_recursion(r, visited))
            },
            GrammarRuleKind::ZeroOrMore(rule) | 
            GrammarRuleKind::OneOrMore(rule) | 
            GrammarRuleKind::Optional(rule) | 
            GrammarRuleKind::NegativeLookahead(rule) | 
            GrammarRuleKind::PositiveLookahead(rule) => {
                self.check_left_recursion(rule, visited)
            },
            _ => false,
        }
    }
}

// ================================
// パブリックAPI関数
// ================================

/// 新しい文法エンジンを作成
pub fn create_grammar_engine(start_rule: &str) -> GrammarEngine {
    GrammarEngine::new(start_rule)
}

/// 基本的なシェルコマンド文法を持つエンジンを作成
pub fn create_shell_grammar_engine() -> GrammarEngine {
    let mut engine = GrammarEngine::new("script");
    
    // 基本的な文法ルールを定義
    define_shell_grammar_rules(&mut engine);
    
    engine
}

/// シェル文法ルールを定義
fn define_shell_grammar_rules(engine: &mut GrammarEngine) {
    // スクリプト: 文の連続
    engine.add_rule("script", 
        GrammarRule::sequence(vec![
            GrammarRule::zero_or_more(GrammarRule::non_terminal("statement")),
        ])
    );
    
    // 文: コマンド、パイプライン、制御構造など
    engine.add_rule("statement", 
        GrammarRule::choice(vec![
            GrammarRule::non_terminal("command"),
            GrammarRule::non_terminal("pipeline"),
            GrammarRule::non_terminal("assignment"),
            GrammarRule::non_terminal("if_statement"),
            GrammarRule::non_terminal("for_statement"),
            GrammarRule::non_terminal("while_statement"),
            GrammarRule::non_terminal("function_definition"),
        ])
    );
    
    // コマンド: コマンド名と引数
    engine.add_rule("command", 
        GrammarRule::sequence(vec![
            GrammarRule::non_terminal("command_name"),
            GrammarRule::zero_or_more(GrammarRule::non_terminal("argument")),
            GrammarRule::zero_or_more(GrammarRule::non_terminal("redirection")),
            GrammarRule::optional(GrammarRule::terminal(TokenKind::Semicolon)),
        ])
    );
    
    // コマンド名: 識別子
    engine.add_rule("command_name", 
        GrammarRule::terminal(TokenKind::Identifier)
    );
    
    // 引数: 文字列、変数、その他
    engine.add_rule("argument", 
        GrammarRule::choice(vec![
            GrammarRule::terminal(TokenKind::String),
            GrammarRule::terminal(TokenKind::Identifier),
            GrammarRule::non_terminal("variable_reference"),
        ])
    );
    
    // パイプライン: コマンドをパイプでつなぐ
    engine.add_rule("pipeline", 
        GrammarRule::sequence(vec![
            GrammarRule::non_terminal("command"),
            GrammarRule::one_or_more(
                GrammarRule::sequence(vec![
                    GrammarRule::terminal(TokenKind::Pipe),
                    GrammarRule::non_terminal("command"),
                ])
            ),
        ])
    );
    
    // リダイレクション: 入出力のリダイレクト
    engine.add_rule("redirection", 
        GrammarRule::choice(vec![
            GrammarRule::sequence(vec![
                GrammarRule::terminal(TokenKind::RedirectIn),
                GrammarRule::terminal(TokenKind::String),
            ]),
            GrammarRule::sequence(vec![
                GrammarRule::terminal(TokenKind::RedirectOut),
                GrammarRule::terminal(TokenKind::String),
            ]),
            GrammarRule::sequence(vec![
                GrammarRule::terminal(TokenKind::RedirectAppend),
                GrammarRule::terminal(TokenKind::String),
            ]),
        ])
    );
    
    // 変数参照
    engine.add_rule("variable_reference", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::Dollar),
            GrammarRule::terminal(TokenKind::Identifier),
        ])
    );
    
    // 変数代入
    engine.add_rule("assignment", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::Identifier),
            GrammarRule::terminal(TokenKind::Equals),
            GrammarRule::non_terminal("value"),
            GrammarRule::optional(GrammarRule::terminal(TokenKind::Semicolon)),
        ])
    );
    
    // 値
    engine.add_rule("value", 
        GrammarRule::choice(vec![
            GrammarRule::terminal(TokenKind::String),
            GrammarRule::terminal(TokenKind::Number),
            GrammarRule::non_terminal("variable_reference"),
        ])
    );
    
    // if文
    engine.add_rule("if_statement", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::If),
            GrammarRule::non_terminal("condition"),
            GrammarRule::terminal(TokenKind::LeftBrace),
            GrammarRule::zero_or_more(GrammarRule::non_terminal("statement")),
            GrammarRule::terminal(TokenKind::RightBrace),
            GrammarRule::optional(
                GrammarRule::sequence(vec![
                    GrammarRule::terminal(TokenKind::Else),
                    GrammarRule::choice(vec![
                        GrammarRule::sequence(vec![
                            GrammarRule::terminal(TokenKind::LeftBrace),
                            GrammarRule::zero_or_more(GrammarRule::non_terminal("statement")),
                            GrammarRule::terminal(TokenKind::RightBrace),
                        ]),
                        GrammarRule::non_terminal("if_statement"), // else if
                    ]),
                ])
            ),
        ])
    );
    
    // 条件
    engine.add_rule("condition", 
        GrammarRule::non_terminal("command") // シェルでは条件もコマンド
    );
    
    // for文
    engine.add_rule("for_statement", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::For),
            GrammarRule::terminal(TokenKind::Identifier),
            GrammarRule::terminal(TokenKind::In),
            GrammarRule::non_terminal("value_list"),
            GrammarRule::terminal(TokenKind::LeftBrace),
            GrammarRule::zero_or_more(GrammarRule::non_terminal("statement")),
            GrammarRule::terminal(TokenKind::RightBrace),
        ])
    );
    
    // 値リスト
    engine.add_rule("value_list", 
        GrammarRule::sequence(vec![
            GrammarRule::non_terminal("value"),
            GrammarRule::zero_or_more(
                GrammarRule::sequence(vec![
                    GrammarRule::terminal(TokenKind::Comma),
                    GrammarRule::non_terminal("value"),
                ])
            ),
        ])
    );
    
    // while文
    engine.add_rule("while_statement", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::While),
            GrammarRule::non_terminal("condition"),
            GrammarRule::terminal(TokenKind::LeftBrace),
            GrammarRule::zero_or_more(GrammarRule::non_terminal("statement")),
            GrammarRule::terminal(TokenKind::RightBrace),
        ])
    );
    
    // 関数定義
    engine.add_rule("function_definition", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::Function),
            GrammarRule::terminal(TokenKind::Identifier),
            GrammarRule::terminal(TokenKind::LeftParen),
            GrammarRule::optional(GrammarRule::non_terminal("parameter_list")),
            GrammarRule::terminal(TokenKind::RightParen),
            GrammarRule::terminal(TokenKind::LeftBrace),
            GrammarRule::zero_or_more(GrammarRule::non_terminal("statement")),
            GrammarRule::terminal(TokenKind::RightBrace),
        ])
    );
    
    // パラメータリスト
    engine.add_rule("parameter_list", 
        GrammarRule::sequence(vec![
            GrammarRule::terminal(TokenKind::Identifier),
            GrammarRule::zero_or_more(
                GrammarRule::sequence(vec![
                    GrammarRule::terminal(TokenKind::Comma),
                    GrammarRule::terminal(TokenKind::Identifier),
                ])
            ),
        ])
    );
}

// テスト用コンポーネント
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_grammar() {
        // 基本的な文法のテスト実装
    }
    
    #[test]
    fn test_left_recursion_detection() {
        // 左再帰検出のテスト実装
    }
    
    #[test]
    fn test_memoization() {
        // メモ化のテスト実装
    }
} 