//! ネットワーク関連コマンドモジュール
//!
//! このモジュールはネットワーク通信、Web操作、サーバー機能などの
//! コマンドの実装を提供します。

pub mod curl;
pub mod httpserver;

// 将来的に追加予定のコマンド
// pub mod wget;
// pub mod ping;
// pub mod ssh;
// pub mod telnet;
// pub mod netstat;
// pub mod ifconfig;
// pub mod route;
// pub mod nslookup;
// pub mod whois;

pub use curl::CurlCommand;
pub use httpserver::HttpServerCommand; 