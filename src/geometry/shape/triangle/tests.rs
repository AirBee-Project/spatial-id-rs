use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Coordinate, CoverSingleIds, Triangle};

fn sorted_ids(tri: &Triangle, z: u8) -> Vec<String> {
    let mut ids: Vec<String> = tri
        .cover_single_ids(z)
        .unwrap()
        .map(|id| id.to_string())
        .collect();
    ids.sort();
    ids
}

mod cover_single_ids {
    use super::*;

    /// 同一高度上に並ぶ水平な三角形を変換する
    #[test]
    fn horizontal_triangle_at_z25() {
        let p0 = Coordinate::new(35.681, 139.766, 10.0).unwrap();
        let p1 = Coordinate::new(35.682, 139.766, 10.0).unwrap();
        let p2 = Coordinate::new(35.681, 139.767, 10.0).unwrap();
        let tri = Triangle::new([p0, p1, p2]);
        insta::assert_debug_snapshot!(sorted_ids(&tri, 25));
    }

    /// 高度方向に傾いた三角形を変換する
    #[test]
    fn tilted_triangle_at_z25() {
        let p0 = Coordinate::new(35.681, 139.766, 0.0).unwrap();
        let p1 = Coordinate::new(35.682, 139.766, 0.0).unwrap();
        let p2 = Coordinate::new(35.681, 139.766, 64.0).unwrap();
        let tri = Triangle::new([p0, p1, p2]);
        insta::assert_debug_snapshot!(sorted_ids(&tri, 25));
    }

    /// 緯度・経度・高度がすべて異なる立体的な三角形を変換する
    #[test]
    fn three_dimensional_triangle_at_z25() {
        let p0 = Coordinate::new(35.681, 139.766, 0.0).unwrap();
        let p1 = Coordinate::new(35.683, 139.767, 0.0).unwrap();
        let p2 = Coordinate::new(35.682, 139.766, 64.0).unwrap();
        let tri = Triangle::new([p0, p1, p2]);
        insta::assert_debug_snapshot!(sorted_ids(&tri, 25));
    }

    /// Triangle を構成する頂点が、ちょうど空間IDの境界付近にあるため、外積・sqrt・floor等の計算過程のわずかな誤差で、隣接する `SingleId` を拾うか拾わないか（OS依存性）が変わるケース。
    #[ignore]
    #[test]
    fn os_rounding_boundary_triangle_at_z25() {
        let p0 = Coordinate::new(35.681, 139.766, 1.0 - f64::EPSILON * 5.0).unwrap();
        let p1 = Coordinate::new(35.681 + f64::EPSILON * 100.0, 139.766, 1.0).unwrap();
        let p2 = Coordinate::new(35.681, 139.766 + f64::EPSILON * 100.0, 1.0).unwrap();
        let tri = Triangle::new([p0, p1, p2]);
        insta::assert_debug_snapshot!(sorted_ids(&tri, 25));
    }
}
