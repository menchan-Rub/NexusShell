use crate::{
    AstNode, Token, TokenKind, Span, ParserContext, ParserError, Result,
    Parser as ParserTrait, PipelineKind, RedirectionKind,
    grammar::{GrammarRule, GrammarManager, GrammarRuleKind, get_grammar_manager}
};
use std::iter::Peekable;
use std::slice::Iter;
use std::collections::{HashMap, HashSet};

/// パーサーの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserType {
    /// 再帰下降型パーサー（手動実装）
    RecursiveDescent,
    /// LL(1)パーサー（予測型）
    PredictiveParsing,
    /// 文法規則ベースの汎用パーサー
    RuleBased,
}

impl Default for ParserType {
    fn default() -> Self {
        Self::RecursiveDescent
    }
}

/// パーサー設定
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// パーサーの種類
    pub parser_type: ParserType,
    /// エラー回復を有効にするか
    pub error_recovery: bool,
    /// 詳細なデバッグ情報を出力するか
    pub verbose: bool,
    /// パースツリーの最大深さ
    pub max_depth: usize,
    /// パフォーマンス測定を行うか
    pub measure_performance: bool,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            parser_type: ParserType::default(),
            error_recovery: true,
            verbose: false,
            max_depth: 100,
            measure_performance: false,
        }
    }
}

/// パーサーのパフォーマンス統計情報
#[derive(Debug, Default, Clone)]
pub struct ParserStats {
    /// パース開始時刻
    pub start_time: std::time::Instant,
    /// パース終了時刻
    pub end_time: std::time::Instant,
    /// パースにかかった時間（ミリ秒）
    pub elapsed_ms: f64,
    /// 処理したトークン数
    pub token_count: usize,
    /// 生成したASTノード数
    pub node_count: usize,
    /// エラー数
    pub error_count: usize,
    /// 最大再帰深度
    pub max_recursion_depth: usize,
    /// バックトラック回数
    pub backtrack_count: usize,
}

/// 再帰下降型の構文解析器
#[derive(Debug)]
pub struct RecursiveDescentParser {
    context: ParserContext,
    config: ParserConfig,
    stats: ParserStats,
    current_depth: usize,
    rule_cache: HashMap<String, Vec<AstNode>>,
}

impl RecursiveDescentParser {
    /// 新しい構文解析器を作成
    pub fn new() -> Self {
        Self {
            context: ParserContext::new(String::new()),
            config: ParserConfig::default(),
            stats: ParserStats::default(),
            current_depth: 0,
            rule_cache: HashMap::new(),
        }
    }
    
    /// 設定でパーサーをカスタマイズ
    pub fn with_config(mut self, config: ParserConfig) -> Self {
        self.config = config;
        self
    }
    
    /// トークンイテレータを初期化
    fn init_tokenizer(&mut self, input: &str, tokens: Vec<Token>) {
        self.context = ParserContext::new(input.to_string());
        self.context.tokens = tokens;
        self.context.current = 0;
        
        if self.config.measure_performance {
            self.stats = ParserStats::default();
            self.stats.start_time = std::time::Instant::now();
            self.stats.token_count = tokens.len();
        }
    }
    
    /// 現在のトークンを取得
    fn current_token(&self) -> Option<&Token> {
        self.context.tokens.get(self.context.current)
    }
    
    /// 次のトークンを取得
    fn peek_token(&self) -> Option<&Token> {
        self.context.tokens.get(self.context.current + 1)
    }
    
    /// N個先のトークンを取得
    fn peek_n_token(&self, n: usize) -> Option<&Token> {
        self.context.tokens.get(self.context.current + n)
    }
    
    /// トークンを消費して次に進む
    fn advance(&mut self) -> Option<&Token> {
        if self.context.current < self.context.tokens.len() {
            let token = &self.context.tokens[self.context.current];
            self.context.current += 1;
            Some(token)
        } else {
            None
        }
    }
    
