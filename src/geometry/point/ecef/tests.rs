use crate::Ecef;

mod ecef_to_coordinate {
    use super::*;

    /// OSによって差が出てしまう計算の例。
    #[ignore]
    #[test]
    fn base_case_snapshot() {
        let ecef = Ecef::new(
            3_503_254.636_950_15,
            3_083_182.692_474_858,
            4_333_089.862_951_96,
        )
        .unwrap();
        let coord = crate::Coordinate::from(ecef);
        insta::assert_debug_snapshot!(coord);
    }

    /// 別の入力でも Bowring 反復の微小な誤差を拾うための例。
    #[ignore]
    #[test]
    fn west_coast_case_snapshot() {
        let ecef = Ecef::new(
            -2_514_383.499_181_965,
            -4_660_897.973_149_88,
            3_567_065.128_947_878,
        )
        .unwrap();
        let coord = crate::Coordinate::from(ecef);
        insta::assert_debug_snapshot!(coord);
    }

    /// 地球中心からの距離が大きい入力でも、OS 差による末尾の揺れを確認する例。
    #[ignore]
    #[test]
    fn high_altitude_case_snapshot() {
        let ecef = Ecef::new(
            4_075_576.132_840_001,
            3_073_465.945_881,
            4_862_865.221_340_001,
        )
        .unwrap();
        let coord = crate::Coordinate::from(ecef);
        insta::assert_debug_snapshot!(coord);
    }

    /// 赤道付近で X/Y のわずかな差が OS 間で出やすい入力。
    #[ignore]
    #[test]
    fn equatorial_case_snapshot() {
        let ecef = Ecef::new(6_378_137.0, f64::EPSILON, 0.0).unwrap();
        let coord = crate::Coordinate::from(ecef);
        insta::assert_debug_snapshot!(coord);
    }
}

/// 範囲の境界にある地理座標を ECEF に変換して戻しても、範囲外にならないことを確認する。
mod boundary_round_trip {
    use crate::{ALT_ABS_MAX, Coordinate, Ecef, LAT_ABS_MAX, Vec3, Vec3Ecef};

    const LATITUDES: [f64; 5] = [LAT_ABS_MAX, -LAT_ABS_MAX, 0.0, 35.0, -60.0];
    const ALTITUDES: [f64; 4] = [ALT_ABS_MAX, 0.0, -10_000.0, 1_000_000.0];

    fn boundary_coordinates() -> impl Iterator<Item = Coordinate> {
        LATITUDES.into_iter().flat_map(|lat| {
            ALTITUDES.into_iter().flat_map(move |alt| {
                (-180..=180)
                    .step_by(15)
                    .map(move |lon| Coordinate::new(lat, lon as f64, alt).unwrap())
            })
        })
    }

    #[test]
    fn vec3_ecef_to_coordinate_succeeds() {
        for coord in boundary_coordinates() {
            let vec: Vec3Ecef = coord.into();
            assert!(Coordinate::try_from(vec).is_ok(), "{coord:?}");
        }
    }

    #[test]
    fn ecef_can_be_recreated_from_components() {
        for coord in boundary_coordinates() {
            let ecef: Ecef = coord.into();
            assert!(Ecef::new(ecef.x(), ecef.y(), ecef.z()).is_ok(), "{coord:?}");
        }
    }

