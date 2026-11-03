use alloc::vec::Vec;

pub mod impls;
#[cfg(test)]
mod tests;

use crate::{Coordinate, Error, GeometryError};

/// 3次元空間におけるパイプを表す型。
///
/// ![パイプ][tube]
#[doc = doc_image!("tube", "docs/images/tube.jpg")]
#[derive(Debug, Clone, PartialEq)]
pub struct Tube {
    points: Vec<Coordinate>,
    radius_m: f64,
}

impl Tube {
    pub fn new(points: Vec<Coordinate>, radius_m: f64) -> Result<Self, Error> {
        if radius_m > 0.0 {
            Ok(Tube { points, radius_m })
        } else {
            Err(GeometryError::RadiusNegative { radius: radius_m }.into())
        }
    }

    /// 中心線を構成する点を返す。
    pub fn points(&self) -> &[Coordinate] {
        &self.points
    }

    /// 半径（メートル）を返す。
    pub fn radius_m(&self) -> f64 {
        self.radius_m
    }
}
