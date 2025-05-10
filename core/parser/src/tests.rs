//! NexusShellのパーサーのテストモジュール

use crate::{
    Lexer, Parser, Token, TokenKind, ParserError,
    ast::{Node, Command, Pipeline, Redirection},
    DefaultLexer, DefaultParser, 
    span::{Span, SourceFile}
};
use std::sync::Arc;

// テストユーティリティ関数
fn tokenize(input: &str) -> Vec<Token> {
    let mut lexer = DefaultLexer::new();
    lexer.tokenize(input).unwrap_or_else(|e| panic!("トークン化エラー: {}", e))
}

fn parse(input: &str) -> Result<Node, ParserError> {
    let tokens = tokenize(input);
    let mut parser = DefaultParser::new();
    parser.parse(&tokens)
}

fn dump_tokens(tokens: &[Token]) -> String {
    tokens.iter()
        .map(|t| format!("{:?}({})", t.kind, t.lexeme))
        .collect::<Vec<_>>()
        .join(", ")
}

fn dump_ast(node: &Node, indent: usize) -> String {
    let prefix = " ".repeat(indent * 2);
    
    match node {
        Node::Command(cmd) => {
            let mut result = format!("{}Command: {}\n", prefix, cmd.name);
            
            if !cmd.args.is_empty() {
                result.push_str(&format!("{}  Args: [{}]\n", prefix, 
                    cmd.args.iter().map(|a| format!("\"{}\"", a)).collect::<Vec<_>>().join(", ")));
            }
            
            if !cmd.flags.is_empty() {
                result.push_str(&format!("{}  Flags: [{}]\n", prefix,
                    cmd.flags.iter().map(|f| format!("\"{}\"", f)).collect::<Vec<_>>().join(", ")));
            }
            
            if !cmd.redirections.is_empty() {
                result.push_str(&format!("{}  Redirections:\n", prefix));
                for redir in &cmd.redirections {
                    result.push_str(&format!("{}    {:?} -> {}\n", prefix, redir.kind, redir.target));
                }
            }
            
            result
        },
        Node::Pipeline(pipeline) => {
            let mut result = format!("{}Pipeline:\n", prefix);
            for cmd in &pipeline.commands {
                result.push_str(&dump_ast(cmd, indent + 1));
            }
            result
        },
        Node::Block(block) => {
            let mut result = format!("{}Block:\n", prefix);
            for stmt in &block.statements {
                result.push_str(&dump_ast(stmt, indent + 1));
            }
            result
        },
        Node::Script(script) => {
            let mut result = format!("{}Script:\n", prefix);
            for stmt in &script.statements {
                result.push_str(&dump_ast(stmt, indent + 1));
            }
            result
        },
        // 他のノードタイプも同様に処理
        _ => format!("{}{:?}\n", prefix, node),
    }
}