    #[test]
    fn ecef_to_coordinate_stays_in_range() {
        for coord in boundary_coordinates() {
            let ecef: Ecef = coord.into();
            let back = Coordinate::from(ecef);
            assert!(
                Coordinate::new(back.latitude(), back.longitude(), back.altitude()).is_ok(),
                "{coord:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn far_out_of_range_is_still_rejected() {
        // 許容幅（1 cm）を大きく超える高度はエラーのまま
        let coord_vec: Vec3Ecef = Coordinate::new(35.0, 135.0, ALT_ABS_MAX).unwrap().into();
        let scaled = coord_vec.scale(1.001);
        assert!(Coordinate::try_from(scaled).is_err());
    }
}

/// `Ecef::new` と各 setter が、`Coordinate` と同じ範囲の点だけを受け付けることを確認する。
mod validation {
    use rand::RngExt;
    use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};

    use crate::{Coordinate, Ecef, LON_ABS_MAX, Point, Vec3, Vec3Ecef};

    /// 早期リターンを使わない、厳密な判定。
    fn is_valid_exact(x: f64, y: f64, z: f64) -> bool {
        Coordinate::try_from(Vec3Ecef::new(x, y, z)).is_ok()
    }

    /// 範囲の検証なしで、緯度・経度・高度から ECEF の成分を作る。
    fn xyz(lat: f64, lon: f64, alt: f64) -> (f64, f64, f64) {
        let ecef = Ecef::from(unsafe { Coordinate::new_unchecked(lat, lon, alt) });
        (ecef.x(), ecef.y(), ecef.z())
    }

    #[test]
    fn accepts_points_near_the_surface() {
        for (lat, lon) in [
            (0.0, 0.0),
            (35.0, 135.0),
            (-60.0, -120.0),
            (85.0, 10.0),
            (-85.0, 170.0),
        ] {
            let (x, y, z) = xyz(lat, lon, 0.0);
            assert!(Ecef::new(x, y, z).is_ok(), "lat={lat}, lon={lon}");
        }
    }

    #[test]
    fn rejects_degenerate_points() {
        // 地球の中心
        assert!(Ecef::new(0.0, 0.0, 0.0).is_err());
        // 極（緯度 90°）
        assert!(Ecef::new(0.0, 0.0, 6_356_752.0).is_err());
        // 有限でない値
        assert!(Ecef::new(f64::NAN, 0.0, 0.0).is_err());
        assert!(Ecef::new(f64::INFINITY, 0.0, 0.0).is_err());
        assert!(Ecef::new(0.0, f64::NEG_INFINITY, 0.0).is_err());
        assert!(Ecef::new(0.0, 0.0, f64::INFINITY).is_err());
    }

    /// 早期リターンの簡易判定が、厳密な判定と常に同じ結果になることを確認する。
    #[test]
    fn new_agrees_with_exact_check() {
        let mut rng = ChaCha8Rng::seed_from_u64(0xEC3F);
        for _ in 0..100_000 {
            // 空間全体からの一様な点
            let (x, y, z) = (
                rng.random_range(-4.5e7..4.5e7),
                rng.random_range(-4.5e7..4.5e7),
                rng.random_range(-4.5e7..4.5e7),
            );
            assert_eq!(
                Ecef::new(x, y, z).is_ok(),
                is_valid_exact(x, y, z),
                "({x}, {y}, {z})"
            );

            // 緯度・高度の範囲の内外にまたがる点
            let (x, y, z) = xyz(
                rng.random_range(-87.0..87.0),
                rng.random_range(-LON_ABS_MAX..LON_ABS_MAX),
                rng.random_range(-7.0e6..3.4e7),
            );
            assert_eq!(
                Ecef::new(x, y, z).is_ok(),
                is_valid_exact(x, y, z),
                "({x}, {y}, {z})"
            );
        }
    }

    /// setter の結果が、同じ成分で `Ecef::new` した場合と一致することを確認する。
    /// 成功したら新しい値になり、失敗したら元の値のまま変わらない。
    #[test]
    fn setters_agree_with_new() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x5E7);
        for _ in 0..20_000 {
            let (x, y, z) = xyz(
                rng.random_range(-85.0..85.0),
                rng.random_range(-LON_ABS_MAX..LON_ABS_MAX),
                rng.random_range(-1.0e5..3.3e7),
            );
            let start = Ecef::new(x, y, z).unwrap();

            for axis in 0..3 {
                let current = [x, y, z][axis];
                // 一様な値と、元の値の近くの値を半々で試す
                let new_value = if rng.random_range(0.0_f64..1.0) < 0.5 {
                    rng.random_range(-4.5e7..4.5e7)
                } else {
                    current + rng.random_range(-3.0e6..3.0e6)
                };

                let mut target = [x, y, z];
                target[axis] = new_value;
                let expected = Ecef::new(target[0], target[1], target[2]);

                let mut actual = start;
                let result = match axis {
                    0 => actual.set_x(new_value),
                    1 => actual.set_y(new_value),
                    _ => actual.set_z(new_value),
                };

                assert_eq!(
                    result.is_ok(),
                    expected.is_ok(),
                    "axis={axis}, start={start:?}, new={new_value}"
                );
                match expected {
                    Ok(expected) => assert_eq!(actual, expected),
                    Err(_) => assert_eq!(actual, start),
                }
            }
        }
    }

    /// 元の点が有効でも、成分を変えた結果が範囲外ならエラーになる。
    #[test]
    fn setters_reject_out_of_range_values() {
        // x を大きくすると高度が上限を超える
        let mut ecef = Ecef::new(1.0e6, 0.0, 6.0e6).unwrap();
        assert!(ecef.set_x(3.9875e7).is_err());
        assert_eq!((ecef.x(), ecef.y(), ecef.z()), (1.0e6, 0.0, 6.0e6));

        // x を小さくすると緯度が上限を超える
        let mut ecef = Ecef::new(2.7e6, 0.0, 3.0e7).unwrap();
        assert!(ecef.set_x(6.0e5).is_err());
        assert_eq!((ecef.x(), ecef.y(), ecef.z()), (2.7e6, 0.0, 3.0e7));

        // y を大きくすると高度が上限を超える
        let mut ecef = Ecef::new(0.0, 1.0e6, 6.0e6).unwrap();
        assert!(ecef.set_y(3.9875e7).is_err());
        assert_eq!((ecef.x(), ecef.y(), ecef.z()), (0.0, 1.0e6, 6.0e6));

        // z を大きくすると緯度も高度も上限を超える
        let mut ecef = Ecef::new(2.7e6, 0.0, 3.0e7).unwrap();
        assert!(ecef.set_z(4.0e7).is_err());
        assert_eq!((ecef.x(), ecef.y(), ecef.z()), (2.7e6, 0.0, 3.0e7));
    }

    #[test]
    fn point_ids_match_coordinate() {
        // 高度や経度がセルの境界ちょうどにならない値を使う
        let coord = Coordinate::new(35.681382, 139.766084, 10.3).unwrap();
        let ecef: Ecef = coord.into();

        for z in [0u8, 5, 10, 20, 25] {
            assert_eq!(
                ecef.single_id(z).unwrap(),
                coord.single_id(z).unwrap(),
                "z={z}"
            );
            assert_eq!(
                ecef.range_id(z).unwrap(),
                coord.range_id(z).unwrap(),
                "z={z}"
            );
        }
        assert!(ecef.single_id(31).is_err());
        assert!(ecef.range_id(31).is_err());
    }
}

