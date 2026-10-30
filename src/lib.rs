#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::disallowed_methods)]
#![deny(clippy::perf)]
#![deny(clippy::needless_collect)]
#[macro_use]
extern crate alloc;

#[cfg(test)]
extern crate std;

// derive マクロが生成する `::spatial_id::…` を、このクレートの中でも解決できるようにする。
extern crate self as spatial_id;

/// 発生し得るすべてのエラーを`enum` 型として定義・集約。
mod error;

pub mod collection;
pub mod geometry;
mod id;
pub mod query;
pub mod time;

pub use error::Error;
pub use geometry::Epsilon;
pub use id::{
    FlexId, IntoRange, RangeId, Side, SingleId, SpatialId, SpatialIdError, Zoom, ZoomLevel,
};

#[allow(unused_imports)]
pub(crate) use error::{GeometryError, QueryError};

// クレート内部では、公開パスを書かずに短く参照する
#[allow(unused_imports)]
pub(crate) use collection::summary::{BitMask, MinMax, NoSummary, Summary, ValueSet};
#[allow(unused_imports)]
pub(crate) use collection::{SpatialIdMap, SpatialIdSet};
#[allow(unused_imports)]
pub(crate) use geometry::constants::{
    ALT_ABS_MAX, LAT_ABS_MAX, LON_ABS_MAX, WGS84_A, WGS84_E2, WGS84_F,
};
#[allow(unused_imports)]
pub(crate) use geometry::vec3::{
    traits::Vec3, vec3_ecef::Vec3Ecef, vec3_fractionalid::Vec3FractionalId,
};
#[allow(unused_imports)]
pub(crate) use geometry::*;
#[allow(unused_imports)]
pub(crate) use id::relative_flex_id::RelativeFlexId;
#[allow(unused_imports)]
pub(crate) use id::*;
#[allow(unused_imports)]
pub(crate) use query::{
    CancellationToken, IntoTargets, Query, QueryOutput, SafeValue, Source, merge_policy,
    merge_policy::MergePolicy,
};
#[allow(unused_imports)]
pub(crate) use time::{AllowedIntervals, Interval};
