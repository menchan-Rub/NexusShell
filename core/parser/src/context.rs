use crate::error::ParserError;
use crate::TokenKind;
use crate::Span;
use std::collections::{HashMap, HashSet};

/// コマンドの実行コンテキスト情報
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// コマンド名
    pub name: String,
    /// コマンドの引数
    pub arguments: Vec<String>,
    /// コマンドのオプション (フラグ)
    pub options: HashMap<String, Option<String>>,
    /// コマンドのリダイレクション
    pub redirections: Vec<RedirectionInfo>,
    /// 変数割り当て
    pub assignments: HashMap<String, String>,
    /// コマンドの開始位置
    pub span: Span,
}

/// リダイレクション情報
#[derive(Debug, Clone)]
pub struct RedirectionInfo {
    /// リダイレクションの種類
    pub kind: RedirectionKind,
    /// リダイレクション先
    pub target: String,
    /// リダイレクション位置
    pub span: Span,
}

/// リダイレクションの種類
#[derive(Debug, Clone, PartialEq)]
pub enum RedirectionKind {
    StdoutOverwrite,   // 標準出力を上書き ">"
    StdoutAppend,      // 標準出力に追記 ">>"
    StderrOverwrite,   // 標準エラー出力を上書き "2>"
    StderrAppend,      // 標準エラー出力に追記 "2>>"
    StdinFrom,         // 標準入力を読み込み "<"
    StdinHeredoc,      // ヒアドキュメント "<<"
    StdinHerestring,   // ヒアストリング "<<<"
    StdoutAndStderrOverwrite, // 標準出力と標準エラー出力を上書き "&>"
    StdoutAndStderrAppend,    // 標準出力と標準エラー出力に追記 "&>>"
    FileDescriptor,    // ファイル記述子 "[n]>&m"
    Close,             // ファイル記述子をクローズ "[n]>&-"
    OutputToInput,     // コマンド出力を別コマンドの入力に "<>"
}

/// コンテキスト解析結果
#[derive(Debug, Clone)]
pub struct ContextAnalysisResult {
    /// コマンドコンテキスト
    pub commands: Vec<CommandContext>,
    /// パイプラインの情報
    pub pipelines: Vec<PipelineInfo>,
    /// サブシェルの情報
    pub subshells: Vec<SubshellInfo>,
    /// 条件分岐の情報
    pub conditionals: Vec<ConditionalInfo>,
    /// ループの情報
    pub loops: Vec<LoopInfo>,
    /// 変数参照
    pub variable_references: HashMap<String, Vec<Span>>,
    /// エラー情報
    pub errors: Vec<ParserError>,
}

/// パイプライン情報
#[derive(Debug, Clone)]
pub struct PipelineInfo {
    /// パイプラインを構成するコマンドのインデックス
    pub command_indices: Vec<usize>,
    /// パイプタイプ（標準、標準エラー、条件付きなど）
    pub pipe_types: Vec<PipelineKind>,
    /// パイプラインの範囲
    pub span: Span,
}

/// パイプラインの種類
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineKind {
    Standard,    // 標準的なパイプ "|"
    StdErr,      // 標準エラー出力のパイプ "|&"
    Background,  // バックグラウンド実行 "&"
    Conditional, // 条件付きパイプ "&&" または "||"
    Process,     // プロセス置換 ">(" または "<("
}

/// サブシェル情報
#[derive(Debug, Clone)]
pub struct SubshellInfo {
    /// サブシェル内のコンテキスト
    pub inner_context: Box<ContextAnalysisResult>,
    /// サブシェルの範囲
    pub span: Span,
}

/// 条件分岐情報
#[derive(Debug, Clone)]
pub struct ConditionalInfo {
    /// 条件部分のコンテキスト
    pub condition: Box<ContextAnalysisResult>,
    /// then部分のコンテキスト
    pub then_branch: Box<ContextAnalysisResult>,
    /// else部分のコンテキスト（存在する場合）
    pub else_branch: Option<Box<ContextAnalysisResult>>,
    /// 条件文の範囲
    pub span: Span,
}