mod point_fractional_id {
    use crate::{Coordinate, Ecef, Point};

    fn sample_ecefs() -> [Ecef; 4] {
        [
            Coordinate::new(35.681382, 139.766084, 10.3).unwrap().into(),
            Coordinate::new(0.0, 0.0, 0.0).unwrap().into(),
            Coordinate::new(-12.5, 90.0, -1234.5).unwrap().into(),
            Ecef::new(6_378_137.0, 0.0, 0.0).unwrap(),
        ]
    }

    /// `Ecef` の `fractional_id` は、`Coordinate` に変換してから求めた値と一致する。
    #[test]
    fn matches_coordinate() {
        for ecef in sample_ecefs() {
            let coord = Coordinate::from(ecef);
            for z in [0u8, 5, 20, 25, 30] {
                assert_eq!(
                    Point::fractional_id(&ecef, z).unwrap(),
                    Point::fractional_id(&coord, z).unwrap(),
                    "{ecef:?}, z={z}"
                );
            }
        }
    }

    #[test]
    fn rejects_invalid_zoom() {
        let ecef = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
        assert!(Point::fractional_id(&ecef, 31).is_err());
    }

    #[test]
    fn eq_epsilon() {
        use crate::{Epsilon, Point};
        let e1 = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
        let e2 = Ecef::new(6_378_137.0, 1e-11, 0.0).unwrap();
        assert!(e1.eq_epsilon(&e2, Epsilon::DEFAULT));
        assert!(!e1.eq_epsilon(&e2, Epsilon::from_meters(1e-12).unwrap()));
    }
}
