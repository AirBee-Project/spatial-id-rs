use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Coordinate, CoverSingleIds, Cylinder};

fn sorted_ids(cylinder: &Cylinder, z: u8) -> Vec<String> {
    let mut ids: Vec<String> = cylinder
        .cover_single_ids(z)
        .unwrap()
        .map(|id| id.to_string())
        .collect();
    ids.sort();
    ids
}

mod cover_single_ids {
    use super::*;

    /// 鉛直方向に伸びる円柱（半径5m、高さ32m）を変換する
    #[test]
    fn vertical_cylinder_at_z18() {
        let start = Coordinate::new(35.681, 139.766, 0.0).unwrap();
        let end = Coordinate::new(35.681, 139.766, 32.0).unwrap();
        let cylinder = Cylinder::new(start, end, 5.0).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&cylinder, 18));
    }

    /// 水平方向に伸びる円柱（半径5m）を変換する
    #[test]
    fn horizontal_cylinder_at_z18() {
        let start = Coordinate::new(35.681, 139.766, 10.0).unwrap();
        let end = Coordinate::new(35.682, 139.767, 10.0).unwrap();
        let cylinder = Cylinder::new(start, end, 5.0).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&cylinder, 18));
    }

    /// 斜め方向に伸びる円柱（半径5m）を変換する
    #[test]
    fn diagonal_cylinder_at_z18() {
        let start = Coordinate::new(35.681, 139.766, 0.0).unwrap();
        let end = Coordinate::new(35.682, 139.767, 32.0).unwrap();
        let cylinder = Cylinder::new(start, end, 5.0).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&cylinder, 18));
    }

    /// cos, sinによる近似曲面の計算で、境界スレスレにある `SingleId` を拾うか拾わないかがOSの標準ライブラリ（libm等）の実装によって揺れるケース。
    #[ignore]
    #[test]
    fn os_rounding_boundary_cylinder_at_z18() {
        let start = Coordinate::new(35.681, 139.766, 0.0).unwrap();
        let end = Coordinate::new(35.681, 139.766, 12.0).unwrap();
        // 微細な半径を設定し、三角関数の結果がそのまま境界となるようにする
        let cylinder = Cylinder::new(start, end, 0.000_000_1).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&cylinder, 18));
    }
}
