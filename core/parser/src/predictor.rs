// predictor.rs
// NexusShellのコマンド予測エンジン
// ユーザーの入力から次のコマンドや引数を予測する高度な機能を提供

use crate::completer::{CompletionContext, CompletionSuggestion, CompletionKind};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use log::{debug, trace, info, warn};

/// 予測モデルの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictionModelType {
    /// N-gram統計モデル
    NGram,
    /// マルコフモデル
    Markov,
    /// ニューラルネットワーク
    Neural,
    /// ルールベース
    RuleBased,
    /// ハイブリッド（複数モデルの組み合わせ）
    Hybrid,
}

/// 予測結果
#[derive(Debug, Clone)]
pub struct PredictionResult {
    /// 予測テキスト
    pub text: String,
    /// 信頼度スコア (0.0-1.0)
    pub confidence: f32,
    /// 予測の種類
    pub kind: PredictionKind,
    /// 予測の根拠
    pub reason: String,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl PredictionResult {
    /// 新しい予測結果を作成
    pub fn new(text: &str, confidence: f32, kind: PredictionKind) -> Self {
        Self {
            text: text.to_string(),
            confidence,
            kind,
            reason: String::new(),
            metadata: HashMap::new(),
        }
    }

    /// 根拠を設定
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = reason.to_string();
        self
    }

    /// メタデータを追加
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// 予測の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredictionKind {
    /// コマンド予測
    Command,
    /// 引数予測
    Argument,
    /// フラグ/オプション予測
    Option,
    /// 次のコマンド予測
    NextCommand,
    /// コマンド補完
    Completion,
    /// エラー修正提案
    ErrorCorrection,
    /// タイプミス修正
    TypoCorrection,
}

/// N-gramモデルの設定
#[derive(Debug, Clone)]
pub struct NGramConfig {
    /// N-gramの次数（2=bigram, 3=trigram, etc.）
    pub n: usize,
    /// スムージングパラメータ
    pub smoothing_factor: f32,
    /// 最小出現回数のしきい値
    pub min_occurrence: usize,
}

impl Default for NGramConfig {
    fn default() -> Self {
        Self {
            n: 3,
            smoothing_factor: 0.1,
            min_occurrence: 2,
        }
    }
}

/// N-gramトークン
type NGram = Vec<String>;

/// N-gramモデル
#[derive(Debug)]
pub struct NGramModel {
    /// モデル設定
    config: NGramConfig,
    /// N-gramカウント
    counts: HashMap<NGram, usize>,
    /// 次トークン確率分布
    distributions: HashMap<NGram, HashMap<String, f32>>,
    /// ボキャブラリ（全トークンセット）
    vocabulary: HashSet<String>,
    /// 総N-gram数
    total_ngrams: usize,
}

impl NGramModel {
    /// 新しいN-gramモデルを作成
    pub fn new(config: NGramConfig) -> Self {
        Self {
            config,
            counts: HashMap::new(),
            distributions: HashMap::new(),
            vocabulary: HashSet::new(),
            total_ngrams: 0,
        }
    }

    /// コマンド履歴からモデルを学習
    pub fn train(&mut self, command_history: &[String]) {
        // ボキャブラリと頻度カウントの構築
        for command in command_history {
            let tokens: Vec<String> = command
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            
            // ボキャブラリに追加
            for token in &tokens {
                self.vocabulary.insert(token.clone());
            }
            
            // N-gramをカウント
            if tokens.len() >= self.config.n {
                for i in 0..=tokens.len() - self.config.n {
                    let ngram: Vec<String> = tokens[i..i+self.config.n-1].to_vec();
                    let next_token = tokens[i+self.config.n-1].clone();
                    
                    // N-gramカウントを更新
                    *self.counts.entry(ngram.clone()).or_insert(0) += 1;
                    self.total_ngrams += 1;
                    
                    // 次トークン分布を更新
                    let dist = self.distributions.entry(ngram).or_insert_with(HashMap::new);
                    *dist.entry(next_token).or_insert(0.0) += 1.0;
                }
            }
        }
        
        // 確率分布の正規化とスムージング
        for (ngram, dist) in &mut self.distributions {
            let total: f32 = dist.values().sum();
            
            // スムージングを適用して確率を計算
            for (_, prob) in dist.iter_mut() {
                *prob = (*prob + self.config.smoothing_factor) / (total + self.config.smoothing_factor * self.vocabulary.len() as f32);
            }
        }
    }

