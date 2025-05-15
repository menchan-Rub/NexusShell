// metrics.rs
// NexusShellのパーサーメトリクス収集
// パフォーマンス測定と品質モニタリング機能を提供

use crate::{
    AstNode, Token, TokenKind, ParserError, Span,
    parser::ParserStats,
    lexer::NexusLexer
};

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock, Mutex};
use std::time::{Duration, Instant};
use std::fmt;

/// メトリクスの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricType {
    /// カウンター型（増加のみ）
    Counter,
    /// ゲージ型（増減可能）
    Gauge,
    /// ヒストグラム型（分布）
    Histogram,
    /// サマリー型（パーセンタイル）
    Summary,
}

/// メトリクス値型
#[derive(Debug, Clone)]
pub enum MetricValue {
    /// 整数値
    Integer(i64),
    /// 浮動小数点値
    Float(f64),
    /// 真偽値
    Boolean(bool),
    /// 文字列値
    String(String),
    /// ヒストグラムデータ
    Histogram(Vec<f64>),
    /// サマリーデータ
    Summary {
        count: usize,
        sum: f64,
        min: f64,
        max: f64,
        mean: f64,
        p50: f64,
        p90: f64,
        p95: f64,
        p99: f64,
    },
}

impl fmt::Display for MetricValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricValue::Integer(i) => write!(f, "{}", i),
            MetricValue::Float(fl) => write!(f, "{:.4}", fl),
            MetricValue::Boolean(b) => write!(f, "{}", b),
            MetricValue::String(s) => write!(f, "{}", s),
            MetricValue::Histogram(_) => write!(f, "[Histogram]"),
            MetricValue::Summary { count, sum, mean, p50, p90, p95, p99, .. } => {
                write!(f, "count={}, sum={:.2}, mean={:.2}, p50={:.2}, p90={:.2}, p95={:.2}, p99={:.2}",
                    count, sum, mean, p50, p90, p95, p99)
            }
        }
    }
}

/// メトリクスデータ
#[derive(Debug, Clone)]
pub struct Metric {
    /// メトリクス名
    pub name: String,
    /// メトリクス種類
    pub metric_type: MetricType,
    /// メトリクス値
    pub value: MetricValue,
    /// ラベル（タグ）
    pub labels: HashMap<String, String>,
    /// 説明
    pub description: String,
    /// 単位
    pub unit: Option<String>,
    /// タイムスタンプ（ミリ秒）
    pub timestamp: u64,
}

impl Metric {
    /// 新しいメトリクスを作成
    pub fn new(
        name: &str,
        metric_type: MetricType,
        value: MetricValue,
        description: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            metric_type,
            value,
            labels: HashMap::new(),
            description: description.to_string(),
            unit: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_millis() as u64,
        }
    }

    /// ラベルを追加
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }

    /// 単位を設定
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }

    /// メトリクスのフルネーム（ラベル付き）を取得
    pub fn full_name(&self) -> String {
        if self.labels.is_empty() {
            self.name.clone()
        } else {
            let labels = self.labels.iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, v))
                .collect::<Vec<_>>()
                .join(",");
            
            format!("{}{{{}}}",  self.name, labels)
        }
    }
}

/// パーサーメトリクスコレクター
pub struct MetricsCollector {
    /// メトリクスストレージ
    metrics: Arc<RwLock<HashMap<String, Metric>>>,
    /// 履歴メトリクス（時系列）
    historical_metrics: Arc<RwLock<HashMap<String, VecDeque<Metric>>>>,
    /// メトリクス収集開始時刻
    start_time: Instant,
    /// 最終収集時刻
    last_collection: Arc<RwLock<Instant>>,
    /// コレクション間隔（ミリ秒）
    collection_interval: u64,
    /// メトリクス履歴の最大サイズ
    max_history_size: usize,
    /// 現在の計測セッションID
    session_id: String,
}

impl MetricsCollector {
    /// 新しいメトリクスコレクターを作成
    pub fn new() -> Self {
        let session_id = format!("session-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
        
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            historical_metrics: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            last_collection: Arc::new(RwLock::new(Instant::now())),
            collection_interval: 1000, // 1秒
            max_history_size: 100,
            session_id,
        }
    }

