use crate::{
    AstNode, NodeKind, ParserContext, ParserError, Result
};
use std::collections::HashMap;

pub struct Interpreter {
    pub variables: HashMap<String, String>,
    pub context: ParserContext,
}

impl Interpreter {
    pub fn new(context: ParserContext) -> Self {
        Self {
            variables: HashMap::new(),
            context,
        }
    }

    pub fn interpret(&mut self, ast: &AstNode) -> Result<String> {
        match &ast.kind {
            NodeKind::Command { name, args, redirects, pipe } => {
                // コマンド解釈の実装
                // 実際のシェルではここで外部コマンドを実行したり組み込みコマンドを呼び出したりする
                let command_name = self.evaluate_expr(name)?;
                let mut arguments = Vec::new();
                
                for arg in args {
                    arguments.push(self.evaluate_expr(arg)?);
                }
                
                // ここでは単にコマンドとその引数を文字列として返すだけ
                if arguments.is_empty() {
                    Ok(command_name)
                } else {
                    Ok(format!("{} {}", command_name, arguments.join(" ")))
                }
            },
            NodeKind::Argument { value } => {
                match &value.kind {
                    NodeKind::String { value } => Ok(value.clone()),
                    NodeKind::Variable { name } => {
                        self.variables.get(name)
                            .ok_or_else(|| ParserError::SemanticError(
                                format!("未定義の変数: {}", name),
                                ast.span.clone()
                            ))
                            .map(|s| s.clone())
                    },
                    _ => Err(ParserError::InternalError(
                        format!("予期しないノード種類: {:?}", value.kind)
                    )),
                }
            },
            NodeKind::String { value } => Ok(value.clone()),
            NodeKind::Variable { name } => {
                self.variables.get(name)
                    .ok_or_else(|| ParserError::SemanticError(
                        format!("未定義の変数: {}", name),
                        ast.span.clone()
                    ))
                    .map(|s| s.clone())
            },
            NodeKind::Pipeline { commands } => {
                // パイプラインの解釈は複雑なので、ここではシンプルに各コマンドを実行して結果を連結するだけ
                let mut results = Vec::new();
                for cmd in commands {
                    results.push(self.interpret(cmd)?);
                }
                Ok(results.join(" | "))
            },
            // その他のノード種類の処理...
            _ => Err(ParserError::InternalError(
                format!("解釈できないノード種類: {:?}", ast.kind)
            )),
        }
    }

    fn evaluate_expr(&mut self, expr: &AstNode) -> Result<String> {
        match &expr.kind {
            NodeKind::String { value } => Ok(value.clone()),
            NodeKind::Variable { name } => {
                self.variables.get(name)
                    .ok_or_else(|| ParserError::SemanticError(
                        format!("未定義の変数: {}", name),
                        expr.span.clone()
                    ))
                    .map(|s| s.clone())
            },
            NodeKind::Argument { value } => self.evaluate_expr(value),
            _ => Err(ParserError::InternalError(
                format!("式として評価できないノード種類: {:?}", expr.kind)
            )),
        }
    }
} 