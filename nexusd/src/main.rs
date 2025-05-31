#![recursion_limit = "256"]

use clap::{Parser, Subcommand};
use log::{debug, error, info, warn};
use std::path::PathBuf;
use std::process;

#[cfg(unix)]
use tokio::signal;

mod daemon;
mod grpc;
mod http;
mod api;
mod container_manager;
mod image_manager;
mod volume_manager;
mod network_manager;
mod event_manager;
mod metrics;
mod config;
mod generated;

use crate::daemon::NexusDaemon;
use crate::config::DaemonConfig;

#[derive(Parser)]
#[command(name = "nexusd")]
#[command(about = "NexusContainer daemon for container orchestration")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 設定ファイルのパス
    #[arg(long = "config", short = 'c')]
    config_file: Option<PathBuf>,

    /// データディレクトリ
    #[arg(long = "data-root", default_value = "/var/lib/nexusd")]
    data_root: PathBuf,

    /// gRPCサーバーのリスンアドレス
    #[arg(long = "grpc-listen", default_value = "127.0.0.1:7890")]
    grpc_listen: String,

    /// HTTPサーバーのリスンアドレス
    #[arg(long = "http-listen", default_value = "127.0.0.1:7891")]
    http_listen: String,

    /// Unixソケットパス
    #[arg(long = "unix-socket", default_value = "/var/run/nexusd.sock")]
    unix_socket: PathBuf,

    /// ログレベルの設定
    #[arg(long = "log-level", default_value = "info")]
    log_level: String,

    /// デーモンとして起動
    #[arg(long = "daemon", short = 'd')]
    daemon: bool,

    /// PIDファイルのパス
    #[arg(long = "pid-file")]
    pid_file: Option<PathBuf>,

    /// systemdによる管理
    #[arg(long = "systemd")]
    systemd: bool,

    /// デバッグモード
    #[arg(long = "debug")]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// デーモンを起動
    Start {
        /// フォアグラウンドで実行
        #[arg(long = "foreground")]
        foreground: bool,
    },
    
    /// デーモンを停止
    Stop {
        /// 強制停止
        #[arg(long = "force")]
        force: bool,
    },
    
    /// デーモンを再起動
    Restart {
        /// フォアグラウンドで実行
        #[arg(long = "foreground")]
        foreground: bool,
    },
    
    /// デーモンの状態を確認
    Status,
    
    /// 設定を検証
    ValidateConfig,
    
    /// バージョン情報を表示
    Version,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    
    // ログレベルの初期化
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&cli.log_level)
    ).init();
    
    info!("NexusContainer Daemon v0.1.0 starting");
    
    // 設定の読み込み
    let config = match load_config(&cli).await {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            process::exit(1);
        }
    };
    
    // デバッグモードの設定
    if cli.debug {
        debug!("Debug mode enabled");
        debug!("Configuration: {:#?}", config);
    }
    
    // コマンドの処理
    match cli.command {
        Some(Commands::Start { foreground }) => {
            start_daemon(config, !foreground || cli.daemon).await;
        }
        Some(Commands::Stop { force }) => {
            stop_daemon(force).await;
        }
        Some(Commands::Restart { foreground }) => {
            stop_daemon(false).await;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            start_daemon(config, !foreground || cli.daemon).await;
        }
        Some(Commands::Status) => {
            show_status().await;
        }
        Some(Commands::ValidateConfig) => {
            validate_config(&config).await;
        }
        Some(Commands::Version) => {
            show_version();
        }
        None => {
            // デフォルトはstart
            start_daemon(config, cli.daemon).await;
        }
    }
}

async fn load_config(cli: &Cli) -> anyhow::Result<DaemonConfig> {
    let mut config = if let Some(config_file) = &cli.config_file {
        DaemonConfig::load_from_file(config_file).await?
    } else {
        DaemonConfig::default()
    };
    
    // CLI引数で設定を上書き
    config.data_root = cli.data_root.clone();
    config.grpc_listen = cli.grpc_listen.clone();
    config.http_listen = cli.http_listen.clone();
    config.unix_socket = cli.unix_socket.clone();
    config.systemd = cli.systemd;
    config.debug = cli.debug;
    
    if let Some(pid_file) = &cli.pid_file {
        config.pid_file = Some(pid_file.clone());
    }
    
    Ok(config)
}

async fn start_daemon(config: DaemonConfig, as_daemon: bool) {
    info!("Starting NexusContainer daemon");
    
    // デーモン化
    if as_daemon && !config.systemd {
        match daemonize_process(&config) {
            Ok(_) => {
                info!("Successfully daemonized");
            }
            Err(e) => {
                error!("Failed to daemonize: {}", e);
                process::exit(1);
            }
        }
    }
    
    // PIDファイルの作成
    if let Some(ref pid_file) = config.pid_file {
        if let Err(e) = create_pid_file(pid_file) {
            error!("Failed to create PID file: {}", e);
            process::exit(1);
        }
    }
    
    // デーモンの初期化と実行
    let daemon = match NexusDaemon::new(config).await {
        Ok(daemon) => daemon,
        Err(e) => {
            error!("Failed to initialize daemon: {}", e);
            process::exit(1);
        }
    };
    
    // シグナルハンドラーの設定
    let daemon_clone = daemon.clone();
    tokio::spawn(async move {
        if let Err(e) = setup_signal_handlers(daemon_clone).await {
            error!("Signal handler error: {}", e);
        }
    });
    
    // デーモンの実行
    if let Err(e) = daemon.run().await {
        error!("Daemon execution failed: {}", e);
        process::exit(1);
    }
    
    info!("NexusContainer daemon shutting down");
}

