use crate::{
    ALT_ABS_MAX, Coordinate, LAT_ABS_MAX, LON_ABS_MAX, Point, RangeId, SingleId, ZoomLevel,
};

#[cfg(not(feature = "temporal_id"))]
mod single_id {
    use super::*;

    /// 緯度がメルカトル投影の境界付近にあるため、OS によって floor の結果がぶれやすい例。
    #[ignore]
    #[test]
    fn boundary_latitude_at_z5() {
        let coord = Coordinate::new(40.979_898_069_620_12, 0.0, 0.0).unwrap();
        insta::assert_debug_snapshot!(coord.single_id(5).unwrap());
    }

    /// 高緯度・高ズームの組み合わせで、投影の最終桁の誤差が出やすい例。
    #[ignore]
    #[test]
    fn near_northern_limit_at_z25() {
        let coord = Coordinate::new(LAT_ABS_MAX, LON_ABS_MAX, 0.0).unwrap();
        insta::assert_debug_snapshot!(coord.single_id(25).unwrap());
    }

    /// 高度の floor がちょうど整数境界に近く、OS 差を拾いやすい例。
    #[ignore]
    #[test]
    fn altitude_boundary_at_z25() {
        let coord = Coordinate::new(35.681_382, 139.766_084, 1.0).unwrap();
        insta::assert_debug_snapshot!(coord.single_id(25).unwrap());
    }

    /// 緯度の境界をわずかにまたぐ入力で、OS ごとの差を拾いやすくする例。
    #[ignore]
    #[test]
    fn latitude_epsilon_boundary_at_z5() {
        let coord = Coordinate::new(40.979_898_069_620_12 + f64::EPSILON, 0.0, 0.0).unwrap();
        insta::assert_debug_snapshot!(coord.single_id(5).unwrap());
    }

    /// 経度の反対側境界付近で、floor の丸め差を確認する例。
    #[ignore]
    #[test]
    fn longitude_dateline_boundary_at_z25() {
        let coord = Coordinate::new(0.0, LON_ABS_MAX - f64::EPSILON, 0.0).unwrap();
        insta::assert_debug_snapshot!(coord.single_id(25).unwrap());
    }
}

mod round_trip {
    use super::*;

    #[test]
    /// Coordinate から Ecef へ変換する際の丸め誤差を確認する例。
    fn coordinate_to_ecef_snapshot() {
        let coord = Coordinate::new(43.068_564, 41.350_713_8, 30.0).unwrap();
        let ecef: crate::Ecef = coord.into();
        insta::assert_debug_snapshot!(ecef);
    }

    #[test]
    /// 別の都市座標でも同様に、OS ごとの差を拾えるか確認する例。
    fn coordinate_to_ecef_snapshot_tokyo() {
        let coord = Coordinate::new(35.681_382, 139.766_084, 10.0).unwrap();
        let ecef: crate::Ecef = coord.into();
        insta::assert_debug_snapshot!(ecef);
    }

    #[test]
    /// 北緯の高い地点では三角関数の丸め誤差が OS 間で出やすい。
    fn coordinate_to_ecef_snapshot_high_latitude() {
        let coord = Coordinate::new(LAT_ABS_MAX - f64::EPSILON, 45.0, 123.0).unwrap();
        let ecef: crate::Ecef = coord.into();
        insta::assert_debug_snapshot!(ecef);
    }
}

mod point_ids {
    use super::*;

    #[test]
    fn single_id_matches_known_value() {
        let coord = Coordinate::new(34.9851603, 135.7584294, 20.0).unwrap();
        assert_eq!(
            coord.single_id(24).unwrap(),
            SingleId::new(24, 10, 14715409, 6646263).unwrap()
        );
    }

    #[test]
    fn single_id_rejects_invalid_zoom() {
        let coord = Coordinate::new(35.0, 139.0, 10.0).unwrap();
        assert!(coord.single_id(31).is_err());
        assert!(coord.range_id(31).is_err());
    }

    /// 経度・高度が範囲の上端ちょうどでも、最後のインデックス値に丸められる。
    #[test]
    fn single_id_clamps_to_last_index() {
        for z in [0u8, 5, 25, 30] {
            let zoom = ZoomLevel::new(z).unwrap();

            let top = Coordinate::new(0.0, LON_ABS_MAX, ALT_ABS_MAX)
                .unwrap()
                .single_id(z)
                .unwrap();
            assert_eq!(top.x(), zoom.xy_max(), "z={z}");
            assert_eq!(top.f(), zoom.f_max(), "z={z}");

            let bottom = Coordinate::new(0.0, -LON_ABS_MAX, -ALT_ABS_MAX)
                .unwrap()
                .single_id(z)
                .unwrap();
            assert_eq!(bottom.x(), 0, "z={z}");
            assert_eq!(bottom.f(), zoom.f_min(), "z={z}");
        }
    }

