use alloc::vec::Vec;

use crate::{Coordinate, Error, GeometryError, Vec3, Vec3Ecef};

/// ポリゴンの基準平面とローカル 2D 直交座標系（単位: メートル）。
///
/// 地球中心からの巨大オフセットによる桁落ちを防ぐため、外周の最初の頂点を原点とした相対座標で投影する。
pub(crate) struct RingPlane {
    origin: Vec3Ecef,
    u: Vec3Ecef,
    v: Vec3Ecef,
}

impl RingPlane {
    /// 外周リングからローカル 2D 直交座標系を構築し、外周の 2D 投影座標を返す。
    pub(crate) fn from_exterior(
        exterior: &[Coordinate],
        eps: f64,
    ) -> Result<(Self, Vec<(f64, f64)>), Error> {
        let origin = Vec3Ecef::from(exterior[0]);
        let rel: Vec<Vec3Ecef> = exterior
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();

        let normal = compute_plane_normal(&rel, eps)?;
        let [u, v] = normal.create_orthonormal_basis();
        let plane = Self { origin, u, v };
        let projected = rel.iter().map(|r| (r.dot(&u), r.dot(&v))).collect();
        Ok((plane, projected))
    }

    /// 座標列をこの平面上の 2D 座標へ投影する。
    pub(crate) fn project_ring(&self, ring: &[Coordinate]) -> Vec<(f64, f64)> {
        ring.iter().map(|&c| self.project_point(c)).collect()
    }

    fn project_point(&self, coord: Coordinate) -> (f64, f64) {
        let r = Vec3Ecef::from(coord) - self.origin;
        (r.dot(&self.u), r.dot(&self.v))
    }
}

/// Newell の法線を優先し、面積の相殺で 0 になる場合（8 の字の自己交差など）は共線でない 3 点から法線を求める。
fn compute_plane_normal(rel: &[Vec3Ecef], eps: f64) -> Result<Vec3Ecef, Error> {
    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;
    let len = rel.len();

    for i in 0..len {
        let curr = rel[i];
        let next = rel[(i + 1) % len];
        nx += (curr.b() - next.b()) * (curr.c() + next.c());
        ny += (curr.c() - next.c()) * (curr.a() + next.a());
        nz += (curr.a() - next.a()) * (curr.b() + next.b());
    }

    // 面積相殺ではなく純粋な共線・一点縮退を先に弾くため、最遠点から直線距離を確認する。
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

    let newell = Vec3Ecef::new(nx, ny, nz);
    if newell.norm_squared() > 0.0
        && let Some(unit) = newell.normalize()
    {
        return Ok(unit);
    }

    axis.cross(&rel[perp_idx])
        .normalize()
        .ok_or_else(|| GeometryError::PolygonZeroArea.into())
}

/// 2D 投影した単一リングの妥当性（非共線・非自己交差・面積が 0 より大きいこと）を検証し、面積を返す。
pub(crate) fn validate_ring(pts: &[(f64, f64)], eps: f64) -> Result<f64, Error> {
    if is_collinear_2d(pts, eps) {
        return Err(GeometryError::PolygonZeroArea.into());
    }
    if has_self_intersection(pts, eps) {
        return Err(GeometryError::PolygonSelfIntersecting.into());
    }
    let area = libm::fabs(signed_area(pts));
    if area == 0.0 || (eps > 0.0 && area <= 0.5 * eps * eps) {
        return Err(GeometryError::PolygonZeroArea.into());
    }
    Ok(area)
}

/// `hole` が外周 `exterior` の内側（境界上を含む）に収まっているかを判定する。
pub(crate) fn ring_inside_exterior(exterior: &[(f64, f64)], hole: &[(f64, f64)], eps: f64) -> bool {
    let eps2 = eps * eps;

    if !hole.iter().all(|&p| point_in_ring(exterior, p, eps2, true)) {
        return false;
    }

    for (a, b) in ring_edges(hole) {
        let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        if !point_in_ring(exterior, mid, eps2, true) {
            return false;
        }
        for (c, d) in ring_edges(exterior) {
            if segments_cross(a, b, c, d, eps) {
                return false;
            }
        }
    }

    true
}

/// 2 つの穴リングが交差、辺重複、または包含関係にあるかを判定する。
pub(crate) fn holes_overlap(hole_a: &[(f64, f64)], hole_b: &[(f64, f64)], eps: f64) -> bool {
    if !aabb_overlaps(ring_aabb(hole_a), ring_aabb(hole_b), eps) {
        return false;
    }

    for (a, b) in ring_edges(hole_a) {
        for (c, d) in ring_edges(hole_b) {
            if segments_cross(a, b, c, d, eps) || segments_collinear_overlap(a, b, c, d, eps) {
                return true;
            }
        }
    }

    let eps2 = eps * eps;
    has_sample_strictly_inside(hole_a, hole_b, eps2)
        || has_sample_strictly_inside(hole_b, hole_a, eps2)
}

fn has_sample_strictly_inside(src: &[(f64, f64)], target: &[(f64, f64)], eps2: f64) -> bool {
    src.iter().any(|&p| point_in_ring(target, p, eps2, false))
        || ring_edges(src).any(|(a, b)| {
            let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
            point_in_ring(target, mid, eps2, false)
        })
}