    /// カウンターメトリクスを記録
    pub fn record_counter(&self, name: &str, value: i64, description: &str) {
        self.record_metric(Metric::new(
            name,
            MetricType::Counter,
            MetricValue::Integer(value),
            description,
        ));
    }

    /// カウンターメトリクスをインクリメント
    pub fn increment_counter(&self, name: &str, description: &str) {
        let mut metrics = self.metrics.write().unwrap();
        
        if let Some(metric) = metrics.get_mut(name) {
            // 既存のカウンターをインクリメント
            if let MetricValue::Integer(ref mut value) = metric.value {
                *value += 1;
                metric.timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_else(|_| Duration::from_secs(0))
                    .as_millis() as u64;
            }
        } else {
            // 新規カウンターを作成
            metrics.insert(
                name.to_string(),
                Metric::new(
                    name,
                    MetricType::Counter,
                    MetricValue::Integer(1),
                    description,
                ),
            );
        }
    }

    /// ゲージメトリクスを記録
    pub fn record_gauge(&self, name: &str, value: f64, description: &str) {
        self.record_metric(Metric::new(
            name,
            MetricType::Gauge,
            MetricValue::Float(value),
            description,
        ));
    }

    /// ヒストグラムメトリクスを記録
    pub fn record_histogram(&self, name: &str, value: f64, description: &str) {
        let mut metrics = self.metrics.write().unwrap();
        
        if let Some(metric) = metrics.get_mut(name) {
            // 既存のヒストグラムに値を追加
            if let MetricValue::Histogram(ref mut values) = metric.value {
                values.push(value);
                metric.timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_else(|_| Duration::from_secs(0))
                    .as_millis() as u64;
            }
        } else {
            // 新規ヒストグラムを作成
            metrics.insert(
                name.to_string(),
                Metric::new(
                    name,
                    MetricType::Histogram,
                    MetricValue::Histogram(vec![value]),
                    description,
                ),
            );
        }
    }

    /// サマリーメトリクスを記録
    pub fn record_summary(&self, name: &str, value: f64, description: &str) {
        let mut metrics = self.metrics.write().unwrap();
        
        if let Some(metric) = metrics.get_mut(name) {
            // 既存のサマリーを更新
            if let MetricValue::Summary { ref mut count, ref mut sum, ref mut min, ref mut max, ref mut mean, .. } = metric.value {
                *count += 1;
                *sum += value;
                *min = (*min).min(value);
                *max = (*max).max(value);
                *mean = *sum / *count as f64;
                
                // パーセンタイルは完全ではないが、ここでは簡略化
                metric.timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_else(|_| Duration::from_secs(0))
                    .as_millis() as u64;
            }
        } else {
            // 新規サマリーを作成
            metrics.insert(
                name.to_string(),
                Metric::new(
                    name,
                    MetricType::Summary,
                    MetricValue::Summary {
                        count: 1,
                        sum: value,
                        min: value,
                        max: value,
                        mean: value,
                        p50: value,
                        p90: value,
                        p95: value,
                        p99: value,
                    },
                    description,
                ),
            );
        }
    }

    /// 一般的なメトリクスを記録
    pub fn record_metric(&self, metric: Metric) {
        let mut metrics = self.metrics.write().unwrap();
        let name = metric.name.clone();
        
        // メトリクスを保存
        metrics.insert(name.clone(), metric.clone());
        
        // 履歴メトリクスも更新
        let mut historical = self.historical_metrics.write().unwrap();
        let history = historical.entry(name).or_insert_with(|| VecDeque::with_capacity(self.max_history_size));
        
        history.push_back(metric);
        
        // 履歴サイズを制限
        if history.len() > self.max_history_size {
            history.pop_front();
        }
        
        // 最終収集時刻を更新
        let mut last_collection = self.last_collection.write().unwrap();
        *last_collection = Instant::now();
    }

    /// メトリクスを取得
    pub fn get_metric(&self, name: &str) -> Option<Metric> {
        let metrics = self.metrics.read().unwrap();
        metrics.get(name).cloned()
    }

    /// 全メトリクスを取得
    pub fn get_all_metrics(&self) -> Vec<Metric> {
        let metrics = self.metrics.read().unwrap();
        metrics.values().cloned().collect()
    }

