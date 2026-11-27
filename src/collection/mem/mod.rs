//! メモリ上で空間IDを保持・操作するコレクション。
//!
//! 空間IDの集合 [`SpatialIdSet`] と、空間IDごとに値を持つマップ [`SpatialIdMap`] を提供する。
//! ディスク永続化が必要な場合は `collection::disk`（`disk` feature / `unstable`）を使用する。

pub(crate) mod core;
pub(crate) mod map;
pub(crate) mod set;
pub(crate) mod traits;

pub use self::core::IntoIter;
pub use map::SpatialIdMap;
pub use set::SpatialIdSet;
