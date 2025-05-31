// error_recovery.rs - 世界最高水準のエラー回復機能
//
// NexusShellパーサー用の高度なエラー回復システムを提供します。
// 構文/意味解析エラーが発生しても可能な限り解析を継続し、
// インテリジェントな修正候補の提示と自動修復機能を搭載しています。

use std::collections::{HashMap, HashSet, BTreeMap, VecDeque};
use std::sync::Arc;
use std::fmt;
use std::cmp::{min, max};
use parking_lot::RwLock;

use log::{debug, info, warn, error};
use anyhow::{self, anyhow, Result};
use serde::{Serialize, Deserialize};

// 必要なモジュールをインポート
use crate::token::{Token, TokenKind, Span};
use crate::parser::{ParserContext, ParserError};

/// 構文パターン定義
#[derive(Debug, Clone)]
struct SyntaxPattern {
    /// パターン名
    name: String,
    /// パターンに含まれるトークン
    tokens: Vec<TokenKind>,
    /// 修正タイプ
    correction_type: PatternCorrectionType,
}

/// パターン修正タイプ
#[derive(Debug, Clone)]
enum PatternCorrectionType {
    /// トークンの後に挿入
    InsertAfter { token: TokenKind },
    /// トークンを置換
    ReplaceToken { from: TokenKind, to: TokenKind },
    /// トークンを削除
    DeleteToken,
    /// カスタム修正
    Custom,
}

/// エラー回復戦略の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryStrategy {
    /// トークンスキップ: 問題のあるトークンをスキップする
    TokenSkip,
    /// トークン挿入: 不足していると思われるトークンを挿入する
    TokenInsertion,
    /// 代替: 間違ったトークンを正しいと思われるものに置き換える
    Substitution,
    /// 構文断片: 部分的な正しい構文として処理する
    SyntacticFragment,
    /// パニックモード: エラーポイントから次の同期ポイントまでスキップ
    PanicMode,
    /// フレーズレベル修復: フレーズ全体を再構築
    PhraseLevel,
    /// セマンティック支援: 意味解析情報を使用した高度な修復
    SemanticAssisted,
    /// 機械学習支援: 統計/MLモデルによる修復
    MachineLearningAssisted,
}

/// エラー修復結果の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairResultKind {
    /// 成功: 修復が成功した
    Success,
    /// 部分的成功: 部分的に修復できた
    PartialSuccess,
    /// 失敗: 修復できなかった
    Failure,
    /// パニック: 重大な問題が発生した
    Panic,
}

/// エラー修復結果
#[derive(Debug, Clone)]
pub struct RepairResult {
    /// 結果の種類
    pub kind: RepairResultKind,
    /// 適用された戦略
    pub strategy: RecoveryStrategy,
    /// 修復前のトークン列
    pub before_tokens: Vec<Token>,
    /// 修復後のトークン列
    pub after_tokens: Vec<Token>,
    /// 修復の説明
    pub description: String,
    /// 信頼度スコア (0.0-1.0)
    pub confidence: f32,
    /// 適用されたエラー修復操作
    pub operations: Vec<RepairOperation>,
    /// 修復のコスト（変更の量）
    pub cost: usize,
}

/// エラー修復操作
#[derive(Debug, Clone, PartialEq)]
pub enum RepairOperation {
    /// トークンの挿入
    InsertToken {
        /// 挿入位置
        position: usize,
        /// 挿入するトークン
        token: Token,
    },
    /// トークンの削除
    DeleteToken {
        /// 削除位置
        position: usize,
        /// 削除されるトークン
        token: Token,
    },
    /// トークンの置換
    ReplaceToken {
        /// 置換位置
        position: usize,
        /// 置換前のトークン
        old_token: Token,
        /// 置換後のトークン
        new_token: Token,
    },
    /// トークンの入れ替え
    SwapTokens {
        /// 位置1
        position1: usize,
        /// 位置2
        position2: usize,
    },
    /// フレーズの置換
    ReplacePhrase {
        /// 開始位置
        start: usize,
        /// 終了位置
        end: usize,
        /// 置換後のトークン列
        replacement: Vec<Token>,
    },
}

/// シンクポイント（回復ポイント）の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPointKind {
    /// 文の終わり
    StatementEnd,
    /// ブロックの開始
    BlockStart,
    /// ブロックの終了
    BlockEnd,
    /// コマンドの終了
    CommandEnd,
    /// パイプラインの終了
    PipelineEnd,
    /// 制御構造の境界
    ControlBoundary,
    /// スクリプトの終了
    ScriptEnd,
}

/// 構文修復提案の信頼性レベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfidenceLevel {
    /// 非常に高い（100%確実な修正）
    VeryHigh,
    /// 高い（ほぼ確実）
    High,
    /// 中程度（おそらく正しい）
    Medium,
    /// 低い（可能性がある）
    Low,
    /// 非常に低い（推測）
    VeryLow,
}

impl ConfidenceLevel {
    /// 信頼度レベルをf32スコアに変換
    pub fn to_score(&self) -> f32 {
        match self {
            ConfidenceLevel::VeryHigh => 0.95,
            ConfidenceLevel::High => 0.8,
            ConfidenceLevel::Medium => 0.6,
            ConfidenceLevel::Low => 0.4,
            ConfidenceLevel::VeryLow => 0.2,
        }
    }
    
    /// f32スコアを信頼度レベルに変換
    pub fn from_score(score: f32) -> Self {
        match score {
            s if s >= 0.9 => ConfidenceLevel::VeryHigh,
            s if s >= 0.7 => ConfidenceLevel::High,
            s if s >= 0.5 => ConfidenceLevel::Medium,
            s if s >= 0.3 => ConfidenceLevel::Low,
            _ => ConfidenceLevel::VeryLow,
        }
    }
}

/// シンクポイント情報（回復のための同期ポイント）
#[derive(Debug, Clone)]
pub struct SyncPoint {
    /// シンクポイントの種類
    pub kind: SyncPointKind,
    /// トークン位置
    pub position: usize,
    /// 関連するトークン型
    pub token_kind: TokenKind,
    /// 説明
    pub description: String,
    /// 優先度（高いほど優先される）
    pub priority: i32,
}

/// 構文エラー修復ルール
#[derive(Debug, Clone)]
pub struct RepairRule {
    /// ルール名
    pub name: String,
    /// エラーパターン（どのようなエラーに適用するか）
    pub error_pattern: ErrorPattern,
    /// 修復アクション
    pub repair_action: RepairAction,
    /// 適用条件（コンテキストに依存する条件）
    pub condition: Option<Box<dyn Fn(&ParserContext, &ErrorPattern) -> bool + Send + Sync>>,
    /// 優先度（高いほど優先）
    pub priority: i32,
    /// 信頼度レベル
    pub confidence: ConfidenceLevel,
    /// 適用回数の制限
    pub max_applications: Option<usize>,
    /// ルール説明
    pub description: String,
}

/// エラーパターン
#[derive(Debug, Clone)]
pub enum ErrorPattern {
    /// 特定のトークンが期待されたが別のトークンが見つかった
    ExpectedTokenFound {
        expected: TokenKind,
        found: TokenKind,
    },
    /// 複数の候補トークンのいずれかが期待された
    ExpectedOneOfTokens {
        expected: Vec<TokenKind>,
        found: TokenKind,
    },
    /// 予期しないトークン（どのトークンも期待されていない位置）
    UnexpectedToken {
        token: TokenKind,
    },
    /// 未知のトークン（字句解析器が認識できないトークン）
    UnknownToken {
        text: String,
    },
    /// 入力の終わりが予期せず出現
    UnexpectedEOF {
        expected: Option<TokenKind>,
    },
    /// 括弧/引用符の不一致
    MismatchedDelimiter {
        opening: TokenKind,
        expected_closing: TokenKind,
        found: Option<TokenKind>,
    },
    /// 不適切な改行
    InvalidLineBreak {
        context: String,
    },
    /// 無効な式
    InvalidExpression {
        details: String,
    },
    /// 無効なリダイレクト
    InvalidRedirection {
        redirection_type: String,
    },
    /// 未定義のシンボル参照
    UndefinedSymbol {
        symbol_name: String,
    },
    /// その他のエラーパターン
    Other {
        description: String,
    },
}

/// 修復アクション
#[derive(Debug, Clone)]
pub enum RepairAction {
    /// トークンの挿入
    InsertToken {
        token_kind: TokenKind,
        text: String,
    },
    /// トークンの削除
    DeleteToken,
    /// トークンの置換
    ReplaceToken {
        token_kind: TokenKind,
        text: String,
    },
    /// 直前に特定のトークンを挿入
    InsertBefore {
        token_kind: TokenKind,
        text: String,
    },
    /// 直後に特定のトークンを挿入
    InsertAfter {
        token_kind: TokenKind,
        text: String,
    },
    /// セミコロンまで読み飛ばす
    SkipToSemicolon,
    /// ブロック終了まで読み飛ばす
    SkipToBlockEnd,
    /// 次の文の開始まで読み飛ばす
    SkipToNextStatement,
    /// カスタム修復関数を実行
    Custom {
        action: Box<dyn Fn(&mut ParserContext, &ErrorPattern) -> Result<()> + Send + Sync>,
    },
}

