/*!
# コア組み込みコマンド

このモジュールは、シェルの基本的な機能を提供する組み込みコマンドを実装しています。
これらのコマンドはシェルの動作に不可欠であり、外部プロセスとして実行するのではなく
シェル自体に組み込まれています。

## 含まれるコマンド

- `cd`: ディレクトリを変更
- `pwd`: 現在の作業ディレクトリを表示
- `echo`: テキストを出力
- `exit`: シェルを終了
- `export`: 環境変数を設定
- `alias`: コマンドのエイリアスを定義
- `history`: コマンド履歴を表示および管理
- `source`: スクリプトファイルを実行

これらのコマンドは全てのシェルセッションで常に利用可能です。
*/

// 各コマンドをサブモジュールとしてエクスポート
pub mod cd;
pub mod pwd;
pub mod echo;
pub mod exit;
pub mod export;
pub mod env;
pub mod alias;
pub mod history;
pub mod source;
pub mod set;
pub mod unalias;
pub mod jobs;
pub mod help;
pub mod type_cmd;
pub mod builtin;
pub mod unset;
pub mod which;
pub mod test;

pub use cd::CdCommand;
pub use echo::EchoCommand;
pub use exit::ExitCommand;
pub use pwd::PwdCommand;
pub use export::ExportCommand;
pub use env::EnvCommand;

/// コアコマンドのバージョン
pub const CORE_COMMANDS_VERSION: &str = "0.1.0";

/// コアコマンドの説明
pub const CORE_COMMANDS_DESCRIPTION: &str = "NexusShellのコア組み込みコマンド";

/// コマンドのショートカット
pub struct CommandShortcut {
    /// コマンド名
    pub name: &'static str,
    /// ショートカット
    pub shortcut: &'static str,
    /// 説明
    pub description: &'static str,
}

/// 利用可能なコマンドショートカット
pub const COMMAND_SHORTCUTS: &[CommandShortcut] = &[
    CommandShortcut {
        name: "cd",
        shortcut: "..",
        description: "親ディレクトリに移動",
    },
    CommandShortcut {
        name: "cd",
        shortcut: "-",
        description: "前のディレクトリに移動",
    },
];

use crate::BuiltinCommand;
use crate::registry::CommandRegistry;
use anyhow::Result;

/// コア組み込みコマンドを登録
pub fn register_core_commands(registry: &mut CommandRegistry) -> Result<()> {
    // 既存のコマンド
    registry.register(Box::new(set::SetCommand::new()));
    registry.register(Box::new(jobs::JobsCommand::new()));
    registry.register(Box::new(source::SourceCommand::new()));
    
    // 新しいコマンド
    registry.register(Box::new(cd::CdCommand::new()));
    registry.register(Box::new(echo::EchoCommand::new()));
    registry.register(Box::new(exit::ExitCommand::new()));
    registry.register(Box::new(export::ExportCommand::new()));
    registry.register(Box::new(pwd::PwdCommand::new()));
    registry.register(Box::new(unset::UnsetCommand::new()));
    registry.register(Box::new(history::HistoryCommand::new()));
    registry.register(Box::new(alias::AliasCommand::new()));
    registry.register(Box::new(help::HelpCommand::new()));
    registry.register(Box::new(type_cmd::TypeCommand::new()));
    registry.register(Box::new(which::WhichCommand::new()));
    registry.register(Box::new(test::TestCommand::new()));
    
    Ok(())
} 