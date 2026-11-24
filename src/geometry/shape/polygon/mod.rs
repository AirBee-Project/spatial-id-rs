use alloc::vec::Vec;

use crate::geometry::Epsilon;
use crate::{Coordinate, Error, GeometryError, Point};

pub mod geometry_relation;
pub mod impls;
mod ring_plane;

#[cfg(test)]
mod tests;

use ring_plane::{project_ring_2d, validate_ring};

#[derive(Debug, Clone, PartialEq)]
/// 3 次元空間における多角形（ポリゴン）を表す型。
///
/// 頂点リスト（[Coordinate] の Vec）で定義される平面的な領域を表す。
/// 生成時に重複頂点を除き、自己交差と面積を検証するため、常に有効な形状のみが構築される。
///
/// ![多角形][polygon]
#[doc = doc_image!("polygon", "docs/images/polygon.jpg")]
pub struct Polygon {
    vertices: Vec<Coordinate>,
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

        let pts_2d = project_ring_2d(&vertices, eps)?;
        validate_ring(&pts_2d, eps)?;

        Ok(Self { vertices })
    }

    /// [Polygon]を構成する点を返す。
    pub fn vertices(&self) -> &Vec<Coordinate> {
        &self.vertices
    }
}