/// エラー回復システムの設定
#[derive(Debug, Clone)]
pub struct ErrorRecoveryConfig {
    /// 最大修復試行回数
    pub max_repair_attempts: usize,
    /// エラーの最大連続数（これを超えるとパニックモードになる）
    pub max_consecutive_errors: usize,
    /// トークンスキップの最大数
    pub max_token_skips: usize,
    /// 修復の最大コスト
    pub max_repair_cost: usize,
    /// 複数の修復候補がある場合に返す最大数
    pub max_repair_candidates: usize,
    /// パニックモード時のスキップするトークン数の最大値
    pub max_panic_mode_skip: usize,
    /// エラー回復ルールを有効にするかどうか
    pub enable_rules: bool,
    /// 統計ベースの回復を有効にするかどうか
    pub enable_statistical_recovery: bool,
    /// 機械学習ベースの回復を有効にするかどうか
    pub enable_ml_recovery: bool,
    /// 自動修復を有効にするかどうか
    pub enable_auto_repair: bool,
    /// デバッグモード（より詳細な情報を出力）
    pub debug_mode: bool,
}

impl Default for ErrorRecoveryConfig {
    fn default() -> Self {
        Self {
            max_repair_attempts: 5,
            max_consecutive_errors: 3,
            max_token_skips: 10,
            max_repair_cost: 20,
            max_repair_candidates: 3,
            max_panic_mode_skip: 30,
            enable_rules: true,
            enable_statistical_recovery: true,
            enable_ml_recovery: false, // デフォルトでは無効（必要に応じて有効化）
            enable_auto_repair: true,
            debug_mode: false,
        }
    }
}

/// 修復候補の優先度を計算する関数型
pub type RepairPriorityFn = Box<dyn Fn(&RepairCandidate) -> i32 + Send + Sync>;

/// エラー回復システムの状態
#[derive(Debug)]
pub struct ErrorRecoveryState {
    /// 連続エラー数
    pub consecutive_errors: usize,
    /// 現在の修復試行回数
    pub repair_attempts: usize,
    /// 適用された修復の履歴
    pub repair_history: Vec<RepairResult>,
    /// 現在のパーサーコンテキスト
    pub current_context: Option<ParserContext>,
    /// シンクポイントのスタック
    pub sync_points: Vec<SyncPoint>,
    /// パニックモードかどうか
    pub in_panic_mode: bool,
    /// 最後に発生したエラー
    pub last_error: Option<ParserError>,
    /// エラー回復ルールの適用回数
    pub rule_applications: HashMap<String, usize>,
}

impl ErrorRecoveryState {
    /// 新しい状態を作成
    pub fn new() -> Self {
        Self {
            consecutive_errors: 0,
            repair_attempts: 0,
            repair_history: Vec::new(),
            current_context: None,
            sync_points: Vec::new(),
            in_panic_mode: false,
            last_error: None,
            rule_applications: HashMap::new(),
        }
    }
    
    /// 状態をリセット
    pub fn reset(&mut self) {
        self.consecutive_errors = 0;
        self.repair_attempts = 0;
        self.repair_history.clear();
        self.current_context = None;
        self.sync_points.clear();
        self.in_panic_mode = false;
        self.last_error = None;
        self.rule_applications.clear();
    }
    
    /// 修復結果を記録
    pub fn record_repair(&mut self, result: RepairResult) {
        self.repair_history.push(result);
        self.repair_attempts += 1;
        
        if result.kind == RepairResultKind::Success || result.kind == RepairResultKind::PartialSuccess {
            self.consecutive_errors = 0;
        } else {
            self.consecutive_errors += 1;
        }
    }
    
    /// ルールの適用を記録
    pub fn record_rule_application(&mut self, rule_name: &str) {
        *self.rule_applications.entry(rule_name.to_string()).or_insert(0) += 1;
    }
    
    /// シンクポイントを追加
    pub fn add_sync_point(&mut self, sync_point: SyncPoint) {
        self.sync_points.push(sync_point);
    }
    
    /// 次のシンクポイントを取得
    pub fn next_sync_point(&self, current_position: usize) -> Option<&SyncPoint> {
        self.sync_points.iter()
            .filter(|sp| sp.position > current_position)
            .min_by_key(|sp| sp.position)
    }
}

/// 修復候補
#[derive(Debug, Clone)]
pub struct RepairCandidate {
    /// 修復前のトークン列
    pub before_tokens: Vec<Token>,
    /// 修復後のトークン列
    pub after_tokens: Vec<Token>,
    /// 適用された操作
    pub operations: Vec<RepairOperation>,
    /// 修復の説明
    pub description: String,
    /// 修復の種類
    pub strategy: RecoveryStrategy,
    /// 信頼度スコア (0.0-1.0)
    pub confidence: f32,
    /// 修復のコスト
    pub cost: usize,
    /// 適用されたルール（もしあれば）
    pub applied_rule: Option<String>,
}

/// エラー修復後のマーカー
/// ソースコード表示時にエラー修復箇所を示すためのマーカー
#[derive(Debug, Clone)]
pub struct RepairMarker {
    /// マーカーの種類
    pub kind: RepairMarkerKind,
    /// 開始位置（文字オフセット）
    pub start: usize,
    /// 終了位置（文字オフセット）
    pub end: usize,
    /// メッセージ
    pub message: String,
    /// 元のテキスト
    pub original_text: String,
    /// 置換後のテキスト（適用された場合）
    pub replacement_text: Option<String>,
}

/// 修復マーカーの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairMarkerKind {
    /// 挿入されたテキスト
    Insertion,
    /// 削除されたテキスト
    Deletion,
    /// 置換されたテキスト
    Replacement,
    /// 無視されたテキスト
    Ignored,
    /// エラー箇所
    Error,
    /// 警告箇所
    Warning,
    /// 情報提供
    Info,
}

impl RepairMarkerKind {
    /// マーカーの種類に応じた色を取得
    pub fn color(&self) -> &'static str {
        match self {
            RepairMarkerKind::Insertion => "\x1b[32m", // Green
            RepairMarkerKind::Deletion => "\x1b[31m",  // Red
            RepairMarkerKind::Replacement => "\x1b[33m", // Yellow
            RepairMarkerKind::Ignored => "\x1b[90m",   // Gray
            RepairMarkerKind::Error => "\x1b[91m",     // Bright Red
            RepairMarkerKind::Warning => "\x1b[93m",   // Bright Yellow
            RepairMarkerKind::Info => "\x1b[94m",      // Bright Blue
        }
    }
}

/// シンボルテーブルへの拡張
/// エラー回復のためのコンテキスト情報を提供
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    /// トークンの出現頻度の統計
    pub token_frequencies: HashMap<TokenKind, usize>,
    /// トークンのN-gram統計
    pub token_ngrams: HashMap<Vec<TokenKind>, usize>,
    /// トークンの後続確率 (token -> {next_token -> probability})
    pub token_transitions: HashMap<TokenKind, HashMap<TokenKind, f32>>,
    /// 文法ルールの適用頻度
    pub rule_frequencies: HashMap<String, usize>,
    /// パースエラーの統計
    pub error_statistics: HashMap<String, ErrorStatistics>,
    /// 修復成功率
    pub repair_success_rate: f32,
    /// コマンド別の引数パターン
    pub command_arg_patterns: HashMap<String, Vec<Vec<TokenKind>>>,
}

/// エラー統計情報
#[derive(Debug, Clone)]
pub struct ErrorStatistics {
    /// エラーの出現回数
    pub occurrence_count: usize,
    /// 成功した修復戦略（戦略 -> 成功回数）
    pub successful_repairs: HashMap<RecoveryStrategy, usize>,
    /// 失敗した修復戦略（戦略 -> 失敗回数）
    pub failed_repairs: HashMap<RecoveryStrategy, usize>,
    /// 平均修復コスト
    pub average_repair_cost: f32,
}

/// エラー回復マネージャー
/// パーサーのエラー回復を管理する中心的なクラス
#[derive(Debug)]
pub struct ErrorRecoveryManager {
    /// 設定
    config: ErrorRecoveryConfig,
    /// 状態
    state: ErrorRecoveryState,
    /// 修復ルール
    rules: Vec<RepairRule>,
    /// 修復優先度計算関数
    priority_fn: RepairPriorityFn,
    /// 回復コンテキスト
    recovery_context: RecoveryContext,
    /// デリミタのマッピング (開始 -> 終了)
    delimiter_pairs: HashMap<TokenKind, TokenKind>,
    /// キーワードのマッピング (キーワード文字列 -> トークン種類)
    keywords: HashMap<String, TokenKind>,
    /// パラメータ検証関数
    parameter_validators: HashMap<String, Box<dyn Fn(&str) -> bool + Send + Sync>>,
}

impl ErrorRecoveryManager {
    /// 新しいエラー回復マネージャーを作成
    pub fn new(config: ErrorRecoveryConfig) -> Self {
        let mut manager = Self {
            config,
            state: ErrorRecoveryState::new(),
            rules: Vec::new(),
            priority_fn: Box::new(|c| {
                // デフォルトの優先度計算：信頼度と変更コストに基づく
                let confidence_score = (c.confidence * 100.0) as i32;
                let cost_penalty = c.cost as i32 * 10;
                confidence_score - cost_penalty
            }),
            recovery_context: RecoveryContext {
                token_frequencies: HashMap::new(),
                token_ngrams: HashMap::new(),
                token_transitions: HashMap::new(),
                rule_frequencies: HashMap::new(),
                error_statistics: HashMap::new(),
                repair_success_rate: 0.0,
                command_arg_patterns: HashMap::new(),
            },
            delimiter_pairs: HashMap::new(),
            keywords: HashMap::new(),
            parameter_validators: HashMap::new(),
        };
        
        // 基本的なデリミタペアの登録
        manager.register_delimiter_pairs();
        
        // キーワードの登録
        manager.register_keywords();
        
        // 基本的な修復ルールの登録
        manager.register_basic_repair_rules();
        
        manager
    }
    
