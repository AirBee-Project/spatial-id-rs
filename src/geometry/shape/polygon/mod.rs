use alloc::vec::Vec;

use crate::geometry::Epsilon;
use crate::{Coordinate, Error, GeometryError, Point, Vec3, Vec3Ecef};

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
        Self::new_epsilon(raw_points, Epsilon::DEFAULT)
    }

    /// 頂点座標のイテレータと指定した許容誤差から新しい [Polygon] を作成する。
    pub fn new_epsilon(
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

    /// 2 つの [Polygon] が同一平面上にあるかを、デフォルト許容誤差（[`Epsilon::DEFAULT`]）で判定する。
    ///
    /// `other` のいずれの頂点も、`self` が張る平面からの距離が許容誤差以内なら `true` を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::{Coordinate, Polygon};
    /// let c = |lat, lon, alt| Coordinate::new(lat, lon, alt).unwrap();
    /// let a = Polygon::new([c(35.0, 139.0, 0.0), c(35.0, 139.01, 0.0), c(35.01, 139.0, 0.0)]).unwrap();
    /// let b = Polygon::new([c(35.0, 139.0, 0.0), c(35.01, 139.01, 0.0), c(35.01, 139.0, 0.0)]).unwrap();
    /// let tilted = Polygon::new([c(35.0, 139.0, 0.0), c(35.0, 139.01, 100.0), c(35.01, 139.0, 0.0)]).unwrap();
    ///
    /// assert!(a.is_coplanar(&b));
    /// assert!(!a.is_coplanar(&tilted));
    /// ```
    pub fn is_coplanar(&self, other: &Polygon) -> bool {
        self.is_coplanar_epsilon(other, Epsilon::DEFAULT)
    }

    /// 2 つの [Polygon] が同一平面上にあるかを、指定した許容誤差で判定する。
    ///
    /// `other` のいずれの頂点も、`self` が張る平面からの距離（メートル）が許容誤差以内なら `true` を返す。
    /// 距離は頂点ごとに平面の法線方向で測り、合計ではなく最も離れた頂点で判定する。
    pub fn is_coplanar_epsilon(&self, other: &Polygon, epsilon: Epsilon) -> bool {
        // 桁落ちを避けるため、最初の頂点を原点とした相対座標で Newell 法により法線を求める。
        let origin = Vec3Ecef::from(self.vertices[0]);
        let rel: Vec<Vec3Ecef> = self
            .vertices
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();
        let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
        for (curr, next) in rel.iter().zip(rel.iter().cycle().skip(1)) {
            nx += (curr.b() - next.b()) * (curr.c() + next.c());
            ny += (curr.c() - next.c()) * (curr.a() + next.a());
            nz += (curr.a() - next.a()) * (curr.b() + next.b());
        }
        let Some(normal) = Vec3Ecef::new(nx, ny, nz).normalize() else {
            return false;
        };
        other.vertices.iter().all(|&c| {
            let distance = (Vec3Ecef::from(c) - origin).dot(&normal);
            libm::fabs(distance) <= epsilon.as_meters()
        })
    }

    /// [Polygon]を構成する点を返す。
    pub fn vertices(&self) -> &Vec<Coordinate> {
        &self.vertices
    }
}
