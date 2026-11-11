//! 空間IDの集合 [`SpatialIdSet`] と、空間IDごとに値を持つマップ [`SpatialIdMap`]。

pub mod mem;

pub use self::mem::{IntoIter, SpatialIdMap, SpatialIdSet};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use self::mem::set;
#[allow(unused_imports)]
pub(crate) use self::mem::{core, time_reconstruct, traits};