    /// 緯度の境界でも、有効な `SingleId` が得られる。
    #[test]
    fn single_id_is_valid_at_latitude_limits() {
        for z in [0u8, 5, 25, 30] {
            for lat in [LAT_ABS_MAX, -LAT_ABS_MAX] {
                let id = Coordinate::new(lat, 0.0, 0.0)
                    .unwrap()
                    .single_id(z)
                    .unwrap();
                assert!(
                    SingleId::new(id.z(), id.f(), id.x(), id.y()).is_ok(),
                    "lat={lat}, z={z}"
                );
            }
        }
    }

    #[test]
    fn range_id_is_the_range_of_single_id() {
        let coord = Coordinate::new(35.681382, 139.766084, 10.3).unwrap();
        for z in [0u8, 10, 20, 25] {
            assert_eq!(
                coord.range_id(z).unwrap(),
                RangeId::from(coord.single_id(z).unwrap()),
                "z={z}"
            );
        }
    }
}

mod point_fractional_id {
    use super::*;
    use crate::FractionalId;

    fn sample_coords() -> [Coordinate; 6] {
        [
            Coordinate::new(34.9851603, 135.7584294, 20.0).unwrap(),
            Coordinate::new(35.681382, 139.766084, 10.3).unwrap(),
            Coordinate::new(0.0, 0.0, 0.0).unwrap(),
            Coordinate::new(-12.5, 90.0, -1234.5).unwrap(),
            Coordinate::new(LAT_ABS_MAX, -LON_ABS_MAX, ALT_ABS_MAX).unwrap(),
            Coordinate::new(-LAT_ABS_MAX, LON_ABS_MAX, -ALT_ABS_MAX).unwrap(),
        ]
    }

    #[test]
    fn fractional_id_matches_known_value() {
        let coord = Coordinate::new(34.9851603, 135.7584294, 20.0).unwrap();
        assert_eq!(
            coord.fractional_id(24).unwrap(),
            FractionalId::new(24, 10.0, 14715409.371845974, 6646263.059889234).unwrap()
        );
    }

    /// 小数部分は切り捨てられず、最後のセル内の位置も保たれる。
    #[test]
    fn trait_fractional_id_keeps_fraction() {
        // z=1 では経度 90° が x=1.5（最後のセルの中央）、z=25 では高度 10.5m が f=10.5 に対応する
        let coord = Coordinate::new(0.0, 90.0, 10.5).unwrap();

        let fid = Point::fractional_id(&coord, 1).unwrap();
        assert_eq!(fid.x(), 1.5);

        let fid = Point::fractional_id(&coord, 25).unwrap();
        assert_eq!(fid.f(), 10.5);
    }

    /// 上端ちょうどの座標は、`FractionalId` では上面（`2^z`）として表される。
    #[test]
    fn trait_fractional_id_reaches_upper_face() {
        for z in [0u8, 5, 25, 30] {
            let zoom = ZoomLevel::new(z).unwrap();
            let fid =
                Point::fractional_id(&Coordinate::new(0.0, LON_ABS_MAX, ALT_ABS_MAX).unwrap(), z)
                    .unwrap();
            assert_eq!(fid.x(), zoom.xy_max() as f64 + 1.0, "z={z}");
            assert_eq!(fid.f(), zoom.f_max() as f64 + 1.0, "z={z}");
        }
    }

    #[test]
    fn trait_fractional_id_rejects_invalid_zoom() {
        let coord = Coordinate::new(35.0, 139.0, 10.0).unwrap();
        assert!(Point::fractional_id(&coord, 31).is_err());
    }

    /// `single_id` は `fractional_id` を丸めた結果と一致する。
    #[test]
    fn single_id_is_rounded_fractional_id() {
        for coord in sample_coords() {
            for z in [0u8, 1, 5, 20, 25, 30] {
                let fid: FractionalId = Point::fractional_id(&coord, z).unwrap();
                assert_eq!(
                    coord.single_id(z).unwrap(),
                    fid.round_to_single_id(),
                    "{coord:?}, z={z}"
                );
            }
        }
    }
}

mod centroid {
    use super::*;

    #[test]
    fn centroid_of_max_lat_coordinates() {
        assert_eq!(
            Coordinate::centroid(
                (0..34).map(|i| Coordinate::new(85.0511, -170.0 + 10.0 * i as f64, 0.0).unwrap())
            )
            .latitude,
            85.0511
        );
    }

    #[test]
    fn cetroid_crossing_antimeridian() {
        let a = Coordinate::new(0.0, 179.999, 0.0).unwrap();
        let b = Coordinate::new(0.0, -179.999, 0.0).unwrap();

        assert!(libm::fabs(Coordinate::centroid([a, b].into_iter()).longitude) > 179.99);
    }
}

mod coordinate_raw {
    use super::*;
    use crate::{Epsilon, Point};

    #[test]
    fn coordinate_eq_with_self() {
        let a = Coordinate::new(35.0, 139.0, 10.0).unwrap();
        assert!(a.eq_epsilon(&a, Epsilon::ZERO));
        assert!(a.eq_epsilon(&a, Epsilon::DEFAULT));
    }
}