/// ループ情報
#[derive(Debug, Clone)]
pub struct LoopInfo {
    /// ループの種類
    pub kind: LoopKind,
    /// ループ条件/イテレータのコンテキスト
    pub condition: Box<ContextAnalysisResult>,
    /// ループ本体のコンテキスト
    pub body: Box<ContextAnalysisResult>,
    /// ループの範囲
    pub span: Span,
}

/// ループの種類
#[derive(Debug, Clone, PartialEq)]
pub enum LoopKind {
    For,
    While,
    Until,
}

/// コンテキスト分析器
pub struct ContextAnalyzer {
    /// 予約語セット
    reserved_words: HashSet<String>,
    /// 組み込みコマンドセット
    builtin_commands: HashSet<String>,
}

impl ContextAnalyzer {
    /// 新しいコンテキスト分析器を作成
    pub fn new() -> Self {
        let mut reserved_words = HashSet::new();
        reserved_words.insert("if".to_string());
        reserved_words.insert("then".to_string());
        reserved_words.insert("else".to_string());
        reserved_words.insert("elif".to_string());
        reserved_words.insert("fi".to_string());
        reserved_words.insert("for".to_string());
        reserved_words.insert("while".to_string());
        reserved_words.insert("until".to_string());
        reserved_words.insert("do".to_string());
        reserved_words.insert("done".to_string());
        reserved_words.insert("case".to_string());
        reserved_words.insert("esac".to_string());
        reserved_words.insert("function".to_string());
        reserved_words.insert("in".to_string());

        let mut builtin_commands = HashSet::new();
        builtin_commands.insert("cd".to_string());
        builtin_commands.insert("pwd".to_string());
        builtin_commands.insert("echo".to_string());
        builtin_commands.insert("export".to_string());
        builtin_commands.insert("source".to_string());
        builtin_commands.insert("alias".to_string());
        builtin_commands.insert("exit".to_string());
        builtin_commands.insert("set".to_string());
        builtin_commands.insert("unset".to_string());
        builtin_commands.insert("history".to_string());
        builtin_commands.insert("help".to_string());
        
        Self {
            reserved_words,
            builtin_commands,
        }
    }

    /// ASTからコンテキスト情報を解析
    pub fn analyze_ast(&self, ast: &crate::AstNode) -> ContextAnalysisResult {
        let mut result = ContextAnalysisResult {
            commands: Vec::new(),
            pipelines: Vec::new(),
            subshells: Vec::new(),
            conditionals: Vec::new(),
            loops: Vec::new(),
            variable_references: HashMap::new(),
            errors: Vec::new(),
        };

        self.analyze_node(ast, &mut result);
        self.post_process(&mut result);

        result
    }

