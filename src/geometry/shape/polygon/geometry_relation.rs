use alloc::vec::Vec;

use super::ring_plane::signed_area;
use crate::{
    Coordinate, Ecef, ExpandCoordinates, ExpandLines, ExpandTriangles, Line, Polygon, Triangle,
};

impl ExpandCoordinates for Polygon {
    fn expand_coordinates(&self) -> impl Iterator<Item = Coordinate> {
        self.exterior
            .iter()
            .chain(self.interiors.iter().flatten())
            .copied()
    }
}

impl ExpandLines for Polygon {
    fn expand_lines(&self) -> impl Iterator<Item = Line> {
        self.expand_triangles()
            .flat_map(|triangle| triangle.expand_lines().collect::<Vec<_>>())
    }
}

impl ExpandTriangles for Polygon {
    fn expand_triangles(&self) -> impl Iterator<Item = Triangle> {
        let n = self.exterior.len();
        if n < 3 {
            return Vec::<Triangle>::new().into_iter();
        }

        if n == 3 {
            return vec![Triangle::new([
                self.exterior[0],
                self.exterior[1],
                self.exterior[2],
            ])]
            .into_iter();
        }

        // 計算用に全て ECEF に変換
        let ecef_points: Vec<Ecef> = self.exterior.iter().map(|&c| c.into()).collect();

        // 投影軸の決定
        let (u_axis, v_axis) = get_projection_axes(&ecef_points);

        // 2D投影
        let points_2d: Vec<(f64, f64)> = ecef_points
            .iter()
            .map(|p| p.project_2d(u_axis, v_axis))
            .collect();

        // 回転方向の検知
        let area = signed_area(&points_2d);
        let area_sign = if area > 0.0 { 1.0 } else { -1.0 };

        // 耳切りループ
        let mut indices: Vec<usize> = (0..n).collect();
        let mut result = Vec::with_capacity(n - 2);
        let mut count = 0;
        let max_iters = n * n;

        while indices.len() > 3 && count < max_iters {
            let mut ear_found = false;
            let len = indices.len();

            for i in 0..len {
                let prev_idx = indices[(i + len - 1) % len];
                let curr_idx = indices[i];
                let next_idx = indices[(i + 1) % len];

                if is_ear(
                    prev_idx, curr_idx, next_idx, &indices, &points_2d, area_sign,
                ) {
                    result.push(Triangle::new([
                        self.exterior[prev_idx],
                        self.exterior[curr_idx],
                        self.exterior[next_idx],
                    ]));
                    indices.remove(i);
                    ear_found = true;
                    break;
                }
            }
            if !ear_found {
                break;
            }
            count += 1;
        }

        if indices.len() == 3 {
            result.push(Triangle::new([
                self.exterior[indices[0]],
                self.exterior[indices[1]],
                self.exterior[indices[2]],
            ]));
        }

        result.into_iter()
    }
}

/// Newell's Method による法線概算と投影軸の選択
pub(crate) fn get_projection_axes(pts: &[Ecef]) -> (usize, usize) {
    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;

    let len = pts.len();

    for i in 0..len {
        let curr = pts[i];
        let next = pts[(i + 1) % len];

        nx += (curr.y() - next.y()) * (curr.z() + next.z());
        ny += (curr.z() - next.z()) * (curr.x() + next.x());
        nz += (curr.x() - next.x()) * (curr.y() + next.y());
    }

    let ax = libm::fabs(nx);
    let ay = libm::fabs(ny);
    let az = libm::fabs(nz);

    if ax >= ay && ax >= az {
        (1, 2)
    } else if ay >= ax && ay >= az {
        (0, 2)
    } else {
        (0, 1)
    }
}

fn is_ear(
    p: usize,
    c: usize,
    n: usize,
    indices: &[usize],
    pts: &[(f64, f64)],
    area_sign: f64,
) -> bool {
    let a = pts[p];
    let b = pts[c];
    let c_pt = pts[n];

    // 2D外積 (z成分)
    let cross = (b.0 - a.0) * (c_pt.1 - b.1) - (b.1 - a.1) * (c_pt.0 - b.0);

    // 凹判定
    if cross * area_sign <= -1e-10 {
        return false;
    }

    // 包含判定
    for &idx in indices {
        if idx == p || idx == c || idx == n {
            continue;
        }
        if is_point_in_triangle(pts[idx], a, b, c_pt) {
            return false;
        }
    }
    true
}

fn is_point_in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let area2 = 0.5 * (-b.1 * c.0 + a.1 * (-b.0 + c.0) + a.0 * (b.1 - c.1) + b.0 * c.1);
    if libm::fabs(area2) < 1e-12 {
        return false;
    }
    let s = 1.0 / (2.0 * area2) * (a.1 * c.0 - a.0 * c.1 + (c.1 - a.1) * p.0 + (a.0 - c.0) * p.1);
    let t = 1.0 / (2.0 * area2) * (a.0 * b.1 - a.1 * b.0 + (a.1 - b.1) * p.0 + (b.0 - a.0) * p.1);
    s > 0.0 && t > 0.0 && (1.0 - s - t) > 0.0
}
