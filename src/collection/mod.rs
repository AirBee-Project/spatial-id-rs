//! 空間IDの集合 [`SpatialIdSet`] と、空間IDごとに値を持つマップ [`SpatialIdMap`]。
//!
//! - [`mem`][] — メモリ上で扱う標準コレクション（このモジュール直下でも再公開している）。
//! - `disk`（`disk` feature / `unstable`） — ディスク永続化コレクション。[`mem`] と対称な `SpatialIdMap` / `SpatialIdSet` を提供する。

#[cfg(feature = "unstable")]
pub(crate) mod json;
pub mod mem;
pub(crate) mod time_reconstruct;

#[cfg(all(feature = "disk", feature = "unstable"))]
pub mod disk;

pub use self::mem::{IntoIter, SpatialIdMap, SpatialIdSet};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use self::mem::set;
#[allow(unused_imports)]
pub(crate) use self::mem::{core, traits};
