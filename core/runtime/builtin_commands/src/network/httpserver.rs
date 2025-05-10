use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncReadExt;
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use std::net::SocketAddr;
use mime_guess::from_path;
use futures::stream::StreamExt;
use tracing::{debug, info, warn, error};

/// シンプルなHTTPサーバーを起動するコマンド
///
/// 指定されたディレクトリをシンプルなHTTPサーバーでホストします。
/// 開発やテスト用の簡易的なWebサーバーとして利用できます。
///
/// # 使用例
///
/// ```bash
/// httpserver                     # カレントディレクトリを8000番ポートでホスト
/// httpserver --port 3000         # 3000番ポートを使用
/// httpserver --dir /path/to/www  # 指定ディレクトリをホスト
/// ```
pub struct HttpServerCommand;

/// HTTPサーバーの設定オプション
struct ServerOptions {
    /// サーバーのバインドアドレス
    bind_address: String,
    /// サーバーのポート
    port: u16,
    /// ホストするディレクトリ
    directory: PathBuf,
    /// インデックスファイル
    index_file: String,
    /// CORS（クロスオリジンリソース共有）を有効にする
    enable_cors: bool,
    /// キャッシュコントロールヘッダー
    cache_control: Option<String>,
    /// サーバーヘッダー
    server_header: String,
    /// 詳細ロギングを有効にする
    verbose: bool,
    /// 隠しファイルを含める
    include_hidden: bool,
    /// ブラウザを自動的に開く
    open_browser: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8000,
            directory: PathBuf::from("."),
            index_file: "index.html".to_string(),
            enable_cors: false,
            cache_control: None,
            server_header: "NexusShell-HttpServer".to_string(),
            verbose: false,
            include_hidden: false,
            open_browser: false,
        }
    }
}