    /// 履歴メトリクスを取得
    pub fn get_historical_metrics(&self, name: &str) -> Option<Vec<Metric>> {
        let historical = self.historical_metrics.read().unwrap();
        historical.get(name).map(|history| history.iter().cloned().collect())
    }

    /// メトリクスを文字列形式で取得
    pub fn get_metrics_string(&self) -> String {
        let metrics = self.metrics.read().unwrap();
        let mut result = String::new();
        
        for (name, metric) in metrics.iter() {
            let value_str = format!("{}", metric.value);
            let labels_str = if metric.labels.is_empty() {
                String::new()
            } else {
                let labels = metric.labels.iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect::<Vec<_>>()
                    .join(",");
                
                format!("{{{}}}", labels)
            };
            
            result.push_str(&format!("# HELP {} {}\n", name, metric.description));
            if let Some(unit) = &metric.unit {
                result.push_str(&format!("# UNIT {} {}\n", name, unit));
            }
            result.push_str(&format!("# TYPE {} {:?}\n", name, metric.metric_type));
            result.push_str(&format!("{}{} {}\n", name, labels_str, value_str));
        }
        
        result
    }

    /// パーサー統計情報からメトリクスを記録
    pub fn record_parser_stats(&self, stats: &ParserStats) {
        self.record_gauge(
            "parser_elapsed_ms",
            stats.elapsed_ms,
            "パーサーの処理時間（ミリ秒）",
        ).with_unit("ms");
        
        self.record_counter(
            "parser_token_count",
            stats.token_count as i64,
            "処理したトークン数",
        );
        
        self.record_counter(
            "parser_node_count",
            stats.node_count as i64,
            "生成したASTノード数",
        );
        
        self.record_counter(
            "parser_error_count",
            stats.error_count as i64,
            "発生したエラー数",
        );
        
        self.record_gauge(
            "parser_max_recursion_depth",
            stats.max_recursion_depth as f64,
            "最大再帰深度",
        );
        
        self.record_counter(
            "parser_backtrack_count",
            stats.backtrack_count as i64,
            "バックトラック回数",
        );
    }