    /// 与えられたコンテキストに基づいて次のトークンを予測
    pub fn predict(&self, context: &[String], max_predictions: usize) -> Vec<(String, f32)> {
        let mut predictions = Vec::new();
        let context_len = context.len();
        
        // コンテキストが短い場合は直前のトークンだけを使用
        let n = self.config.n - 1;
        let lookup_context: Vec<String> = if context_len >= n {
            context[context_len - n..].to_vec()
        } else {
            // コンテキストが不足している場合、一部だけを使用
            context.to_vec()
        };
        
        // 一致するN-gramから予測
        if let Some(dist) = self.distributions.get(&lookup_context) {
            // 確率の降順でソート
            let mut sorted: Vec<(String, f32)> = dist.iter()
                .map(|(token, prob)| (token.clone(), *prob))
                .collect();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            
            // 上位N個を取得
            predictions = sorted.into_iter()
                .take(max_predictions)
                .collect();
        }
        
        // バックオフ: 短いコンテキストで再試行
        if predictions.is_empty() && lookup_context.len() > 1 {
            let shorter_context = lookup_context[1..].to_vec();
            if let Some(dist) = self.distributions.get(&shorter_context) {
                let mut sorted: Vec<(String, f32)> = dist.iter()
                    .map(|(token, prob)| (token.clone(), *prob))
                    .collect();
                sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                
                predictions = sorted.into_iter()
                    .take(max_predictions)
                    .collect();
            }
        }
        
        predictions
    }
}

/// マルコフモデル状態
#[derive(Debug)]
pub struct MarkovModel {
    /// 遷移確率行列
    transitions: HashMap<String, HashMap<String, f32>>,
    /// 初期状態分布
    initial_states: HashMap<String, f32>,
    /// 状態数（総コマンド数）
    state_count: usize,
}

impl MarkovModel {
    /// 新しいマルコフモデルを作成
    pub fn new() -> Self {
        Self {
            transitions: HashMap::new(),
            initial_states: HashMap::new(),
            state_count: 0,
        }
    }

    /// コマンド履歴からモデルを学習
    pub fn train(&mut self, command_history: &[String]) {
        // 状態遷移をカウント
        let mut state_counts: HashMap<String, usize> = HashMap::new();
        let mut transition_counts: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut initial_counts: HashMap<String, usize> = HashMap::new();
        
        let mut prev_command: Option<String> = None;
        
        for command in command_history {
            let cmd = command.split_whitespace().next().unwrap_or("").to_string();
            if cmd.is_empty() {
                continue;
            }
            
            // 状態カウントを更新
            *state_counts.entry(cmd.clone()).or_insert(0) += 1;
            
            // 初期状態カウントを更新（セッション開始時）
            if prev_command.is_none() {
                *initial_counts.entry(cmd.clone()).or_insert(0) += 1;
            }
            
            // 遷移カウントを更新
            if let Some(prev) = prev_command {
                let transitions = transition_counts.entry(prev).or_insert_with(HashMap::new);
                *transitions.entry(cmd.clone()).or_insert(0) += 1;
            }
            
            prev_command = Some(cmd);
        }
        
        // 遷移確率を計算
        for (state, transitions) in &transition_counts {
            let state_total = state_counts.get(state).unwrap_or(&1);
            let mut state_transitions = HashMap::new();
            
            for (next_state, count) in transitions {
                let prob = *count as f32 / *state_total as f32;
                state_transitions.insert(next_state.clone(), prob);
            }
            
            self.transitions.insert(state.clone(), state_transitions);
        }
        
        // 初期状態確率を計算
        let total_sessions = initial_counts.values().sum::<usize>().max(1);
        for (state, count) in initial_counts {
            let prob = count as f32 / total_sessions as f32;
            self.initial_states.insert(state, prob);
        }
        
        self.state_count = state_counts.len();
    }

