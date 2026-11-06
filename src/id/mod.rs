//! 空間IDとズームレベルを表現する。

pub(crate) mod dimension;
pub(crate) mod single_id;
pub(crate) mod traits;
pub(crate) mod zoom_level;

pub(crate) mod flex_id;
pub(crate) mod helpers;
pub(crate) mod range_id;
pub(crate) mod relative_flex_id;

#[cfg(test)]
mod tests;

pub use crate::error::SpatialIdError;
pub(crate) use dimension::Dimension;
pub use flex_id::FlexId;
pub use helpers::IntoRange;
pub(crate) use helpers::Side;
pub use range_id::RangeId;
pub use single_id::SingleId;
pub use traits::SpatialId;
pub use zoom_level::{Zoom, ZoomLevel};
