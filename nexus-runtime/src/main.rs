use clap::{Parser, Subcommand};
use log::{error, info};
use std::path::PathBuf;

mod runtime;
mod oci;
mod spec;

use runtime::NexusRuntime;

#[derive(Parser)]
#[command(name = "nexus-runtime")]
#[command(about = "NexusContainer OCI Runtime")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// ランタイムルートディレクトリ
    #[arg(long = "root", default_value = "/run/nexus-runtime")]
    root: PathBuf,
    
    /// ログレベル
    #[arg(long = "log-level", default_value = "info")]
    log_level: String,
    
    /// systemd cgroup を使用
    #[arg(long = "systemd-cgroup")]
    systemd_cgroup: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// コンテナを作成
    Create {
        /// コンテナID
        container_id: String,
        /// バンドルディレクトリ
        bundle: PathBuf,
    },
    /// コンテナを開始
    Start {
        /// コンテナID
        container_id: String,
    },
    /// コンテナを停止
    Kill {
        /// コンテナID
        container_id: String,
        /// シグナル
        #[arg(default_value = "TERM")]
        signal: String,
    },
    /// コンテナを削除
    Delete {
        /// コンテナID
        container_id: String,
        /// 強制削除
        #[arg(short, long)]
        force: bool,
    },
    /// コンテナ状態を取得
    State {
        /// コンテナID
        container_id: String,
    },
    /// 実行中のコンテナ一覧
    List,
    /// コンテナ内でプロセスを実行
    Exec {
        /// コンテナID
        container_id: String,
        /// プロセス設定ファイル
        #[arg(long = "process")]
        process: Option<PathBuf>,
        /// 標準入力をアタッチ
        #[arg(short = 'i', long = "stdin")]
        stdin: bool,
        /// TTYを割り当て
        #[arg(short = 't', long = "tty")]
        tty: bool,
        /// 実行するコマンド
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    
    // ログ初期化
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&cli.log_level)
    )
    .format_timestamp_secs()
    .init();
    
    info!("NexusRuntime v0.1.0 starting");
    
    // ランタイム初期化
    let mut runtime = match NexusRuntime::new(cli.root, cli.systemd_cgroup) {
        Ok(runtime) => runtime,
        Err(e) => {
            error!("Failed to initialize runtime: {}", e);
            std::process::exit(1);
        }
    };
    
    let result = match cli.command {
        Commands::Create { container_id, bundle } => {
            runtime.create(&container_id, &bundle, None, None)
        }
        Commands::Start { container_id } => {
            runtime.start(&container_id)
        }
        Commands::Kill { container_id, signal } => {
            runtime.kill(&container_id, &signal, false)
        }
        Commands::Delete { container_id, force } => {
            runtime.delete(&container_id, force)
        }
        Commands::State { container_id } => {
            match runtime.state(&container_id) {
                Ok(state) => {
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::List => {
            runtime.list("json", false)
        }
        Commands::Exec { container_id, process, stdin, tty, args } => {
            runtime.exec(&container_id, process.as_deref(), stdin, tty, None, None, &args)
        }
    };
    
    if let Err(e) = result {
        error!("Runtime error: {}", e);
        std::process::exit(1);
    }
} 