    /// 与えられた現在の状態から次の状態を予測
    pub fn predict(&self, current_state: &str, max_predictions: usize) -> Vec<(String, f32)> {
        let mut predictions = Vec::new();
        
        if let Some(transitions) = self.transitions.get(current_state) {
            // 確率の降順でソート
            let mut sorted: Vec<(String, f32)> = transitions.iter()
                .map(|(state, prob)| (state.clone(), *prob))
                .collect();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            
            // 上位N個を取得
            predictions = sorted.into_iter()
                .take(max_predictions)
                .collect();
        }
        
        // 予測がない場合は初期状態分布から予測
        if predictions.is_empty() {
            let mut sorted: Vec<(String, f32)> = self.initial_states.iter()
                .map(|(state, prob)| (state.clone(), *prob))
                .collect();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            
            predictions = sorted.into_iter()
                .take(max_predictions)
                .collect();
        }
        
        predictions
    }
}

/// 予測エンジン設定
#[derive(Debug, Clone)]
pub struct PredictorConfig {
    /// 使用するモデルの種類
    pub model_type: PredictionModelType,
    /// N-gramモデル設定
    pub ngram_config: NGramConfig,
    /// 最大予測数
    pub max_predictions: usize,
    /// 最小信頼度しきい値
    pub min_confidence: f32,
    /// コマンド履歴の最大サイズ
    pub max_history_size: usize,
    /// コンテキスト考慮のウィンドウサイズ
    pub context_window: usize,
}

impl Default for PredictorConfig {
    fn default() -> Self {
        Self {
            model_type: PredictionModelType::Hybrid,
            ngram_config: NGramConfig::default(),
            max_predictions: 5,
            min_confidence: 0.1,
            max_history_size: 1000,
            context_window: 10,
        }
    }
}

/// 予測エンジン
#[derive(Debug)]
pub struct Predictor {
    /// 設定
    config: PredictorConfig,
    /// N-gramモデル
    ngram_model: NGramModel,
    /// マルコフモデル
    markov_model: MarkovModel,
    /// コマンド履歴
    command_history: VecDeque<String>,
    /// コマンド頻度マップ
    command_frequency: HashMap<String, usize>,
    /// コマンド連鎖マップ (cmd1 -> cmd2 -> cmd3)
    command_chains: HashMap<Vec<String>, HashMap<String, usize>>,
    /// タイムスタンプ付きコマンド履歴
    timestamped_history: Vec<(String, Instant)>,
    /// 学習済みフラグ
    trained: bool,
}

impl Predictor {
    /// 新しい予測エンジンを作成
    pub fn new() -> Self {
        let config = PredictorConfig::default();
        Self::with_config(config)
    }

    /// 設定を指定して新しい予測エンジンを作成
    pub fn with_config(config: PredictorConfig) -> Self {
        Self {
            config: config.clone(),
            ngram_model: NGramModel::new(config.ngram_config),
            markov_model: MarkovModel::new(),
            command_history: VecDeque::with_capacity(config.max_history_size),
            command_frequency: HashMap::new(),
            command_chains: HashMap::new(),
            timestamped_history: Vec::new(),
            trained: false,
        }
    }

