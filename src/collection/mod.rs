//! 空間IDの集合 `SpatialIdSet` と、空間IDごとに値を持つマップ `SpatialIdMap`。
//!
//! - [`mem`] — メモリ上で扱う [`SpatialIdSet`](mem::SpatialIdSet) と [`SpatialIdMap`](mem::SpatialIdMap)。

#[cfg(feature = "unstable")]
pub(crate) mod json;
pub mod mem;
pub(crate) mod time_reconstruct;

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use self::mem::set;
#[allow(unused_imports)]
pub(crate) use self::mem::{core, traits};