#[async_trait]
impl BuiltinCommand for HttpServerCommand {
    fn name(&self) -> &'static str {
        "httpserver"
    }

    fn description(&self) -> &'static str {
        "シンプルなHTTPサーバーを起動します"
    }

    fn usage(&self) -> &'static str {
        "httpserver [オプション]\n\n\
        オプション:\n\
        --port <PORT>            サーバーのポート番号（デフォルト: 8000）\n\
        --bind <ADDRESS>         バインドするアドレス（デフォルト: 127.0.0.1）\n\
        --dir <PATH>             ホストするディレクトリ（デフォルト: カレントディレクトリ）\n\
        --index <FILE>           インデックスファイル名（デフォルト: index.html）\n\
        --cors                   CORS（クロスオリジンリソース共有）を有効化\n\
        --cache <DIRECTIVE>      Cache-Controlヘッダーを設定\n\
        --server <NAME>          サーバーヘッダーを設定\n\
        --verbose                詳細ロギングを有効化\n\
        --include-hidden         隠しファイルの提供を許可\n\
        --open                   デフォルトブラウザでサーバーを開く"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // オプションを初期化
        let mut options = ServerOptions::default();
        
        // カレントディレクトリを設定
        options.directory = context.current_dir.clone();
        
        // オプション解析
        let mut i = 1;
        while i < context.args.len() {
            match context.args[i].as_str() {
                "--port" => {
                    i += 1;
                    if i < context.args.len() {
                        options.port = context.args[i].parse()
                            .map_err(|_| anyhow!("ポートは数値である必要があります"))?;
                    } else {
                        return Err(anyhow!("--port オプションには値が必要です"));
                    }
                },
                "--bind" => {
                    i += 1;
                    if i < context.args.len() {
                        options.bind_address = context.args[i].clone();
                    } else {
                        return Err(anyhow!("--bind オプションには値が必要です"));
                    }
                },
                "--dir" => {
                    i += 1;
                    if i < context.args.len() {
                        let path = PathBuf::from(&context.args[i]);
                        options.directory = if path.is_absolute() {
                            path
                        } else {
                            context.current_dir.join(path)
                        };
                        
                        if !options.directory.exists() || !options.directory.is_dir() {
                            return Err(anyhow!("指定されたディレクトリが存在しないか、ディレクトリではありません: {}", 
                                options.directory.display()));
                        }
                    } else {
                        return Err(anyhow!("--dir オプションには値が必要です"));
                    }
                },
                "--index" => {
                    i += 1;
                    if i < context.args.len() {
                        options.index_file = context.args[i].clone();
                    } else {
                        return Err(anyhow!("--index オプションには値が必要です"));
                    }
                },
                "--cors" => {
                    options.enable_cors = true;
                },
                "--cache" => {
                    i += 1;
                    if i < context.args.len() {
                        options.cache_control = Some(context.args[i].clone());
                    } else {
                        return Err(anyhow!("--cache オプションには値が必要です"));
                    }
                },
                "--server" => {
                    i += 1;
                    if i < context.args.len() {
                        options.server_header = context.args[i].clone();
                    } else {
                        return Err(anyhow!("--server オプションには値が必要です"));
                    }
                },
                "--verbose" => {
                    options.verbose = true;
                },
                "--include-hidden" => {
                    options.include_hidden = true;
                },
                "--open" => {
                    options.open_browser = true;
                },
                _ => {
                    return Err(anyhow!("不明なオプション: {}", context.args[i]));
                }
            }
            
            i += 1;
        }
        
        // サーバー設定をArcで包んで共有
        let options = Arc::new(options);
        
        // アドレスをパース
        let addr: SocketAddr = format!("{}:{}", options.bind_address, options.port).parse()
            .map_err(|e| anyhow!("アドレスのパースに失敗: {}", e))?;
        
        // サービス関数を作成
        let make_svc = make_service_fn(move |_conn| {
            let options = Arc::clone(&options);
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    let options = Arc::clone(&options);
                    handle_request(req, options)
                }))
            }
        });
        
        // サーバーを構築
        let server = Server::bind(&addr).serve(make_svc);
        
        // ブラウザで開く
        if options.open_browser {
            let url = format!("http://{}:{}", options.bind_address, options.port);
            if let Err(e) = webbrowser::open(&url) {
                warn!("ブラウザを開けませんでした: {}", e);
            }
        }
        
        // スタート情報を出力
        let output_msg = format!(
            "HTTPサーバーを起動しました - http://{}:{}\nディレクトリ: {}\nCtrl+Cで終了\n",
            options.bind_address, options.port, options.directory.display()
        );
        
        info!("{}", output_msg);
        
        // サーバーを実行
        let mut output = Vec::new();
        output.extend_from_slice(output_msg.as_bytes());
        
        // 制御はフロントエンドのシェルに戻して、サーバーはバックグラウンドで実行されるようにする
        tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("HTTPサーバーエラー: {}", e);
            }
        });
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// HTTPリクエストを処理する関数
async fn handle_request(req: Request<Body>, options: Arc<ServerOptions>) -> Result<Response<Body>, hyper::Error> {
    let uri_path = req.uri().path();
    let method = req.method();
    
    // リクエスト情報をログに記録
    if options.verbose {
        debug!("{} {}", method, uri_path);
    }
    
    // GETメソッド以外は405エラー
    if method != hyper::Method::GET && method != hyper::Method::HEAD {
        return Ok(Response::builder()
            .status(405)
            .header("Allow", "GET, HEAD")
            .body(Body::from("Method Not Allowed"))
            .unwrap());
    }
    
    // パスをデコード
    let path = match percent_encoding::percent_decode_str(uri_path).decode_utf8() {
        Ok(p) => p,
        Err(_) => {
            return Ok(Response::builder()
                .status(400)
                .body(Body::from("Bad Request: Invalid URI"))
                .unwrap());
        }
    };
    
    // パストラバーサル攻撃を防止
    if path.contains("..") {
        return Ok(Response::builder()
            .status(403)
            .body(Body::from("Forbidden: Path traversal attempt"))
            .unwrap());
    }
    
    // ファイルパスを構築
    let mut file_path = options.directory.clone();
    
    // ルートまたはディレクトリの場合はインデックスファイルを使用
    let path_str = path.as_ref();
    if path_str == "/" {
        file_path.push(&options.index_file);
    } else {
        // 先頭の / を削除
        file_path.push(&path_str[1..]);
    }
    
    // 隠しファイルの扱い
    if !options.include_hidden && is_hidden_path(&file_path) {
        return Ok(Response::builder()
            .status(403)
            .body(Body::from("Forbidden: Hidden file access denied"))
            .unwrap());
    }
    
    // ファイルまたはディレクトリが存在するか確認
    if !file_path.exists() {
        return Ok(Response::builder()
            .status(404)
            .body(Body::from("Not Found"))
            .unwrap());
    }
    
    // ディレクトリの場合はインデックスファイルか、ディレクトリリスティングを提供
    if file_path.is_dir() {
        let index_path = file_path.join(&options.index_file);
        if index_path.exists() {
            file_path = index_path;
        } else {
            // ディレクトリリスティングを生成
            return Ok(generate_directory_listing(&file_path, &path, &options).await);
        }
    }
    
    // ファイルのMIMEタイプを推測
    let mime_type = from_path(&file_path)
        .first_or_octet_stream()
        .to_string();
    
    // ファイルを読み込み
    let mut file = match fs::File::open(&file_path).await {
        Ok(file) => file,
        Err(_) => {
            return Ok(Response::builder()
                .status(500)
                .body(Body::from("Internal Server Error: Could not read file"))
                .unwrap());
        }
    };
    
    // ファイルサイズを取得
    let metadata = match file.metadata().await {
        Ok(meta) => meta,
        Err(_) => {
            return Ok(Response::builder()
                .status(500)
                .body(Body::from("Internal Server Error: Could not read file metadata"))
                .unwrap());
        }
    };
    
    let file_size = metadata.len();
    
    // レスポンスビルダー
    let mut builder = Response::builder()
        .header("Content-Type", mime_type)
        .header("Content-Length", file_size)
        .header("Server", &options.server_header);
    
    // CORSヘッダーを追加
    if options.enable_cors {
        builder = builder
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET, HEAD")
            .header("Access-Control-Allow-Headers", "Content-Type");
    }
    
    // キャッシュコントロールヘッダーを追加
    if let Some(cache) = &options.cache_control {
        builder = builder.header("Cache-Control", cache);
    }
    
    // HEADメソッドの場合は本文なし
    if method == hyper::Method::HEAD {
        return Ok(builder.body(Body::empty()).unwrap());
    }
    
    // ファイル内容を読み込み
    let mut contents = Vec::with_capacity(file_size as usize);
    if let Err(_) = file.read_to_end(&mut contents).await {
        return Ok(Response::builder()
            .status(500)
            .body(Body::from("Internal Server Error: Failed to read file"))
            .unwrap());
    }
    
    // レスポンスを返す
    Ok(builder.body(Body::from(contents)).unwrap())
}