    /// コマンド履歴を追加
    pub fn add_command(&mut self, command: &str) {
        // 空のコマンドは無視
        if command.trim().is_empty() {
            return;
        }
        
        // 履歴に追加
        self.command_history.push_back(command.to_string());
        
        // 最大サイズを超えたら古いコマンドを削除
        if self.command_history.len() > self.config.max_history_size {
            self.command_history.pop_front();
        }
        
        // タイムスタンプ付き履歴に追加
        self.timestamped_history.push((command.to_string(), Instant::now()));
        
        // 頻度マップを更新
        let cmd = command.split_whitespace().next().unwrap_or("").to_string();
        if !cmd.is_empty() {
            *self.command_frequency.entry(cmd.clone()).or_insert(0) += 1;
        }
        
        // コマンド連鎖を更新
        let context_size = self.config.ngram_config.n - 1;
        if self.command_history.len() >= context_size {
            let context: Vec<String> = self.command_history
                .iter()
                .rev()
                .skip(1)
                .take(context_size)
                .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
                .rev()
                .collect();
            
            if !context.iter().any(|s| s.is_empty()) {
                let chain_map = self.command_chains.entry(context).or_insert_with(HashMap::new);
                *chain_map.entry(cmd).or_insert(0) += 1;
            }
        }
        
        // 再学習フラグをリセット
        self.trained = false;
    }

    /// モデルを学習
    pub fn train(&mut self) {
        let history: Vec<String> = self.command_history.iter().cloned().collect();
        
        // N-gramモデルを学習
        self.ngram_model = NGramModel::new(self.config.ngram_config.clone());
        self.ngram_model.train(&history);
        
        // マルコフモデルを学習
        self.markov_model = MarkovModel::new();
        self.markov_model.train(&history);
        
        self.trained = true;
        info!("予測モデルを学習しました: {}件のコマンド履歴", history.len());
    }

    /// 予測を生成
    pub fn predict(&mut self, context: &CompletionContext) -> Vec<PredictionResult> {
        // 必要に応じて学習
        if !self.trained && !self.command_history.is_empty() {
            self.train();
        }
        
        let input = &context.input[..context.cursor_position.min(context.input.len())];
        let words: Vec<&str> = input.trim().split_whitespace().collect();
        
        // 異なる予測タスクを実行
        let mut results = Vec::new();
        
        // 入力が空の場合、次のコマンドを予測
        if words.is_empty() {
            results.extend(self.predict_next_command(context));
        }
        // コマンド名だけが入力されている場合、引数を予測
        else if words.len() == 1 {
            results.extend(self.predict_arguments(context, words[0]));
        }
        // コマンドと引数が入力されている場合、追加の引数やオプションを予測
        else {
            let command = words[0];
            results.extend(self.predict_options(context, command));
        }
        
        // 予測結果をフィルタリングして返す
        results.into_iter()
            .filter(|r| r.confidence >= self.config.min_confidence)
            .take(self.config.max_predictions)
            .collect()
    }

