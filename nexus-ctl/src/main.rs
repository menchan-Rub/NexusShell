use clap::{Parser, Subcommand};
use colored::*;
use log::{debug, error, info};
use std::path::PathBuf;
use std::process;

mod commands;
mod daemon;
mod image;
mod network;
mod volume;
mod config;
mod utils;

use crate::commands::CommandHandler;

#[derive(Parser)]
#[command(name = "nexus-ctl")]
#[command(about = "NexusContainer管理用CLIツール")]
#[command(version = "0.1.0")]
#[command(author = "NexusShell Team <info@nexusshell.com>")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 設定ファイルのパス
    #[arg(long = "config", short = 'c')]
    config: Option<PathBuf>,

    /// ログレベルの設定
    #[arg(long = "log-level", default_value = "info")]
    log_level: String,

    /// JSONで出力
    #[arg(long = "json")]
    json: bool,

    /// 静かに実行
    #[arg(long = "quiet", short = 'q')]
    quiet: bool,

    /// 詳細な出力
    #[arg(long = "verbose", short = 'v')]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// コンテナ管理
    #[command(subcommand)]
    Container(ContainerCommands),
    
    /// イメージ管理
    #[command(subcommand)]
    Image(ImageCommands),
    
    /// ボリューム管理
    #[command(subcommand)]
    Volume(VolumeCommands),
    
    /// ネットワーク管理
    #[command(subcommand)]
    Network(NetworkCommands),
    
    /// システム情報と管理
    #[command(subcommand)]
    System(SystemCommands),
    
    /// プロファイルとポリシー管理
    #[command(subcommand)]
    Profile(ProfileCommands),
}

#[derive(Subcommand)]
enum ContainerCommands {
    /// コンテナを作成
    Create {
        /// コンテナ名
        name: String,
        /// イメージ名
        image: String,
        /// 実行するコマンド
        #[arg(last = true)]
        command: Vec<String>,
        
        /// 環境変数
        #[arg(short = 'e', long = "env")]
        env: Vec<String>,
        
        /// ボリュームマウント
        #[arg(short = 'v', long = "volume")]
        volumes: Vec<String>,
        
        /// ポートマッピング
        #[arg(short = 'p', long = "port")]
        ports: Vec<String>,
        
        /// ワーキングディレクトリ
        #[arg(short = 'w', long = "workdir")]
        workdir: Option<String>,
        
        /// ユーザー
        #[arg(short = 'u', long = "user")]
        user: Option<String>,
        
        /// ホスト名
        #[arg(long = "hostname")]
        hostname: Option<String>,
        
        /// 特権モード
        #[arg(long = "privileged")]
        privileged: bool,
        
        /// 読み取り専用ルートファイルシステム
        #[arg(long = "read-only")]
        read_only: bool,
        
        /// ネットワークモード
        #[arg(long = "network")]
        network: Option<String>,
        
        /// セキュリティプロファイル
        #[arg(long = "security-profile")]
        security_profile: Option<String>,
        
        /// 作成後に開始
        #[arg(long = "run")]
        run: bool,
    },
    
    /// コンテナを開始
    Start {
        /// コンテナ名またはID
        container: String,
        
        /// アタッチ
        #[arg(short = 'a', long = "attach")]
        attach: bool,
        
        /// インタラクティブ
        #[arg(short = 'i', long = "interactive")]
        interactive: bool,
    },
    
