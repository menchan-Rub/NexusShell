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
pub mod alias;
pub mod history;
pub mod source;

pub use cd::CdCommand;
pub use echo::EchoCommand;
pub use exit::ExitCommand;
pub use pwd::PwdCommand;
pub use export::ExportCommand; 