/// ディレクトリリスティングのHTMLを生成
async fn generate_directory_listing(dir_path: &Path, request_path: &str, options: &ServerOptions) -> Response<Body> {
    // ディレクトリのエントリを読み込み
    let mut entries = Vec::new();
    let mut reader = match fs::read_dir(dir_path).await {
        Ok(reader) => reader,
        Err(_) => {
            return Response::builder()
                .status(500)
                .body(Body::from("Internal Server Error: Could not read directory"))
                .unwrap();
        }
    };
    
    while let Some(entry) = reader.next().await {
        if let Ok(entry) = entry {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy().to_string();
            
            // 隠しファイルをフィルタリング
            if !options.include_hidden && file_name_str.starts_with('.') {
                continue;
            }
            
            let metadata = match entry.metadata().await {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            
            let is_dir = metadata.is_dir();
            let size = if is_dir { 0 } else { metadata.len() };
            
            entries.push((file_name_str, is_dir, size));
        }
    }
    
    // エントリをソート（ディレクトリが先、次にファイル名でアルファベット順）
    entries.sort_by(|a, b| {
        match (a.1, b.1) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.0.cmp(&b.0),
        }
    });
    
    // HTMLを構築
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str(&format!("<title>Index of {}</title>\n", request_path));
    html.push_str("<style>\n");
    html.push_str("body { font-family: sans-serif; margin: 20px; }\n");
    html.push_str("h1 { border-bottom: 1px solid #ddd; padding-bottom: 10px; }\n");
    html.push_str("table { border-collapse: collapse; width: 100%; }\n");
    html.push_str("th, td { text-align: left; padding: 8px; }\n");
    html.push_str("tr:nth-child(even) { background-color: #f2f2f2; }\n");
    html.push_str(".name { width: 60%; }\n");
    html.push_str(".size { width: 20%; text-align: right; }\n");
    html.push_str(".dir { font-weight: bold; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n");
    html.push_str(&format!("<h1>Index of {}</h1>\n", request_path));
    html.push_str("<table>\n");
    html.push_str("<tr><th class=\"name\">Name</th><th class=\"size\">Size</th></tr>\n");
    
    // 親ディレクトリへのリンク（ルートでない場合）
    if request_path != "/" {
        html.push_str("<tr><td class=\"name\"><a href=\"");
        
        let parent_path = if request_path.ends_with('/') {
            if request_path.len() > 1 {
                let path_without_trailing = &request_path[0..request_path.len() - 1];
                if let Some(last_slash) = path_without_trailing.rfind('/') {
                    &request_path[0..last_slash + 1]
                } else {
                    "/"
                }
            } else {
                "/"
            }
        } else {
            if let Some(last_slash) = request_path.rfind('/') {
                if last_slash == 0 {
                    "/"
                } else {
                    &request_path[0..last_slash + 1]
                }
            } else {
                "/"
            }
        };
        
        html.push_str(parent_path);
        html.push_str("\">..</a></td><td class=\"size\">-</td></tr>\n");
    }
    
    // ディレクトリとファイルのリスト
    for (name, is_dir, size) in entries {
        html.push_str("<tr><td class=\"name\">");
        
        // リンクを構築
        let link_path = if request_path.ends_with('/') {
            format!("{}{}", request_path, name)
        } else {
            format!("{}/{}", request_path, name)
        };
        
        let display_name = if is_dir {
            format!("{}/", name)
        } else {
            name.clone()
        };
        
        if is_dir {
            html.push_str(&format!("<a href=\"{}\" class=\"dir\">{}</a>", link_path, display_name));
        } else {
            html.push_str(&format!("<a href=\"{}\">{}</a>", link_path, display_name));
        }
        
        html.push_str("</td><td class=\"size\">");
        
        // サイズを表示
        if is_dir {
            html.push_str("-");
        } else {
            html.push_str(&format_size(size));
        }
        
        html.push_str("</td></tr>\n");
    }
    
    html.push_str("</table>\n");
    html.push_str(&format!("<hr><p>NexusShell-HttpServer running at {}</p>\n", 
        format!("{}:{}", options.bind_address, options.port)));
    html.push_str("</body>\n</html>");
    
    // レスポンスを構築
    let mut builder = Response::builder()
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Server", &options.server_header);
    
    // CORSヘッダーを追加
    if options.enable_cors {
        builder = builder
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET, HEAD")
            .header("Access-Control-Allow-Headers", "Content-Type");
    }
    
    builder.body(Body::from(html)).unwrap()
}

/// パスが隠しファイルまたは隠しディレクトリかチェック
fn is_hidden_path(path: &Path) -> bool {
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            if name.starts_with('.') && name != "." && name != ".." {
                return true;
            }
        }
    }
    false
}

/// ファイルサイズを人間が読みやすい形式にフォーマット
fn format_size(size: u64) -> String {
    const UNIT: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    
    if size == 0 {
        return "0 B".to_string();
    }
    
    let digits = ((size as f64).log10() / 3.0).floor() as usize;
    let unit = UNIT[digits.min(UNIT.len() - 1)];
    let scaled_size = size as f64 / 10_f64.powi((digits * 3) as i32);
    
    if scaled_size.fract() < 0.01 {
        format!("{:.0} {}", scaled_size, unit)
    } else {
        format!("{:.1} {}", scaled_size, unit)
    }
} 