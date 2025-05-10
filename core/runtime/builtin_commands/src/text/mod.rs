//! テキスト処理コマンドモジュール
//!
//! このモジュールはテキストデータの処理、分析、変換を行う
//! コマンドの実装を提供します。

pub mod map;
pub mod stats;
pub mod slice;
pub mod distinct;

// 将来的に追加予定のコマンド
// pub mod cat;
// pub mod grep;
// pub mod sed;
// pub mod awk;
// pub mod wc;
// pub mod sort;
// pub mod uniq;
// pub mod head;
// pub mod tail;
// pub mod cut;
// pub mod tr;
// pub mod diff;
// pub mod patch;

pub use map::MapCommand;
pub use stats::StatsCommand;
pub use slice::SliceCommand;
pub use distinct::DistinctCommand; 