fn is_collinear_2d(pts: &[(f64, f64)], eps: f64) -> bool {
    let eps2 = eps * eps;
    let p0 = pts[0];
    let mut a = p0;
    let mut max_d2 = 0.0;
    for &p in &pts[1..] {
        let d2 = dist_sq(p0, p);
        if d2 > max_d2 {
            max_d2 = d2;
            a = p;
        }
    }
    let mut b = a;
    max_d2 = 0.0;
    for &p in pts {
        let d2 = dist_sq(a, p);
        if d2 > max_d2 {
            max_d2 = d2;
            b = p;
        }
    }
    if max_d2 <= eps2 {
        return true;
    }

    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    let mut max_h2 = 0.0;
    for &p in pts {
        let cross = dx * (p.1 - a.1) - dy * (p.0 - a.0);
        let h2 = (cross * cross) / len2;
        if h2 > max_h2 {
            max_h2 = h2;
        }
    }
    max_h2 <= eps2
}

fn has_self_intersection(pts: &[(f64, f64)], eps: f64) -> bool {
    let n = pts.len();
    let eps2 = eps * eps;

    for i in 0..n {
        let prev = pts[(i + n - 1) % n];
        let curr = pts[i];
        let next = pts[(i + 1) % n];

        let ux = prev.0 - curr.0;
        let uy = prev.1 - curr.1;
        let vx = next.0 - curr.0;
        let vy = next.1 - curr.1;
        // 直線上の通過点（内積 <= 0）は辺の分割として許容し、同じ側に戻る折り返しのみ弾く。
        if ux * vx + uy * vy > 0.0
            && (point_segment_distance_sq(next, prev, curr) <= eps2
                || point_segment_distance_sq(prev, curr, next) <= eps2)
        {
            return true;
        }
    }

    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let aabb_i = seg_aabb(a, b);

        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let c = pts[j];
            let d = pts[(j + 1) % n];
            if !aabb_overlaps(aabb_i, seg_aabb(c, d), eps) {
                continue;
            }
            if segments_touch_or_cross(a, b, c, d, eps2) {
                return true;
            }
        }
    }

    false
}

pub(crate) fn signed_area(pts: &[(f64, f64)]) -> f64 {
    let mut area = 0.0;
    for (a, b) in ring_edges(pts) {
        area += a.0 * b.1 - b.0 * a.1;
    }
    area * 0.5
}

fn ring_edges(pts: &[(f64, f64)]) -> impl Iterator<Item = ((f64, f64), (f64, f64))> + '_ {
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(&a, &b)| (a, b))
}

fn point_in_ring(ring: &[(f64, f64)], p: (f64, f64), eps2: f64, include_boundary: bool) -> bool {
    let mut inside = false;
    for (a, b) in ring_edges(ring) {
        if point_segment_distance_sq(p, a, b) <= eps2 {
            return include_boundary;
        }
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
            inside = !inside;
        }
    }
    inside
}

fn segments_touch_or_cross(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
    eps2: f64,
) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);

    if ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0))
        && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
    {
        return true;
    }

    point_segment_distance_sq(a, c, d) <= eps2
        || point_segment_distance_sq(b, c, d) <= eps2
        || point_segment_distance_sq(c, a, b) <= eps2
        || point_segment_distance_sq(d, a, b) <= eps2
}

/// 端点の接触を除いて 2 線分が真に交差するかを判定する。
fn segments_cross(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64), eps: f64) -> bool {
    let straddles = |d1: f64, d2: f64| (d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps);
    straddles(signed_distance(c, a, b), signed_distance(d, a, b))
        && straddles(signed_distance(a, c, d), signed_distance(b, c, d))
}

/// 1 点の頂点接触を除き、2 線分が辺に沿って重なっているかを判定する。
fn segments_collinear_overlap(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
    eps: f64,
) -> bool {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = libm::hypot(dx, dy);
    if len <= eps {
        return false;
    }

    if libm::fabs(signed_distance(c, a, b)) > eps || libm::fabs(signed_distance(d, a, b)) > eps {
        return false;
    }

    let ux = dx / len;
    let uy = dy / len;
    let tc = (c.0 - a.0) * ux + (c.1 - a.1) * uy;
    let td = (d.0 - a.0) * ux + (d.1 - a.1) * uy;
    let (lo, hi) = if tc <= td { (tc, td) } else { (td, tc) };

    let overlap = hi.min(len) - lo.max(0.0);
    overlap > eps
}

fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn signed_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = libm::hypot(dx, dy);
    if len == 0.0 {
        return 0.0;
    }
    (dx * (p.1 - a.1) - dy * (p.0 - a.0)) / len
}

fn point_segment_distance_sq(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let rx = p.0 - (a.0 + t * dx);
    let ry = p.1 - (a.1 + t * dy);
    rx * rx + ry * ry
}

fn dist_sq(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    dx * dx + dy * dy
}

fn seg_aabb(a: (f64, f64), b: (f64, f64)) -> (f64, f64, f64, f64) {
    (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
}

fn ring_aabb(pts: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &(x, y) in pts {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    (min_x, min_y, max_x, max_y)
}

fn aabb_overlaps(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), eps: f64) -> bool {
    a.2 + eps >= b.0 && b.2 + eps >= a.0 && a.3 + eps >= b.1 && b.3 + eps >= a.1
}