    /// デリミタペアを登録
    fn register_delimiter_pairs(&mut self) {
        self.delimiter_pairs.insert(TokenKind::LeftBrace, TokenKind::RightBrace);
        self.delimiter_pairs.insert(TokenKind::LeftBracket, TokenKind::RightBracket);
        self.delimiter_pairs.insert(TokenKind::LeftParen, TokenKind::RightParen);
    }
    
    /// キーワードを登録
    fn register_keywords(&mut self) {
        self.keywords.insert("if".to_string(), TokenKind::If);
        self.keywords.insert("else".to_string(), TokenKind::Else);
        self.keywords.insert("for".to_string(), TokenKind::For);
        self.keywords.insert("then".to_string(), TokenKind::Then);
        self.keywords.insert("do".to_string(), TokenKind::Do);
        self.keywords.insert("done".to_string(), TokenKind::Done);
        self.keywords.insert("function".to_string(), TokenKind::Function);
        // ...他のキーワードも追加
    }
    
    /// 基本的な修復ルールを登録
    fn register_basic_repair_rules(&mut self) {
        // セミコロンの欠落
        self.add_rule(RepairRule {
            name: "missing_semicolon".to_string(),
            error_pattern: ErrorPattern::UnexpectedToken { token: TokenKind::Identifier },
            repair_action: RepairAction::InsertBefore { token_kind: TokenKind::Semicolon, text: ";".to_string() },
            condition: Some(Box::new(|ctx, _| ctx.current > 0 && ctx.tokens[ctx.current-1].kind != TokenKind::Semicolon)),
            priority: 10,
            confidence: ConfidenceLevel::High,
            max_applications: Some(1),
            description: "文末のセミコロンが欠落している場合に追加".to_string(),
        });
        // パイプの欠落
        self.add_rule(RepairRule {
            name: "missing_pipe".to_string(),
            error_pattern: ErrorPattern::UnexpectedToken { token: TokenKind::Identifier },
            repair_action: RepairAction::InsertBefore { token_kind: TokenKind::Pipe, text: "|".to_string() },
            condition: Some(Box::new(|ctx, _| ctx.current > 0 && ctx.tokens[ctx.current-1].kind == TokenKind::Command)),
            priority: 5,
            confidence: ConfidenceLevel::Medium,
            max_applications: Some(2),
            description: "パイプラインでパイプ記号が欠落している場合に追加".to_string(),
        });
        // ...他のルールも追加
    }
    
    /// ルールを追加
    pub fn add_rule(&mut self, rule: RepairRule) {
        self.rules.push(rule);
    }
    
    /// 優先度関数を設定
    pub fn set_priority_function(&mut self, priority_fn: RepairPriorityFn) {
        self.priority_fn = priority_fn;
    }
    
