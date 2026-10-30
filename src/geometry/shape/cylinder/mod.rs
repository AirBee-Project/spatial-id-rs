use alloc::vec::Vec;

pub mod impls;
#[cfg(test)]
mod tests;
use core::f64::consts::PI;

use crate::{Coordinate, Epsilon, Error, GeometryError, Polygon, Solid, Vec3, Vec3Ecef, ZoomLevel};
#[derive(Debug, Clone, Copy, PartialEq)]
/// 3次元空間における円柱を表す型。
///
/// 中心線及び半径によって定義される立体的な領域を表現する。
pub struct Cylinder {
    start: Coordinate,
    end: Coordinate,
    radius_m: f64,
}

impl Cylinder {
    pub fn new(start: Coordinate, end: Coordinate, radius_m: f64) -> Result<Self, Error> {
        if radius_m > 0.0 {
            Ok(Cylinder {
                start,
                end,
                radius_m,
            })
        } else {
            Err(GeometryError::RadiusNegative { radius: radius_m }.into())
        }
    }

    /// 中心線の始点を返す。
    pub fn start(&self) -> Coordinate {
        self.start
    }

    /// 中心線の終点を返す。
    pub fn end(&self) -> Coordinate {
        self.end
    }

    /// 半径（メートル）を返す。
    pub fn radius_m(&self) -> f64 {
        self.radius_m
    }

    /// [Cylinder]を近似した[Solid]を作成する関数
    pub fn rough_solid(&self) -> Solid {
        let polygons = self.rough_surfaces_without_z().collect();
        Solid::with_epsilon(polygons, Epsilon::ZERO).unwrap()
    }

    /// [Cylinder]を近似した立体の表面を、ズームレベルを指定せずに[Polygon]として返す関数
    pub(crate) fn rough_surfaces_without_z(&self) -> impl Iterator<Item = Polygon> {
        let vecs: [Vec3Ecef; 2] = [self.start.into(), self.end.into()];
        let vec_n = vecs[1] - vecs[0];
        let basis = vec_n
            .create_orthonormal_basis()
            .map(|v| v.scale(self.radius_m));
        let divide_num = 57_u32;
        let vertices: Vec<_> = (0..divide_num)
            .map(|i| {
                let theta = 2.0 * PI * i as f64 / divide_num as f64;
                let v = basis[0].scale(libm::cos(theta)) + basis[1].scale(libm::sin(theta));
                [vecs[0] + v, vecs[1] + v]
            })
            .collect();

        // 共有頂点は同じ Vec3 から計算されビット単位で一致するため、許容誤差は不要。
        // DEFAULT を使うと許容誤差より細い円柱が縮退して面が消える。
        let to_coord = |v: Vec3Ecef| Coordinate::try_from(v).unwrap();

        // 側面の頂点リスト（イテレータ）を作成
        let side_surfaces = (0..divide_num).map(|i| {
            let next_i = (i + 1) % divide_num;
            let v_curr = vertices[i as usize];
            let v_next = vertices[next_i as usize];

            Polygon::with_epsilon(
                vec![
                    to_coord(v_curr[0]),
                    to_coord(v_next[0]),
                    to_coord(v_next[1]),
                    to_coord(v_curr[1]),
                ],
                Epsilon::ZERO,
            )
        });

        // 側面を全て集める
        #[allow(clippy::needless_collect)]
        let mut raw_surfaces: Vec<Polygon> = side_surfaces.collect();

        // 底面
        raw_surfaces.push(Polygon::with_epsilon(
            vertices.iter().rev().map(|v| to_coord(v[0])).collect(),
            Epsilon::ZERO,
        ));
        // 上面
        raw_surfaces.push(Polygon::with_epsilon(
            vertices.iter().map(|v| to_coord(v[1])).collect(),
            Epsilon::ZERO,
        ));

        raw_surfaces.into_iter()
    }
    /// [Cylinder]を近似した立体の表面を、ズームレベルを指定してに[Polygon]として返す関数
    pub fn rough_surfaces_with_z(&self, z: ZoomLevel) -> impl Iterator<Item = Polygon> {
        let vecs: [Vec3Ecef; 2] = [self.start.into(), self.end.into()];
        let vec_n = vecs[1] - vecs[0];
        let basis = vec_n
            .create_orthonormal_basis()
            .map(|v| v.scale(self.radius_m));
        let divide_num = (57_u32).max(
            (self.radius_m * PI / (libm::pow(2.0, (25 - u8::from(z) as i16) as f64) * 3.0)) as u32,
        );
        let vertices: Vec<_> = (0..divide_num)
            .map(|i| {
                let theta = 2.0 * PI * i as f64 / divide_num as f64;
                let v = basis[0].scale(libm::cos(theta)) + basis[1].scale(libm::sin(theta));
                [vecs[0] + v, vecs[1] + v]
            })
            .collect();

        // 共有頂点は同じ Vec3 から計算されビット単位で一致するため、許容誤差は不要。
        // DEFAULT を使うと許容誤差より細い円柱が縮退して面が消える。
        let to_coord = |v: Vec3Ecef| Coordinate::try_from(v).unwrap();

        // 側面の頂点リスト（イテレータ）を作成
        let side_surfaces = (0..divide_num).map(|i| {
            let next_i = (i + 1) % divide_num;
            let v_curr = vertices[i as usize];
            let v_next = vertices[next_i as usize];

            Polygon::with_epsilon(
                vec![
                    to_coord(v_curr[0]),
                    to_coord(v_next[0]),
                    to_coord(v_next[1]),
                    to_coord(v_curr[1]),
                ],
                Epsilon::ZERO,
            )
        });

        // 側面を全て集める
        #[allow(clippy::needless_collect)]
        let mut raw_surfaces: Vec<Polygon> = side_surfaces.collect();

        // 底面
        raw_surfaces.push(Polygon::with_epsilon(
            vertices.iter().rev().map(|v| to_coord(v[0])).collect(),
            Epsilon::ZERO,
        ));
        // 上面
        raw_surfaces.push(Polygon::with_epsilon(
            vertices.iter().map(|v| to_coord(v[1])).collect(),
            Epsilon::ZERO,
        ));

        raw_surfaces.into_iter()
    }
}
