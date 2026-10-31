use crate::{Coordinate, Ecef, FractionalId, Point, ZoomLevel, geometry::traits::CoverSingleIds};

#[test]
fn test_fractional_id_to_single_id() {
    let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    let sid = fid.round_to_single_id();
    assert_eq!(sid.z(), 4);
    assert_eq!(sid.f(), 5);
    assert_eq!(sid.x(), 6);
    assert_eq!(sid.y(), 7);
}

#[test]
fn test_fractional_id_to_ecef() {
    let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    let coord: Coordinate = fid.into();
    let ecef: Ecef = fid.into();

    // Ecef を Coordinate に戻すと、元の座標と（浮動小数点誤差の範囲で）一致する
    let back = Coordinate::from(ecef);
    assert!(libm::fabs(back.latitude() - coord.latitude()) < 1e-9);
    assert!(libm::fabs(back.longitude() - coord.longitude()) < 1e-9);
    assert!(libm::fabs(back.altitude() - coord.altitude()) < 1e-3);
}

#[test]
fn test_cover_single_ids() {
    let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();

    let mut iter = fid.cover_single_ids(4).unwrap();
    let sid = iter.next().unwrap();

    assert_eq!(sid.z(), 4);

    assert!(iter.next().is_none());
}

#[test]
fn round_to_single_id_clamps_upper_boundary() {
    for z in [0u8, 4, 25, 30] {
        let zoom = ZoomLevel::new(z).unwrap();
        // 上端ちょうどの値は有効な FractionalId だが、SingleId としては範囲外になる
        let top = FractionalId::new(
            z,
            zoom.f_max() as f64 + 1.0,
            zoom.xy_max() as f64 + 1.0,
            zoom.xy_max() as f64 + 1.0,
        )
        .unwrap();
        let sid = top.round_to_single_id();
        assert_eq!(sid.f(), zoom.f_max(), "z={z}");
        assert_eq!(sid.x(), zoom.xy_max(), "z={z}");
        assert_eq!(sid.y(), zoom.xy_max(), "z={z}");
    }
}

#[test]
fn round_to_single_id_keeps_lower_boundary() {
    for z in [0u8, 4, 25, 30] {
        let zoom = ZoomLevel::new(z).unwrap();
        let bottom = FractionalId::new(z, zoom.f_min() as f64, 0.0, 0.0).unwrap();
        let sid = bottom.round_to_single_id();
        assert_eq!(sid.f(), zoom.f_min(), "z={z}");
        assert_eq!(sid.x(), 0, "z={z}");
        assert_eq!(sid.y(), 0, "z={z}");
    }
}

mod zoom_conversion {
    use super::*;

    #[test]
    fn into_another_zoomlevel_scales_each_index() {
        let fid = FractionalId::new(4, 5.5, 6.25, 7.75).unwrap();

        let deeper = fid.into_another_zoomlevel(6).unwrap();
        assert_eq!(deeper.z(), 6);
        assert_eq!(deeper.f(), 22.0);
        assert_eq!(deeper.x(), 25.0);
        assert_eq!(deeper.y(), 31.0);

        let shallower = fid.into_another_zoomlevel(2).unwrap();
        assert_eq!(shallower.z(), 2);
        assert_eq!(shallower.f(), 1.375);
        assert_eq!(shallower.x(), 1.5625);
        assert_eq!(shallower.y(), 1.9375);
    }

    #[test]
    fn into_another_zoomlevel_round_trips() {
        let fid = FractionalId::new(4, 5.5, 6.25, 7.75).unwrap();
        for z in [0u8, 2, 4, 10, 30] {
            let converted = fid.into_another_zoomlevel(z).unwrap();
            assert_eq!(converted.into_another_zoomlevel(4).unwrap(), fid, "z={z}");
        }
    }

    #[test]
    fn into_another_zoomlevel_rejects_invalid_zoom() {
        let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
        assert!(fid.into_another_zoomlevel(31).is_err());
    }
}

mod point_single_id {
    use super::*;

    #[test]
    fn converts_to_another_zoomlevel() {
        let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();

        // ズームレベルが 2 深くなると、各インデックスは 4 倍になってから切り捨てられる
        let sid = fid.single_id(6).unwrap();
        assert_eq!(sid.z(), 6);
        assert_eq!(sid.f(), 22);
        assert_eq!(sid.x(), 24);
        assert_eq!(sid.y(), 31);

        // 浅くなる場合は、より粗いセルに含まれる
        let fid = FractionalId::new(6, 22.0, 24.8, 31.2).unwrap();
        let sid = fid.single_id(4).unwrap();
        assert_eq!(sid.z(), 4);
        assert_eq!(sid.f(), 5);
        assert_eq!(sid.x(), 6);
        assert_eq!(sid.y(), 7);
    }

    #[test]
    fn rejects_invalid_zoom() {
        let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
        assert!(fid.single_id(31).is_err());
        assert!(fid.range_id(31).is_err());
    }

    /// `Coordinate` を経由した場合と同じ `SingleId` になる。上端ちょうどの座標も含む。
    #[test]
    fn matches_coordinate_single_id() {
        let coords = [
            Coordinate::new(35.681382, 139.766084, 10.3).unwrap(),
            Coordinate::new(0.0, 0.0, 0.0).unwrap(),
            Coordinate::new(0.0, 180.0, 0.0).unwrap(),
            Coordinate::new(85.0511, -180.0, 33_554_432.0).unwrap(),
            Coordinate::new(-85.0511, 180.0, -33_554_432.0).unwrap(),
        ];
        for coord in coords {
            for z in [0u8, 5, 20, 25] {
                let via_fractional = coord.fractional_id(z).unwrap().single_id(z).unwrap();
                assert_eq!(
                    via_fractional,
                    coord.single_id(z).unwrap(),
                    "{coord:?}, z={z}"
                );
            }
        }
    }
}

mod point_fractional_id {
    use super::*;

    /// 別のズームレベルを指定すると、`into_another_zoomlevel` と同じ結果になり、元の値は変わらない。
    #[test]
    fn matches_into_another_zoomlevel() {
        let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
        let original = fid;
        for z in [0u8, 2, 6, 30] {
            assert_eq!(
                Point::fractional_id(&fid, z).unwrap(),
                fid.into_another_zoomlevel(z).unwrap(),
                "z={z}"
            );
        }
        assert_eq!(fid, original);
    }

    /// `single_id` は `fractional_id` を丸めた結果と一致する。
    #[test]
    fn single_id_is_rounded_fractional_id() {
        let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
        for z in [0u8, 2, 4, 6, 30] {
            assert_eq!(
                fid.single_id(z).unwrap(),
                Point::fractional_id(&fid, z).unwrap().round_to_single_id(),
                "z={z}"
            );
        }
    }
}
