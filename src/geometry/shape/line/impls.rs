use alloc::vec::Vec;

use crate::{
    Coordinate, Error, LON_ABS_MAX, Line, Point, Shape, SingleId, Vec3, Vec3Ecef,
    geometry::traits::CoverSingleIds,
};

impl Shape for Line {
    fn center(&self) -> Coordinate {
        Coordinate::centroid(self.points)
    }
}

impl CoverSingleIds for Line {
    type Value<'a> = ();

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        let z = z.into();
        let a = self.points[0];
        let b = self.points[1];
        let vec_a: Vec3Ecef = a.into();
        let vec_b: Vec3Ecef = b.into();
        let vec_ba = vec_a - vec_b;
        let distance = vec_ba.norm();
        let (v1, v2) = (a.single_id(z)?, b.single_id(z)?);
        let diff = ((v1.f() - v2.f()).abs()
            + (v1.x() as i32 - v2.x() as i32).abs()
            + (v1.y() as i32 - v2.y() as i32).abs()) as f64;
        let devide_num = 5 + libm::floor(diff / 120.0 + distance / 2000.0) as u16;
        let mut coordinates = Vec::with_capacity(devide_num as usize + 1);
        for i in 0..=devide_num {
            let t = i as f64 / devide_num as f64;
            let v = vec_a.scale(1.0 - t) + vec_b.scale(t);
            let coo: Coordinate = v.try_into()?;
            coordinates.push(coo);
        }
        let estimated_capacity = (diff as usize) + 10;
        let mut voxels: Vec<SingleId> = Vec::with_capacity(estimated_capacity);
        for pair in coordinates.windows(2) {
            let start = pair[0];
            let end = pair[1];
            let line_iter = line_dda(z, start, end)?;
            voxels.pop();
            voxels.extend(line_iter);
        }
        Ok(voxels.into_iter().map(move |id| (id, value.clone())))
    }
}

///DDAを用いたLine関数
fn line_dda(z: u8, a: Coordinate, b: Coordinate) -> Result<impl Iterator<Item = SingleId>, Error> {
    let zoom = crate::id::zoom_level::ZoomLevel::new(z)?;
    let z = zoom.get();
    let origin1 = coordinate_to_matrix(a, z);
    let origin2 = coordinate_to_matrix(b, z);
    let offsets = origin1.map(libm::floor);
    let vp1 = [
        origin1[0] - offsets[0],
        origin1[1] - offsets[1],
        origin1[2] - offsets[2],
    ];
    let vp2 = [
        origin2[0] - offsets[0],
        origin2[1] - offsets[1],
        origin2[2] - offsets[2],
    ];
    let d_total = [
        libm::fabs(vp2[0] - vp1[0]),
        libm::fabs(vp2[1] - vp1[1]),
        libm::fabs(vp2[2] - vp1[2]),
    ];
    let offsets_int = offsets.map(|x| x as i32);
    let max_d = d_total[0].max(d_total[1]).max(d_total[2]);
    let max_flag: usize = if max_d == d_total[0] {
        0
    } else if max_d == d_total[1] {
        1
    } else {
        2
    };
    let other_flag_1 = (max_flag + 1) % 3;
    let other_flag_2 = (max_flag + 2) % 3;
    let i1 = libm::floor(vp1[max_flag]) as i32;
    let j1 = libm::floor(vp1[other_flag_1]) as i32;
    let k1 = libm::floor(vp1[other_flag_2]) as i32;
    let i2 = libm::floor(vp2[max_flag]) as i32;
    let j2 = libm::floor(vp2[other_flag_1]) as i32;
    let k2 = libm::floor(vp2[other_flag_2]) as i32;
    let d_o1 = if vp2[other_flag_1] != vp1[other_flag_1] {
        d_total[max_flag] / d_total[other_flag_1]
    } else {
        f64::INFINITY
    };
    let d_o2 = if vp2[other_flag_2] != vp1[other_flag_2] {
        d_total[max_flag] / d_total[other_flag_2]
    } else {
        f64::INFINITY
    };
    let tm = if i2 > i1 {
        1.0 - vp1[max_flag] + libm::floor(vp1[max_flag])
    } else if i2 == i1 {
        f64::INFINITY
    } else {
        vp1[max_flag] - libm::floor(vp1[max_flag])
    };
    let mut to1 = if j2 > j1 {
        (1.0 - vp1[other_flag_1] + libm::floor(vp1[other_flag_1])) * d_o1 - tm
    } else if j2 == j1 {
        f64::INFINITY
    } else {
        (vp1[other_flag_1] - libm::floor(vp1[other_flag_1])) * d_o1 - tm
    };
    let mut to2 = if k2 > k1 {
        (1.0 - vp1[other_flag_2] + libm::floor(vp1[other_flag_2])) * d_o2 - tm
    } else if k2 == k1 {
        f64::INFINITY
    } else {
        (vp1[other_flag_2] - libm::floor(vp1[other_flag_2])) * d_o2 - tm
    };
    let max_steps = ((i2 - i1).abs() + (j2 - j1).abs() + (k2 - k1).abs()) as usize;
    let pull_index = [
        (3 - max_flag) % 3,
        (3 - other_flag_2) % 3,
        (3 - other_flag_1) % 3,
    ];
    let mut current = [i1, j1, k1];
    let sign_i = (vp2[max_flag] - vp1[max_flag]).signum() as i32;
    let sign_j = (vp2[other_flag_1] - vp1[other_flag_1]).signum() as i32;
    let sign_k = (vp2[other_flag_2] - vp1[other_flag_2]).signum() as i32;
    let mut tm_int = 0;
    let first = SingleId::new(
        z,
        current[pull_index[0]] + offsets_int[0],
        (current[pull_index[1]] + offsets_int[1]) as u32,
        (current[pull_index[2]] + offsets_int[2]) as u32,
    )
    .unwrap();
    let iter = core::iter::once(first).chain((1..=max_steps).map(move |_| {
        let min_wall = (tm_int as f64).min(to1).min(to2);
        if min_wall == tm_int as f64 {
            tm_int += 1;
            current[0] += sign_i;
        } else if min_wall == to1 {
            to1 += d_o1;
            current[1] += sign_j;
        } else {
            to2 += d_o2;
            current[2] += sign_k;
        }
        SingleId::new(
            z,
            current[pull_index[0]] + offsets_int[0],
            (current[pull_index[1]] + offsets_int[1]) as u32,
            (current[pull_index[2]] + offsets_int[2]) as u32,
        )
        .unwrap()
    }));
    Ok(iter)
}

fn coordinate_to_matrix(p: Coordinate, z: u8) -> [f64; 3] {
    let lat = p.latitude();
    let lon = p.longitude();
    let alt = p.altitude();

    // 空間idの高さはz=25でちょうど1mになるように定義されている
    let factor = libm::pow(2_f64, (z as i32 - 25) as f64);
    let f = factor * alt;

    let n = 2u64.pow(z as u32) as f64;
    let x = (lon + LON_ABS_MAX) / (2.0 * LON_ABS_MAX) * n;

    let lat_rad = lat.to_radians();
    let y = (1.0
        - libm::log(libm::tan(lat_rad) + 1.0 / libm::cos(lat_rad)) / core::f64::consts::PI)
        / 2.0
        * n;
    [f, x, y]
}
