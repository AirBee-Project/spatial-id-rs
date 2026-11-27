use alloc::vec::Vec;

use crate::geometry::Epsilon;
use crate::{Coordinate, Error, GeometryError, Point};

pub mod geometry_relation;
pub mod impls;
mod ring_plane;

#[cfg(test)]
mod tests;

use ring_plane::{RingPlane, holes_overlap, ring_inside_exterior, validate_ring};

#[derive(Debug, Clone, PartialEq)]
/// 3 次元空間における多角形（ポリゴン）を表す型。
///
/// 外周リングと 0 個以上の内周リング（穴）で定義される平面的な領域を表す。
/// 生成時に重複頂点を除き、自己交差・面積・穴の包含関係を検証するため、常に有効な形状のみが構築される。
///
/// ![多角形][polygon]
#[doc = doc_image!("polygon", "docs/images/polygon.jpg")]
pub struct Polygon {
    vertices: Vec<Coordinate>,
    interiors: Vec<Vec<Coordinate>>,
}

impl Polygon {
    /// 頂点座標のイテレータからデフォルト許容誤差（[`Epsilon::DEFAULT`]）を使って新しい [Polygon] を作成する。
    ///
    /// # エラー
    /// - 正規化後の頂点数が 3 未満の場合は [`GeometryError::PolygonTooFewVertices`] を返す。
    /// - 面積が 0（共線など）の場合は [`GeometryError::PolygonZeroArea`] を返す。
    /// - リングが自己交差している場合は [`GeometryError::PolygonSelfIntersecting`] を返す。
    pub fn new(raw_points: impl IntoIterator<Item = Coordinate>) -> Result<Self, Error> {
        Self::with_epsilon(raw_points, Epsilon::DEFAULT)
    }

    /// 頂点座標のイテレータと指定した許容誤差から新しい [Polygon] を作成する。
    pub fn with_epsilon(
        raw_points: impl IntoIterator<Item = Coordinate>,
        epsilon: Epsilon,
    ) -> Result<Self, Error> {
        let eps = epsilon.as_meters();
        let vertices = normalize_ring(raw_points, epsilon)?;
        let (_, ext_2d) = RingPlane::from_exterior(&vertices, eps)?;
        validate_ring(&ext_2d, eps)?;
        Ok(Self {
            vertices,
            interiors: Vec::new(),
        })
    }

    /// 外周と内周（穴）の頂点座標イテレータからデフォルト許容誤差（[`Epsilon::DEFAULT`]）を使って新しい [Polygon] を作成する。
    ///
    /// # エラー
    /// [`Polygon::new`] の条件に加え、次の場合にエラーを返す。
    /// - 穴が外周の内側に収まっていない場合は [`GeometryError::HoleOutsideExterior`] を返す。
    /// - 穴同士が交差、重複、または包含関係にある場合は [`GeometryError::HoleOverlap`] を返す。
    pub fn with_interiors(
        exterior: impl IntoIterator<Item = Coordinate>,
        interiors: impl IntoIterator<Item = impl IntoIterator<Item = Coordinate>>,
    ) -> Result<Self, Error> {
        Self::with_interiors_and_epsilon(exterior, interiors, Epsilon::DEFAULT)
    }

    /// 外周と内周（穴）の頂点座標イテレータ、および指定した許容誤差から新しい [Polygon] を作成する。
    pub fn with_interiors_and_epsilon(
        exterior: impl IntoIterator<Item = Coordinate>,
        interiors: impl IntoIterator<Item = impl IntoIterator<Item = Coordinate>>,
        epsilon: Epsilon,
    ) -> Result<Self, Error> {
        let eps = epsilon.as_meters();
        let vertices = normalize_ring(exterior, epsilon)?;
        let (plane, ext_2d) = RingPlane::from_exterior(&vertices, eps)?;
        let ext_area = validate_ring(&ext_2d, eps)?;

        let interiors_iter = interiors.into_iter();
        let cap = interiors_iter.size_hint().0;
        let mut normalized_holes = Vec::with_capacity(cap);
        let mut projected_holes = Vec::with_capacity(cap);
        let mut total_hole_area = 0.0;

        for (index, raw_hole) in interiors_iter.enumerate() {
            let hole = normalize_ring(raw_hole, epsilon)?;
            let hole_2d = plane.project_ring(&hole);
            let hole_area = validate_ring(&hole_2d, eps)?;

            if !ring_inside_exterior(&ext_2d, &hole_2d, eps) {
                return Err(GeometryError::HoleOutsideExterior { index }.into());
            }

            total_hole_area += hole_area;
            normalized_holes.push(hole);
            projected_holes.push(hole_2d);
        }

        if normalized_holes.is_empty() {
            return Ok(Self {
                vertices,
                interiors: normalized_holes,
            });
        }

        for i in 0..projected_holes.len() {
            for j in (i + 1)..projected_holes.len() {
                if holes_overlap(&projected_holes[i], &projected_holes[j], eps) {
                    return Err(GeometryError::HoleOverlap {
                        first: i,
                        second: j,
                    }
                    .into());
                }
            }
        }

        let net_area = ext_area - total_hole_area;
        if net_area <= 0.0 || (eps > 0.0 && net_area <= 0.5 * eps * eps) {
            return Err(GeometryError::PolygonZeroArea.into());
        }

        Ok(Self {
            vertices,
            interiors: normalized_holes,
        })
    }

    /// 外周リングを構成する頂点のリストを返す。
    pub fn vertices(&self) -> &Vec<Coordinate> {
        &self.vertices
    }

    /// 内周リング（穴）のリストを返す。
    pub fn interiors(&self) -> &[Vec<Coordinate>] {
        &self.interiors
    }
}

fn normalize_ring(
    raw_points: impl IntoIterator<Item = Coordinate>,
    epsilon: Epsilon,
) -> Result<Vec<Coordinate>, Error> {
    let iter = raw_points.into_iter();
    let mut vertices: Vec<Coordinate> = Vec::with_capacity(iter.size_hint().0);
    for point in iter {
        if vertices
            .last()
            .is_some_and(|last| last.eq_epsilon(&point, epsilon))
        {
            continue;
        }
        vertices.push(point);
    }

    if vertices.len() > 1 && vertices[0].eq_epsilon(vertices.last().unwrap(), epsilon) {
        vertices.pop();
    }

    if vertices.len() < 3 {
        return Err(GeometryError::PolygonTooFewVertices {
            count: vertices.len(),
        }
        .into());
    }

    Ok(vertices)
}
