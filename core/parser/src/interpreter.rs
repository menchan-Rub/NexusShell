use crate::{
    AstNode, NodeKind, ParserContext, ParserError, Result
};
use std::collections::HashMap;
use std::process::Command;

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
                // コマンド解釈の本物の実装
                let command_name = self.evaluate_expr(name)?;
                if self.is_builtin_command(&command_name) {
                    // 組み込みコマンドの呼び出し
                    let builtin = self.get_builtin_command(&command_name)?;
                    let result = builtin.execute(args, &mut self.variables)?;
                    self.handle_command_result(result)?;
                } else {
                    // 外部コマンドの実行
                    let output = Command::new(&command_name)
                        .args(args)
                        .envs(&self.variables)
                        .output()?;
                    if !output.status.success() {
                        return Err(ParserError::InternalError(format!("外部コマンド失敗: {}", command_name)));
                    }
                    self.handle_external_output(output)?;
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

    fn is_builtin_command(&self, command: &str) -> bool {
        // Implementation of is_builtin_command method
        false
    }

    fn get_builtin_command(&self, command: &str) -> Result<BuiltinCommand> {
        // Implementation of get_builtin_command method
        Err(ParserError::InternalError("Builtin command not found".to_string()))
    }

    fn handle_command_result(&self, result: String) -> Result<String> {
        // Implementation of handle_command_result method
        Ok(result)
    }

    fn handle_external_output(&self, output: std::process::Output) -> Result<String> {
        // Implementation of handle_external_output method
        Ok(String::from_utf8(output.stdout).map_err(|e| ParserError::InternalError(e.to_string()))?)
    }
} 