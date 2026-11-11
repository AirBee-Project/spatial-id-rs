//! メモリ上で空間IDを保持・操作するコレクション。

pub(crate) mod core;
#[cfg(feature = "unstable")]
pub(crate) mod json;
pub(crate) mod map;
pub(crate) mod set;
pub(crate) mod time_reconstruct;
pub(crate) mod traits;

pub use self::core::IntoIter;
pub use map::SpatialIdMap;
pub use set::SpatialIdSet;
