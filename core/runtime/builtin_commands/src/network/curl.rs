use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::{Client, Method, header};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn, error};

/// HTTPリクエストを送信するコマンド
///
/// さまざまなHTTPメソッドを使用してウェブリソースを操作します。
/// Get, Post, Put, Delete などの基本的なHTTPメソッドをサポートし、
/// ヘッダーやボディの設定、リクエストパラメータの指定などが可能です。
///
/// # 使用例
///
/// ```bash
/// curl https://example.com               # 単純なGETリクエスト
/// curl -X POST -d "data" example.com     # POSTリクエスト
/// curl -H "Content-Type: application/json" -d '{"key":"value"}' example.com
/// ```
pub struct CurlCommand;

#[async_trait]
impl BuiltinCommand for CurlCommand {
    fn name(&self) -> &'static str {
        "curl"
    }

    fn description(&self) -> &'static str {
        "URLに対してHTTPリクエストを送信します"
    }

    fn usage(&self) -> &'static str {
        "curl [オプション] <URL>\n\n\
        オプション:\n\
        -X, --request <METHOD>   HTTPメソッドを指定 (GET, POST, PUT, DELETE等)\n\
        -H, --header <LINE>      リクエストヘッダーを追加 (「Name: Value」形式)\n\
        -d, --data <DATA>        HTTPリクエストボディを指定\n\
        -F, --form <KEY=VALUE>   multipart/form-dataとしてデータを送信\n\
        -u, --user <USER:PASS>   サーバー認証の資格情報\n\
        -A, --user-agent <NAME>  ユーザーエージェント文字列を指定\n\
        -e, --referer <URL>      リファラーURLを指定\n\
        -k, --insecure           SSL証明書の検証をスキップ\n\
        -L, --location           リダイレクトに従う\n\
        -I, --head               ヘッダーのみを取得 (HTTPヘッド)\n\
        -s, --silent             進行状況や警告を非表示\n\
        -v, --verbose            詳細な情報を表示\n\
        -o, --output <FILE>      出力を指定ファイルに書き込み\n\
        --connect-timeout <SEC>  接続タイムアウトを秒単位で指定\n\
        --max-time <SEC>         リクエスト最大時間を秒単位で指定\n\
        -i, --include            レスポンスヘッダーを含めて出力\n\
        --compressed             圧縮コンテンツを要求\n\
        --json                   リクエストボディをJSON形式として扱う"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 2 {
            return Err(anyhow!("URLを指定してください。使用方法: curl [オプション] <URL>"));
        }

        // デフォルト設定
        let mut method = Method::GET;
        let mut headers = HeaderMap::new();
        let mut data = None;
        let mut form_data = HashMap::new();
        let mut auth = None;
        let mut user_agent = None;
        let mut referer = None;
        let mut insecure = false;
        let mut follow_redirects = false;
        let mut head_only = false;
        let mut include_headers = false;
        let mut connect_timeout = 30;
        let mut max_time = 300;
        let mut verbose = false;
        let mut silent = false;
        let mut use_json = false;
        
        // URLは最後の非オプション引数として扱う
        let mut url = None;
        
        let mut i = 1;
        while i < context.args.len() {
            match context.args[i].as_str() {
                "-X" | "--request" => {
                    i += 1;
                    if i < context.args.len() {
                        method = match context.args[i].to_uppercase().as_str() {
                            "GET" => Method::GET,
                            "POST" => Method::POST,
                            "PUT" => Method::PUT,
                            "DELETE" => Method::DELETE,
                            "HEAD" => Method::HEAD,
                            "OPTIONS" => Method::OPTIONS,
                            "PATCH" => Method::PATCH,
                            _ => return Err(anyhow!("未サポートのHTTPメソッド: {}", context.args[i])),
                        };
                    } else {
                        return Err(anyhow!("-X/--request オプションにはメソッド名が必要です"));
                    }
                },
                "-H" | "--header" => {
                    i += 1;
                    if i < context.args.len() {
                        let header_str = &context.args[i];
                        if let Some(colon_pos) = header_str.find(':') {
                            let (name, value) = header_str.split_at(colon_pos);
                            
                            // ヘッダー名と値を正規化
                            let header_name = name.trim();
                            let header_value = value[1..].trim(); // ':'の次の文字から取得
                            
                            // ヘッダーを追加
                            if let Ok(name) = HeaderName::from_bytes(header_name.as_bytes()) {
                                if let Ok(value) = HeaderValue::from_str(header_value) {
                                    headers.insert(name, value);
                                } else {
                                    warn!("無効なヘッダー値: {}", header_value);
                                }
                            } else {
                                warn!("無効なヘッダー名: {}", header_name);
                            }
                        } else {
                            return Err(anyhow!("ヘッダーは 'Name: Value' 形式である必要があります"));
                        }
                    } else {
                        return Err(anyhow!("-H/--header オプションには値が必要です"));
                    }
                },
                "-d" | "--data" => {
                    i += 1;
                    if i < context.args.len() {
                        data = Some(context.args[i].clone());
                    } else {
                        return Err(anyhow!("-d/--data オプションにはデータが必要です"));
                    }
                },
                "-F" | "--form" => {
                    i += 1;
                    if i < context.args.len() {
                        let form_str = &context.args[i];
                        if let Some(eq_pos) = form_str.find('=') {
                            let (key, value) = form_str.split_at(eq_pos);
                            form_data.insert(key.to_string(), value[1..].to_string());
                        } else {
                            return Err(anyhow!("フォームデータは 'key=value' 形式である必要があります"));
                        }
                    } else {
                        return Err(anyhow!("-F/--form オプションには値が必要です"));
                    }
                },
                "-u" | "--user" => {
                    i += 1;
                    if i < context.args.len() {
                        auth = Some(context.args[i].clone());
                    } else {
                        return Err(anyhow!("-u/--user オプションには値が必要です"));
                    }
                },
                "-A" | "--user-agent" => {
                    i += 1;
                    if i < context.args.len() {
                        user_agent = Some(context.args[i].clone());
                    } else {
                        return Err(anyhow!("-A/--user-agent オプションには値が必要です"));
                    }
                },
                "-e" | "--referer" => {
                    i += 1;
                    if i < context.args.len() {
                        referer = Some(context.args[i].clone());
                    } else {
                        return Err(anyhow!("-e/--referer オプションには値が必要です"));
                    }
                },
                "-k" | "--insecure" => {
                    insecure = true;
                },
                "-L" | "--location" => {
                    follow_redirects = true;
                },
                "-I" | "--head" => {
                    head_only = true;
                    method = Method::HEAD;
                },
                "-i" | "--include" => {
                    include_headers = true;
                },
                "-v" | "--verbose" => {
                    verbose = true;
                },
                "-s" | "--silent" => {
                    silent = true;
                },
                "--connect-timeout" => {
                    i += 1;
                    if i < context.args.len() {
                        connect_timeout = context.args[i].parse::<u64>()
                            .map_err(|_| anyhow!("タイムアウトは数値である必要があります"))?;
                    } else {
                        return Err(anyhow!("--connect-timeout オプションには値が必要です"));
                    }
                },
                "--max-time" => {
                    i += 1;
                    if i < context.args.len() {
                        max_time = context.args[i].parse::<u64>()
                            .map_err(|_| anyhow!("最大時間は数値である必要があります"))?;
                    } else {
                        return Err(anyhow!("--max-time オプションには値が必要です"));
                    }
                },
                "--json" => {
                    use_json = true;
                    if !headers.contains_key(header::CONTENT_TYPE) {
                        headers.insert(
                            header::CONTENT_TYPE,
                            HeaderValue::from_static("application/json")
                        );
                    }
                },
                arg if !arg.starts_with('-') => {
                    // 非オプション引数はURLとして扱う
                    url = Some(arg.to_string());
                },
                _ => {
                    return Err(anyhow!("不明なオプション: {}", context.args[i]));
                }
            }
            
            i += 1;
        }
        
        // URLが指定されていない場合はエラー
        let url = url.ok_or_else(|| anyhow!("URLが指定されていません"))?;
        
        // URLにプロトコルがない場合はhttpを付加
        let url = if !url.starts_with("http://") && !url.starts_with("https://") {
            format!("http://{}", url)
        } else {
            url
        };
        
        // HTTPクライアントの構築
        let mut client_builder = Client::builder()
            .timeout(Duration::from_secs(max_time))
            .connect_timeout(Duration::from_secs(connect_timeout))
            .pool_max_idle_per_host(0);
        
        // 設定オプションの適用
        if insecure {
            client_builder = client_builder
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true);
        }
        
        if follow_redirects {
            client_builder = client_builder.redirect(reqwest::redirect::Policy::limited(10));
        } else {
            client_builder = client_builder.redirect(reqwest::redirect::Policy::none());
        }
        
        let client = client_builder.build()?;
        
        // リクエストを構築
        let mut request_builder = client.request(method, &url);
        
        // ヘッダーを設定
        request_builder = request_builder.headers(headers);
        
        // 認証情報を設定
        if let Some(auth_str) = auth {
            if let Some(colon_pos) = auth_str.find(':') {
                let (username, password) = auth_str.split_at(colon_pos);
                request_builder = request_builder.basic_auth(username, Some(&password[1..]));
            } else {
                request_builder = request_builder.basic_auth(auth_str, None);
            }
        }
        
        // ユーザーエージェントを設定
        if let Some(agent) = user_agent {
            request_builder = request_builder.header(header::USER_AGENT, agent);
        } else {
            request_builder = request_builder.header(
                header::USER_AGENT,
                format!("NexusShell-Curl/{}", env!("CARGO_PKG_VERSION"))
            );
        }
        
        // リファラーを設定
        if let Some(ref_url) = referer {
            request_builder = request_builder.header(header::REFERER, ref_url);
        }
        
        // ボディデータを設定
        if let Some(body_data) = data {
            if use_json && !body_data.trim_start().starts_with('{') && !body_data.trim_start().starts_with('[') {
                // JSONではないデータをJSON形式に変換
                let mut json_data = std::collections::HashMap::new();
                for pair in body_data.split('&') {
                    if let Some(eq_pos) = pair.find('=') {
                        let (key, value) = pair.split_at(eq_pos);
                        json_data.insert(key, &value[1..]);
                    } else {
                        json_data.insert(pair, "");
                    }
                }
                request_builder = request_builder.json(&json_data);
            } else if use_json {
                // 既にJSON形式のデータ
                request_builder = request_builder.body(body_data);
            } else {
                // 通常のボディデータ
                request_builder = request_builder.body(body_data);
            }
        } else if !form_data.is_empty() {
            request_builder = request_builder.form(&form_data);
        }
        
        // 詳細モードの場合、リクエスト情報を表示
        if verbose && !silent {
            debug!("リクエスト: {} {}", method, url);
            for (name, value) in request_builder.headers_ref().unwrap().iter() {
                debug!("ヘッダー: {}: {}", name, value.to_str().unwrap_or("不明な値"));
            }
            if let Some(ref body) = data {
                debug!("ボディ: {}", body);
            }
        }
        
        // リクエストを送信
        let response = request_builder.send().await
            .map_err(|e| anyhow!("リクエストの送信に失敗しました: {}", e))?;
        
        // レスポンスのステータスコード
        let status = response.status();
        let status_code = status.as_u16();
        let is_success = status.is_success();
        
        // レスポンス結果を整形
        let mut output = Vec::new();
        
        // ヘッダーを含める場合
        if include_headers || head_only {
            output.extend_from_slice(format!("HTTP/{:?} {} {}\r\n", 
                response.version(), status_code, status.canonical_reason().unwrap_or("")).as_bytes());
            
            for (name, value) in response.headers() {
                output.extend_from_slice(format!("{}: {}\r\n", 
                    name, value.to_str().unwrap_or("[不明な値]")).as_bytes());
            }
            
            output.extend_from_slice(b"\r\n");
        }
        
        // HEAD以外のメソッドの場合、ボディを含める
        if !head_only {
            let body = response.bytes().await
                .map_err(|e| anyhow!("レスポンスボディの読み取りに失敗しました: {}", e))?;
            
            output.extend_from_slice(&body);
        }
        
        // 結果を返す
        if is_success {
            Ok(CommandResult::success().with_stdout(output))
        } else {
            Ok(CommandResult {
                exit_code: status_code as i32,
                stdout: output,
                stderr: Vec::new(),
            })
        }
    }
} 