    /// 次のコマンドを予測
    fn predict_next_command(&self, context: &CompletionContext) -> Vec<PredictionResult> {
        let mut results = Vec::new();
        
        // 最近使ったコマンドから予測
        if !self.timestamped_history.is_empty() {
            let recent_commands: Vec<&String> = self.timestamped_history.iter()
                .rev()
                .take(10)
                .map(|(cmd, _)| cmd)
                .collect();
            
            // 最後に使ったコマンドから予測
            if let Some(last_cmd) = recent_commands.first() {
                let cmd = last_cmd.split_whitespace().next().unwrap_or("");
                
                // マルコフモデルによる予測
                let predictions = self.markov_model.predict(cmd, 3);
                for (cmd, prob) in predictions {
                    results.push(
                        PredictionResult::new(&cmd, prob, PredictionKind::NextCommand)
                            .with_reason("最近のコマンド連鎖からの予測")
                    );
                }
            }
        }
        
        // 最頻使用コマンドから予測
        if results.len() < self.config.max_predictions {
            let mut frequent_cmds: Vec<(String, usize)> = self.command_frequency.iter()
                .map(|(cmd, count)| (cmd.clone(), *count))
                .collect();
            frequent_cmds.sort_by(|a, b| b.1.cmp(&a.1));
            
            for (cmd, count) in frequent_cmds.iter().take(3) {
                let total_cmds = self.command_frequency.values().sum::<usize>().max(1);
                let confidence = *count as f32 / total_cmds as f32;
                
                if !results.iter().any(|r| r.text == *cmd) {
                    results.push(
                        PredictionResult::new(cmd, confidence, PredictionKind::NextCommand)
                            .with_reason("頻繁に使用されるコマンド")
                    );
                }
            }
        }
        
        // 時間帯に基づく予測
        let hour = chrono::Local::now().hour();
        let time_context = if hour < 12 { "morning" } 
                          else if hour < 18 { "afternoon" } 
                          else { "evening" };
        
        let mut time_based_commands = HashMap::new();
        for (cmd, timestamp) in &self.timestamped_history {
            let cmd_hour = chrono::Local::now().hour();
            let cmd_time_context = if cmd_hour < 12 { "morning" } 
                                  else if cmd_hour < 18 { "afternoon" } 
                                  else { "evening" };
            
            if cmd_time_context == time_context {
                let command = cmd.split_whitespace().next().unwrap_or("").to_string();
                if !command.is_empty() {
                    *time_based_commands.entry(command).or_insert(0) += 1;
                }
            }
        }
        
        let mut time_predictions: Vec<(String, usize)> = time_based_commands.into_iter().collect();
        time_predictions.sort_by(|a, b| b.1.cmp(&a.1));
        
        for (cmd, count) in time_predictions.iter().take(2) {
            let confidence = (*count as f32 * 0.8).min(0.9);
            if !results.iter().any(|r| r.text == *cmd) {
                results.push(
                    PredictionResult::new(cmd, confidence, PredictionKind::NextCommand)
                        .with_reason(format!("この時間帯 ({}) によく使われるコマンド", time_context))
                );
            }
        }
        
        results
    }

    /// コマンドの引数を予測
    fn predict_arguments(&self, context: &CompletionContext, command: &str) -> Vec<PredictionResult> {
        let mut results = Vec::new();
        
        // コマンド固有の引数予測
        match command {
            "cd" => {
                // 頻繁に移動するディレクトリを予測
                let cd_history: Vec<String> = self.command_history.iter()
                    .filter(|cmd| cmd.starts_with("cd "))
                    .map(|cmd| cmd[3..].trim().to_string())
                    .collect();
                
                let mut dir_counts = HashMap::new();
                for dir in cd_history {
                    if !dir.is_empty() {
                        *dir_counts.entry(dir).or_insert(0) += 1;
                    }
                }
                
                let mut sorted_dirs: Vec<(String, usize)> = dir_counts.into_iter().collect();
                sorted_dirs.sort_by(|a, b| b.1.cmp(&a.1));
                
                for (dir, count) in sorted_dirs.iter().take(3) {
                    let total = cd_history.len().max(1);
                    let confidence = *count as f32 / total as f32;
                    results.push(
                        PredictionResult::new(&format!("cd {}", dir), confidence, PredictionKind::Argument)
                            .with_reason("頻繁に移動するディレクトリ")
                    );
                }
            },
            "git" => {
                // よく使うgitコマンドを予測
                let git_subcommands = [
                    ("status", "リポジトリの状態を確認"),
                    ("pull", "リモートからの変更を取得"),
                    ("push", "ローカルの変更をリモートに送信"),
                    ("add .", "すべての変更をステージング"),
                    ("commit -m \"\"", "変更をコミット"),
                ];
                
                for (i, (subcmd, reason)) in git_subcommands.iter().enumerate() {
                    let confidence = 0.9 - (i as f32 * 0.1);
                    results.push(
                        PredictionResult::new(&format!("git {}", subcmd), confidence, PredictionKind::Argument)
                            .with_reason(reason)
                    );
                }
            },
            _ => {
                // 一般的なコマンドの引数を予測
                let cmd_history: Vec<String> = self.command_history.iter()
                    .filter(|cmd| cmd.starts_with(&format!("{} ", command)))
                    .cloned()
                    .collect();
                
                if !cmd_history.is_empty() {
                    // N-gramモデルを使用して引数を予測
                    let context_tokens: Vec<String> = vec![command.to_string()];
                    let predictions = self.ngram_model.predict(&context_tokens, 3);
                    
                    for (arg, prob) in predictions {
                        results.push(
                            PredictionResult::new(&format!("{} {}", command, arg), prob, PredictionKind::Argument)
                                .with_reason("過去の使用パターンからの予測")
                        );
                    }
                }
            }
        }
        
        results
    }

