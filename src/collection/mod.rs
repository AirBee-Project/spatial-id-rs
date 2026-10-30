//! 空間IDの集合 [`SpatialIdSet`] と、空間IDごとに値を持つマップ [`SpatialIdMap`]。

pub(crate) mod core;
#[cfg(feature = "unstable")]
mod json;
mod map;
mod set;
pub(crate) mod time_reconstruct;
mod traits;

pub use self::core::IntoIter;
pub use map::SpatialIdMap;
pub use set::SpatialIdSet;