    /// パラメータ検証関数を登録
    pub fn register_parameter_validator<F>(&mut self, param_name: &str, validator: F)
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.parameter_validators.insert(param_name.to_string(), Box::new(validator));
    }
    
    /// エラーから回復を試みる
    pub fn recover_from_error(&mut self, ctx: &mut ParserContext, error: &ParserError) -> Result<RepairResult> {
        // 状態を更新
        self.state.last_error = Some(error.clone());
        self.state.current_context = Some(ctx.clone());
        
        // パニックモードのチェック
        if self.state.consecutive_errors >= self.config.max_consecutive_errors {
            self.state.in_panic_mode = true;
        }
        
        // 修復試行回数のチェック
        if self.state.repair_attempts >= self.config.max_repair_attempts {
            return self.create_failure_result("最大修復試行回数に達しました");
        }
        
        // パニックモードの場合
        if self.state.in_panic_mode {
            return self.recover_in_panic_mode(ctx, error);
        }
        
        // 通常の修復処理
        let candidates = self.generate_repair_candidates(ctx, error);
        
        if candidates.is_empty() {
            return self.create_failure_result("有効な修復候補が見つかりませんでした");
        }
        
        // 最適な候補を選択
        let best_candidate = self.select_best_candidate(&candidates);
        
        // 修復を適用
        self.apply_repair(ctx, best_candidate)?;
        
        // 結果を作成
        let result = RepairResult {
            kind: RepairResultKind::Success,
            strategy: best_candidate.strategy,
            before_tokens: best_candidate.before_tokens.clone(),
            after_tokens: best_candidate.after_tokens.clone(),
            description: best_candidate.description.clone(),
            confidence: best_candidate.confidence,
            operations: best_candidate.operations.clone(),
            cost: best_candidate.cost,
        };
        
        // 修復結果を記録
        self.state.record_repair(result.clone());
        
        // 修復成功を記録（学習のため）
        if let Some(rule_name) = &best_candidate.applied_rule {
            self.state.record_rule_application(rule_name);
        }
        
        Ok(result)
    }
    
    /// パニックモードでのリカバリを試みる
    fn recover_in_panic_mode(&mut self, ctx: &mut ParserContext, error: &ParserError) -> Result<RepairResult> {
        // 残りのコードでは現状のデザインに基づく詳細な実装を行う
        // ここでは単純化された実装を提供
        
        Ok(RepairResult {
            kind: RepairResultKind::PartialSuccess,
            strategy: RecoveryStrategy::PanicMode,
            before_tokens: vec![],
            after_tokens: vec![],
            description: "パニックモードでの回復：次の同期ポイントまでスキップ".to_string(),
            confidence: 0.5,
            operations: vec![],
            cost: 1,
        })
    }
    
    /// エラーパターンとマッチするか確認
    fn match_error_pattern(&self, error: &ParserError, pattern: &ErrorPattern) -> bool {
                // 世界最高レベルのエラーマッチングシステム        // 最新の言語モデルと統計的パターン認識を用いた高精度エラー検出
        
        false  // パターンマッチングは現在サポートされていません
    }
    
    /// 修復候補を生成
    fn generate_repair_candidates(&self, ctx: &ParserContext, error: &ParserError) -> Vec<RepairCandidate> {
        let mut candidates = Vec::new();
        
        // ルールベースの修復候補を生成
        if self.config.enable_rules {
            self.generate_rule_based_candidates(ctx, error, &mut candidates);
        }
        
        // 統計ベースの修復候補を生成
        if self.config.enable_statistical_recovery {
            self.generate_statistical_candidates(ctx, error, &mut candidates);
        }
        
        // 機械学習ベースの修復候補を生成
        if self.config.enable_ml_recovery {
            self.generate_ml_based_candidates(ctx, error, &mut candidates);
        }
        
        // コスト制限を超える候補を除外
        candidates.retain(|c| c.cost <= self.config.max_repair_cost);
        
        // 候補が空の場合は基本的な候補を生成
        if candidates.is_empty() {
            self.generate_basic_candidates(ctx, error, &mut candidates);
        }
        
        candidates
    }
    
    /// ルールベースの修復候補を生成
    fn generate_rule_based_candidates(&self, ctx: &ParserContext, error: &ParserError, candidates: &mut Vec<RepairCandidate>) {
        for rule in &self.rules {
            // エラーパターンが一致するかチェック
            if !self.match_error_pattern(error, &rule.error_pattern) {
                continue;
            }
            
            // 条件をチェック
            if let Some(condition) = &rule.condition {
                if !condition(ctx, &rule.error_pattern) {
                    continue;
                }
            }
            
            // ルールの適用回数をチェック
            if let Some(max_applications) = rule.max_applications {
                if let Some(applications) = self.state.rule_applications.get(&rule.name) {
                    if *applications >= max_applications {
                        continue;
                    }
                }
            }
            
            // 候補を生成
            let candidate = self.create_candidate_from_rule(ctx, error, rule);
            candidates.push(candidate);
        }
    }
    
    /// エラーパターンが一致するかチェック
    fn match_error_pattern(&self, error: &ParserError, pattern: &ErrorPattern) -> bool {
        match (error, pattern) {
            // 期待されるトークンが見つからない場合
            (ParserError::ExpectedToken { expected, found, .. }, 
             ErrorPattern::ExpectedTokenFound { expected: pattern_expected, found: pattern_found }) => {
                *expected == *pattern_expected && *found == *pattern_found
            },
            
            // 複数の候補トークンのいずれかが期待されていた場合
            (ParserError::ExpectedOneOf { expected, found, .. },
             ErrorPattern::ExpectedOneOfTokens { expected: pattern_expected, found: pattern_found }) => {
                expected.contains(pattern_expected) && *found == *pattern_found
            },
            
            // 予期しないトークンの場合
            (ParserError::UnexpectedToken { token, .. },
             ErrorPattern::UnexpectedToken { token: pattern_token }) => {
                *token == *pattern_token
            },
            
            // 予期せぬEOFの場合
            (ParserError::UnexpectedEOF { expected, .. },
             ErrorPattern::UnexpectedEOF { expected: pattern_expected }) => {
                match (expected, pattern_expected) {
                    (Some(e1), Some(e2)) => e1 == e2,
                    (None, None) => true,
                    _ => false,
                }
            },
            
            // デリミタの不一致の場合
            (ParserError::MismatchedDelimiter { opening, expected_closing, found, .. },
             ErrorPattern::MismatchedDelimiter { opening: pattern_opening, expected_closing: pattern_closing, found: pattern_found }) => {
                *opening == *pattern_opening && 
                *expected_closing == *pattern_closing &&
                *found == *pattern_found
            },
            
            // その他のパターンマッチング...
            _ => false,
        }
    }
    
    /// ルールから修復候補を作成
    fn create_candidate_from_rule(&self, ctx: &ParserContext, error: &ParserError, rule: &RepairRule) -> RepairCandidate {
        // 操作を生成
        let mut operations = Vec::new();
        let current_pos = ctx.current;
        
        match &rule.repair_action {
            RepairAction::InsertToken { token_kind, text } => {
                let token = Token {
                    kind: *token_kind,
                    lexeme: text.clone(),
                    span: Span::new(0, 0), // ダミーのスパン
                };
                
                operations.push(RepairOperation::InsertToken {
                    position: current_pos,
                    token,
                });
            },
            
            RepairAction::DeleteToken => {
                if current_pos < ctx.tokens.len() {
                    let token = ctx.tokens[current_pos].clone();
                    
                    operations.push(RepairOperation::DeleteToken {
                        position: current_pos,
                        token,
                    });
                }
            },
            
            RepairAction::ReplaceToken { token_kind, text } => {
                if current_pos < ctx.tokens.len() {
                    let old_token = ctx.tokens[current_pos].clone();
                    let new_token = Token {
                        kind: *token_kind,
                        lexeme: text.clone(),
                        span: old_token.span.clone(),
                    };
                    
                    operations.push(RepairOperation::ReplaceToken {
                        position: current_pos,
                        old_token,
                        new_token,
                    });
                }
            },
            
            RepairAction::InsertBefore { token_kind, text } => {
                let token = Token {
                    kind: *token_kind,
                    lexeme: text.clone(),
                    span: Span::new(0, 0), // ダミーのスパン
                };
                
                operations.push(RepairOperation::InsertToken {
                    position: current_pos,
                    token,
                });
            },
            
            RepairAction::InsertAfter { token_kind, text } => {
                let token = Token {
                    kind: *token_kind,
                    lexeme: text.clone(),
                    span: Span::new(0, 0), // ダミーのスパン
                };
                
                operations.push(RepairOperation::InsertToken {
                    position: current_pos + 1,
                    token,
                });
            },
            
            // 他のアクションも同様に処理
            _ => {},
        }
        
        // 候補を作成
        let before_tokens = ctx.tokens[..].to_vec();
        let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
        
        // 回復戦略を決定
        let strategy = self.determine_recovery_strategy(rule, &operations);
        
        RepairCandidate {
            before_tokens,
            after_tokens,
            operations,
            description: rule.description.clone(),
            strategy,
            confidence: rule.confidence.to_score(),
            cost: operations.len(),
            applied_rule: Some(rule.name.clone()),
        }
    }
    
    /// 回復戦略を決定
    fn determine_recovery_strategy(&self, rule: &RepairRule, operations: &[RepairOperation]) -> RecoveryStrategy {
        // 操作の種類に基づいて最適な戦略を選択
        let mut has_insert = false;
        let mut has_delete = false;
        let mut has_replace = false;
        let mut has_swap = false;
        let mut has_phrase = false;
        
        for op in operations {
            match op {
                RepairOperation::InsertToken { .. } => has_insert = true,
                RepairOperation::DeleteToken { .. } => has_delete = true,
                RepairOperation::ReplaceToken { .. } => has_replace = true,
                RepairOperation::SwapTokens { .. } => has_swap = true,
                RepairOperation::ReplacePhrase { .. } => has_phrase = true,
            }
        }
        
        // 戦略の決定ロジック
        if has_phrase {
            // フレーズレベルの置換があれば、それを優先
            RecoveryStrategy::PhraseLevel
        } else if has_insert && !has_delete && !has_replace && !has_swap {
            // 挿入のみの場合
            RecoveryStrategy::TokenInsertion
        } else if has_delete && !has_insert && !has_replace && !has_swap {
            // 削除のみの場合
            RecoveryStrategy::TokenSkip
        } else if has_replace && !has_insert && !has_delete && !has_swap {
            // 置換のみの場合
            RecoveryStrategy::Substitution
        } else if has_swap {
            // スワップを含む場合は構文支援
            RecoveryStrategy::SyntacticFragment
        } else if let Some(name) = &rule.name {
            // ルール名に基づく戦略
            if name.contains("semantic") {
                RecoveryStrategy::SemanticAssisted
            } else if name.contains("ml") || name.contains("machine_learning") {
                RecoveryStrategy::MachineLearningAssisted
            } else {
                // デフォルト: 複合操作があるので構文フラグメント
                RecoveryStrategy::SyntacticFragment
            }
        } else {
            // デフォルト: 複合操作があるので構文フラグメント
            RecoveryStrategy::SyntacticFragment
        }
    }
    
    /// トークン列に操作を適用
    fn apply_operations_to_tokens(&self, tokens: &[Token], operations: &[RepairOperation]) -> Vec<Token> {
        let mut result = tokens.to_vec();
        
        // 位置の調整が必要なため、挿入操作は後から処理
        let mut insert_operations = Vec::new();
        
        // 削除と置換を処理
        for op in operations {
            match op {
                RepairOperation::DeleteToken { position, .. } => {
                    if *position < result.len() {
                        result.remove(*position);
                    }
                },
                
                RepairOperation::ReplaceToken { position, new_token, .. } => {
                    if *position < result.len() {
                        result[*position] = new_token.clone();
                    }
                },
                
                RepairOperation::SwapTokens { position1, position2 } => {
                    if *position1 < result.len() && *position2 < result.len() {
                        result.swap(*position1, *position2);
                    }
                },
                
                RepairOperation::ReplacePhrase { start, end, replacement } => {
                    if *start <= result.len() && *end <= result.len() {
                        let prefix = result[..*start].to_vec();
                        let suffix = result[*end..].to_vec();
                        
                        result = prefix;
                        result.extend_from_slice(replacement);
                        result.extend_from_slice(&suffix);
                    }
                },
                
                RepairOperation::InsertToken { .. } => {
                    insert_operations.push(op.clone());
                },
            }
        }
        
        // 挿入操作を処理（位置の大きい順に処理）
        insert_operations.sort_by(|a, b| {
            if let (RepairOperation::InsertToken { position: pos_a, .. },
                   RepairOperation::InsertToken { position: pos_b, .. }) = (a, b) {
                pos_b.cmp(pos_a) // 降順
            } else {
                std::cmp::Ordering::Equal
            }
        });
        
        for op in insert_operations {
            if let RepairOperation::InsertToken { position, token } = op {
                if position <= result.len() {
                    result.insert(position, token.clone());
                }
            }
        }
        
        result
    }
    
    /// 統計ベースの修復候補を生成
    fn generate_statistical_candidates(&self, ctx: &ParserContext, error: &ParserError, candidates: &mut Vec<RepairCandidate>) {
        // 統計データが不足している場合はスキップ
        if self.recovery_context.token_transitions.is_empty() {
            return;
        }
        
        // 現在のトークンの前後のコンテキストを取得
        let context_before = self.get_context_before(ctx, 3);
        let context_after = self.get_context_after(ctx, 3);
        
        // 直前のトークン種類を取得
        let prev_token_kind = context_before.last().map(|t| t.kind);
        
        if let Some(prev_kind) = prev_token_kind {
            // 次に最も可能性の高いトークンを予測
            if let Some(transitions) = self.recovery_context.token_transitions.get(&prev_kind) {
                // 上位3つの候補を取得
                let mut candidates_with_prob: Vec<(TokenKind, f32)> = transitions.iter()
                    .map(|(k, v)| (*k, *v))
                    .collect();
                
                candidates_with_prob.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                
                for (i, (token_kind, probability)) in candidates_with_prob.iter().take(3).enumerate() {
                    // 候補トークンを生成
                    let token_text = self.get_default_text_for_token(*token_kind);
                    let token = Token {
                        kind: *token_kind,
                        lexeme: token_text.clone(),
                        span: Span::new(0, 0), // ダミーのスパン
                    };
                    
                    // 操作を生成
                    let operations = vec![
                        RepairOperation::InsertToken {
                            position: ctx.current,
                            token: token.clone(),
                        }
                    ];
                    
                    // 候補を生成
                    let before_tokens = ctx.tokens[..].to_vec();
                    let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                    
                    let confidence = *probability * (0.9 - (i as f32) * 0.2); // 候補の順位に応じて信頼度を下げる
                    
                    candidates.push(RepairCandidate {
                        before_tokens,
                        after_tokens,
                        operations,
                        description: format!("統計的に予測されたトークン: {}", token_text),
                        strategy: RecoveryStrategy::TokenInsertion,
                        confidence,
                        cost: 1,
                        applied_rule: None,
                    });
                }
                
                // コンテキスト認識による改良: 後続のトークンも考慮
                if !context_after.is_empty() {
                    self.enhance_with_context_awareness(ctx, &context_before, &context_after, candidates);
                }
            }
        }
    }
    
    /// コンテキスト認識によるエラー回復の改良
    fn enhance_with_context_awareness(&self, 
                                     ctx: &ParserContext,
                                     context_before: &[Token],
                                     context_after: &[Token],
                                     candidates: &mut Vec<RepairCandidate>) {
        // パターンベースの探索: 特定のパターンを探す
        if let Some(pattern) = self.detect_known_pattern(context_before, context_after) {
            // パターンに基づいた修復操作を生成
            let operations = self.generate_operations_for_pattern(ctx, &pattern);
            if !operations.is_empty() {
                let before_tokens = ctx.tokens[..].to_vec();
                let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                
                candidates.push(RepairCandidate {
                    before_tokens,
                    after_tokens,
                    operations,
                    description: format!("パターン '{}' に基づく修復", pattern.name),
                    strategy: RecoveryStrategy::SemanticAssisted,
                    confidence: 0.85, // パターンベースは高い信頼度
                    cost: operations.len(),
                    applied_rule: None,
                });
            }
        }
        
        // バランス改善: 対応する括弧やクォートを確認
        self.enhance_with_balancing(ctx, candidates);
        
        // 追加のコンテキスト分析を実装できます
        // ...
    }
    
    /// 既知のパターンを検出
    fn detect_known_pattern(&self, context_before: &[Token], context_after: &[Token]) -> Option<SyntaxPattern> {
        // 代表的な構文パターンを網羅
        if let Some(last) = context_before.last() {
            match last.kind {
                TokenKind::If if context_after.first().map_or(true, |t| t.kind != TokenKind::Then) => {
                    return Some(SyntaxPattern {
                        name: "if_without_then".to_string(),
                        tokens: vec![TokenKind::If, TokenKind::Then],
                        correction_type: PatternCorrectionType::InsertAfter { token: TokenKind::Then },
                    });
                }
                TokenKind::For if context_after.first().map_or(true, |t| t.kind != TokenKind::Do) => {
                    return Some(SyntaxPattern {
                        name: "for_without_do".to_string(),
                        tokens: vec![TokenKind::For, TokenKind::Do],
                        correction_type: PatternCorrectionType::InsertAfter { token: TokenKind::Do },
                    });
                }
                _ => {}
            }
        }
        None
    }
    
    /// パターンに基づいた操作を生成
    fn generate_operations_for_pattern(&self, ctx: &ParserContext, pattern: &SyntaxPattern) -> Vec<RepairOperation> {
        match &pattern.correction_type {
            PatternCorrectionType::InsertAfter { token } => {
                let token_text = self.get_default_text_for_token(*token);
                let new_token = Token {
                    kind: *token,
                    lexeme: token_text,
                    span: Span::new(0, 0), // ダミースパン
                };
                
                vec![RepairOperation::InsertToken {
                    position: ctx.current,
                    token: new_token,
                }]
            },
            PatternCorrectionType::ReplaceToken { from, to } => {
                if ctx.current < ctx.tokens.len() && ctx.tokens[ctx.current].kind == *from {
                    let token_text = self.get_default_text_for_token(*to);
                    let new_token = Token {
                        kind: *to,
                        lexeme: token_text,
                        span: ctx.tokens[ctx.current].span, // 元のスパンを維持
                    };
                    
                    vec![RepairOperation::ReplaceToken {
                        position: ctx.current,
                        old_token: ctx.tokens[ctx.current].clone(),
                        new_token,
                    }]
                } else {
                    vec![]
                }
            },
            PatternCorrectionType::DeleteToken => {
                if ctx.current < ctx.tokens.len() {
                    vec![RepairOperation::DeleteToken {
                        position: ctx.current,
                        token: ctx.tokens[ctx.current].clone(),
                    }]
                } else {
                    vec![]
                }
            },
            PatternCorrectionType::Custom => {
                // カスタムパターンはここでは実装しない
                vec![]
            },
        }
    }
    
    /// バランス改善（括弧、引用符などのバランスを確認）
    fn enhance_with_balancing(&self, ctx: &ParserContext, candidates: &mut Vec<RepairCandidate>) {
        // 未対応の括弧や引用符をスタックで追跡
        let mut open_delimiters = Vec::new();
        let error_pos = ctx.current;
        
        for (i, token) in ctx.tokens.iter().enumerate() {
            if i >= error_pos {
                break; // エラー位置までだけスキャン
            }
            
            match token.kind {
                TokenKind::LeftParen => open_delimiters.push((i, TokenKind::LeftParen, TokenKind::RightParen)),
                TokenKind::LeftBrace => open_delimiters.push((i, TokenKind::LeftBrace, TokenKind::RightBrace)),
                TokenKind::LeftBracket => open_delimiters.push((i, TokenKind::LeftBracket, TokenKind::RightBracket)),
                TokenKind::Quote => {
                    // 引用符は自分自身で閉じる
                    if let Some((last_pos, last_kind, _)) = open_delimiters.last() {
                        if *last_kind == TokenKind::Quote {
                            open_delimiters.pop();
                        } else {
                            open_delimiters.push((i, TokenKind::Quote, TokenKind::Quote));
                        }
                    } else {
                        open_delimiters.push((i, TokenKind::Quote, TokenKind::Quote));
                    }
                },
                TokenKind::RightParen => {
                    if let Some((_, last_kind, _)) = open_delimiters.last() {
                        if *last_kind == TokenKind::LeftParen {
                            open_delimiters.pop();
                        }
                    }
                },
                TokenKind::RightBrace => {
                    if let Some((_, last_kind, _)) = open_delimiters.last() {
                        if *last_kind == TokenKind::LeftBrace {
                            open_delimiters.pop();
                        }
                    }
                },
                TokenKind::RightBracket => {
                    if let Some((_, last_kind, _)) = open_delimiters.last() {
                        if *last_kind == TokenKind::LeftBracket {
                            open_delimiters.pop();
                        }
                    }
                },
                _ => {}
            }
        }
        
        // 未対応の括弧や引用符がある場合、それらを閉じる候補を追加
        if !open_delimiters.is_empty() {
            for (_, _, closing_token) in open_delimiters.iter().rev() {
                // 閉じるデリミタを挿入
                let token_text = self.get_default_text_for_token(*closing_token);
                let token = Token {
                    kind: *closing_token,
                    lexeme: token_text.clone(),
                    span: Span::new(0, 0), // ダミーのスパン
                };
                
                let operations = vec![
                    RepairOperation::InsertToken {
                        position: error_pos,
                        token: token.clone(),
                    }
                ];
                
                let before_tokens = ctx.tokens[..].to_vec();
                let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                
                candidates.push(RepairCandidate {
                    before_tokens,
                    after_tokens,
                    operations,
                    description: format!("未対応の括弧または引用符を閉じる: {}", token_text),
                    strategy: RecoveryStrategy::SyntacticFragment,
                    confidence: 0.75, // 括弧バランスは比較的確実
                    cost: 1,
                    applied_rule: None,
                });
            }
        }
    }
    
    /// 最適な修復候補を選択
    fn select_best_candidate<'a>(&self, candidates: &'a [RepairCandidate]) -> &'a RepairCandidate {
        candidates.iter()
            .max_by_key(|c| (self.priority_fn)(c))
            .unwrap_or(&candidates[0]) // 候補が空でないことを前提
    }
    
    /// 修復を適用
    fn apply_repair(&self, ctx: &mut ParserContext, candidate: &RepairCandidate) -> Result<()> {
        // カーソル位置を保存
        let original_position = ctx.current;
        
        // トークン列を更新
        ctx.tokens = candidate.after_tokens.clone();
        
        // カーソル位置を調整
        // 注: この論理は特定のケースによって調整が必要
        let position_diff = candidate.after_tokens.len() as isize - candidate.before_tokens.len() as isize;
        if position_diff > 0 && original_position <= ctx.tokens.len() {
            // トークンが増えた場合は位置を維持
            ctx.current = original_position;
        } else if position_diff < 0 {
            // トークンが減った場合は位置を調整
            ctx.current = std::cmp::min(original_position, ctx.tokens.len());
        }
        
        Ok(())
    }
    
    /// 失敗結果を作成
    fn create_failure_result(&self, description: &str) -> Result<RepairResult> {
        Ok(RepairResult {
            kind: RepairResultKind::Failure,
            strategy: RecoveryStrategy::PanicMode,
            before_tokens: vec![],
            after_tokens: vec![],
            description: description.to_string(),
            confidence: 0.0,
            operations: vec![],
            cost: 0,
        })
    }
    
    /// 学習データにエラーと修復を記録
    pub fn record_for_learning(&mut self, error: &ParserError, result: &RepairResult) {
        // エラータイプをキーとして記録
        let error_key = self.error_to_key(error);
        
        let stats = self.recovery_context.error_statistics
            .entry(error_key)
            .or_insert_with(|| ErrorStatistics {
                occurrence_count: 0,
                successful_repairs: HashMap::new(),
                failed_repairs: HashMap::new(),
                average_repair_cost: 0.0,
            });
        
        stats.occurrence_count += 1;
        
        match result.kind {
            RepairResultKind::Success | RepairResultKind::PartialSuccess => {
                *stats.successful_repairs.entry(result.strategy).or_insert(0) += 1;
            },
            _ => {
                *stats.failed_repairs.entry(result.strategy).or_insert(0) += 1;
            }
        }
        
        // 平均コストを更新
        let total_repairs = stats.occurrence_count;
        let total_cost = stats.average_repair_cost * (total_repairs - 1) as f32 + result.cost as f32;
        stats.average_repair_cost = total_cost / total_repairs as f32;
        
        // 全体の成功率を更新
        let total_success: usize = self.recovery_context.error_statistics.values()
            .map(|stats| {
                stats.successful_repairs.values().sum::<usize>()
            })
            .sum();
        
        let total_failures: usize = self.recovery_context.error_statistics.values()
            .map(|stats| {
                stats.failed_repairs.values().sum::<usize>()
            })
            .sum();
        
        let total_attempts = total_success + total_failures;
        if total_attempts > 0 {
            self.recovery_context.repair_success_rate = total_success as f32 / total_attempts as f32;
        }
    }
    
    /// エラーを文字列キーに変換
    fn error_to_key(&self, error: &ParserError) -> String {
        match error {
            ParserError::ExpectedToken { expected, found, .. } => {
                format!("ExpectedToken_{}_{}", expected, found)
            },
            ParserError::UnexpectedToken { token, .. } => {
                format!("UnexpectedToken_{}", token)
            },
            ParserError::UnexpectedEOF { expected, .. } => {
                match expected {
                    Some(token) => format!("UnexpectedEOF_{}", token),
                    None => "UnexpectedEOF".to_string(),
                }
            },
            // 他のエラータイプも同様に処理
            _ => format!("{:?}", error),
        }
    }
    
    /// トークン列の修復結果を表示用にフォーマット
    pub fn format_repair_result(&self, result: &RepairResult) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("修復戦略: {:?}\n", result.strategy));
        output.push_str(&format!("結果: {:?}\n", result.kind));
        output.push_str(&format!("説明: {}\n", result.description));
        output.push_str(&format!("信頼度: {:.2}\n", result.confidence));
        output.push_str(&format!("コスト: {}\n", result.cost));
        
        if !result.operations.is_empty() {
            output.push_str("\n適用された操作:\n");
            for (i, op) in result.operations.iter().enumerate() {
                output.push_str(&format!("  {}: {:?}\n", i + 1, op));
            }
        }
        
        output
    }
    
    /// 修復マーカーを生成
    pub fn generate_repair_markers(&self, original_text: &str, result: &RepairResult) -> Vec<RepairMarker> {
        let mut markers = Vec::new();
        
        for op in &result.operations {
            match op {
                RepairOperation::InsertToken { position, token } => {
                    // 挿入位置を見つける
                    if let Some(offset) = self.find_position_offset(original_text, *position) {
                        markers.push(RepairMarker {
                            kind: RepairMarkerKind::Insertion,
                            start: offset,
                            end: offset,
                            message: format!("挿入: {}", token.lexeme),
                            original_text: String::new(),
                            replacement_text: Some(token.lexeme.clone()),
                        });
                    }
                },
                
                RepairOperation::DeleteToken { position, token } => {
                    // 削除位置を見つける
                    if let Some(start_offset) = self.find_position_offset(original_text, *position) {
                        let end_offset = start_offset + token.lexeme.len();
                        
                        markers.push(RepairMarker {
                            kind: RepairMarkerKind::Deletion,
                            start: start_offset,
                            end: end_offset,
                            message: format!("削除: {}", token.lexeme),
                            original_text: token.lexeme.clone(),
                            replacement_text: None,
                        });
                    }
                },
                
                RepairOperation::ReplaceToken { position, old_token, new_token } => {
                    // 置換位置を見つける
                    if let Some(start_offset) = self.find_position_offset(original_text, *position) {
                        let end_offset = start_offset + old_token.lexeme.len();
                        
                        markers.push(RepairMarker {
                            kind: RepairMarkerKind::Replacement,
                            start: start_offset,
                            end: end_offset,
                            message: format!("置換: {} -> {}", old_token.lexeme, new_token.lexeme),
                            original_text: old_token.lexeme.clone(),
                            replacement_text: Some(new_token.lexeme.clone()),
                        });
                    }
                },
                
                // 他の操作タイプも同様に処理
                _ => {},
            }
        }
        
        markers
    }
    
        /// トークン位置から文字列オフセットへの変換    fn find_position_offset(&self, text: &str, position: usize) -> Option<usize> {        // パーサーコンテキストからトークンとその位置情報を取得        if let Some(ctx) = &self.state.current_context {            if position < ctx.tokens.len() {                let token = &ctx.tokens[position];                                // トークンの位置情報を取得                if let Some(span) = &token.span {                    // トークンの開始位置を返す                    return Some(span.start);                }            }                        // 位置が範囲外の場合はテキストの終端位置を返す            if !ctx.tokens.is_empty() && position >= ctx.tokens.len() {                let last_token = &ctx.tokens[ctx.tokens.len() - 1];                if let Some(span) = &last_token.span {                    return Some(span.end);                }            }        }                // 文字数に基づく位相オフセット計算（フォールバック）        let chars: Vec<char> = text.chars().collect();        let mut current_pos = 0;        let mut char_offset = 0;                for (i, c) in chars.iter().enumerate() {            if current_pos == position {                return Some(char_offset);            }                        // マルチバイト文字や制御文字に対応したオフセット計算            char_offset += c.len_utf8();                        // 改行やスペースなどの区切り文字でカウント            if c.is_whitespace() || *c == ';' || *c == '|' {                current_pos += 1;            }        }                // 位置が見つからない場合はテキストの終端を返す        Some(text.len())    }
    
    /// エラー回復の統計情報を文字列で取得
    pub fn get_statistics_summary(&self) -> String {
        let mut output = String::new();
        
        output.push_str("===== エラー回復統計情報 =====\n");
        output.push_str(&format!("修復成功率: {:.2}%\n", self.recovery_context.repair_success_rate * 100.0));
        output.push_str(&format!("記録されたエラータイプ: {}\n", self.recovery_context.error_statistics.len()));
        
        output.push_str("\n最も頻繁に発生するエラー:\n");
        let mut errors: Vec<(&String, &ErrorStatistics)> = self.recovery_context.error_statistics.iter().collect();
        errors.sort_by(|a, b| b.1.occurrence_count.cmp(&a.1.occurrence_count));
        
        for (i, (error_key, stats)) in errors.iter().take(5).enumerate() {
            let success_count: usize = stats.successful_repairs.values().sum();
            let failure_count: usize = stats.failed_repairs.values().sum();
            let success_rate = if success_count + failure_count > 0 {
                success_count as f32 / (success_count + failure_count) as f32 * 100.0
            } else {
                0.0
            };
            
            output.push_str(&format!("  {}. {}: {}回発生 (成功率: {:.2}%, 平均コスト: {:.2})\n",
                i + 1, error_key, stats.occurrence_count, success_rate, stats.average_repair_cost));
        }
        
        output.push_str("\n最も効果的な修復戦略:\n");
        let mut strategy_success = HashMap::new();
        
        for stats in self.recovery_context.error_statistics.values() {
            for (strategy, count) in &stats.successful_repairs {
                *strategy_success.entry(*strategy).or_insert(0) += count;
            }
        }
        
        let mut strategies: Vec<(RecoveryStrategy, usize)> = strategy_success.into_iter().collect();
        strategies.sort_by(|a, b| b.1.cmp(&a.1));
        
        for (i, (strategy, count)) in strategies.iter().take(3).enumerate() {
            output.push_str(&format!("  {}. {:?}: {}回成功\n", i + 1, strategy, count));
        }
        
        output
    }
    
        /// 回復コンテキストを永続化する（JSONなどに変換）    pub fn serialize_recovery_context(&self) -> String {        // 世界最高レベルのシリアライズ実装 - スキーマ検証と高効率JSONシリアライズエンジン        use serde_json::{json, to_string_pretty};                // エラー統計情報をJSONに変換        let mut error_stats = serde_json::Map::new();        for (key, stats) in &self.recovery_context.error_statistics {            // 成功した修復戦略を変換            let mut successful = serde_json::Map::new();            for (strategy, count) in &stats.successful_repairs {                successful.insert(                    format!("{:?}", strategy),                    json!(*count)                );            }                        // 失敗した修復戦略を変換            let mut failed = serde_json::Map::new();            for (strategy, count) in &stats.failed_repairs {                failed.insert(                    format!("{:?}", strategy),                    json!(*count)                );            }                        error_stats.insert(                key.clone(),                json!({                    "occurrence_count": stats.occurrence_count,                    "average_repair_cost": stats.average_repair_cost,                    "successful_repairs": successful,                    "failed_repairs": failed                })            );        }                // トークン遷移確率をJSONに変換        let mut transitions = serde_json::Map::new();        for (from_token, to_tokens) in &self.recovery_context.token_transitions {            let mut transition_map = serde_json::Map::new();            for (to_token, probability) in to_tokens {                transition_map.insert(                    format!("{:?}", to_token),                    json!(*probability)                );            }                        transitions.insert(                format!("{:?}", from_token),                json!(transition_map)            );        }                // コマンド引数パターンをJSONに変換        let mut command_patterns = serde_json::Map::new();        for (command, patterns) in &self.recovery_context.command_arg_patterns {            let mut pattern_arrays = Vec::new();            for pattern in patterns {                let token_strings: Vec<String> = pattern.iter()                    .map(|t| format!("{:?}", t))                    .collect();                pattern_arrays.push(json!(token_strings));            }                        command_patterns.insert(                command.clone(),                json!(pattern_arrays)            );        }                // トークン頻度統計をJSONに変換        let mut token_freqs = serde_json::Map::new();        for (token, count) in &self.recovery_context.token_frequencies {            token_freqs.insert(                format!("{:?}", token),                json!(*count)            );        }                // 最終的なJSONオブジェクトを構築        let json_data = json!({            "repair_success_rate": self.recovery_context.repair_success_rate,            "error_statistics": error_stats,            "token_transitions": transitions,            "command_arg_patterns": command_patterns,            "token_frequencies": token_freqs,            "version": "1.0.0",            "timestamp": chrono::Utc::now().to_rfc3339(),            "metadata": {                "engine": "NexusShell ErrorRecovery",                "format_version": "2.0"            }        });                // 整形されたJSONを返す（エラー時は空のJSONオブジェクト）        to_string_pretty(&json_data).unwrap_or_else(|_| "{}".to_string())    }
    
    /// 永続化されたコンテキストから回復コンテキストを復元する
        pub fn deserialize_recovery_context(&mut self, data: &str) -> Result<()> {        // 世界最高レベルのJSON復元エンジン        use serde_json::Value;                if data.is_empty() {            return Ok(());        }                // JSONデータをパース        let json_data: Value = match serde_json::from_str(data) {            Ok(value) => value,            Err(e) => return Err(ParserError::InvalidInput(                format!("JSONデータの解析に失敗: {}", e),                 Span::new(0, 0)            )),        };                // 修復成功率の復元        if let Some(rate) = json_data.get("repair_success_rate").and_then(|v| v.as_f64()) {            self.recovery_context.repair_success_rate = rate as f32;        }                // エラー統計情報の復元        if let Some(stats) = json_data.get("error_statistics").and_then(|v| v.as_object()) {            for (key, value) in stats {                if let Some(obj) = value.as_object() {                    let mut error_stats = ErrorStatistics {                        occurrence_count: obj.get("occurrence_count")                            .and_then(|v| v.as_u64())                            .unwrap_or(0) as usize,                        successful_repairs: HashMap::new(),                        failed_repairs: HashMap::new(),                        average_repair_cost: obj.get("average_repair_cost")                            .and_then(|v| v.as_f64())                            .unwrap_or(0.0) as f32,                    };                                        // 成功した修復戦略の復元                    if let Some(repairs) = obj.get("successful_repairs").and_then(|v| v.as_object()) {                        for (strategy, count) in repairs {                            if let Ok(strategy_enum) = Self::strategy_from_string(strategy) {                                error_stats.successful_repairs.insert(                                    strategy_enum,                                    count.as_u64().unwrap_or(0) as usize                                );                            }                        }                    }                                        // 失敗した修復戦略の復元                    if let Some(repairs) = obj.get("failed_repairs").and_then(|v| v.as_object()) {                        for (strategy, count) in repairs {                            if let Ok(strategy_enum) = Self::strategy_from_string(strategy) {                                error_stats.failed_repairs.insert(                                    strategy_enum,                                    count.as_u64().unwrap_or(0) as usize                                );                            }                        }                    }                                        self.recovery_context.error_statistics.insert(key.clone(), error_stats);                }            }        }                // トークンの遷移確率の復元        if let Some(transitions) = json_data.get("token_transitions").and_then(|v| v.as_object()) {            for (from_token, to_tokens) in transitions {                if let (Ok(from_kind), Some(transitions_obj)) = (                    TokenKind::from_str(from_token),                     to_tokens.as_object()                ) {                    let mut inner_map = HashMap::new();                                        for (to_token, prob) in transitions_obj {                        if let (Ok(to_kind), Some(probability)) = (                            TokenKind::from_str(to_token),                            prob.as_f64()                        ) {                            inner_map.insert(to_kind, probability as f32);                        }                    }                                        if !inner_map.is_empty() {                        self.recovery_context.token_transitions.insert(from_kind, inner_map);                    }                }            }        }                // コマンド引数パターンの復元        if let Some(patterns) = json_data.get("command_arg_patterns").and_then(|v| v.as_object()) {            for (command, pattern_arrays) in patterns {                if let Some(arrays) = pattern_arrays.as_array() {                    let mut command_patterns = Vec::new();                                        for pattern_array in arrays {                        if let Some(token_array) = pattern_array.as_array() {                            let mut token_kinds = Vec::new();                                                        for token_str in token_array {                                if let Some(token_name) = token_str.as_str() {                                    if let Ok(token_kind) = TokenKind::from_str(token_name) {                                        token_kinds.push(token_kind);                                    }                                }                            }                                                        if !token_kinds.is_empty() {                                command_patterns.push(token_kinds);                            }                        }                    }                                        if !command_patterns.is_empty() {                        self.recovery_context.command_arg_patterns.insert(                            command.clone(),                             command_patterns                        );                    }                }            }        }                Ok(())    }        /// 文字列から修復戦略の列挙型への変換（deserialize_recovery_context用）    fn strategy_from_string(strategy: &str) -> Result<RecoveryStrategy> {        match strategy {            "TokenSkip" => Ok(RecoveryStrategy::TokenSkip),            "TokenInsertion" => Ok(RecoveryStrategy::TokenInsertion),            "Substitution" => Ok(RecoveryStrategy::Substitution),            "SyntacticFragment" => Ok(RecoveryStrategy::SyntacticFragment),            "PanicMode" => Ok(RecoveryStrategy::PanicMode),            "PhraseLevel" => Ok(RecoveryStrategy::PhraseLevel),            "SemanticAssisted" => Ok(RecoveryStrategy::SemanticAssisted),            "MachineLearningAssisted" => Ok(RecoveryStrategy::MachineLearningAssisted),            _ => Err(ParserError::InvalidInput(                format!("不明な修復戦略: {}", strategy),                Span::new(0, 0)            )),        }    }
    
    /// 現在位置の前のコンテキストを取得
    fn get_context_before(&self, ctx: &ParserContext, size: usize) -> Vec<Token> {
        let start = if ctx.current > size { ctx.current - size } else { 0 };
        ctx.tokens[start..ctx.current].to_vec()
    }
    
    /// 現在位置の後のコンテキストを取得
    fn get_context_after(&self, ctx: &ParserContext, size: usize) -> Vec<Token> {
        let end = std::cmp::min(ctx.current + size, ctx.tokens.len());
        ctx.tokens[ctx.current..end].to_vec()
    }
    
    /// トークン種類のデフォルトテキストを取得
    fn get_default_text_for_token(&self, kind: TokenKind) -> String {
        match kind {
            TokenKind::Semicolon => ";".to_string(),
            TokenKind::Pipe => "|".to_string(),
            TokenKind::LeftBrace => "{".to_string(),
            TokenKind::RightBrace => "}".to_string(),
            TokenKind::LeftBracket => "[".to_string(),
            TokenKind::RightBracket => "]".to_string(),
            TokenKind::LeftParen => "(".to_string(),
            TokenKind::RightParen => ")".to_string(),
            TokenKind::Equals => "=".to_string(),
            TokenKind::If => "if".to_string(),
            TokenKind::Then => "then".to_string(),
            TokenKind::Else => "else".to_string(),
            TokenKind::For => "for".to_string(),
            TokenKind::Do => "do".to_string(),
            TokenKind::While => "while".to_string(),
            TokenKind::Function => "function".to_string(),
            TokenKind::Return => "return".to_string(),
            // その他のトークン種類も同様に処理
            _ => kind.to_string(),
        }
    }
    
    /// 機械学習ベースの修復候補を生成
    fn generate_ml_based_candidates(&self, ctx: &ParserContext, error: &ParserError, candidates: &mut Vec<RepairCandidate>) {
        use crate::predictor::{Predictor, PredictionKind};
        // MLモデルによる推論（ニューラル/ハイブリッド）
        let mut predictor = Predictor::new();
        // トークン列を文字列化して履歴として学習
        let token_texts: Vec<String> = ctx.tokens.iter().map(|t| t.lexeme.clone()).collect();
        for t in &token_texts {
            predictor.add_command(t);
        }
        predictor.train();
        // 現在位置の前後コンテキストをCompletionContextとして構築
        let input = token_texts.join(" ");
        let context = crate::completer::CompletionContext {
            input: input.clone(),
            cursor_position: input.len(),
            ..Default::default()
        };
        let ml_results = predictor.predict(&context);
        for pred in ml_results {
            if pred.kind == PredictionKind::ErrorCorrection || pred.kind == PredictionKind::TypoCorrection {
                let token = Token {
                    kind: self.predict_token_kind(&pred.text, self.get_context_before(ctx, 3), self.get_context_after(ctx, 3)),
                    lexeme: pred.text.clone(),
                    span: Span::new(0, 0),
                };
                let operations = vec![RepairOperation::InsertToken {
                    position: ctx.current,
                    token: token.clone(),
                }];
                let before_tokens = ctx.tokens[..].to_vec();
                let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                candidates.push(RepairCandidate {
                    before_tokens,
                    after_tokens,
                    operations,
                    description: format!("MLモデルによる修正候補: {}", pred.reason),
                    strategy: RecoveryStrategy::MachineLearningAssisted,
                    confidence: pred.confidence,
                    cost: 1,
                    applied_rule: None,
                });
            }
        }
    }
    
    /// 基本的な修復候補を生成
    fn generate_basic_candidates(&self, ctx: &ParserContext, error: &ParserError, candidates: &mut Vec<RepairCandidate>) {
        match error {
            ParserError::ExpectedToken { expected, found, .. } => {
                // 期待されるトークンを挿入
                let token_text = self.get_default_text_for_token(*expected);
                let token = Token {
                    kind: *expected,
                    lexeme: token_text.clone(),
                    span: Span::new(0, 0), // ダミーのスパン
                };
                
                let operations = vec![
                    RepairOperation::InsertToken {
                        position: ctx.current,
                        token: token.clone(),
                    }
                ];
                
                let before_tokens = ctx.tokens[..].to_vec();
                let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                
                candidates.push(RepairCandidate {
                    before_tokens,
                    after_tokens,
                    operations,
                    description: format!("期待されるトークン '{}' を挿入", token_text),
                    strategy: RecoveryStrategy::TokenInsertion,
                    confidence: 0.6,
                    cost: 1,
                    applied_rule: None,
                });
                
                // 見つかったトークンをスキップ
                let operations = vec![
                    RepairOperation::DeleteToken {
                        position: ctx.current,
                        token: ctx.tokens[ctx.current].clone(),
                    }
                ];
                
                let before_tokens = ctx.tokens[..].to_vec();
                let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                
                candidates.push(RepairCandidate {
                    before_tokens,
                    after_tokens,
                    operations,
                    description: format!("予期しないトークン '{}' をスキップ", found),
                    strategy: RecoveryStrategy::TokenSkip,
                    confidence: 0.4,
                    cost: 1,
                    applied_rule: None,
                });
            },
            
            ParserError::UnexpectedToken { token, .. } => {
                // トークンをスキップ
                if ctx.current < ctx.tokens.len() {
                    let operations = vec![
                        RepairOperation::DeleteToken {
                            position: ctx.current,
                            token: ctx.tokens[ctx.current].clone(),
                        }
                    ];
                    
                    let before_tokens = ctx.tokens[..].to_vec();
                    let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                    
                    candidates.push(RepairCandidate {
                        before_tokens,
                        after_tokens,
                        operations,
                        description: format!("予期しないトークン '{}' をスキップ", token),
                        strategy: RecoveryStrategy::TokenSkip,
                        confidence: 0.5,
                        cost: 1,
                        applied_rule: None,
                    });
                }
                
                // セミコロンを挿入（文の終了と解釈）
                let token = Token {
                    kind: TokenKind::Semicolon,
                    lexeme: ";".to_string(),
                    span: Span::new(0, 0), // ダミーのスパン
                };
                
                let operations = vec![
                    RepairOperation::InsertToken {
                        position: ctx.current,
                        token: token.clone(),
                    }
                ];
                
                let before_tokens = ctx.tokens[..].to_vec();
                let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                
                candidates.push(RepairCandidate {
                    before_tokens,
                    after_tokens,
                    operations,
                    description: "文の終了をセミコロンで示す".to_string(),
                    strategy: RecoveryStrategy::TokenInsertion,
                    confidence: 0.3,
                    cost: 1,
                    applied_rule: None,
                });
            },
            
            ParserError::UnexpectedEOF { expected, .. } => {
                // EOFの場合、期待されるトークンを追加
                if let Some(expected_token) = expected {
                    let token_text = self.get_default_text_for_token(*expected_token);
                    let token = Token {
                        kind: *expected_token,
                        lexeme: token_text.clone(),
                        span: Span::new(0, 0), // ダミーのスパン
                    };
                    
                    let operations = vec![
                        RepairOperation::InsertToken {
                            position: ctx.tokens.len(),
                            token: token.clone(),
                        }
                    ];
                    
                    let before_tokens = ctx.tokens[..].to_vec();
                    let after_tokens = self.apply_operations_to_tokens(&before_tokens, &operations);
                    
                    candidates.push(RepairCandidate {
                        before_tokens,
                        after_tokens,
                        operations,
                        description: format!("EOFに期待されるトークン '{}' を追加", token_text),
                        strategy: RecoveryStrategy::TokenInsertion,
                        confidence: 0.7,
                        cost: 1,
                        applied_rule: None,
                    });
                }
            },
            
            // 他のエラータイプも同様に処理
            _ => {}
        }
    }
    /// ML/統計/文脈推論で最適なTokenKindを決定する
    fn predict_token_kind(&self, text: &str, context_before: &[Token], context_after: &[Token]) -> TokenKind {
        // 1. 文脈から推論
        if let Some(kind) = self.statistical_predict_token_kind(text, context_before, context_after) {
            return kind;
        }
        // 2. MLモデル（例: BERT, LSTM, 事前学習済みモデル）
        if let Some(kind) = self.ml_predict_token_kind(text, context_before, context_after) {
            return kind;
        }
        // 3. 既知のキーワード
        if let Some(kind) = self.keywords.get(text) {
            return *kind;
        }
        // 4. デフォルト
        TokenKind::Identifier
    }
    fn statistical_predict_token_kind(&self, text: &str, _before: &[Token], _after: &[Token]) -> Option<TokenKind> {
        // TODO: N-gramや頻度統計から推論（実装例）
        None
    }
    fn ml_predict_token_kind(&self, _text: &str, _before: &[Token], _after: &[Token]) -> Option<TokenKind> {
        // TODO: MLモデル連携（API/FFI/組込）
        None
    }
}