    /// コンテナを停止
    Stop {
        /// コンテナ名またはID
        container: String,
        
        /// 停止タイムアウト（秒）
        #[arg(short = 't', long = "timeout", default_value = "10")]
        timeout: u64,
        
        /// 強制停止
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    
    /// コンテナを再起動
    Restart {
        /// コンテナ名またはID
        container: String,
        
        /// 停止タイムアウト（秒）
        #[arg(short = 't', long = "timeout", default_value = "10")]
        timeout: u64,
    },
    
    /// コンテナを削除
    Remove {
        /// コンテナ名またはID
        containers: Vec<String>,
        
        /// 強制削除
        #[arg(short = 'f', long = "force")]
        force: bool,
        
        /// ボリュームも削除
        #[arg(short = 'v', long = "volumes")]
        volumes: bool,
    },
    
    /// コンテナ一覧を表示
    List {
        /// すべてのコンテナを表示
        #[arg(short = 'a', long = "all")]
        all: bool,
        
        /// フィルタ
        #[arg(short = 'f', long = "filter")]
        filter: Vec<String>,
        
        /// フォーマット
        #[arg(long = "format")]
        format: Option<String>,
    },
    
    /// コンテナ内でコマンドを実行
    Exec {
        /// コンテナ名またはID
        container: String,
        
        /// 実行するコマンド
        #[arg(last = true)]
        command: Vec<String>,
        
        /// インタラクティブ
        #[arg(short = 'i', long = "interactive")]
        interactive: bool,
        
        /// TTYを割り当て
        #[arg(short = 't', long = "tty")]
        tty: bool,
        
        /// ユーザー
        #[arg(short = 'u', long = "user")]
        user: Option<String>,
        
        /// ワーキングディレクトリ
        #[arg(short = 'w', long = "workdir")]
        workdir: Option<String>,
        
        /// 環境変数
        #[arg(short = 'e', long = "env")]
        env: Vec<String>,
    },
    
    /// コンテナのログを表示
    Logs {
        /// コンテナ名またはID
        container: String,
        
        /// 最新N行を表示
        #[arg(long = "tail")]
        tail: Option<usize>,
        
        /// リアルタイムでフォロー
        #[arg(short = 'f', long = "follow")]
        follow: bool,
        
        /// タイムスタンプを表示
        #[arg(short = 't', long = "timestamps")]
        timestamps: bool,
        
        /// 開始時刻
        #[arg(long = "since")]
        since: Option<String>,
        
        /// 終了時刻
        #[arg(long = "until")]
        until: Option<String>,
    },
    
    /// コンテナの詳細情報を表示
    Inspect {
        /// コンテナ名またはID
        containers: Vec<String>,
        
        /// フォーマット
        #[arg(short = 'f', long = "format")]
        format: Option<String>,
    },
    
    /// コンテナの統計情報を表示
    Stats {
        /// コンテナ名またはID（指定なしで全コンテナ）
        containers: Vec<String>,
        
        /// リアルタイムでフォロー
        #[arg(long = "no-stream")]
        no_stream: bool,
    },
    
    /// コンテナを一時停止
    Pause {
        /// コンテナ名またはID
        containers: Vec<String>,
    },
    
    /// コンテナを再開
    Unpause {
        /// コンテナ名またはID
        containers: Vec<String>,
    },
    
    /// コンテナをイメージにコミット
    Commit {
        /// コンテナ名またはID
        container: String,
        
        /// 新しいイメージ名
        image: String,
        
        /// コミットメッセージ
        #[arg(short = 'm', long = "message")]
        message: Option<String>,
        
        /// 作成者
        #[arg(short = 'a', long = "author")]
        author: Option<String>,
    },
}

#[derive(Subcommand)]
enum ImageCommands {
    /// イメージ一覧を表示
    List {
        /// フィルタ
        #[arg(short = 'f', long = "filter")]
        filter: Vec<String>,
        
        /// フォーマット
        #[arg(long = "format")]
        format: Option<String>,
        
        /// すべてのイメージを表示
        #[arg(short = 'a', long = "all")]
        all: bool,
    },
    
    /// イメージをビルド
    Build {
        /// Dockerfileのパス
        #[arg(short = 'f', long = "file", default_value = "Dockerfile")]
        dockerfile: PathBuf,
        
        /// ビルドコンテキスト
        #[arg(default_value = ".")]
        context: PathBuf,
        
        /// イメージ名とタグ
        #[arg(short = 't', long = "tag")]
        tags: Vec<String>,
        
        /// ビルド引数
        #[arg(long = "build-arg")]
        build_args: Vec<String>,
        
        /// キャッシュを使わない
        #[arg(long = "no-cache")]
        no_cache: bool,
        
        /// ベースイメージを強制プル
        #[arg(long = "pull")]
        pull: bool,
        
        /// 静かにビルド
        #[arg(short = 'q', long = "quiet")]
        quiet: bool,
    },
    
    /// イメージをプル
    Pull {
        /// イメージ名
        image: String,
        
        /// プラットフォーム
        #[arg(long = "platform")]
        platform: Option<String>,
        
        /// すべてのタグをプル
        #[arg(short = 'a', long = "all-tags")]
        all_tags: bool,
    },
    