async fn stop_daemon(_force: bool) {
    info!("Stopping NexusContainer daemon");
    
    // PIDファイルから実行中のデーモンを特定
    let pid_file = PathBuf::from("/var/run/nexusd.pid");
    
    if !pid_file.exists() {
        warn!("PID file not found, daemon may not be running");
        return;
    }
    
    match std::fs::read_to_string(&pid_file) {
        Ok(pid_str) => {
            if let Ok(_pid) = pid_str.trim().parse::<i32>() {
                #[cfg(unix)]
                {
                    let signal = if _force {
                        nix::sys::signal::SIGKILL
                    } else {
                        nix::sys::signal::SIGTERM
                    };
                    
                    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal) {
                        Ok(_) => {
                            info!("Signal sent to daemon process");
                            
                            // PIDファイルの削除
                            if let Err(e) = std::fs::remove_file(&pid_file) {
                                warn!("Failed to remove PID file: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Failed to send signal to daemon: {}", e);
                        }
                    }
                }
                
                #[cfg(not(unix))]
                {
                    warn!("Signal-based process termination is not supported on this platform");
                    warn!("Please stop the daemon manually");
                }
            } else {
                error!("Invalid PID in PID file");
            }
        }
        Err(e) => {
            error!("Failed to read PID file: {}", e);
        }
    }
}

async fn show_status() {
    println!("NexusContainer Daemon Status");
    println!("============================");
    
    let pid_file = PathBuf::from("/var/run/nexusd.pid");
    
    if pid_file.exists() {
        match std::fs::read_to_string(&pid_file) {
            Ok(pid_str) => {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    // プロセスが実行中かチェック
                    #[cfg(unix)]
                    {
                        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None) {
                            Ok(_) => {
                                println!("Status: Running");
                                println!("PID: {}", pid);
                            }
                            Err(_) => {
                                println!("Status: Not running (stale PID file)");
                            }
                        }
                    }
                    
                    #[cfg(not(unix))]
                    {
                        println!("Status: Unknown (process check not supported on this platform)");
                        println!("PID: {}", pid);
                    }
                } else {
                    println!("Status: Unknown (invalid PID file)");
                }
            }
            Err(_) => {
                println!("Status: Unknown (cannot read PID file)");
            }
        }
    } else {
        println!("Status: Not running");
    }
}

async fn validate_config(config: &DaemonConfig) {
    println!("Validating configuration...");
    
    match config.validate() {
        Ok(warnings) => {
            println!("✓ Configuration is valid");
            
            if !warnings.is_empty() {
                println!("\nWarnings:");
                for warning in warnings {
                    println!("  ⚠ {}", warning);
                }
            }
        }
        Err(errors) => {
            println!("✗ Configuration validation failed:");
            for error in errors {
                println!("  ✗ {}", error);
            }
            process::exit(1);
        }
    }
}

fn show_version() {
    println!("NexusContainer Daemon v0.1.0");
    println!("Build information:");
    println!("  Package: {}", env!("CARGO_PKG_NAME"));
    println!("  Version: {}", env!("CARGO_PKG_VERSION"));
    println!("  Authors: {}", env!("CARGO_PKG_AUTHORS"));
}

async fn setup_signal_handlers(daemon: NexusDaemon) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
        let mut sighup = signal::unix::signal(signal::unix::SignalKind::hangup())?;
        
        loop {
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down gracefully");
                    daemon.shutdown().await?;
                    break;
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, shutting down gracefully");
                    daemon.shutdown().await?;
                    break;
                }
                _ = sighup.recv() => {
                    info!("Received SIGHUP, reloading configuration");
                    if let Err(e) = daemon.reload_config().await {
                        error!("Failed to reload configuration: {}", e);
                    }
                }
            }
        }
    }
    
    #[cfg(not(unix))]
    {
        // Windows環境では基本的なCtrl+Cハンドリングのみ
        tokio::signal::ctrl_c().await?;
        info!("Received Ctrl+C, shutting down gracefully");
        daemon.shutdown().await?;
    }
    
    Ok(())
}

fn daemonize_process(_config: &DaemonConfig) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use daemonize::Daemonize;
        
        let mut daemonize = Daemonize::new();
        
        if let Some(ref pid_file) = _config.pid_file {
            daemonize = daemonize.pid_file(pid_file);
        }
        
        // 作業ディレクトリの設定
        daemonize = daemonize.working_directory(&_config.data_root);
        
        // ユーザー・グループの設定（必要に応じて）
        if let Some(ref user) = _config.user {
            daemonize = daemonize.user(user);
        }
        
        if let Some(ref group) = _config.group {
            daemonize = daemonize.group(group);
        }
        
        daemonize.start()?;
    }
    
    #[cfg(not(unix))]
    {
        warn!("Daemonization is not supported on this platform");
    }
    
    Ok(())
}

fn create_pid_file(pid_file: &PathBuf) -> anyhow::Result<()> {
    let pid = std::process::id();
    std::fs::write(pid_file, pid.to_string())?;
    info!("PID file created: {}", pid_file.display());
    Ok(())
} 