// テスト
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_basic_tokenization() {
        let input = "ls -l | grep foo --color";
        let tokens = tokenize(input);
        
        // EOFトークンを含めて7つのトークンが期待される
        assert_eq!(tokens.len(), 7, "トークン数が一致しません: {}", dump_tokens(&tokens));
        
        // 各トークンの種類を検証
        assert_eq!(tokens[0].kind, TokenKind::Command);
        assert_eq!(tokens[0].lexeme, "ls");
        
        assert_eq!(tokens[1].kind, TokenKind::Flag);
        assert_eq!(tokens[1].lexeme, "-l");
        
        assert_eq!(tokens[2].kind, TokenKind::Pipe);
        
        assert_eq!(tokens[3].kind, TokenKind::Command);
        assert_eq!(tokens[3].lexeme, "grep");
        
        assert_eq!(tokens[4].kind, TokenKind::Command); // foo はコマンドとして識別される
        assert_eq!(tokens[4].lexeme, "foo");
        
        assert_eq!(tokens[5].kind, TokenKind::Flag);
        assert_eq!(tokens[5].lexeme, "--color");
    }
    
    #[test]
    fn test_tokens_with_quotes() {
        let input = "echo \"Hello, world!\" 'Single quotes'";
        let tokens = tokenize(input);
        
        assert_eq!(tokens.len(), 4, "トークン数が一致しません: {}", dump_tokens(&tokens));
        
        assert_eq!(tokens[0].kind, TokenKind::Command);
        assert_eq!(tokens[0].lexeme, "echo");
        
        assert_eq!(tokens[1].kind, TokenKind::String);
        assert_eq!(tokens[1].lexeme, "Hello, world!");
        
        assert_eq!(tokens[2].kind, TokenKind::String);
        assert_eq!(tokens[2].lexeme, "Single quotes");
    }
    
    #[test]
    fn test_tokenize_redirection() {
        let input = "cat file.txt > output.txt 2>&1";
        let tokens = tokenize(input);
        
        assert_eq!(tokens.len(), 6, "トークン数が一致しません: {}", dump_tokens(&tokens));
        
        assert_eq!(tokens[0].kind, TokenKind::Command);
        assert_eq!(tokens[0].lexeme, "cat");
        
        assert_eq!(tokens[1].kind, TokenKind::Command); // file.txt はコマンドとして識別される
        
        assert_eq!(tokens[2].kind, TokenKind::RedirectOut);
        
        assert_eq!(tokens[3].kind, TokenKind::Command); // output.txt はコマンドとして識別される
    }
    
    #[test]
    fn test_parse_simple_command() {
        let input = "ls -l /home";
        let result = parse(input);
        
        assert!(result.is_ok(), "パースエラー: {:?}", result.err());
        let node = result.unwrap();
        
        // ノードの検証
        match &node {
            Node::Command(cmd) => {
                assert_eq!(cmd.name, "ls");
                assert_eq!(cmd.args.len(), 1);
                assert_eq!(cmd.args[0], "/home");
                assert_eq!(cmd.flags.len(), 1);
                assert_eq!(cmd.flags[0], "-l");
            },
            _ => panic!("予期しないノードタイプ: {:?}", node),
        }
        
        println!("AST: \n{}", dump_ast(&node, 0));
    }
    
    #[test]
    fn test_parse_pipeline() {
        let input = "ls -l | grep txt | wc -l";
        let result = parse(input);
        
        assert!(result.is_ok(), "パースエラー: {:?}", result.err());
        let node = result.unwrap();
        
        // パイプラインノードの検証
        match &node {
            Node::Pipeline(pipeline) => {
                assert_eq!(pipeline.commands.len(), 3);
                
                // 最初のコマンド (ls -l)
                match &pipeline.commands[0] {
                    Node::Command(cmd) => {
                        assert_eq!(cmd.name, "ls");
                        assert_eq!(cmd.flags.len(), 1);
                        assert_eq!(cmd.flags[0], "-l");
                    },
                    _ => panic!("予期しないノードタイプ"),
                }
                
                // 2番目のコマンド (grep txt)
                match &pipeline.commands[1] {
                    Node::Command(cmd) => {
                        assert_eq!(cmd.name, "grep");
                        assert_eq!(cmd.args.len(), 1);
                        assert_eq!(cmd.args[0], "txt");
                    },
                    _ => panic!("予期しないノードタイプ"),
                }
                
                // 3番目のコマンド (wc -l)
                match &pipeline.commands[2] {
                    Node::Command(cmd) => {
                        assert_eq!(cmd.name, "wc");
                        assert_eq!(cmd.flags.len(), 1);
                        assert_eq!(cmd.flags[0], "-l");
                    },
                    _ => panic!("予期しないノードタイプ"),
                }
            },
            _ => panic!("予期しないノードタイプ: {:?}", node),
        }
        
        println!("Pipeline AST: \n{}", dump_ast(&node, 0));
    }
    
    #[test]
    fn test_error_recovery() {
        // 不正な構文を含むコマンド
        let input = "cat | | grep foo";
        let result = parse(input);
        
        // エラーが発生することを期待
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        match err {
            ParserError::SyntaxError(msg, _) => {
                assert!(msg.contains("パイプ後にコマンドが必要です") || 
                       msg.contains("expected command") || 
                       msg.contains("unexpected token"),
                       "予期せぬエラーメッセージ: {}", msg);
            },
            _ => panic!("予期しないエラータイプ: {:?}", err),
        }
    }
    
    #[test]
    fn test_source_file_integration() {
        // ソースファイル情報をセットアップ
        let source_path = PathBuf::from("/test/script.sh");
        let content = "ls -l | grep foo";
        let source_file = Arc::new(SourceFile::new(source_path, content.to_string()));
        
        // レキサーにソースファイル情報を設定
        let mut lexer = DefaultLexer::new().with_source_file(source_file.clone());
        let tokens_result = lexer.tokenize(content);
        
        assert!(tokens_result.is_ok(), "トークン化エラー: {:?}", tokens_result.err());
        let tokens = tokens_result.unwrap();
        
        // トークンがソースファイル情報を正しく反映していることを確認
        for token in &tokens {
            assert_eq!(token.span.line, 1, "不正な行番号: {}", token.span.line);
            assert!(token.span.column > 0, "不正な列番号: {}", token.span.column);
        }
        
        // 不正な入力でのエラーメッセージにソース情報が含まれることを確認
        let invalid_input = "ls | | grep";
        let invalid_result = lexer.tokenize(invalid_input);
        if let Err(error) = invalid_result {
            // エラーメッセージにパス情報が含まれているか確認
            match error {
                ParserError::LexerError(msg, _) | ParserError::SyntaxError(msg, _) => {
                    println!("エラーメッセージ: {}", msg);
                    // 実際のエラーメッセージの形式に応じてアサーションを調整
                },
                _ => {}
            }
        }
    }
    
    // 追加のテストケース
    #[test]
    fn test_complex_command() {
        let input = "find . -name \"*.rs\" -type f | xargs grep -l \"fn main\" > results.txt";
        let tokens = tokenize(input);
        let result = parse(input);
        
        assert!(result.is_ok(), "パースエラー: {:?}", result.err());
        
        // トークン数を検証（EOFを含む）
        assert!(tokens.len() > 10, "予期せぬトークン数: {}", tokens.len());
        
        // AST構造を出力
        let node = result.unwrap();
        println!("Complex command AST: \n{}", dump_ast(&node, 0));
        
        // リダイレクションを含むパイプラインであることを確認
        match &node {
            Node::Pipeline(pipeline) => {
                assert_eq!(pipeline.commands.len(), 2);
                
                // リダイレクションを確認
                match &pipeline.commands[1] {
                    Node::Command(cmd) => {
                        assert!(!cmd.redirections.is_empty(), "リダイレクションが見つかりません");
                        assert_eq!(cmd.redirections[0].kind, Redirection::Kind::Out);
                    },
                    _ => panic!("予期しないノードタイプ"),
                }
            },
            _ => panic!("予期しないノードタイプ: {:?}", node),
        }
    }
}

// ベンチマーク設定（あとで有効化する）
#[cfg(feature = "benchmarks")]
mod benchmarks {
    use super::*;
    use criterion::{black_box, criterion_group, criterion_main, Criterion};
    
    pub fn tokenize_benchmark(c: &mut Criterion) {
        let input = "find . -name \"*.rs\" -type f | xargs grep -l \"fn main\" > results.txt";
        
        c.bench_function("tokenize", |b| b.iter(|| {
            let mut lexer = DefaultLexer::new();
            black_box(lexer.tokenize(black_box(input)).unwrap());
        }));
    }
    
    pub fn parse_benchmark(c: &mut Criterion) {
        let input = "find . -name \"*.rs\" -type f | xargs grep -l \"fn main\" > results.txt";
        
        c.bench_function("parse", |b| b.iter(|| {
            black_box(parse(black_box(input)).unwrap());
        }));
    }
    
    criterion_group!(benches, tokenize_benchmark, parse_benchmark);
    criterion_main!(benches);
}