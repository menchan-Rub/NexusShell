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
use std::path::PathBuf;
use std::io::{self, Write};
use clap::{App, Arg, SubCommand};
use colored::Colorize;
use rustyline::{Editor, Config};
use rustyline::error::ReadlineError;
use rustyline::hint::HistoryHinter;

// コアモジュールのインポート
use nexusshell_executor::Executor;
use nexusshell_runtime::{Runtime, RuntimeOptions, ShellState, ExecutionResult};
use ui::NexusTerminal;

// 各モジュールをインポート
mod shell;
mod config;
mod prompt;
mod completion;
mod history;
mod plugins;
mod themes;
mod builtins;
mod utils;

use shell::{Shell, ShellOptions};
use config::ShellConfig;
use prompt::Prompt;
use history::HistoryManager;
use completion::Completer;
use plugins::PluginManager;
use themes::ThemeManager;

/// アプリケーションのバージョン
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// アプリケーションの名前
const APP_NAME: &str = env!("CARGO_PKG_NAME");
/// アプリケーションの説明
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
/// アプリケーションの著者
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

/// メインエントリーポイント
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // コマンドライン引数の解析
    let matches = App::new(APP_NAME)
        .version(VERSION)
        .author(AUTHORS)
        .about(DESCRIPTION)
        .arg(Arg::with_name("config")
            .short("c")
            .long("config")
            .value_name("FILE")
            .help("設定ファイルのパスを指定します")
            .takes_value(true))
        .arg(Arg::with_name("interactive")
            .short("i")
            .long("interactive")
            .help("対話モードで起動します（デフォルト）"))
        .arg(Arg::with_name("login")
            .short("l")
            .long("login")
            .help("ログインシェルとして起動します"))
        .arg(Arg::with_name("noprofile")
            .long("noprofile")
            .help("プロファイルファイルを読み込みません"))
        .arg(Arg::with_name("norc")
            .long("norc")
            .help("rcファイルを読み込みません"))
        .arg(Arg::with_name("command")
            .short("c")
            .long("command")
            .value_name("COMMAND")
            .help("指定されたコマンドを実行します")
            .takes_value(true))
        .arg(Arg::with_name("debug")
            .short("d")
            .long("debug")
            .help("デバッグモードで起動します"))
        .arg(Arg::with_name("SCRIPT")
            .help("実行するスクリプトファイル")
            .index(1))
        .get_matches();

    // 設定ファイルのパスを取得
    let config_path = matches.value_of("config").map(PathBuf::from).or_else(|| {
        let mut path = dirs::config_dir()?;
        path.push(APP_NAME);
        path.push("config.toml");
        Some(path)
    });

    // デバッグモードかどうか
    let debug_mode = matches.is_present("debug");

    // ログイン設定
    let is_login = matches.is_present("login");
    let no_profile = matches.is_present("noprofile");
    let no_rc = matches.is_present("norc");

    // 対話モードかどうか
    let is_interactive = matches.is_present("interactive") || 
                         (!matches.is_present("command") && matches.value_of("SCRIPT").is_none());

    // コマンドラインオプションで指定されたコマンド
    let command = matches.value_of("command");

    // スクリプトファイル
    let script = matches.value_of("SCRIPT").map(PathBuf::from);

    // シェルの起動
    if debug_mode {
        println!("{} デバッグモードで起動します...", "[DEBUG]".bright_yellow());
    }

    // 設定の読み込み
    let config = match config_path {
        Some(path) if path.exists() => {
            match ShellConfig::load_from_file(&path) {
                Ok(config) => {
                    if debug_mode {
                        println!("{} 設定を読み込みました: {:?}", "[DEBUG]".bright_yellow(), path);
                    }
                    config
                },
                Err(err) => {
                    eprintln!("{} 設定ファイルの読み込みに失敗しました: {}", "エラー:".bright_red(), err);
                    ShellConfig::default()
                }
            }
        },
        _ => {
            if debug_mode {
                println!("{} デフォルト設定を使用します", "[DEBUG]".bright_yellow());
            }
            ShellConfig::default()
        }
    };

    // シェルオプションの設定
    let options = ShellOptions {
        is_interactive,
        is_login,
        no_profile,
        no_rc,
        debug_mode,
    };

    // シェルの初期化
    let mut shell = Shell::new(config, options)?;

    // ログインシェルの場合はプロファイルを読み込む
    if is_login && !no_profile {
        shell.load_login_profile()?;
    }

    // インタラクティブシェルの場合は.rcファイルを読み込む
    if is_interactive && !no_rc {
        shell.load_rc_file()?;
    }

    // シェルの実行
    match (command, script) {
        (Some(cmd), _) => {
            // コマンドを実行
            let exit_code = shell.execute_command(cmd)?;
            exit(exit_code);
        },
        (_, Some(script_path)) => {
            // スクリプトを実行
            let exit_code = shell.execute_script(script_path)?;
            exit(exit_code);
        },
        _ if is_interactive => {
            // 対話モードの実行
            run_interactive_shell(shell)?;
        },
        _ => {
            eprintln!("{} コマンドまたはスクリプトが指定されていません", "エラー:".bright_red());
            exit(1);
        }
    }

    Ok(())
}

/// 対話モードのシェルを実行します
fn run_interactive_shell(mut shell: Shell) -> Result<(), Box<dyn std::error::Error>> {
    // バージョン情報を表示
    println!("{} v{}", APP_NAME.bright_green(), VERSION);
    println!("{}", DESCRIPTION);
    println!("Type {} for help.", "help".bright_blue());

    // ラインエディタの設定
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(rustyline::CompletionType::List)
        .build();

    let mut rl = Editor::<Completer, HistoryHinter>::with_config(config);
    
    // コンプリータの設定
    let completer = Completer::new(shell.get_builtins(), shell.get_aliases());
    rl.set_helper(Some(completer));

    // 履歴ファイルの読み込み
    let history_path = shell.get_history_path();
    if let Some(path) = &history_path {
        if let Err(err) = rl.load_history(path) {
            if shell.is_debug_mode() {
                println!("{} 履歴ファイルの読み込みに失敗しました: {}", "[DEBUG]".bright_yellow(), err);
            }
        }
    }

    // プロンプトマネージャーの取得
    let prompt_manager = shell.get_prompt_manager();

    // メインループ
    loop {
        // プロンプトの取得
        let prompt = prompt_manager.get_prompt();

        // 入力の読み込み
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // 履歴に追加
                rl.add_history_entry(line);

                // 特殊なコマンドを処理
                if line == "exit" || line == "quit" {
                    break;
                }

                // コマンドの実行
                if let Err(err) = shell.execute_command(line) {
                    eprintln!("{} {}", "エラー:".bright_red(), err);
                }
            },
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C
                println!("^C");
                continue;
            },
            Err(ReadlineError::Eof) => {
                // Ctrl-D
                println!("exit");
                break;
            },
            Err(err) => {
                eprintln!("{} 入力の読み込みに失敗しました: {}", "エラー:".bright_red(), err);
                break;
            }
        }
    }

    // 履歴の保存
    if let Some(path) = history_path {
        if let Err(err) = rl.save_history(&path) {
            if shell.is_debug_mode() {
                println!("{} 履歴ファイルの保存に失敗しました: {}", "[DEBUG]".bright_yellow(), err);
            }
        }
    }

    // シェルのクリーンアップ
    shell.cleanup()?;

    println!("さようなら！");
    Ok(())
}