    /// 指定された種類のトークンを取得、または期待
    fn expect(&mut self, kind: TokenKind) -> Result<&Token> {
        if let Some(token) = self.current_token() {
            if token.kind == kind {
                self.advance();
                Ok(token)
            } else {
                Err(ParserError::UnexpectedToken {
                    expected: format!("{:?}", kind),
                    actual: format!("{:?}", token.kind),
                    span: token.span.clone(),
                })
            }
        } else {
            Err(ParserError::UnexpectedEOF(format!("Expected {:?}", kind)))
        }
    }
    
    /// 現在の位置まで巻き戻し
    fn rewind(&mut self, position: usize) {
        self.context.current = position;
        if self.config.measure_performance {
            self.stats.backtrack_count += 1;
        }
    }

    /// コマンドをパース
    fn parse_command(&mut self) -> Result<AstNode> {
        self.track_depth();
        
        let token = self.current_token().ok_or_else(|| ParserError::SyntaxError(
            "予期しないEOF".to_string(),
            Span::default(),
        ))?;
        
        if token.kind != TokenKind::Command && token.kind != TokenKind::Identifier {
            return Err(ParserError::SyntaxError(
                "コマンドの識別子が必要です".to_string(),
                token.span.clone(),
            ));
        }
        
        let command_token = self.advance().unwrap();
        let command_name = command_token.lexeme.clone();
        let command_span = command_token.span.clone();
        
        let mut arguments = Vec::new();
        let mut redirections = Vec::new();
        
        // 引数とリダイレクションをパース
        while let Some(token) = self.current_token() {
            match token.kind {
                TokenKind::Argument | TokenKind::String | TokenKind::Integer | TokenKind::Variable => {
                    // 引数
                    let arg = self.parse_argument()?;
                    arguments.push(arg);
                },
                TokenKind::Option | TokenKind::Flag => {
                    // オプション/フラグも引数として扱う
                    let option = self.parse_option()?;
                    arguments.push(option);
                },
                TokenKind::RedirectOut | TokenKind::RedirectIn | TokenKind::RedirectAppend | TokenKind::RedirectMerge => {
                    // リダイレクション
                    let redirection = self.parse_redirection()?;
                    redirections.push(redirection);
                },
                TokenKind::Pipe | TokenKind::PipeTyped | TokenKind::PipeConditional | 
                TokenKind::PipeParallel | TokenKind::PipeError | TokenKind::Semicolon | 
                TokenKind::Ampersand | TokenKind::RightBrace | TokenKind::RightParen => {
                    // コマンドの終了を示すトークン
                    break;
                },
                _ => {
                    // 不明なトークン
                    if self.config.error_recovery {
                        // エラーを記録して次へ
                        self.context.errors.push(ParserError::SyntaxError(
                            format!("コマンド引数として不明なトークン: {:?}", token.kind),
                            token.span.clone(),
                        ));
                        self.advance();
                    } else {
                        return Err(ParserError::SyntaxError(
                            format!("コマンド引数として不明なトークン: {:?}", token.kind),
                            token.span.clone(),
                        ));
                    }
                }
            }
        }
        
        self.untrack_depth();
        
        Ok(AstNode::Command {
            name: command_name,
            arguments,
            redirections,
            span: command_span,
        })
    }
    
    /// 引数をパース
    fn parse_argument(&mut self) -> Result<AstNode> {
        self.track_depth();
        
        let token = self.current_token().ok_or_else(|| ParserError::SyntaxError(
            "予期しないEOF".to_string(),
            Span::default(),
        ))?;
        
        let value = token.lexeme.clone();
        let span = token.span.clone();
        
        self.advance();
        self.untrack_depth();
        
        Ok(AstNode::Argument {
            value,
            span,
        })
    }
    
