use alloc::vec::Vec;

use crate::{
    Coordinate, Ecef, Point, Shape, SingleId, SpatialId, Sphere, WGS84_A,
    geometry::traits::CoverSingleIds,
};

impl Shape for Sphere {
    fn center(&self) -> Coordinate {
        self.center
    }
}

/// 球（左）を、それが覆う空間ID（右）へ変換する。図はズームレベル 21 の例。
///
/// ![球と、それが覆う空間ID][sphere_cover]
#[doc = doc_image!("sphere_cover", "docs/images/sphere_cover.jpg")]
impl CoverSingleIds for Sphere {
    type Value<'a> = ();

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, crate::Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        let zoom = crate::id::zoom_level::ZoomLevel::new(z.into())?;
        let z = zoom.get();

        let center = self.center;
        let radius = self.radius_m;

        let voxel_diag_half = voxel_length_xy(z) * libm::sqrt(3.0) / 2.0;
        let center_ecef: Ecef = (center).into();

        // 球の8頂点 -> 探索範囲推定
        let mut corners = Vec::with_capacity(8);
        for &sx in &[1.0, -1.0] {
            for &sy in &[1.0, -1.0] {
                for &sz in &[1.0, -1.0] {
                    let e = Ecef::new(
                        center_ecef.x() + radius * sx,
                        center_ecef.y() + radius * sy,
                        center_ecef.z() + radius * sz,
                    )?;

                    corners.push(e.single_id(z)?);
                }
            }
        }

        let x_min = corners.iter().map(|v| v.x()).min().unwrap();
        let x_max = corners.iter().map(|v| v.x()).max().unwrap();
        let y_min = corners.iter().map(|v| v.y()).min().unwrap();
        let y_max = corners.iter().map(|v| v.y()).max().unwrap();
        let f_min = corners.iter().map(|v| v.f()).min().unwrap();
        let f_max = corners.iter().map(|v| v.f()).max().unwrap();

        Ok((x_min..=x_max)
            .flat_map(move |x| {
                (y_min..=y_max).flat_map(move |y| {
                    (f_min..=f_max).map(move |f| SingleId::new(z, f, x, y).unwrap())
                })
            })
            .filter(move |id| {
                let p: Coordinate = id.spatial_center();
                center.distance(&p) <= radius + voxel_diag_half
            })
            .map(move |id| (id, value.clone())))
    }
}

/// ズームレベル `z` における、水平方向（X/Y、値は同じ）のボクセル1辺の長さ`[m]`。
///
/// 赤道周長 `2πa` をズームレベルで等分した値。
fn voxel_length_xy(z: u8) -> f64 {
    let n = libm::pow(2_f64, (z as i32) as f64);
    2.0 * core::f64::consts::PI * WGS84_A / n
}