    /// イメージをプッシュ
    Push {
        /// イメージ名
        image: String,
        
        /// すべてのタグをプッシュ
        #[arg(short = 'a', long = "all-tags")]
        all_tags: bool,
    },
    
    /// イメージを削除
    Remove {
        /// イメージ名またはID
        images: Vec<String>,
        
        /// 強制削除
        #[arg(short = 'f', long = "force")]
        force: bool,
        
        /// 未使用イメージも削除
        #[arg(long = "prune")]
        prune: bool,
    },
    
    /// イメージの詳細情報を表示
    Inspect {
        /// イメージ名またはID
        images: Vec<String>,
        
        /// フォーマット
        #[arg(short = 'f', long = "format")]
        format: Option<String>,
    },
    
    /// イメージの履歴を表示
    History {
        /// イメージ名またはID
        image: String,
        
        /// 切り詰めなし
        #[arg(long = "no-trunc")]
        no_trunc: bool,
    },
    
    /// イメージにタグを付ける
    Tag {
        /// ソースイメージ
        source: String,
        
        /// ターゲットイメージ
        target: String,
    },
    
    /// ディレクトリからイメージをインポート
    Import {
        /// ディレクトリパス
        path: PathBuf,
        
        /// イメージ名
        name: String,
        
        /// タグ
        #[arg(short = 't', long = "tag", default_value = "latest")]
        tag: String,
    },
    
    /// イメージをtarアーカイブにエクスポート
    Export {
        /// イメージ名またはID
        image: String,
        
        /// 出力ファイル
        #[arg(short = 'o', long = "output")]
        output: PathBuf,
    },
    