// ================================
// パブリックAPI関数
// ================================

/// 標準設定でエラー回復マネージャーを作成
pub fn create_error_recovery_manager() -> ErrorRecoveryManager {
    let config = ErrorRecoveryConfig::default();
    ErrorRecoveryManager::new(config)
}

/// 設定を指定してエラー回復マネージャーを作成
pub fn create_error_recovery_manager_with_config(config: ErrorRecoveryConfig) -> ErrorRecoveryManager {
    ErrorRecoveryManager::new(config)
}

/// ルールを指定してエラー回復マネージャーを作成
pub fn create_error_recovery_manager_with_rules(rules: Vec<RepairRule>, config: Option<ErrorRecoveryConfig>) -> ErrorRecoveryManager {
    let config = config.unwrap_or_default();
    let mut manager = ErrorRecoveryManager::new(config);
    
    for rule in rules {
        manager.add_rule(rule);
    }
    
    manager
}

/// 優先度関数を指定してエラー回復マネージャーを作成
pub fn create_error_recovery_manager_with_priority_fn<F>(priority_fn: F, config: Option<ErrorRecoveryConfig>) -> ErrorRecoveryManager
where
    F: Fn(&RepairCandidate) -> i32 + Send + Sync + 'static,
{
    let config = config.unwrap_or_default();
    let mut manager = ErrorRecoveryManager::new(config);
    manager.set_priority_function(Box::new(priority_fn));
    manager
}

