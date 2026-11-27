//! ディスク上で空間IDを永続化・操作するコレクション（`unstable`: `disk` feature）。
//!
//! [`SpatialIdSet`] と [`SpatialIdMap`] は、[`crate::collection::mem`] の同名型と対称な
//! インターフェースをディスク永続化向けに提供する。
//!
//! > **注意**: このモジュール配下の API はすべて `unstable` 扱いであり、今後のバージョンで
//! > 破壊的な変更が入る可能性がある。
mod convert;
mod core;
mod io;
mod map;
mod set;
mod traits;
mod value;

pub use self::core::{IntoIter, Iter};
pub use self::map::SpatialIdMap;
pub use self::set::SpatialIdSet;
pub use self::value::{DiskValue, MAX_VALUE_LEN};
