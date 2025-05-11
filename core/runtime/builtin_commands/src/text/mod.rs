//! テキスト処理コマンドモジュール
//!
//! このモジュールはテキストデータの処理、分析、変換を行う
//! コマンドの実装を提供します。

pub mod map;
pub mod stats;
pub mod slice;
pub mod distinct;
pub mod cat;
pub mod grep;
pub mod sort;
pub mod head;
pub mod tail;
pub mod wc;
pub mod sed;
pub mod uniq;
pub mod cut;
pub mod tr;

// 将来的に追加予定のコマンド
// pub mod awk;
// pub mod diff;
// pub mod patch;

pub use map::MapCommand;
pub use stats::StatsCommand;
pub use slice::SliceCommand;
pub use distinct::DistinctCommand; 
pub use cat::CatCommand;
pub use grep::GrepCommand;
pub use sort::SortCommand;
pub use head::HeadCommand;
pub use tail::TailCommand;
pub use wc::WcCommand;
pub use sed::SedCommand;
pub use uniq::UniqCommand;
pub use cut::CutCommand;
pub use tr::TrCommand; 