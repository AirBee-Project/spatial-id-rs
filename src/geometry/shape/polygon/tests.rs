use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Coordinate, CoverSingleIds, Polygon};

fn sorted_ids(polygon: &Polygon, z: u8) -> Vec<String> {
    let mut ids: Vec<String> = polygon
        .cover_single_ids(z)
        .unwrap()
        .map(|id| id.to_string())
        .collect();
    ids.sort();
    ids
}

mod cover_single_ids {
    use super::*;

    /// 水平面上の四角形ポリゴンを変換する
    #[test]
    fn horizontal_quadrilateral_at_z18() {
        let p0 = Coordinate::new(35.681, 139.766, 10.0).unwrap();
        let p1 = Coordinate::new(35.684, 139.766, 10.0).unwrap();
        let p2 = Coordinate::new(35.684, 139.769, 10.0).unwrap();
        let p3 = Coordinate::new(35.681, 139.769, 10.0).unwrap();
        let polygon = Polygon::new(vec![p0, p1, p2, p3]).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&polygon, 18));
    }

    /// 水平面上の三角形ポリゴンを変換する
    #[test]
    fn horizontal_triangle_polygon_at_z18() {
        let p0 = Coordinate::new(35.681, 139.766, 10.0).unwrap();
        let p1 = Coordinate::new(35.684, 139.766, 10.0).unwrap();
        let p2 = Coordinate::new(35.682, 139.769, 10.0).unwrap();
        let polygon = Polygon::new(vec![p0, p1, p2]).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&polygon, 18));
    }

    /// 頂点数が多い多角形ポリゴンを変換する
    #[test]
    fn pentagon_at_z18() {
        let p0 = Coordinate::new(35.682, 139.767, 10.0).unwrap();
        let p1 = Coordinate::new(35.683, 139.766, 10.0).unwrap();
        let p2 = Coordinate::new(35.684, 139.767, 10.0).unwrap();
        let p3 = Coordinate::new(35.684, 139.769, 10.0).unwrap();
        let p4 = Coordinate::new(35.681, 139.769, 10.0).unwrap();
        let polygon = Polygon::new(vec![p0, p1, p2, p3, p4]).unwrap();
        insta::assert_debug_snapshot!(sorted_ids(&polygon, 18));
    }

    #[test]
    fn polygon_new_epsilon() {
        use crate::Epsilon;
        let p0 = Coordinate::new(35.0, 139.0, 0.0).unwrap();
        let p1 = Coordinate::new(35.0, 139.001, 0.0).unwrap();
        let p_close = Coordinate::new(35.001, 139.000001, 0.0).unwrap();
        let p2 = Coordinate::new(35.001, 139.0, 0.0).unwrap();

        // 許容誤差大: p2 は p_close と同一視されて縮退
        let poly = Polygon::new_epsilon(
            vec![p0, p1, p_close, p2],
            Epsilon::from_meters(1.0).unwrap(),
        )
        .unwrap();
        assert_eq!(poly.vertices().len(), 3);

        // 許容誤差極小: p2 は独立した頂点
        let poly_strict = Polygon::new_epsilon(
            vec![p0, p1, p_close, p2],
            Epsilon::from_meters(1e-6).unwrap(),
        )
        .unwrap();
        assert_eq!(poly_strict.vertices().len(), 4);
    }
}

mod coplanar {
    use crate::{Coordinate, Epsilon, Polygon};

    fn c(lat: f64, lon: f64, alt: f64) -> Coordinate {
        Coordinate::new(lat, lon, alt).unwrap()
    }