    /// コマンドのオプションを予測
    fn predict_options(&self, context: &CompletionContext, command: &str) -> Vec<PredictionResult> {
        let mut results = Vec::new();
        
        // 入力テキストを取得
        let input = &context.input[..context.cursor_position.min(context.input.len())];
        
        // コマンドに基づいてよく使われるオプションを予測
        let option_map: HashMap<&str, Vec<(&str, &str)>> = [
            ("ls", vec![
                ("-la", "すべてのファイルを詳細表示"),
                ("-lh", "読みやすいサイズ表示"),
                ("--color=auto", "色付き表示"),
            ]),
            ("grep", vec![
                ("-i", "大文字小文字を区別しない"),
                ("-r", "再帰的に検索"),
                ("--color=auto", "一致部分を色付け"),
            ]),
            ("docker", vec![
                ("ps", "実行中のコンテナを表示"),
                ("images", "イメージを表示"),
                ("build -t", "イメージをビルド"),
            ]),
        ].iter().cloned().collect();
        
        if let Some(options) = option_map.get(command) {
            for (i, (opt, reason)) in options.iter().enumerate() {
                // 既に入力されているオプションは除外
                if !input.contains(opt) {
                    let confidence = 0.8 - (i as f32 * 0.1);
                    results.push(
                        PredictionResult::new(&format!("{} {}", command, opt), confidence, PredictionKind::Option)
                            .with_reason(reason)
                    );
                }
            }
        }
        
        // 一般的なオプションパターンを予測
        if results.is_empty() {
            let cmd_history: Vec<String> = self.command_history.iter()
                .filter(|cmd| cmd.starts_with(&format!("{} ", command)))
                .cloned()
                .collect();
            
            if !cmd_history.is_empty() {
                // すでに入力された単語を取得
                let words: Vec<&str> = input.trim().split_whitespace().collect();
                
                // 履歴からオプションパターンを抽出
                let mut option_patterns = HashMap::new();
                for cmd in cmd_history {
                    let cmd_words: Vec<&str> = cmd.split_whitespace().collect();
                    if cmd_words.len() > words.len() {
                        let next_word = cmd_words[words.len()];
                        if next_word.starts_with("-") {
                            *option_patterns.entry(next_word.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                
                let mut sorted_options: Vec<(String, usize)> = option_patterns.into_iter().collect();
                sorted_options.sort_by(|a, b| b.1.cmp(&a.1));
                
                for (opt, count) in sorted_options.iter().take(3) {
                    let total = cmd_history.len().max(1);
                    let confidence = *count as f32 / total as f32;
                    let predicted_cmd = format!("{} {}", input.trim(), opt);
                    results.push(
                        PredictionResult::new(&predicted_cmd, confidence, PredictionKind::Option)
                            .with_reason("過去のコマンドで使われたオプション")
                    );
                }
            }
        }
        
        results
    }

    /// タイプミスを修正
    pub fn correct_typo(&self, input: &str) -> Option<PredictionResult> {
        // 単語を分割
        let words: Vec<&str> = input.trim().split_whitespace().collect();
        if words.is_empty() {
            return None;
        }
        
        // 最初の単語（コマンド）のみをチェック
        let command = words[0];
        
        // コマンド履歴から出現したコマンドのリストを構築
        let mut command_set: HashSet<String> = self.command_history.iter()
            .filter_map(|cmd| cmd.split_whitespace().next().map(|s| s.to_string()))
            .collect();
        
        // 組み込みコマンドとよく使われるコマンドを追加
        for cmd in ["ls", "cd", "grep", "cat", "echo", "find", "git", "docker", "ssh", "cp", "mv"].iter() {
            command_set.insert(cmd.to_string());
        }
        
        // レーベンシュタイン距離でタイプミスをチェック
        let mut closest_match = None;
        let mut min_distance = 3; // 最大許容距離
        
        for cmd in &command_set {
            let distance = levenshtein_distance(command, cmd);
            if distance > 0 && distance < min_distance {
                min_distance = distance;
                closest_match = Some(cmd.clone());
            }
        }
        
        // タイプミス修正候補を返す
        if let Some(corrected) = closest_match {
            let confidence = 1.0 - (min_distance as f32 * 0.2);
            let corrected_input = input.replacen(command, &corrected, 1);
            
            Some(PredictionResult::new(
                &corrected_input,
                confidence,
                PredictionKind::TypoCorrection
            ).with_reason(&format!("「{}」の代わりに「{}」", command, corrected)))
        } else {
            None
        }
    }
}

/// レーベンシュタイン距離を計算
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();
    
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
    
    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    
    for j in 0..=len2 {
        matrix[0][j] = j;
    }
    
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    
    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
            
            matrix[i][j] = std::cmp::min(
                std::cmp::min(
                    matrix[i - 1][j] + 1,      // 削除
                    matrix[i][j - 1] + 1       // 挿入
                ),
                matrix[i - 1][j - 1] + cost    // 置換
            );
        }
    }
    
    matrix[len1][len2]
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ngram_model() {
        let config = NGramConfig {
            n: 3,
            smoothing_factor: 0.1,
            min_occurrence: 1,
        };
        
        let mut model = NGramModel::new(config);
        
        let history = vec![
            "git status".to_string(),
            "git add .".to_string(),
            "git commit -m 'test'".to_string(),
            "git push".to_string(),
            "git status".to_string(),
            "git pull".to_string(),
        ];
        
        model.train(&history);
        
        let context = vec!["git".to_string(), "status".to_string()];
        let predictions = model.predict(&context, 3);
        
        assert!(!predictions.is_empty());
    }
    
    #[test]
    fn test_markov_model() {
        let mut model = MarkovModel::new();
        
        let history = vec![
            "git status".to_string(),
            "git add .".to_string(),
            "git commit -m 'test'".to_string(),
            "git push".to_string(),
            "ls -la".to_string(),
            "cd /tmp".to_string(),
        ];
        
        model.train(&history);
        
        let predictions = model.predict("git", 3);
        
        assert!(!predictions.is_empty());
    }
    
    #[test]
    fn test_predictor() {
        let mut predictor = Predictor::new();
        
        predictor.add_command("git status");
        predictor.add_command("git add .");
        predictor.add_command("git commit -m 'test'");
        predictor.add_command("git push");
        predictor.add_command("ls -la");
        predictor.add_command("cd /tmp");
        
        let context = CompletionContext::new("git ", 4);
        let predictions = predictor.predict(&context);
        
        assert!(!predictions.is_empty());
    }
    
    #[test]
    fn test_typo_correction() {
        let mut predictor = Predictor::new();
        
        predictor.add_command("git status");
        predictor.add_command("ls -la");
        predictor.add_command("cd /tmp");
        
        let correction = predictor.correct_typo("gti status");
        
        assert!(correction.is_some());
        assert_eq!(correction.unwrap().text, "git status");
    }
} 