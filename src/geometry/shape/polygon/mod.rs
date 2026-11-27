#[cfg(feature = "unstable")]
use alloc::vec;
use alloc::vec::Vec;

#[cfg(feature = "unstable")]
use i_overlay::core::fill_rule::FillRule;
#[cfg(feature = "unstable")]
use i_overlay::core::overlay::ContourDirection;
#[cfg(feature = "unstable")]
use i_overlay::core::overlay_rule::OverlayRule;
#[cfg(feature = "unstable")]
use i_overlay::float::overlay::{FloatOverlay, OverlayOptions};

use crate::geometry::Epsilon;
use crate::{Coordinate, Error, GeometryError, Point, Vec2, Vec3, Vec3Ecef};

pub mod geometry_relation;
pub mod impls;

#[cfg(test)]
mod tests;

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
    #[cfg(feature = "unstable")]
    holes: Vec<Vec<Coordinate>>,
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

        let origin = Vec3Ecef::from(vertices[0]);
        let rel: Vec<Vec3Ecef> = vertices
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();
        let normal = Self::plane_normal(&rel, eps)?;
        let [u, v] = normal.create_orthonormal_basis();
        let pts_2d: Vec<Vec2> = rel
            .iter()
            .map(|r| Vec2::new(r.dot(&u), r.dot(&v)))
            .collect();
        Vec2::validate_ring(&pts_2d, eps)?;

        Ok(Self {
            vertices,
            #[cfg(feature = "unstable")]
            holes: Vec::new(),
        })
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
        let origin = Vec3Ecef::from(self.vertices[0]);
        let rel: Vec<Vec3Ecef> = self
            .vertices
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();
        let Some(normal) = Self::newell_normal(&rel) else {
            return false;
        };
        other
            .vertices
            .iter()
            .all(|&c| libm::fabs((Vec3Ecef::from(c) - origin).dot(&normal)) <= epsilon.as_meters())
    }

    /// 複数の同一平面上の [Polygon] を、デフォルト許容誤差（[`Epsilon::DEFAULT`]）で統合し、
    /// 統合結果の内部に発生した穴は埋めて返す。
    ///
    /// # エラー
    /// - 基準となる先頭要素と同一平面上にない要素がある場合は [`GeometryError::PolygonNotCoplanar`] を返す。
    #[cfg(feature = "unstable")]
    pub fn merge_filling_holes(polygons: &[Polygon]) -> Result<Vec<Polygon>, Error> {
        Self::merge_filling_holes_epsilon(polygons, Epsilon::DEFAULT)
    }

    /// 複数の同一平面上の [Polygon] を、指定した許容誤差で統合し、
    /// 統合結果の内部に発生した穴は埋めて返す。
    ///
    /// # エラー
    /// - 基準となる先頭要素と同一平面上にない要素がある場合は [`GeometryError::PolygonNotCoplanar`] を返す。
    #[cfg(feature = "unstable")]
    pub fn merge_filling_holes_epsilon(
        polygons: &[Polygon],
        epsilon: Epsilon,
    ) -> Result<Vec<Polygon>, Error> {
        if polygons.is_empty() {
            return Ok(Vec::new());
        }
        if polygons.len() == 1 {
            return Ok(vec![polygons[0].clone()]);
        }

        let base = &polygons[0];
        for (idx, poly) in polygons.iter().enumerate().skip(1) {
            if !base.is_coplanar_epsilon(poly, epsilon) {
                return Err(GeometryError::PolygonNotCoplanar { index: idx }.into());
            }
        }

        let eps = epsilon.as_meters();
        let origin = Vec3Ecef::from(base.vertices[0]);
        let rel: Vec<Vec3Ecef> = base
            .vertices
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();
        let normal = Self::plane_normal(&rel, eps)?;
        let [u, v] = normal.create_orthonormal_basis();

        let mut contours: Vec<Vec<[f64; 2]>> = Vec::with_capacity(polygons.len());
        let base_pts: Vec<Vec2> = rel
            .iter()
            .map(|r| Vec2::new(r.dot(&u), r.dot(&v)))
            .collect();
        contours.push(Self::to_ccw_contour(base_pts));
        for poly in polygons.iter().skip(1) {
            let pts: Vec<Vec2> = poly
                .vertices()
                .iter()
                .map(|&c| {
                    let r = Vec3Ecef::from(c) - origin;
                    Vec2::new(r.dot(&u), r.dot(&v))
                })
                .collect();
            contours.push(Self::to_ccw_contour(pts));
        }

        let mut options = OverlayOptions::<f64, i64>::default();
        options.output_direction = ContourDirection::CounterClockwise;
        options.ogc = true;
        if eps > 0.0 {
            options.min_output_area = 0.5 * eps * eps;
        }

        let shapes =
            FloatOverlay::<[f64; 2], i64>::from_subj_custom(&contours, options, Default::default())
                .overlay(OverlayRule::Subject, FillRule::Positive);

        let dedup_eps2 = eps * eps;
        let collinear_eps = if eps > 0.0 { eps.min(1e-6) } else { 1e-12 };
        let collinear_eps2 = collinear_eps * collinear_eps;

        let mut outers: Vec<(Vec<Vec2>, f64)> = Vec::with_capacity(shapes.len());
        for mut shape in shapes {
            if let Some(outer_pts) = shape.drain(..1).next() {
                let mut pts: Vec<Vec2> = outer_pts
                    .into_iter()
                    .map(|[x, y]| Vec2::new(x, y))
                    .collect();
                Vec2::simplify_ring(&mut pts, dedup_eps2, collinear_eps2);
                if pts.len() < 3 {
                    continue;
                }
                let area = libm::fabs(Vec2::signed_area(&pts));
                outers.push((pts, area));
            }
        }

        // 穴を埋めた結果、元は穴の中の島だった外周が大きな外周に内包される場合は除外する
        let accepted = if outers.len() > 1 {
            outers.sort_by(|a, b| b.1.total_cmp(&a.1));
            let mut kept: Vec<(Vec<Vec2>, f64)> = Vec::with_capacity(outers.len());
            for cand in outers {
                if !kept
                    .iter()
                    .any(|parent| Vec2::ring_is_inside(&cand.0, &parent.0))
                {
                    kept.push(cand);
                }
            }
            kept
        } else {
            outers
        };

        let mut result = Vec::with_capacity(accepted.len());
        for (pts, _) in accepted {
            let coords: Vec<_> = pts
                .iter()
                .map(|&pt| Coordinate::try_from(origin + u.scale(pt.x) + v.scale(pt.y)))
                .collect::<Result<_, _>>()?;
            result.push(Polygon::new_epsilon(coords, epsilon)?);
        }
        Ok(result)
    }

    /// 指定した複数の穴（内周リング）をあけた新しい [Polygon] を、デフォルト許容誤差（[`Epsilon::DEFAULT`]）で作成する。
    ///
    /// 重なりや辺の接触がある穴同士は統合され、穴の中に発生する島も埋められる。
    /// 既存の穴がある場合はそれらも合わせて統合される。
    ///
    /// # エラー
    /// - 基準となる `self` と同一平面上にない穴がある場合は [`GeometryError::PolygonNotCoplanar`] を返す。
    /// - 統合後の穴が外周の境界に接触している、または外周からはみ出している場合（外周を完全に包む場合を含む）は
    ///   [`GeometryError::HoleOutsideExterior`] を返す。
    #[cfg(feature = "unstable")]
    pub fn with_holes(&self, holes: &[Polygon]) -> Result<Self, Error> {
        self.with_holes_epsilon(holes, Epsilon::DEFAULT)
    }

    /// 指定した複数の穴（内周リング）をあけた新しい [Polygon] を、指定した許容誤差で作成する。
    ///
    /// # エラー
    /// - 基準となる `self` と同一平面上にない穴がある場合は [`GeometryError::PolygonNotCoplanar`] を返す。
    /// - 統合後の穴が外周の境界に接触している、または外周からはみ出している場合（外周を完全に包む場合を含む）は
    ///   [`GeometryError::HoleOutsideExterior`] を返す。
    #[cfg(feature = "unstable")]
    pub fn with_holes_epsilon(&self, holes: &[Polygon], epsilon: Epsilon) -> Result<Self, Error> {
        if holes.is_empty() {
            return Ok(self.clone());
        }

        for (index, hole) in holes.iter().enumerate() {
            if !self.is_coplanar_epsilon(hole, epsilon) {
                return Err(GeometryError::PolygonNotCoplanar { index }.into());
            }
        }

        let mut all_holes: Vec<Polygon> = Vec::with_capacity(self.holes.len() + holes.len());
        for existing in &self.holes {
            all_holes.push(Polygon::new_epsilon(existing.iter().copied(), epsilon)?);
        }
        all_holes.extend(holes.iter().cloned());

        let merged_holes = Self::merge_filling_holes_epsilon(&all_holes, epsilon)?;

        let eps = epsilon.as_meters();
        let origin = Vec3Ecef::from(self.vertices[0]);
        let rel: Vec<Vec3Ecef> = self
            .vertices
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();
        let normal = Self::plane_normal(&rel, eps)?;
        let [u, v] = normal.create_orthonormal_basis();
        let project = |c: Coordinate| {
            let r = Vec3Ecef::from(c) - origin;
            Vec2::new(r.dot(&u), r.dot(&v))
        };
        let outer_pts: Vec<Vec2> = rel
            .iter()
            .map(|r| Vec2::new(r.dot(&u), r.dot(&v)))
            .collect();

        let mut new_holes = Vec::with_capacity(merged_holes.len());
        for (index, hole) in merged_holes.into_iter().enumerate() {
            let hole_pts: Vec<Vec2> = hole.vertices.iter().copied().map(project).collect();
            if !Vec2::ring_is_strictly_inside(&hole_pts, &outer_pts, eps) {
                return Err(GeometryError::HoleOutsideExterior { index }.into());
            }
            new_holes.push(hole.vertices);
        }

        Ok(Self {
            vertices: self.vertices.clone(),
            holes: new_holes,
        })
    }

    /// [Polygon]を構成する点を返す。
    pub fn vertices(&self) -> &Vec<Coordinate> {
        &self.vertices
    }

    /// [Polygon] の穴（内周リング）のリストを返す。
    #[cfg(feature = "unstable")]
    pub fn holes(&self) -> &[Vec<Coordinate>] {
        &self.holes
    }

    #[cfg(feature = "unstable")]
    fn to_ccw_contour(mut pts: Vec<Vec2>) -> Vec<[f64; 2]> {
        Vec2::ensure_ccw(&mut pts);
        pts.into_iter().map(|p| [p.x, p.y]).collect()
    }

    fn newell_normal(rel: &[Vec3Ecef]) -> Option<Vec3Ecef> {
        rel.iter()
            .zip(rel.iter().cycle().skip(1))
            .fold(Vec3Ecef::new(0.0, 0.0, 0.0), |sum, (curr, next)| {
                sum + curr.cross(next)
            })
            .normalize()
    }

    /// Newell の法線を優先し、面積の相殺で 0 になる場合（8 の字の自己交差など）は共線でない 3 点から法線を求める。
    fn plane_normal(rel: &[Vec3Ecef], eps: f64) -> Result<Vec3Ecef, Error> {
        let eps2 = eps * eps;
        let mut far_idx = 0;
        let mut max_d2 = 0.0;
        for (i, r) in rel.iter().enumerate().skip(1) {
            let d2 = r.norm_squared();
            if d2 > max_d2 {
                max_d2 = d2;
                far_idx = i;
            }
        }
        if max_d2 <= eps2 {
            return Err(GeometryError::PolygonZeroArea.into());
        }

        let axis = rel[far_idx];
        let mut perp_idx = 0;
        let mut max_h2 = 0.0;
        for (i, r) in rel.iter().enumerate() {
            let cross = axis.cross(r);
            let h2 = cross.norm_squared() / max_d2;
            if h2 > max_h2 {
                max_h2 = h2;
                perp_idx = i;
            }
        }
        if max_h2 <= eps2 {
            return Err(GeometryError::PolygonZeroArea.into());
        }

        if let Some(unit) = Self::newell_normal(rel) {
            return Ok(unit);
        }

        axis.cross(&rel[perp_idx])
            .normalize()
            .ok_or_else(|| GeometryError::PolygonZeroArea.into())
    }
}