    /// 未使用イメージをクリーンアップ
    Prune {
        /// すべての未使用イメージを削除
        #[arg(short = 'a', long = "all")]
        all: bool,
        
        /// フィルタ
        #[arg(long = "filter")]
        filter: Vec<String>,
        
        /// 確認をスキップ
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum VolumeCommands {
    /// ボリューム一覧を表示
    List {
        /// フィルタ
        #[arg(short = 'f', long = "filter")]
        filter: Vec<String>,
        
        /// フォーマット
        #[arg(long = "format")]
        format: Option<String>,
    },
    
    /// ボリュームを作成
    Create {
        /// ボリューム名
        name: String,
        
        /// ドライバー
        #[arg(short = 'd', long = "driver", default_value = "local")]
        driver: String,
        
        /// ドライバーオプション
        #[arg(short = 'o', long = "opt")]
        options: Vec<String>,
        
        /// ラベル
        #[arg(long = "label")]
        labels: Vec<String>,
    },
    
    /// ボリュームを削除
    Remove {
        /// ボリューム名
        volumes: Vec<String>,
        
        /// 強制削除
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    
    /// ボリュームの詳細情報を表示
    Inspect {
        /// ボリューム名
        volumes: Vec<String>,
        
        /// フォーマット
        #[arg(short = 'f', long = "format")]
        format: Option<String>,
    },
    
    /// 未使用ボリュームをクリーンアップ
    Prune {
        /// フィルタ
        #[arg(long = "filter")]
        filter: Vec<String>,
        
        /// 確認をスキップ
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum NetworkCommands {
    /// ネットワーク一覧を表示
    List {
        /// フィルタ
        #[arg(short = 'f', long = "filter")]
        filter: Vec<String>,
        
        /// フォーマット
        #[arg(long = "format")]
        format: Option<String>,
    },
    
    /// ネットワークを作成
    Create {
        /// ネットワーク名
        name: String,
        
        /// ドライバー
        #[arg(short = 'd', long = "driver", default_value = "bridge")]
        driver: String,
        
        /// サブネット
        #[arg(long = "subnet")]
        subnet: Option<String>,
        
        /// ゲートウェイ
        #[arg(long = "gateway")]
        gateway: Option<String>,
        
        /// IPレンジ
        #[arg(long = "ip-range")]
        ip_range: Option<String>,
        
        /// ラベル
        #[arg(long = "label")]
        labels: Vec<String>,
    },
    
    /// ネットワークを削除
    Remove {
        /// ネットワーク名
        networks: Vec<String>,
        
        /// 強制削除
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    
    /// ネットワークの詳細情報を表示
    Inspect {
        /// ネットワーク名
        networks: Vec<String>,
        
        /// フォーマット
        #[arg(short = 'f', long = "format")]
        format: Option<String>,
    },
    
    /// コンテナをネットワークに接続
    Connect {
        /// ネットワーク名
        network: String,
        
        /// コンテナ名
        container: String,
        
        /// IPアドレス
        #[arg(long = "ip")]
        ip: Option<String>,
        
        /// エイリアス
        #[arg(long = "alias")]
        alias: Vec<String>,
    },
    
    /// コンテナをネットワークから切断
    Disconnect {
        /// ネットワーク名
        network: String,
        
        /// コンテナ名
        container: String,
        
        /// 強制切断
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    
    /// 未使用ネットワークをクリーンアップ
    Prune {
        /// フィルタ
        #[arg(long = "filter")]
        filter: Vec<String>,
        
        /// 確認をスキップ
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum SystemCommands {
    /// システム情報を表示
    Info,
    
    /// システム全体の統計情報を表示
    Stats,
    
    /// バージョン情報を表示
    Version,
    
    /// システム全体をクリーンアップ
    Prune {
        /// コンテナも削除
        #[arg(long = "containers")]
        containers: bool,
        
        /// イメージも削除
        #[arg(long = "images")]
        images: bool,
        
        /// ボリュームも削除
        #[arg(long = "volumes")]
        volumes: bool,
        
        /// ネットワークも削除
        #[arg(long = "networks")]
        networks: bool,
        
        /// すべて削除
        #[arg(short = 'a', long = "all")]
        all: bool,
        
        /// 確認をスキップ
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    
    /// イベントを監視
    Events {
        /// フィルタ
        #[arg(short = 'f', long = "filter")]
        filter: Vec<String>,
        
        /// 開始時刻
        #[arg(long = "since")]
        since: Option<String>,
        
        /// 終了時刻
        #[arg(long = "until")]
        until: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// プロファイル一覧を表示
    List,
    
    /// プロファイルを作成
    Create {
        /// プロファイル名
        name: String,
        
        /// 設定ファイル
        #[arg(short = 'f', long = "file")]
        file: Option<PathBuf>,
    },
    
    /// プロファイルを削除
    Remove {
        /// プロファイル名
        profiles: Vec<String>,
        
        /// 強制削除
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    
    /// プロファイルの詳細情報を表示
    Inspect {
        /// プロファイル名
        profiles: Vec<String>,
    },
    
    /// プロファイルを適用
    Apply {
        /// プロファイル名
        profile: String,
        
        /// コンテナ名
        container: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    
    // ログレベルの初期化
    let log_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "error"
    } else {
        &cli.log_level
    };
    
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(log_level)
    )
    .format_timestamp_secs()
    .init();
    
    info!("NexusCtl v0.1.0 starting");
    
    // 設定の読み込み
    let config_path = cli.config.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nexuscontainer")
            .join("config.toml")
    });
    
    let config = match config::Config::load(&config_path) {
        Ok(config) => config,
        Err(e) => {
            if config_path.exists() {
                error!("Failed to load config: {}", e);
                process::exit(1);
            } else {
                debug!("Config file not found, using defaults");
                config::Config::default()
            }
        }
    };
    
    // コマンドハンドラーの初期化
    let mut handler = match CommandHandler::new(config, cli.json, cli.quiet).await {
        Ok(handler) => handler,
        Err(e) => {
            error!("Failed to initialize command handler: {}", e);
            process::exit(1);
        }
    };
    
    // コマンドの実行
    let result = match cli.command {
        Commands::Container(cmd) => handler.handle_command(cmd).await,
        Commands::Image(cmd) => handler.handle_image_command(cmd).await,
        Commands::Volume(cmd) => handler.handle_volume_command(cmd).await,
        Commands::Network(cmd) => handler.handle_network_command(cmd).await,
        Commands::System(cmd) => handler.handle_system_command(cmd).await,
        Commands::Profile(cmd) => handler.handle_profile_command(cmd).await,
    };
    
    match result {
        Ok(_) => {
            debug!("Command completed successfully");
        }
        Err(e) => {
            if !cli.quiet {
                eprintln!("{}: {}", "Error".red().bold(), e);
            }
            error!("Command failed: {}", e);
            process::exit(1);
        }
    }
} 