    /// パーサーパフォーマンスレポートを生成
    pub fn generate_performance_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("# NexusShell Parser Performance Report\n"));
        report.push_str(&format!("Session ID: {}\n", self.session_id));
        report.push_str(&format!("Timestamp: {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        report.push_str(&format!("Uptime: {:.2} seconds\n\n", self.start_time.elapsed().as_secs_f64()));
        
        // 基本メトリクス
        report.push_str("## Basic Metrics\n\n");
        
        if let Some(elapsed) = self.get_metric("parser_elapsed_ms") {
            report.push_str(&format!("- Parse Time: {:.2} ms\n", 
                if let MetricValue::Float(val) = elapsed.value { val } else { 0.0 }));
        }
        
        if let Some(tokens) = self.get_metric("parser_token_count") {
            report.push_str(&format!("- Token Count: {}\n", 
                if let MetricValue::Integer(val) = tokens.value { val } else { 0 }));
        }
        
        if let Some(nodes) = self.get_metric("parser_node_count") {
            report.push_str(&format!("- AST Node Count: {}\n", 
                if let MetricValue::Integer(val) = nodes.value { val } else { 0 }));
        }
        
        if let Some(errors) = self.get_metric("parser_error_count") {
            report.push_str(&format!("- Error Count: {}\n", 
                if let MetricValue::Integer(val) = errors.value { val } else { 0 }));
        }
        
        // パフォーマンス分析
        report.push_str("\n## Performance Analysis\n\n");
        
        if let (Some(elapsed), Some(tokens)) = (
            self.get_metric("parser_elapsed_ms"),
            self.get_metric("parser_token_count")
        ) {
            let elapsed_val = if let MetricValue::Float(val) = elapsed.value { val } else { 0.0 };
            let tokens_val = if let MetricValue::Integer(val) = tokens.value { val as f64 } else { 0.0 };
            
            if elapsed_val > 0.0 && tokens_val > 0.0 {
                let tokens_per_ms = tokens_val / elapsed_val;
                report.push_str(&format!("- Tokens/ms: {:.2}\n", tokens_per_ms));
            }
        }
        
        if let Some(backtrack) = self.get_metric("parser_backtrack_count") {
            report.push_str(&format!("- Backtrack Count: {}\n", 
                if let MetricValue::Integer(val) = backtrack.value { val } else { 0 }));
        }
        
        // 履歴データ
        if let Some(history) = self.get_historical_metrics("parser_elapsed_ms") {
            if !history.is_empty() {
                report.push_str("\n## Historical Performance\n\n");
                
                // 最近の5回の処理時間を出力
                let recent = history.iter().rev().take(5).collect::<Vec<_>>();
                for (i, metric) in recent.iter().enumerate() {
                    if let MetricValue::Float(val) = metric.value {
                        report.push_str(&format!("- Run {}: {:.2} ms\n", history.len() - i, val));
                    }
                }
                
                // 平均値を計算
                let avg = history.iter()
                    .filter_map(|m| if let MetricValue::Float(val) = m.value { Some(val) } else { None })
                    .sum::<f64>() / history.len() as f64;
                
                report.push_str(&format!("\n- Average Parse Time: {:.2} ms\n", avg));
            }
        }
        
        report
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// パーサーパフォーマンスプロファイラー
pub struct ParserProfiler {
    /// タイマーマップ（名前 -> 開始時刻）
    timers: HashMap<String, Instant>,
    /// タイマー結果（名前 -> 累積時間）
    results: HashMap<String, Duration>,
    /// 呼び出し回数カウンター
    calls: HashMap<String, usize>,
    /// プロファイル中フラグ
    is_profiling: bool,
    /// メトリクスコレクター
    metrics: Option<Arc<MetricsCollector>>,
}

impl ParserProfiler {
    /// 新しいプロファイラーを作成
    pub fn new() -> Self {
        Self {
            timers: HashMap::new(),
            results: HashMap::new(),
            calls: HashMap::new(),
            is_profiling: false,
            metrics: None,
        }
    }

    /// メトリクスコレクターを設定
    pub fn with_metrics(mut self, metrics: Arc<MetricsCollector>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// プロファイリングを開始
    pub fn start_profiling(&mut self) {
        self.is_profiling = true;
        self.timers.clear();
        self.results.clear();
        self.calls.clear();
    }

    /// プロファイリングを停止
    pub fn stop_profiling(&mut self) {
        self.is_profiling = false;
    }

    /// タイマーを開始
    pub fn start_timer(&mut self, name: &str) {
        if self.is_profiling {
            self.timers.insert(name.to_string(), Instant::now());
        }
    }

    /// タイマーを停止
    pub fn stop_timer(&mut self, name: &str) {
        if self.is_profiling {
            if let Some(start_time) = self.timers.remove(name) {
                let elapsed = start_time.elapsed();
                
                // 累積時間を更新
                *self.results.entry(name.to_string()).or_insert(Duration::from_secs(0)) += elapsed;
                
                // 呼び出し回数を更新
                *self.calls.entry(name.to_string()).or_insert(0) += 1;
                
                // メトリクスに記録
                if let Some(metrics) = &self.metrics {
                    let metric_name = format!("parser_profile_{}", name.replace(" ", "_").to_lowercase());
                    metrics.record_histogram(&metric_name, elapsed.as_micros() as f64 / 1000.0, &format!("{}の処理時間（ミリ秒）", name));
                }
            }
        }
    }

    /// 指定した操作のプロファイリングを実行
    pub fn profile<F, R>(&mut self, name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.start_timer(name);
        let result = f();
        self.stop_timer(name);
        result
    }

    /// プロファイリング結果を取得
    pub fn get_results(&self) -> &HashMap<String, Duration> {
        &self.results
    }

    /// 呼び出し回数を取得
    pub fn get_calls(&self) -> &HashMap<String, usize> {
        &self.calls
    }

    /// プロファイリングレポートを生成
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str("# Parser Profiling Report\n\n");
        
        // 結果をソートして出力
        let mut sorted_results: Vec<(String, Duration, usize)> = self.results.iter()
            .map(|(name, duration)| {
                let calls = self.calls.get(name).cloned().unwrap_or(1);
                (name.clone(), *duration, calls)
            })
            .collect();
        
        // 合計時間で降順ソート
        sorted_results.sort_by(|a, b| b.1.cmp(&a.1));
        
        // 合計時間を計算
        let total_time: Duration = sorted_results.iter()
            .map(|(_, duration, _)| *duration)
            .sum();
        
        report.push_str(&format!("Total profiled time: {:.3} ms\n\n", total_time.as_micros() as f64 / 1000.0));
        report.push_str("| Operation | Time (ms) | Calls | Avg Time (ms) | % of Total |\n");
        report.push_str("|-----------|-----------|-------|---------------|------------|\n");
        
        for (name, duration, calls) in sorted_results {
            let time_ms = duration.as_micros() as f64 / 1000.0;
            let avg_time_ms = time_ms / calls as f64;
            let percentage = if total_time.as_nanos() > 0 {
                (duration.as_nanos() as f64 / total_time.as_nanos() as f64) * 100.0
            } else {
                0.0
            };
            
            report.push_str(&format!("| {} | {:.3} | {} | {:.3} | {:.1}% |\n",
                name, time_ms, calls, avg_time_ms, percentage));
        }
        
        report
    }
}

impl Default for ParserProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// パーサー品質メトリクス
#[derive(Debug, Clone)]
pub struct QualityMetrics {
    /// 解析されたコマンド数
    pub command_count: usize,
    /// 解析されたパイプライン数
    pub pipeline_count: usize,
    /// エラー数
    pub error_count: usize,
    /// 警告数
    pub warning_count: usize,
    /// 曖昧な構文数
    pub ambiguity_count: usize,
    /// 複雑度スコア
    pub complexity_score: f64,
    /// エラー回復回数
    pub error_recovery_count: usize,
    /// 最大ネスト深度
    pub max_nesting_depth: usize,
}

impl QualityMetrics {
    /// 新しい品質メトリクスを作成
    pub fn new() -> Self {
        Self {
            command_count: 0,
            pipeline_count: 0,
            error_count: 0,
            warning_count: 0,
            ambiguity_count: 0,
            complexity_score: 0.0,
            error_recovery_count: 0,
            max_nesting_depth: 0,
        }
    }

    /// ASTから品質メトリクスを計算
    pub fn from_ast(ast: &AstNode) -> Self {
        let mut metrics = Self::new();
        metrics.analyze_ast(ast, 0);
        metrics
    }

    /// ASTノードを分析してメトリクスを更新
    fn analyze_ast(&mut self, node: &AstNode, depth: usize) {
        // 最大ネスト深度を更新
        self.max_nesting_depth = self.max_nesting_depth.max(depth);
        
        // ノード種類に基づいてメトリクスを更新
        match node {
            AstNode::Command { .. } => {
                self.command_count += 1;
                // コマンドの複雑度: 1.0
                self.complexity_score += 1.0;
            },
            AstNode::Pipeline { commands, .. } => {
                self.pipeline_count += 1;
                // パイプラインの複雑度: コマンド数 * 1.5
                self.complexity_score += commands.len() as f64 * 1.5;
                
                // 子ノードを再帰的に分析
                for cmd in commands {
                    self.analyze_ast(cmd, depth + 1);
                }
            },
            AstNode::Block { commands, .. } => {
                // ブロックの複雑度: コマンド数 * 1.2
                self.complexity_score += commands.len() as f64 * 1.2;
                
                // 子ノードを再帰的に分析
                for cmd in commands {
                    self.analyze_ast(cmd, depth + 1);
                }
            },
            AstNode::Conditional { condition, then_branch, else_branch, .. } => {
                // 条件分岐の複雑度: 3.0 + else分岐があれば +2.0
                self.complexity_score += 3.0;
                if else_branch.is_some() {
                    self.complexity_score += 2.0;
                }
                
                // 子ノードを再帰的に分析
                self.analyze_ast(condition, depth + 1);
                self.analyze_ast(then_branch, depth + 1);
                if let Some(else_node) = else_branch {
                    self.analyze_ast(else_node, depth + 1);
                }
            },
            AstNode::Loop { body, .. } => {
                // ループの複雑度: 5.0
                self.complexity_score += 5.0;
                
                // 子ノードを再帰的に分析
                self.analyze_ast(body, depth + 1);
            },
            AstNode::Error { .. } => {
                self.error_count += 1;
            },
            // 他のノード種類も同様に分析
            _ => {}
        }
    }

    /// 品質スコアを計算（0.0-100.0）
    pub fn calculate_quality_score(&self) -> f64 {
        // 基準スコア
        let mut score = 100.0;
        
        // エラーごとに10点減点
        score -= self.error_count as f64 * 10.0;
        
        // 警告ごとに3点減点
        score -= self.warning_count as f64 * 3.0;
        
        // 曖昧性ごとに2点減点
        score -= self.ambiguity_count as f64 * 2.0;
        
        // 複雑度による減点（複雑度が20を超えると減点開始）
        if self.complexity_score > 20.0 {
            score -= (self.complexity_score - 20.0) * 0.5;
        }
        
        // ネスト深度による減点（深度が5を超えると減点開始）
        if self.max_nesting_depth > 5 {
            score -= (self.max_nesting_depth - 5) as f64 * 2.0;
        }
        
        // 最低0点、最高100点に制限
        score.max(0.0).min(100.0)
    }

    /// 品質レポートを生成
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str("# Parser Quality Report\n\n");
        
        // 基本メトリクス
        report.push_str("## Basic Metrics\n\n");
        report.push_str(&format!("- Commands: {}\n", self.command_count));
        report.push_str(&format!("- Pipelines: {}\n", self.pipeline_count));
        report.push_str(&format!("- Errors: {}\n", self.error_count));
        report.push_str(&format!("- Warnings: {}\n", self.warning_count));
        report.push_str(&format!("- Ambiguities: {}\n", self.ambiguity_count));
        
        // 複雑度メトリクス
        report.push_str("\n## Complexity Metrics\n\n");
        report.push_str(&format!("- Complexity Score: {:.1}\n", self.complexity_score));
        report.push_str(&format!("- Max Nesting Depth: {}\n", self.max_nesting_depth));
        report.push_str(&format!("- Error Recovery Count: {}\n", self.error_recovery_count));
        
        // 品質スコア
        let quality_score = self.calculate_quality_score();
        report.push_str("\n## Quality Score\n\n");
        report.push_str(&format!("- Overall Quality: {:.1}/100.0\n", quality_score));
        
        // 品質評価
        report.push_str("\n## Quality Assessment\n\n");
        if quality_score >= 90.0 {
            report.push_str("- Excellent: コードの品質は非常に高いです\n");
        } else if quality_score >= 75.0 {
            report.push_str("- Good: コードの品質は良好です\n");
        } else if quality_score >= 60.0 {
            report.push_str("- Acceptable: コードの品質は許容範囲内です\n");
        } else if quality_score >= 40.0 {
            report.push_str("- Concerning: コードの品質に懸念があります\n");
        } else {
            report.push_str("- Poor: コードの品質は低く、改善が必要です\n");
        }
        
        report
    }
}

impl Default for QualityMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        
        collector.record_counter("test_counter", 5, "テストカウンター");
        collector.record_gauge("test_gauge", 3.14, "テストゲージ");
        
        // メトリクスが正しく記録されたか確認
        let counter = collector.get_metric("test_counter").unwrap();
        let gauge = collector.get_metric("test_gauge").unwrap();
        
        assert_eq!(counter.metric_type, MetricType::Counter);
        if let MetricValue::Integer(val) = counter.value {
            assert_eq!(val, 5);
        } else {
            panic!("予期しないメトリクス値の型");
        }
        
        assert_eq!(gauge.metric_type, MetricType::Gauge);
        if let MetricValue::Float(val) = gauge.value {
            assert!((val - 3.14).abs() < 0.001);
        } else {
            panic!("予期しないメトリクス値の型");
        }
    }
    
    #[test]
    fn test_parser_profiler() {
        let mut profiler = ParserProfiler::new();
        
        profiler.start_profiling();
        
        // テスト関数をプロファイル
        profiler.profile("test_operation", || {
            // 時間のかかる処理をシミュレート
            std::thread::sleep(std::time::Duration::from_millis(10));
        });
        
        profiler.stop_profiling();
        
        // プロファイリング結果を確認
        let results = profiler.get_results();
        let calls = profiler.get_calls();
        
        assert!(results.contains_key("test_operation"));
        assert_eq!(calls.get("test_operation"), Some(&1));
        
        // 処理時間が記録されているか確認
        let duration = results.get("test_operation").unwrap();
        assert!(duration.as_millis() >= 10);
    }
} 