use crate::{Coordinate, Error, GeometryError};
pub mod impls;
#[cfg(test)]
mod tests;

/// 球体を表す型。
///
/// ![球][sphere]
#[doc = doc_image!("sphere", "docs/images/sphere.jpg")]
pub struct Sphere {
    center: Coordinate,
    radius_m: f64,
}

impl Sphere {
    ///[Sphere]を作成する。
    pub fn new(center: Coordinate, radius_m: f64) -> Result<Self, Error> {
        if radius_m > 0.0 {
            Ok(Sphere { center, radius_m })
        } else {
            Err(GeometryError::RadiusNegative { radius: radius_m }.into())
        }
    }
}
