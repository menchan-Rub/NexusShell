/**
 * NexusShell - 次世代インテリジェントシェル
 * 
 * 高性能、型付き、モジュラー設計のRust製シェル
 */

mod ui;

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::env;
use std::process::exit;
use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use log::{debug, info, error, warn};

// コアモジュールのインポート
use nexusshell_executor::Executor;
use nexusshell_runtime::{Runtime, RuntimeOptions, ShellState, ExecutionResult};
use ui::NexusTerminal;

/// コマンドライン引数
#[derive(Parser, Debug)]
#[clap(
    name = "NexusShell",
    version = env!("CARGO_PKG_VERSION"),
    author = "NexusShell Team",
    about = "次世代インテリジェントシェル",
    long_about = "NexusShellは高性能、型付き、モジュラー設計のRust製シェルです"
)]
struct Cli {
    /// 実行するスクリプトファイル
    #[clap(name = "SCRIPT")]
    script: Option<String>,

    /// スクリプトに渡す引数
    #[clap(name = "ARGS", trailing_var_arg = true)]
    args: Vec<String>,

    /// コマンドを直接実行
    #[clap(short = 'c', long = "command")]
    command: Option<String>,

    /// 履歴ファイルの場所
    #[clap(long = "histfile")]
    histfile: Option<String>,

    /// サンドボックスモードを無効化
    #[clap(long = "no-sandbox")]
    no_sandbox: bool,

    /// プラグインを無効化
    #[clap(long = "no-plugins")]
    no_plugins: bool,

    /// デバッグモード
    #[clap(short = 'd', long = "debug")]
    debug: bool,

    /// 起動時に実行するスクリプト
    #[clap(short = 'r', long = "rcfile")]
    rcfile: Option<String>,

    /// サブコマンド
    #[clap(subcommand)]
    subcmd: Option<SubCommands>,
}

/// サブコマンド
#[derive(Subcommand, Debug)]
enum SubCommands {
    /// 構文チェック
    Check {
        /// チェックするファイル
        #[clap(name = "FILE")]
        file: String,
    },
    /// プラグイン管理
    Plugin {
        /// プラグイン操作
        #[clap(subcommand)]
        action: PluginAction,
    },
}

/// プラグイン操作
#[derive(Subcommand, Debug)]
enum PluginAction {
    /// プラグインをインストール
    Install {
        /// プラグイン名
        name: String,
    },
    /// プラグインを削除
    Remove {
        /// プラグイン名
        name: String,
    },
    /// プラグイン一覧を表示
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    // ロギングの初期化
    env_logger::init();
    info!("NexusShell を起動しています...");
    
    // コマンドライン引数の解析
    let cli = Cli::parse();
    
    // ランタイムオプションの設定
    let options = RuntimeOptions {
        sandbox_mode: !cli.no_sandbox,
        enable_plugins: !cli.no_plugins,
        debug_mode: cli.debug,
        ..Default::default()
    };
    
    // ランタイムとエグゼキュータの初期化
    let runtime = match Runtime::with_options(options) {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            error!("ランタイムの初期化に失敗しました: {}", e);
            return Err(anyhow::anyhow!("ランタイムの初期化に失敗しました: {}", e));
        }
    };
    
    let executor = Arc::new(Executor::new());
    
    // RC ファイルの実行
    let rcfile = cli.rcfile.unwrap_or_else(|| {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".nexusshellrc").to_string_lossy().to_string()
    });
    
    if Path::new(&rcfile).exists() {
        info!("RCファイルを実行中: {}", rcfile);
        match runtime.execute_script(Path::new(&rcfile)).await {
            Ok(_) => {},
            Err(e) => warn!("RCファイルの実行中にエラーが発生しました: {}", e),
        }
    }
    
    // サブコマンド処理
    if let Some(subcmd) = cli.subcmd {
        match subcmd {
            SubCommands::Check { file } => {
                return check_syntax(&file, &runtime).await;
            },
            SubCommands::Plugin { action } => {
                return handle_plugin_action(action, &runtime).await;
            },
        }
    }
    
    // コマンド実行モード
    if let Some(cmd) = cli.command {
        return execute_command(&cmd, &runtime).await;
    }
    
    // スクリプト実行モード
    if let Some(script) = cli.script {
        return execute_script(&script, &cli.args, &runtime).await;
    }
    
    // インタラクティブモード
    run_interactive(runtime, executor).await
}

