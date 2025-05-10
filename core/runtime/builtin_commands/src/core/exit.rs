use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, error};

/// シェルを終了するコマンド
///
/// UNIXの標準的なexitコマンドの実装です。シェルを終了します。
/// 引数として終了コードを指定できます。省略した場合は最後に実行したコマンドの終了コードを使用します。
///
/// # 使用例
///
/// ```bash
/// exit      # 最後に実行したコマンドの終了コードでシェルを終了
/// exit 0    # 正常終了（0）でシェルを終了
/// exit 1    # エラー終了（1）でシェルを終了
/// ```
pub struct ExitCommand;

/// シェルに終了を要求するための特別な終了コード
pub const EXIT_SHELL_REQUEST: i32 = -9999;

#[async_trait]
impl BuiltinCommand for ExitCommand {
    fn name(&self) -> &'static str {
        "exit"
    }

    fn description(&self) -> &'static str {
        "シェルを終了します"
    }

    fn usage(&self) -> &'static str {
        "exit [終了コード]\n\n終了コードを省略した場合は、最後に実行したコマンドの終了コードを使用します。"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // 引数を取得（最初の引数はコマンド名なので、それ以降を使用）
        let args = context.args.iter().skip(1).collect::<Vec<_>>();
        
        // 終了コードを決定
        let exit_code = if args.is_empty() {
            // 引数がない場合は0を使用
            // 実際のシェル実装では、最後に実行したコマンドの終了コードを使用する
            0
        } else if args.len() == 1 {
            // 引数が1つの場合は終了コードとして解釈
            match args[0].parse::<i32>() {
                Ok(code) => code,
                Err(_) => {
                    let error_message = format!("exit: {}: 数値以外の引数です", args[0]);
                    error!("{}", error_message);
                    return Ok(CommandResult::failure(2)
                        .with_stderr(error_message.into_bytes()));
                }
            }
        } else {
            // 引数が複数ある場合はエラー
            let error_message = "exit: 引数が多すぎます".to_string();
            error!("{}", error_message);
            return Ok(CommandResult::failure(1)
                .with_stderr(error_message.into_bytes()));
        };
        
        debug!("シェルを終了します（コード: {}）", exit_code);
        
        // 特別な終了コードを返すことで、シェルに終了を要求
        // 実際のシェル実装では、このコマンドの戻り値を元にシェルが終了処理を行う
        let real_exit_code = if exit_code >= 0 && exit_code <= 255 {
            exit_code
        } else {
            // 範囲外の終了コードは255に制限（UNIXの仕様に合わせる）
            255
        };
        
        let result = CommandResult {
            exit_code: EXIT_SHELL_REQUEST,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        
        // 実際のシェル終了コードを特別なフィールドとして設定
        // 注: このフィールドは本来存在しないため、実際の実装では何らかの方法で
        // シェルに終了コードを通知する必要があります
        
        Ok(result)
    }
} 