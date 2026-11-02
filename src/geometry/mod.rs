//! 座標と図形。図形が覆う空間IDを求める。
//!
//! 距離、半径、高さなどの長さは、特に明記しない限りメートル（m）単位で扱う。

/// 地理空間座標の計算で使用される各種の代表的な定数。
pub(crate) mod constants;

pub(crate) mod point;

pub(crate) mod shape;

pub(crate) mod traits;

pub(crate) mod vec3;

pub(crate) mod epsilon;

pub mod format;

pub use crate::error::GeometryError;
pub use epsilon::Epsilon;
pub use point::{coordinate::Coordinate, ecef::Ecef, fractionalid::FractionalId, traits::Point};
pub use shape::traits::{ExpandCoordinates, ExpandLines, ExpandPolygons, ExpandTriangles, Shape};
pub use shape::{
    cylinder::Cylinder, line::Line, polygon::Polygon, solid::Solid, sphere::Sphere,
    triangle::Triangle, tube::Tube,
};
pub use traits::{CoverRangeIds, CoverSingleIds};