/// 構文チェック
async fn check_syntax(file: &str, runtime: &Arc<Runtime>) -> Result<()> {
    info!("構文チェック: {}", file);
    
    let path = Path::new(file);
    if !path.exists() {
        error!("ファイルが存在しません: {}", file);
        return Err(anyhow::anyhow!("ファイルが存在しません: {}", file));
    }
    
    match runtime.get_evaluation_engine().check_syntax_file(path).await {
        Ok(issues) => {
            if issues.is_empty() {
                println!("構文チェックに成功しました: {}", file);
                Ok(())
            } else {
                for issue in &issues {
                    println!("{}: {}", issue.level, issue.message);
                    if let Some(line) = issue.line {
                        println!("  行 {}", line);
                    }
                }
                Err(anyhow::anyhow!("構文エラーが見つかりました: {} 件", issues.len()))
            }
        },
        Err(e) => {
            error!("構文チェック中にエラーが発生しました: {}", e);
            Err(anyhow::anyhow!("構文チェック中にエラーが発生しました: {}", e))
        }
    }
}

/// プラグイン操作処理
async fn handle_plugin_action(action: PluginAction, runtime: &Arc<Runtime>) -> Result<()> {
    let plugin_manager = runtime.get_plugin_manager();
    
    match action {
        PluginAction::Install { name } => {
            info!("プラグインをインストール中: {}", name);
            plugin_manager.install(&name).await?;
            println!("プラグイン '{}' をインストールしました", name);
        },
        PluginAction::Remove { name } => {
            info!("プラグインを削除中: {}", name);
            plugin_manager.uninstall(&name).await?;
            println!("プラグイン '{}' を削除しました", name);
        },
        PluginAction::List => {
            let plugins = plugin_manager.list_plugins().await?;
            println!("インストール済みプラグイン:");
            for plugin in plugins {
                println!("- {} (v{}) - {}", 
                         plugin.name, 
                         plugin.version, 
                         plugin.description.unwrap_or_default());
            }
        },
    }
    
    Ok(())
}

/// コマンド実行モード
async fn execute_command(cmd: &str, runtime: &Arc<Runtime>) -> Result<()> {
    info!("コマンド実行モード: {}", cmd);
    
    match runtime.execute_command(cmd).await {
        Ok(result) => {
            if !result.success {
                exit(result.exit_code.unwrap_or(1));
            }
            Ok(())
        },
        Err(e) => {
            error!("コマンド実行エラー: {}", e);
            exit(1);
        }
    }
}

/// スクリプト実行モード
async fn execute_script(script: &str, args: &[String], runtime: &Arc<Runtime>) -> Result<()> {
    info!("スクリプト実行モード: {}", script);
    
    // スクリプト引数を環境にセット
    let env = runtime.get_environment();
    env.set("NEXUS_SCRIPT", script).await?;
    
    for (i, arg) in args.iter().enumerate() {
        env.set(&format!("NEXUS_ARG{}", i), arg).await?;
    }
    env.set("NEXUS_ARGC", &args.len().to_string()).await?;
    
    // スクリプト実行
    match runtime.execute_script(Path::new(script)).await {
        Ok(result) => {
            if !result.success {
                exit(result.exit_code.unwrap_or(1));
            }
            Ok(())
        },
        Err(e) => {
            error!("スクリプト実行エラー: {}", e);
            exit(1);
        }
    }
}

/// インタラクティブモードを実行
async fn run_interactive(runtime: Arc<Runtime>, executor: Arc<Executor>) -> Result<()> {
    info!("インタラクティブモードを開始します");
    
    // 実行結果チャンネル
    let (tx, mut rx) = mpsc::channel::<ExecutionResult>(100);
    
    // ターミナルUIの初期化
    let mut terminal = match NexusTerminal::new() {
        Ok(term) => term,
        Err(e) => {
            error!("ターミナルの初期化に失敗しました: {}", e);
            return Err(anyhow::anyhow!("ターミナルの初期化に失敗しました: {}", e));
        }
    };
    
    // ランタイムとエグゼキュータをUIに登録
    terminal.register_runtime(runtime.clone());
    terminal.register_executor(executor.clone());
    terminal.register_result_channel(tx);
    
    // 受信リスナーを起動
    let result_handler = tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            debug!("コマンド実行結果: exit_code={:?}, success={}", 
                   result.exit_code, result.success);
            
            // ここで結果に応じた処理（シェル状態の更新など）
            let mut state = runtime.get_shell_state().await;
            if let Some(code) = result.exit_code {
                state.last_status = code;
            }
            if let Err(e) = runtime.set_shell_state(state).await {
                error!("シェル状態の更新に失敗しました: {}", e);
            }
        }
    });
    
    // UIのメインループを実行
    terminal.run()?;
    
    // リスナーを終了
    result_handler.abort();
    
    info!("インタラクティブモードを終了します");
    Ok(())
}