    fn base() -> Polygon {
        Polygon::new([
            c(35.0, 139.0, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
        ])
        .unwrap()
    }

    #[test]
    fn same_plane_is_coplanar() {
        let other = Polygon::new([
            c(35.0, 139.0, 0.0),
            c(35.01, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
        ])
        .unwrap();
        assert!(base().is_coplanar(&other));
        assert!(base().is_coplanar(&base()));
    }

    #[test]
    fn tilted_is_not_coplanar() {
        let tilted = Polygon::new([
            c(35.0, 139.0, 0.0),
            c(35.0, 139.01, 100.0),
            c(35.01, 139.0, 0.0),
        ])
        .unwrap();
        assert!(!base().is_coplanar(&tilted));
    }

    #[test]
    fn epsilon_controls_tolerance() {
        // 平面から 0.5 m 浮いた頂点を持つ
        let slightly_off = Polygon::new([
            c(35.0, 139.0, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.01, 139.0, 0.5),
        ])
        .unwrap();
        assert!(!base().is_coplanar(&slightly_off));
        assert!(base().is_coplanar_epsilon(&slightly_off, Epsilon::from_meters(1.0).unwrap()));
        assert!(!base().is_coplanar_epsilon(&slightly_off, Epsilon::ZERO));
    }
}

mod validation {
    use crate::{Coordinate, Epsilon, Error, GeometryError, Polygon};

    fn c(lat: f64, lon: f64, alt: f64) -> Coordinate {
        Coordinate::new(lat, lon, alt).unwrap()
    }

    #[test]
    fn rejects_too_few_vertices() {
        assert_eq!(
            Polygon::new(vec![]).unwrap_err(),
            Error::Geometry(GeometryError::PolygonTooFewVertices { count: 0 })
        );
        assert_eq!(
            Polygon::new(vec![c(35.0, 139.0, 0.0), c(35.0, 139.01, 0.0)]).unwrap_err(),
            Error::Geometry(GeometryError::PolygonTooFewVertices { count: 2 })
        );
        // 始点・終点の重複を除去した後に 2 頂点になる場合
        assert_eq!(
            Polygon::new(vec![
                c(35.0, 139.0, 0.0),
                c(35.0, 139.01, 0.0),
                c(35.0, 139.0, 0.0),
            ])
            .unwrap_err(),
            Error::Geometry(GeometryError::PolygonTooFewVertices { count: 2 })
        );
    }

    #[test]
    fn rejects_zero_area_collinear() {
        // 地球楕円体の曲率による浮きを避け、ECEF 空間でも一直線になる座標で検証する。
        let err = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.0, 139.0, 5.0),
            c(35.0, 139.0, 10.0),
        ])
        .unwrap_err();
        assert_eq!(err, Error::Geometry(GeometryError::PolygonZeroArea));

        let err = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.0, 139.00001, 0.0),
            c(35.0, 139.00002, 0.0),
        ])
        .unwrap_err();
        assert_eq!(err, Error::Geometry(GeometryError::PolygonZeroArea));
    }

    #[test]
    fn rejects_self_intersection_bowtie_and_symmetric_bowtie() {
        let err = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.01, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
            c(35.0, 139.01, 0.0),
        ])
        .unwrap_err();
        assert_eq!(err, Error::Geometry(GeometryError::PolygonSelfIntersecting));

        // Newell 法線が面積の相殺で 0 になる対称な 8 の字も自己交差として弾く必要がある。
        let err = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.01, 139.01, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
        ])
        .unwrap_err();
        assert_eq!(err, Error::Geometry(GeometryError::PolygonSelfIntersecting));
    }

    #[test]
    fn rejects_spike_backtrack_and_repeated_nonadjacent_vertex() {
        let err = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.0, 139.005, 0.0),
            c(35.01, 139.0, 0.0),
        ])
        .unwrap_err();
        assert_eq!(err, Error::Geometry(GeometryError::PolygonSelfIntersecting));

        let err = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.005, 139.005, 0.0),
            c(35.01, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
            c(35.005, 139.005, 0.0),
        ])
        .unwrap_err();
        assert_eq!(err, Error::Geometry(GeometryError::PolygonSelfIntersecting));
    }

    #[test]
    fn accepts_collinear_midpoint_on_edge_and_closed_ring() {
        let poly = Polygon::new(vec![
            c(35.0, 139.0, 0.0),
            c(35.0, 139.005, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.01, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
            c(35.0, 139.0, 0.0),
        ])
        .unwrap();
        assert_eq!(poly.vertices().len(), 5);
    }

    #[test]
    fn accepts_array_and_iterator_inputs() {
        let arr = [
            c(35.0, 139.0, 0.0),
            c(35.0, 139.01, 0.0),
            c(35.01, 139.0, 0.0),
        ];
        let from_arr = Polygon::new(arr).unwrap();
        let from_iter = Polygon::new(arr.iter().copied()).unwrap();
        assert_eq!(from_arr, from_iter);
    }

    #[test]
    fn epsilon_zero_accepts_tiny_polygon() {
        let tiny = vec![
            c(35.0, 139.0, 0.0),
            c(35.0, 139.000000001, 0.0),
            c(35.000000001, 139.0, 0.0),
        ];
        assert!(Polygon::new_epsilon(tiny, Epsilon::ZERO).is_ok());
    }
}
