use alloc::vec::Vec;

use crate::geometry::shape::polygon::geometry_relation::get_projection_axes;
use crate::{Coordinate, Ecef};

/// 外周リングを ECEF の座標平面に投影したもの。穴の包含判定に使う。
pub(super) struct ProjectedRing {
    axes: (usize, usize),
    points: Vec<(f64, f64)>,
}

impl ProjectedRing {
    pub(super) fn new(ring: &[Coordinate]) -> Self {
        let ecef: Vec<Ecef> = ring.iter().map(|&c| c.into()).collect();
        let axes = get_projection_axes(&ecef);
        let points = ecef.iter().map(|p| p.project_2d(axes.0, axes.1)).collect();
        Self { axes, points }
    }

    /// `ring` が境界上を含めて内側に収まるかを判定する。
    ///
    /// 全頂点が内側にあり、かつ辺同士が真に交差しないことで判定する。
    /// 辺が外周の頂点だけを経由して外に出る退化したケースは検出しない。
    pub(super) fn contains_ring(&self, ring: &[Coordinate], epsilon: f64) -> bool {
        let inner: Vec<(f64, f64)> = ring
            .iter()
            .map(|&c| Ecef::from(c).project_2d(self.axes.0, self.axes.1))
            .collect();

        inner.iter().all(|&p| self.contains_point(p, epsilon))
            && ring_edges(&inner).all(|(a, b)| {
                ring_edges(&self.points).all(|(c, d)| !segments_cross(a, b, c, d, epsilon))
            })
    }

    fn contains_point(&self, p: (f64, f64), epsilon: f64) -> bool {
        let mut inside = false;
        for (a, b) in ring_edges(&self.points) {
            if point_segment_distance(p, a, b) <= epsilon {
                return true;
            }
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                inside = !inside;
            }
        }
        inside
    }
}

fn ring_edges(pts: &[(f64, f64)]) -> impl Iterator<Item = ((f64, f64), (f64, f64))> + '_ {
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(&a, &b)| (a, b))
}

/// 線分 `ab` に対する点 `p` の符号付き距離。`ab` が退化している場合は 0 を返す。
fn signed_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = libm::hypot(dx, dy);
    if len == 0.0 {
        return 0.0;
    }
    (dx * (p.1 - a.1) - dy * (p.0 - a.0)) / len
}

fn point_segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    libm::hypot(p.0 - (a.0 + t * dx), p.1 - (a.1 + t * dy))
}

/// 線分 `ab` と `cd` が端点の接触を除いて交差するかを判定する。
fn segments_cross(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
    epsilon: f64,
) -> bool {
    let straddles =
        |d1: f64, d2: f64| (d1 > epsilon && d2 < -epsilon) || (d1 < -epsilon && d2 > epsilon);
    straddles(signed_distance(c, a, b), signed_distance(d, a, b))
        && straddles(signed_distance(a, c, d), signed_distance(b, c, d))
}
