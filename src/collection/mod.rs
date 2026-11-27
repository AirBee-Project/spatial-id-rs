//! 空間IDの集合 [`SpatialIdSet`] と、空間IDごとに値を持つマップ [`SpatialIdMap`]。

#[cfg(feature = "unstable")]
pub(crate) mod json;
pub mod mem;
pub(crate) mod time_reconstruct;

pub use self::mem::{IntoIter, MapIter, SetIter, SpatialIdMap, SpatialIdSet};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use self::mem::set;
#[allow(unused_imports)]
pub(crate) use self::mem::{core, traits};