    /// ASTノードを解析してコンテキスト情報を抽出
    fn analyze_node(&self, node: &crate::AstNode, result: &mut ContextAnalysisResult) {
        match node {
            crate::AstNode::Command { name, args, options, redirects, span } => {
                let cmd_index = result.commands.len();
                
                // コマンドコンテキストを作成
                let mut context = CommandContext {
                    name: name.clone(),
                    arguments: Vec::new(),
                    options: HashMap::new(),
                    redirections: Vec::new(),
                    assignments: HashMap::new(),
                    span: span.clone(),
                };
                
                // 引数を処理
                for arg in args {
                    if let crate::AstNode::Argument { value, .. } = arg {
                        context.arguments.push(value.clone());
                    }
                }
                
                // オプションを処理
                for opt in options {
                    if let crate::AstNode::Option { name, value, .. } = opt {
                        context.options.insert(name.clone(), value.clone());
                    }
                }
                
                // リダイレクションを処理
                for redir in redirects {
                    if let crate::AstNode::Redirection { kind, target, span, .. } = redir {
                        let redir_kind = match kind {
                            crate::RedirectionKind::StdoutOverwrite => RedirectionKind::StdoutOverwrite,
                            crate::RedirectionKind::StdoutAppend => RedirectionKind::StdoutAppend,
                            crate::RedirectionKind::StderrOverwrite => RedirectionKind::StderrOverwrite,
                            crate::RedirectionKind::StderrAppend => RedirectionKind::StderrAppend,
                            crate::RedirectionKind::StdinFrom => RedirectionKind::StdinFrom,
                            crate::RedirectionKind::StdinHeredoc => RedirectionKind::StdinHeredoc,
                            crate::RedirectionKind::StdinHerestring => RedirectionKind::StdinHerestring,
                            crate::RedirectionKind::StdoutAndStderrOverwrite => RedirectionKind::StdoutAndStderrOverwrite,
                            crate::RedirectionKind::StdoutAndStderrAppend => RedirectionKind::StdoutAndStderrAppend,
                            crate::RedirectionKind::FileDescriptor => RedirectionKind::FileDescriptor,
                            crate::RedirectionKind::Close => RedirectionKind::Close,
                            crate::RedirectionKind::OutputToInput => RedirectionKind::OutputToInput,
                        };
                        
                        context.redirections.push(RedirectionInfo {
                            kind: redir_kind,
                            target: target.clone(),
                            span: span.clone(),
                        });
                    }
                }
                
                result.commands.push(context);
            },
            crate::AstNode::Pipeline { commands, pipe_types, span } => {
                let start_idx = result.commands.len();
                
                // パイプライン内の各コマンドを処理
                for cmd in commands {
                    self.analyze_node(cmd, result);
                }
                
                // コマンドインデックスを収集
                let end_idx = result.commands.len();
                let cmd_indices: Vec<usize> = (start_idx..end_idx).collect();
                
                // パイプタイプを変換
                let converted_pipe_types: Vec<PipelineKind> = pipe_types.iter().map(|pt| {
                    match pt {
                        crate::PipelineKind::Standard => PipelineKind::Standard,
                        crate::PipelineKind::StdErr => PipelineKind::StdErr,
                        crate::PipelineKind::Background => PipelineKind::Background,
                        crate::PipelineKind::Conditional => PipelineKind::Conditional,
                        crate::PipelineKind::Process => PipelineKind::Process,
                    }
                }).collect();
                
                // パイプライン情報を追加
                result.pipelines.push(PipelineInfo {
                    command_indices: cmd_indices,
                    pipe_types: converted_pipe_types,
                    span: span.clone(),
                });
            },
            crate::AstNode::Subshell { commands, span } => {
                let mut inner_result = ContextAnalysisResult {
                    commands: Vec::new(),
                    pipelines: Vec::new(),
                    subshells: Vec::new(),
                    conditionals: Vec::new(),
                    loops: Vec::new(),
                    variable_references: HashMap::new(),
                    errors: Vec::new(),
                };
                
                // サブシェル内の各コマンドを処理
                for cmd in commands {
                    self.analyze_node(cmd, &mut inner_result);
                }
                
                // サブシェル情報を追加
                result.subshells.push(SubshellInfo {
                    inner_context: Box::new(inner_result),
                    span: span.clone(),
                });
            },
            crate::AstNode::Conditional { condition, then_branch, else_branch, span } => {
                let mut condition_result = ContextAnalysisResult {
                    commands: Vec::new(),
                    pipelines: Vec::new(),
                    subshells: Vec::new(),
                    conditionals: Vec::new(),
                    loops: Vec::new(),
                    variable_references: HashMap::new(),
                    errors: Vec::new(),
                };
                
                let mut then_result = condition_result.clone();
                let mut else_result = None;
                
                // 条件部分を処理
                self.analyze_node(condition, &mut condition_result);
                
                // then部分を処理
                self.analyze_node(then_branch, &mut then_result);
                
                // else部分を処理（存在する場合）
                if let Some(else_node) = else_branch {
                    let mut else_ctx = ContextAnalysisResult {
                        commands: Vec::new(),
                        pipelines: Vec::new(),
                        subshells: Vec::new(),
                        conditionals: Vec::new(),
                        loops: Vec::new(),
                        variable_references: HashMap::new(),
                        errors: Vec::new(),
                    };
                    
                    self.analyze_node(else_node, &mut else_ctx);
                    else_result = Some(Box::new(else_ctx));
                }
                
                // 条件分岐情報を追加
                result.conditionals.push(ConditionalInfo {
                    condition: Box::new(condition_result),
                    then_branch: Box::new(then_result),
                    else_branch: else_result,
                    span: span.clone(),
                });
            },
            crate::AstNode::Loop { kind, condition, body, span } => {
                let mut condition_result = ContextAnalysisResult {
                    commands: Vec::new(),
                    pipelines: Vec::new(),
                    subshells: Vec::new(),
                    conditionals: Vec::new(),
                    loops: Vec::new(),
                    variable_references: HashMap::new(),
                    errors: Vec::new(),
                };
                
                let mut body_result = condition_result.clone();
                
                // ループ条件を処理
                self.analyze_node(condition, &mut condition_result);
                
                // ループ本体を処理
                self.analyze_node(body, &mut body_result);
                
                // ループ種類を変換
                let loop_kind = match kind {
                    crate::LoopKind::For => LoopKind::For,
                    crate::LoopKind::While => LoopKind::While,
                    crate::LoopKind::Until => LoopKind::Until,
                };
                
                // ループ情報を追加
                result.loops.push(LoopInfo {
                    kind: loop_kind,
                    condition: Box::new(condition_result),
                    body: Box::new(body_result),
                    span: span.clone(),
                });
            },
            crate::AstNode::VariableReference { name, span, .. } => {
                // 変数参照を追加
                result.variable_references
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .push(span.clone());
            },
            crate::AstNode::Error { message, span } => {
                // エラーを追加
                result.errors.push(ParserError::SyntaxError {
                    message: message.clone(),
                    span: span.clone(),
                });
            },
            // 他のノードタイプも必要に応じて処理
            _ => {}
        }
    }