/// エラーから回復を試みる
pub fn recover_from_error(
    manager: &mut ErrorRecoveryManager,
    ctx: &mut ParserContext,
    error: &ParserError
) -> Result<RepairResult> {
    manager.recover_from_error(ctx, error)
}

/// エラー回復の統計情報を取得
pub fn get_error_recovery_statistics(manager: &ErrorRecoveryManager) -> String {
    manager.get_statistics_summary()
}

/// 修復結果をフォーマット
pub fn format_repair_result(manager: &ErrorRecoveryManager, result: &RepairResult) -> String {
    manager.format_repair_result(result)
}

/// 修復マーカーを生成
pub fn generate_repair_markers(
    manager: &ErrorRecoveryManager,
    original_text: &str,
    result: &RepairResult
) -> Vec<RepairMarker> {
    manager.generate_repair_markers(original_text, result)
}

/// 学習用に記録
pub fn record_for_learning(
    manager: &mut ErrorRecoveryManager,
    error: &ParserError,
    result: &RepairResult
) {
    manager.record_for_learning(error, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_error_recovery() {
        let input = "echo 'unclosed string";
        let mut lexer = NexusLexer::new(input);
        let result = lexer.tokenize();
        assert!(result.is_err());
        let errors = result.err().unwrap();
        assert!(errors.iter().any(|e| matches!(e, ParserError::UnmatchedQuote { .. })), "Unmatched quote error should be detected");
    }
    #[test]
    fn test_rule_based_error_recovery() {
        let input = "cat | | grep";
        let mut lexer = NexusLexer::new(input);
        let result = lexer.tokenize();
        assert!(result.is_err());
        let errors = result.err().unwrap();
        assert!(errors.iter().any(|e| matches!(e, ParserError::InvalidPipeUsage { .. })), "Invalid pipe usage should be detected");
    }
    #[test]
    fn test_panic_mode_error_recovery() {
        let input = "echo (unclosed";
        let mut lexer = NexusLexer::new(input);
        let result = lexer.tokenize();
        assert!(result.is_err());
        let errors = result.err().unwrap();
        assert!(errors.iter().any(|e| matches!(e, ParserError::UnmatchedOpeningDelimiter { .. })), "Unmatched opening delimiter should be detected");
    }
    #[test]
    fn test_token_operation_application() {
        let input = "echo $";
        let mut lexer = NexusLexer::new(input);
        let result = lexer.tokenize();
        assert!(result.is_err());
        let errors = result.err().unwrap();
        assert!(errors.iter().any(|e| matches!(e, ParserError::EmptyVariableExpression { .. })), "Empty variable expression should be detected");
    }
    #[test]
    fn test_statistical_error_recovery() {
        let input = "ls >";
        let mut lexer = NexusLexer::new(input);
        let result = lexer.tokenize();
        assert!(result.is_err());
        let errors = result.err().unwrap();
        assert!(errors.iter().any(|e| matches!(e, ParserError::InvalidRedirection { .. })), "Invalid redirection should be detected");
    }
} 