    /// オプションをパース
    fn parse_option(&mut self) -> Result<AstNode> {
        self.track_depth();
        
        let token = self.current_token().ok_or_else(|| ParserError::SyntaxError(
            "予期しないEOF".to_string(),
            Span::default(),
        ))?;
        
        let name = token.lexeme.clone();
        let span = token.span.clone();
        
        self.advance();
        
        // オプションに値があるかチェック（次のトークンが引数の場合）
        let value = if let Some(next_token) = self.current_token() {
            if matches!(next_token.kind, TokenKind::Argument | TokenKind::String | TokenKind::Integer | TokenKind::Variable) {
                Some(Box::new(self.parse_argument()?))
            } else {
                None
            }
        } else {
            None
        };
        
        self.untrack_depth();
        
        Ok(AstNode::Option {
            name,
            value,
            span,
        })
    }
    
    /// リダイレクションをパース
    fn parse_redirection(&mut self) -> Result<AstNode> {
        self.track_depth();
        
        let token = self.current_token().ok_or_else(|| ParserError::SyntaxError(
            "予期しないEOF".to_string(),
            Span::default(),
        ))?;
        
        let kind = match token.kind {
            TokenKind::RedirectOut => RedirectionKind::Output,
            TokenKind::RedirectAppend => RedirectionKind::Append,
            TokenKind::RedirectIn => RedirectionKind::Input,
            TokenKind::RedirectMerge => RedirectionKind::Merge,
            _ => return Err(ParserError::SyntaxError(
                format!("不正なリダイレクションタイプ: {:?}", token.kind),
                token.span.clone(),
            )),
        };
        
        let redir_span = token.span.clone();
        self.advance();
        
        // リダイレクション先をパース
        let target = if let Some(next_token) = self.current_token() {
            if matches!(next_token.kind, TokenKind::Argument | TokenKind::String | TokenKind::Integer | TokenKind::Variable) {
                Box::new(self.parse_argument()?)
            } else {
                return Err(ParserError::SyntaxError(
                    "リダイレクション先が必要です".to_string(),
                    next_token.span.clone(),
                ));
            }
        } else {
            return Err(ParserError::UnexpectedEOF("リダイレクション先が必要です".to_string()));
        };
        
        self.untrack_depth();
        
        Ok(AstNode::Redirection {
            kind,
            target,
            span: redir_span,
        })
    }
    
    /// パイプラインをパース
    fn parse_pipeline(&mut self) -> Result<AstNode> {
        self.track_depth();
        
        let command = self.parse_command()?;
        let mut commands = vec![command];
        let start_span = match &commands[0] {
            AstNode::Command { span, .. } => span.clone(),
            _ => Span::default(),
        };
        
        let mut end_span = start_span.clone();
        
        // パイプトークンがあれば処理
        while let Some(token) = self.current_token() {
            if !matches!(token.kind, 
                TokenKind::Pipe | TokenKind::PipeTyped | 
                TokenKind::PipeConditional | TokenKind::PipeParallel | 
                TokenKind::PipeError
            ) {
                break;
            }
            
            let pipe_kind = match token.kind {
                TokenKind::Pipe => PipelineKind::Standard,
                TokenKind::PipeTyped => PipelineKind::Typed,
                TokenKind::PipeConditional => PipelineKind::Conditional,
                TokenKind::PipeParallel => PipelineKind::Parallel,
                TokenKind::PipeError => PipelineKind::Error,
                _ => unreachable!(),
            };
            
            self.advance();
            
            // パイプの後にはコマンドが必要
            if let Some(next_token) = self.current_token() {
                if matches!(next_token.kind, TokenKind::Command | TokenKind::Identifier) {
                    let next_command = self.parse_command()?;
                    
                    // スパンを更新
                    if let AstNode::Command { span, .. } = &next_command {
                        end_span = span.clone();
                    }
                    
                    commands.push(next_command);
                } else {
                    return Err(ParserError::SyntaxError(
                        "パイプ後にコマンドが必要です".to_string(),
                        next_token.span.clone(),
                    ));
                }
            } else {
                return Err(ParserError::UnexpectedEOF("パイプ後にコマンドが必要です".to_string()));
            }
        }
        
        self.untrack_depth();
        
        // コマンドが1つだけならそのまま返す
        if commands.len() == 1 {
            return Ok(commands.remove(0));
        }
        
        // パイプラインのスパンは最初のコマンドから最後のコマンドまで
        let pipeline_span = Span {
            start: start_span.start,
            end: end_span.end,
            line: start_span.line,
            column: start_span.column,
        };
        
        Ok(AstNode::Pipeline {
            commands,
            kind: PipelineKind::Standard, // デフォルトは標準パイプ
            span: pipeline_span,
        })
    }
    
