use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Coordinate, CoverSingleIds, Polygon, Solid};

fn sorted_ids(solid: &Solid, z: u8) -> Vec<String> {
    let mut ids: Vec<String> = solid
        .cover_single_ids(z)
        .unwrap()
        .map(|id| id.to_string())
        .collect();
    ids.sort();
    ids
}

/// 閉じた四面体（テトラヘドロン）を生成するヘルパー
fn tetrahedron(p0: Coordinate, p1: Coordinate, p2: Coordinate, p3: Coordinate) -> Solid {
    let faces = vec![
        Polygon::new(vec![p0, p2, p1]), // 底面
        Polygon::new(vec![p0, p1, p3]), // 前面
        Polygon::new(vec![p1, p2, p3]), // 右面
        Polygon::new(vec![p2, p0, p3]), // 左面
    ];
    Solid::new(faces).expect("tetrahedron must be watertight")
}

mod cover_single_ids {
    use super::*;

    /// 小さな四面体（辺 ~111m）の内部ボクセルを変換する
    #[test]
    fn small_tetrahedron_at_z16() {
        let p0 = Coordinate::new(35.000, 139.000, 10.0).unwrap();
        let p1 = Coordinate::new(35.001, 139.000, 10.0).unwrap();
        let p2 = Coordinate::new(35.000, 139.001, 10.0).unwrap();
        let p3 = Coordinate::new(35.000, 139.000, 110.0).unwrap();
        let solid = tetrahedron(p0, p1, p2, p3);
        insta::assert_debug_snapshot!(sorted_ids(&solid, 16));
    }

    /// 高度方向に細長い四面体を変換する
    #[test]
    fn tall_tetrahedron_at_z16() {
        let p0 = Coordinate::new(35.000, 139.000, 0.0).unwrap();
        let p1 = Coordinate::new(35.001, 139.000, 0.0).unwrap();
        let p2 = Coordinate::new(35.000, 139.001, 0.0).unwrap();
        let p3 = Coordinate::new(35.000, 139.000, 500.0).unwrap();
        let solid = tetrahedron(p0, p1, p2, p3);
        insta::assert_debug_snapshot!(sorted_ids(&solid, 16));
    }

    #[test]
    fn solid_with_epsilon() {
        use crate::Epsilon;
        let p0 = Coordinate::new(35.000, 139.000, 0.0).unwrap();
        let p1 = Coordinate::new(35.001, 139.000, 0.0).unwrap();
        let p2 = Coordinate::new(35.000, 139.001, 0.0).unwrap();
        let p3 = Coordinate::new(35.000, 139.000, 500.0).unwrap();

        let faces = vec![
            Polygon::new(vec![p0, p2, p1]),
            Polygon::new(vec![p0, p1, p3]),
            Polygon::new(vec![p1, p2, p3]),
            Polygon::new(vec![p2, p0, p3]),
        ];
        let solid = Solid::with_epsilon(faces, Epsilon::from_meters(0.001).unwrap());
        assert!(solid.is_ok());
    }
}
