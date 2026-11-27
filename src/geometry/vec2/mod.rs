use core::ops::{Add, Sub};

use crate::{Error, GeometryError};

/// 2D 座標またはベクトル。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    /// 新しい [`Vec2`] を生成する。
    pub(crate) const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// 内積（ドット積）を計算する。
    pub(crate) fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// 2D 外積（`self.x * other.y - self.y * other.x`）を計算する。
    pub(crate) fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// ベクトルの長さの 2 乗を計算する。
    pub(crate) fn norm_sq(self) -> f64 {
        self.dot(self)
    }

    /// 2 点間の距離の 2 乗を計算する。
    pub(crate) fn dist_sq(self, other: Self) -> f64 {
        (other - self).norm_sq()
    }

    /// 線分 `ab` までの距離の 2 乗を返す。
    pub(crate) fn dist_sq_to_segment(self, a: Self, b: Self) -> f64 {
        let ab = b - a;
        let len2 = ab.norm_sq();
        let t = if len2 == 0.0 {
            0.0
        } else {
            ((self - a).dot(ab) / len2).clamp(0.0, 1.0)
        };
        let proj = Self::new(a.x + t * ab.x, a.y + t * ab.y);
        proj.dist_sq(self)
    }

    /// 線分 `ab` と `cd` が許容誤差内で接触または交差するかを判定する。
    pub(crate) fn segments_touch_or_cross(a: Self, b: Self, c: Self, d: Self, eps2: f64) -> bool {
        let ab = b - a;
        let cd = d - c;
        let o1 = ab.cross(c - a);
        let o2 = ab.cross(d - a);
        let o3 = cd.cross(a - c);
        let o4 = cd.cross(b - c);

        if ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0))
            && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
        {
            return true;
        }

        a.dist_sq_to_segment(c, d) <= eps2
            || b.dist_sq_to_segment(c, d) <= eps2
            || c.dist_sq_to_segment(a, b) <= eps2
            || d.dist_sq_to_segment(a, b) <= eps2
    }

    /// 閉リングの符号付き面積（反時計回りで正）を返す。
    pub(crate) fn signed_area(pts: &[Self]) -> f64 {
        let area: f64 = pts
            .iter()
            .zip(pts.iter().cycle().skip(1))
            .map(|(&a, &b)| a.cross(b))
            .sum();
        area * 0.5
    }

    /// 単一リングの妥当性（非共線・非自己交差・面積が 0 より大きいこと）を検証し、面積を返す。
    pub(crate) fn validate_ring(pts: &[Self], eps: f64) -> Result<f64, Error> {
        if Self::is_collinear_ring(pts, eps) {
            return Err(GeometryError::PolygonZeroArea.into());
        }
        if Self::has_self_intersection(pts, eps) {
            return Err(GeometryError::PolygonSelfIntersecting.into());
        }
        let area = libm::fabs(Self::signed_area(pts));
        if area == 0.0 || (eps > 0.0 && area <= 0.5 * eps * eps) {
            return Err(GeometryError::PolygonZeroArea.into());
        }
        Ok(area)
    }

    /// 頂点列が許容誤差 `eps` 以内で一直線上に並んでいるかを判定する。
    fn is_collinear_ring(pts: &[Self], eps: f64) -> bool {
        let eps2 = eps * eps;
        let p0 = pts[0];
        let mut a = p0;
        let mut max_d2 = 0.0;
        for &p in &pts[1..] {
            let d2 = p0.dist_sq(p);
            if d2 > max_d2 {
                max_d2 = d2;
                a = p;
            }
        }
        let mut b = a;
        max_d2 = 0.0;
        for &p in pts {
            let d2 = a.dist_sq(p);
            if d2 > max_d2 {
                max_d2 = d2;
                b = p;
            }
        }
        if max_d2 <= eps2 {
            return true;
        }

        let ab = b - a;
        let len2 = ab.norm_sq();
        let mut max_h2 = 0.0;
        for &p in pts {
            let cross = ab.cross(p - a);
            let h2 = (cross * cross) / len2;
            if h2 > max_h2 {
                max_h2 = h2;
            }
        }
        max_h2 <= eps2
    }

    /// 閉リングに自己交差または折り返しがあるかを判定する。
    fn has_self_intersection(pts: &[Self], eps: f64) -> bool {
        let n = pts.len();
        let eps2 = eps * eps;

        for i in 0..n {
            let prev = pts[(i + n - 1) % n];
            let curr = pts[i];
            let next = pts[(i + 1) % n];

            // 直線上の通過点（内積 <= 0）は辺の分割として許容し、同じ側に戻る折り返しのみ弾く。
            if (prev - curr).dot(next - curr) > 0.0
                && (next.dist_sq_to_segment(prev, curr) <= eps2
                    || prev.dist_sq_to_segment(curr, next) <= eps2)
            {
                return true;
            }
        }

        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let (ax0, ay0, ax1, ay1) = (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));

            for j in (i + 2)..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                let c = pts[j];
                let d = pts[(j + 1) % n];
                let (cx0, cy0, cx1, cy1) = (c.x.min(d.x), c.y.min(d.y), c.x.max(d.x), c.y.max(d.y));
                if ax1 + eps < cx0 || cx1 + eps < ax0 || ay1 + eps < cy0 || cy1 + eps < ay0 {
                    continue;
                }
                if Self::segments_touch_or_cross(a, b, c, d, eps2) {
                    return true;
                }
            }
        }

        false
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}
