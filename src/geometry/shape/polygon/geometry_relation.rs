use alloc::vec::Vec;

#[cfg(feature = "unstable")]
use i_triangle::float::triangulatable::Triangulatable;

use crate::{
    Coordinate, Ecef, ExpandCoordinates, ExpandLines, ExpandTriangles, Line, Polygon, Triangle,
    Vec2,
};
#[cfg(feature = "unstable")]
use crate::{Vec3, Vec3Ecef};

impl ExpandCoordinates for Polygon {
    fn expand_coordinates(&self) -> impl Iterator<Item = Coordinate> {
        #[cfg(feature = "unstable")]
        {
            self.vertices
                .iter()
                .chain(self.holes.iter().flatten())
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
        }
        #[cfg(not(feature = "unstable"))]
        {
            self.vertices.clone().into_iter()
        }
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
        let n = self.vertices.len();
        if n < 3 {
            return Vec::<Triangle>::new().into_iter();
        }

        #[cfg(feature = "unstable")]
        if !self.holes.is_empty() {
            return self.triangulate_with_holes().into_iter();
        }

        if n == 3 {
            return vec![Triangle::new([
                self.vertices[0],
                self.vertices[1],
                self.vertices[2],
            ])]
            .into_iter();
        }

        // 計算用に全て ECEF に変換
        let ecef_points: Vec<Ecef> = self.vertices.iter().map(|&c| c.into()).collect();

        // 投影軸の決定
        let (u_axis, v_axis) = get_projection_axes(&ecef_points);

        // 2D投影
        let points_2d: Vec<Vec2> = ecef_points
            .iter()
            .map(|p| {
                let (x, y) = p.project_2d(u_axis, v_axis);
                Vec2::new(x, y)
            })
            .collect();

        // 回転方向の検知
        let area = Vec2::signed_area(&points_2d);
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
                        self.vertices[prev_idx],
                        self.vertices[curr_idx],
                        self.vertices[next_idx],
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
                self.vertices[indices[0]],
                self.vertices[indices[1]],
                self.vertices[indices[2]],
            ]));
        }

        result.into_iter()
    }
}

#[cfg(feature = "unstable")]
impl Polygon {
    /// 穴付きポリゴンを 2D 平面へ投影し、`i_triangle` で三角形分割して 3D `Triangle` 列を返す。
    fn triangulate_with_holes(&self) -> Vec<Triangle> {
        let origin = Vec3Ecef::from(self.vertices[0]);
        let rel: Vec<Vec3Ecef> = self
            .vertices
            .iter()
            .map(|&c| Vec3Ecef::from(c) - origin)
            .collect();
        let normal = Self::newell_normal(&rel).unwrap_or_else(|| Vec3Ecef::new(0.0, 0.0, 1.0));
        let [u, v] = normal.create_orthonormal_basis();

        let project = |ring: &[Coordinate]| -> Vec<Vec2> {
            ring.iter()
                .map(|&c| {
                    let r = Vec3Ecef::from(c) - origin;
                    Vec2::new(r.dot(&u), r.dot(&v))
                })
                .collect()
        };

        let mut outer = project(&self.vertices);
        Vec2::ensure_ccw(&mut outer);

        let mut contours: Vec<Vec<[f64; 2]>> = Vec::with_capacity(1 + self.holes.len());
        contours.push(outer.into_iter().map(|p| [p.x, p.y]).collect());
        for hole in &self.holes {
            let mut pts = project(hole);
            if Vec2::signed_area(&pts) > 0.0 {
                pts.reverse();
            }
            contours.push(pts.into_iter().map(|p| [p.x, p.y]).collect());
        }

        let tri = contours.triangulate_as::<i64>().to_triangulation::<usize>();
        let coords: Vec<Coordinate> = tri
            .points
            .into_iter()
            .filter_map(|[x, y]| Coordinate::try_from(origin + u.scale(x) + v.scale(y)).ok())
            .collect();
        if coords.len() < tri.indices.iter().copied().max().map_or(0, |m| m + 1) {
            return Vec::new();
        }

        tri.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[a, b, c]| Triangle::new([coords[a], coords[b], coords[c]]))
            .collect()
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

fn is_ear(p: usize, c: usize, n: usize, indices: &[usize], pts: &[Vec2], area_sign: f64) -> bool {
    let a = pts[p];
    let b = pts[c];
    let c_pt = pts[n];

    let cross = (b - a).cross(c_pt - b);
    if cross * area_sign <= -1e-10 {
        return false;
    }

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

fn is_point_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let area2 = (b - a).cross(c - a);
    if libm::fabs(area2) < 1e-12 {
        return false;
    }
    let s = (p - a).cross(c - a) / area2;
    let t = (b - a).cross(p - a) / area2;
    s > 0.0 && t > 0.0 && (1.0 - s - t) > 0.0
}