    /// コンテキスト情報の後処理
    fn post_process(&self, result: &mut ContextAnalysisResult) {
        // 変数参照の検証
        self.validate_variable_references(result);
        
        // コマンドの検証
        self.validate_commands(result);
        
        // リダイレクションの検証
        self.validate_redirections(result);
    }

    /// 変数参照の検証
    fn validate_variable_references(&self, result: &mut ContextAnalysisResult) {
        // 未定義変数の検出など
        // 実際の実装はもっと複雑になる可能性があります
    }

    /// コマンドの検証
    fn validate_commands(&self, result: &mut ContextAnalysisResult) {
        for cmd in &result.commands {
            // 予約語がコマンドとして使用されていないか確認
            if self.reserved_words.contains(&cmd.name) {
                result.errors.push(ParserError::SemanticError {
                    message: format!("予約語 '{}' がコマンドとして使用されています", cmd.name),
                    span: cmd.span.clone(),
                });
            }
            
            // その他のコマンド固有のバリデーション
        }
    }

    /// リダイレクションの検証
    fn validate_redirections(&self, result: &mut ContextAnalysisResult) {
        for cmd in &result.commands {
            for redir in &cmd.redirections {
                match redir.kind {
                    RedirectionKind::StdinFrom | 
                    RedirectionKind::StdoutOverwrite | 
                    RedirectionKind::StdoutAppend => {
                        // 基本的なリダイレクションの検証
                    },
                    RedirectionKind::FileDescriptor => {
                        // ファイル記述子のフォーマット検証
                        if !redir.target.contains('>') && !redir.target.contains('<') {
                            result.errors.push(ParserError::SemanticError {
                                message: format!("無効なファイル記述子リダイレクション: '{}'", redir.target),
                                span: redir.span.clone(),
                            });
                        }
                    },
                    _ => {
                        // 他のリダイレクション種類の検証
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // テストケース
    // 実際の実装ではここにテストを追加
} 