    /// ルートノードをパース
    fn parse_root(&mut self) -> Result<AstNode> {
        self.track_depth();
        
        // パイプラインをパース
        let result = self.parse_pipeline();
        
        self.untrack_depth();
        result
    }
    
    /// 文法規則に基づいてパース
    fn parse_with_grammar(&mut self, rule_name: &str) -> Result<AstNode> {
        // すでにキャッシュされた結果があれば使用
        if let Some(nodes) = self.rule_cache.get(rule_name) {
            if !nodes.is_empty() {
                return Ok(nodes[0].clone());
            }
        }
        
        // 文法マネージャーから規則を取得
        let grammar_manager = get_grammar_manager();
        let rule = grammar_manager.get_rule(rule_name).ok_or_else(|| {
            ParserError::NotImplemented(format!("未定義の文法規則: {}", rule_name))
        })?;
        
        // 規則の種類に基づいてパース
        let result = match rule.kind {
            GrammarRuleKind::Command => self.parse_command(),
            GrammarRuleKind::Pipeline => self.parse_pipeline(),
            GrammarRuleKind::Redirection => self.parse_redirection(),
            // 他の規則も同様に実装
            _ => Err(ParserError::NotImplemented(
                format!("未実装の文法規則種類: {:?}", rule.kind)
            )),
        };
        
        // 結果をキャッシュに保存
        if let Ok(node) = &result {
            let mut nodes = Vec::new();
            nodes.push(node.clone());
            self.rule_cache.insert(rule_name.to_string(), nodes);
        }
        
        result
    }
    
    /// パフォーマンス統計情報を更新
    fn update_stats(&mut self) {
        if self.config.measure_performance {
            self.stats.end_time = std::time::Instant::now();
            self.stats.elapsed_ms = self.stats.end_time.duration_since(self.stats.start_time).as_secs_f64() * 1000.0;
            self.stats.error_count = self.context.errors.len();
        }
    }
    
    /// パーサーの再帰深度を追跡
    fn track_depth(&mut self) {
        self.current_depth += 1;
        if self.config.measure_performance {
            self.stats.max_recursion_depth = self.stats.max_recursion_depth.max(self.current_depth);
        }
    }
    
    /// パーサーの再帰深度追跡を終了
    fn untrack_depth(&mut self) {
        self.current_depth -= 1;
    }
    
    /// パース実行
    pub fn parse(&mut self, input: &str, tokens: Vec<Token>) -> Result<AstNode> {
        // トークナイザを初期化
        self.init_tokenizer(input, tokens);
        
        // トークンが存在するか確認
        if self.context.tokens.is_empty() {
            return Err(ParserError::SyntaxError(
                "入力が空です".to_string(),
                Span::default(),
            ));
        }
        
        // パーサータイプに基づいて解析
        let result = match self.config.parser_type {
            ParserType::RecursiveDescent => self.parse_root(),
            ParserType::RuleBased => self.parse_with_grammar("command"),
            ParserType::PredictiveParsing => {
                // LL(1)パーサーの実装（将来的に実装予定）
                Err(ParserError::NotImplemented("LL(1)パーサーは未実装です".to_string()))
            }
        };
        
        // 統計情報を更新
        self.update_stats();
        
        result
    }
    
    /// コンテキストを取得
    pub fn get_context(&self) -> &ParserContext {
        &self.context
    }
    
    /// 統計情報を取得
    pub fn get_stats(&self) -> &ParserStats {
        &self.stats
    }
} 