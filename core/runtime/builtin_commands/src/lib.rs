/*!
# NexusShell 組み込みコマンドライブラリ

このクレートはNexusShellに含まれる組み込みコマンドの実装を提供します。
これらのコマンドはシェルに直接組み込まれており、外部プロセスを起動することなく
実行されるため、高速で効率的です。

## 主な特徴

- 30以上の標準的なUNIXコマンドの組み込み実装
- 拡張可能なプラグインシステム
- 非同期実行のサポート
- 高度なエラー処理とレポート機能
- クロスプラットフォーム互換性

## モジュール構成

- `core`: 基本的なコマンド実装（cd, pwd, echo など）
- `fs`: ファイルシステム操作コマンド（ls, cp, mv など）
- `text`: テキスト処理コマンド（cat, grep, sed など）
- `network`: ネットワーク関連コマンド（curl, wget など）
- `process`: プロセス管理コマンド（ps, kill など）
- `system`: システム情報コマンド（uname, uptime など）
- `security`: セキュリティ関連コマンド（chmod, chown など）
- `plugin`: プラグイン管理コマンド

*/

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn, error};

/// コマンド実行時のコンテキスト情報
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// 現在の作業ディレクトリ
    pub current_dir: PathBuf,
    /// 環境変数
    pub env_vars: HashMap<String, String>,
    /// コマンドライン引数
    pub args: Vec<String>,
    /// 標準入力が接続されているかどうか
    pub stdin_connected: bool,
    /// 標準出力が接続されているかどうか
    pub stdout_connected: bool,
    /// 標準エラーが接続されているかどうか
    pub stderr_connected: bool,
}

/// コマンド実行の結果
#[derive(Debug)]
pub struct CommandResult {
    /// 終了コード
    pub exit_code: i32,
    /// 標準出力
    pub stdout: Vec<u8>,
    /// 標準エラー
    pub stderr: Vec<u8>,
}

impl CommandResult {
    /// 成功した結果を作成
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// 失敗した結果を作成
    pub fn failure(exit_code: i32) -> Self {
        Self {
            exit_code,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// 標準出力にデータを追加
    pub fn with_stdout(mut self, data: Vec<u8>) -> Self {
        self.stdout = data;
        self
    }

    /// 標準エラーにデータを追加
    pub fn with_stderr(mut self, data: Vec<u8>) -> Self {
        self.stderr = data;
        self
    }
}

/// ビルトインコマンドの特性
#[async_trait]
pub trait BuiltinCommand: Send + Sync {
    /// コマンド名を取得
    fn name(&self) -> &'static str;

    /// コマンドの短い説明を取得
    fn description(&self) -> &'static str;

    /// 使用方法の例を取得
    fn usage(&self) -> &'static str;

    /// コマンドを実行
    async fn execute(&self, context: CommandContext) -> Result<CommandResult>;
}

// 各コマンドカテゴリのモジュールをエクスポート
pub mod core;
pub mod fs;
pub mod text;
pub mod network;
pub mod process;
pub mod system;
pub mod security;
pub mod plugin;

// レジストリモジュール
pub mod registry;

// 各コマンドの実装をエクスポート
pub use registry::CommandRegistry;

/// 全ての組み込みコマンドを含むレジストリを作成
pub fn create_default_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    
    // コアコマンド
    registry.register(Box::new(core::cd::CdCommand));
    registry.register(Box::new(core::pwd::PwdCommand));
    registry.register(Box::new(core::echo::EchoCommand));
    registry.register(Box::new(core::exit::ExitCommand));
    registry.register(Box::new(core::export::ExportCommand));
    
    // 既存コマンド（将来的に実装予定）
    // registry.register(Box::new(core::alias::AliasCommand));
    // registry.register(Box::new(core::history::HistoryCommand));
    // registry.register(Box::new(core::source::SourceCommand));
    
    // ファイルシステムコマンド
    registry.register(Box::new(fs::ls::LsCommand));
    registry.register(Box::new(fs::find::FindCommand));
    registry.register(Box::new(fs::cp::CpCommand));
    registry.register(Box::new(fs::mv::MvCommand));
    registry.register(Box::new(fs::rm::RmCommand));
    registry.register(Box::new(fs::mkdir::MkdirCommand));
    registry.register(Box::new(fs::touch::TouchCommand));
    
    // テキスト処理コマンド
    registry.register(Box::new(text::map::MapCommand));
    registry.register(Box::new(text::stats::StatsCommand));
    registry.register(Box::new(text::slice::SliceCommand));
    registry.register(Box::new(text::distinct::DistinctCommand));
    registry.register(Box::new(text::cat::CatCommand));
    registry.register(Box::new(text::grep::GrepCommand));
    registry.register(Box::new(text::sort::SortCommand));
    registry.register(Box::new(text::head::HeadCommand));
    registry.register(Box::new(text::tail::TailCommand));
    registry.register(Box::new(text::wc::WcCommand));
    registry.register(Box::new(text::sed::SedCommand));
    registry.register(Box::new(text::uniq::UniqCommand));
    registry.register(Box::new(text::cut::CutCommand));
    registry.register(Box::new(text::tr::TrCommand));
    
    // ネットワークコマンド
    registry.register(Box::new(network::curl::CurlCommand));
    registry.register(Box::new(network::httpserver::HttpServerCommand));
    // registry.register(Box::new(network::wget::WgetCommand));
    // registry.register(Box::new(network::ping::PingCommand));
    // registry.register(Box::new(network::ssh::SshCommand));
    
    // プロセス管理コマンド（将来的に実装予定）
    registry.register(Box::new(system::ps::PsCommand));
    // registry.register(Box::new(process::kill::KillCommand));
    // registry.register(Box::new(process::jobs::JobsCommand));
    // registry.register(Box::new(process::fg::FgCommand));
    // registry.register(Box::new(process::bg::BgCommand));
    
    // システム情報コマンド（将来的に実装予定）
    // registry.register(Box::new(system::uname::UnameCommand));
    // registry.register(Box::new(system::uptime::UptimeCommand));
    // registry.register(Box::new(system::df::DfCommand));
    // registry.register(Box::new(system::free::FreeCommand));
    
    // セキュリティコマンド（将来的に実装予定）
    // registry.register(Box::new(security::chmod::ChmodCommand));
    // registry.register(Box::new(security::chown::ChownCommand));
    // registry.register(Box::new(security::passwd::PasswdCommand));
    
    // プラグイン管理コマンド（将来的に実装予定）
    // registry.register(Box::new(plugin::plugin::PluginCommand));
    
    registry
} 