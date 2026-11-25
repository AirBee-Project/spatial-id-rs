//! 空間IDの集合 [`SpatialIdSet`] と、空間IDごとに値を持つマップ [`SpatialIdMap`]。

pub mod mem;
pub(crate) mod traits;

pub use self::mem::{IntoIter, SpatialIdMap, SpatialIdSet};
pub use self::traits::IntoSpatialIds;

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use self::mem::set;
#[allow(unused_imports)]
pub(crate) use self::mem::{core, time_reconstruct};
