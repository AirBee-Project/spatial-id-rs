use crate::Coordinate;
#[cfg(any(all(feature = "std", feature = "unstable"), test))]
use crate::{CoverSingleIds, Error, Point, SingleId};
pub mod geometry_relation;
pub mod impls;
#[cfg(test)]
mod tests;

/// 直線を表す型。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    points: [Coordinate; 2],
}

impl Line {
    ///[Line]を作成する。
    pub fn new(points: [Coordinate; 2]) -> Self {
        Self { points }
    }

    /// [Line]の両端の点を返す。
    pub fn points(&self) -> &[Coordinate; 2] {
        &self.points
    }

    /// 連続する頂点列（折れ線）が覆う [`SingleId`] のイテレータを返す。
    ///
    /// 頂点が1点のみの場合はその点の空間ID、2点以上の場合は隣り合う各線分が覆う空間IDを返す。
    #[cfg(any(all(feature = "std", feature = "unstable"), test))]
    pub(crate) fn cover_polyline<'a>(
        coordinates: &'a [Coordinate],
        z: impl Into<u8>,
    ) -> Result<impl Iterator<Item = SingleId> + 'a, Error> {
        let z = z.into();
        let mut ids = alloc::vec::Vec::new();
        if let [only] = coordinates {
            ids.push(only.single_id(z)?);
        } else {
            for pair in coordinates.windows(2) {
                ids.extend(Line::new([pair[0], pair[1]]).cover_single_ids(z)?);
            }
        }
        Ok(ids.into_iter())
    }
}
