use ::geojson::GeoJson;

use crate::{Coordinate, CoverSingleIds, Error, GeometryError, Point};

#[test]
fn cover_map_with_returns_error_of_invalid_feature() {
    let geojson: GeoJson = r#"{
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": null,
                "geometry": { "type": "Point", "coordinates": [139.0, 35.0] }
            },
            {
                "type": "Feature",
                "properties": null,
                "geometry": { "type": "Point", "coordinates": [139.0, 100.0] }
            }
        ]
    }"#
    .parse()
    .unwrap();

    assert!(geojson.cover_map_with(20, |_| ()).is_err());
}

#[test]
fn cover_map_with_passes_none_for_bare_geometry() {
    let geojson: GeoJson = r#"{ "type": "Point", "coordinates": [139.0, 35.0] }"#
        .parse()
        .unwrap();

    let map = geojson
        .cover_map_with(20, |feature| feature.is_none())
        .unwrap();
    assert!(map.iter().all(|(_, v)| *v));
}

#[test]
fn cover_map_borrows_feature() {
    let geojson: GeoJson = r#"{
        "type": "Feature",
        "properties": { "name": "A" },
        "geometry": { "type": "Point", "coordinates": [139.0, 35.0] }
    }"#
    .parse()
    .unwrap();
    let GeoJson::Feature(feature) = &geojson else {
        unreachable!()
    };

    let map = geojson.cover_map(20).unwrap();
    assert!(map.iter().all(|(_, v)| *v == Some(feature)));
}

#[test]
fn polygon_with_hole_excludes_interior_and_keeps_boundary() {
    let geojson: GeoJson = r#"{
        "type": "Polygon",
        "coordinates": [
            [[139.0, 35.0, 10.0], [139.02, 35.0, 10.0], [139.02, 35.02, 10.0], [139.0, 35.02, 10.0], [139.0, 35.0, 10.0]],
            [[139.005, 35.005, 10.0], [139.005, 35.015, 10.0], [139.015, 35.015, 10.0], [139.015, 35.005, 10.0], [139.005, 35.005, 10.0]]
        ]
    }"#
    .parse()
    .unwrap();

    let set = geojson.cover_set(18).unwrap();
    let hole_center = Coordinate::new(35.01, 139.01, 10.0)
        .unwrap()
        .single_id(18)
        .unwrap();
    let hole_corner = Coordinate::new(35.005, 139.005, 10.0)
        .unwrap()
        .single_id(18)
        .unwrap();
    let exterior_ring_pt = Coordinate::new(35.002, 139.002, 10.0)
        .unwrap()
        .single_id(18)
        .unwrap();

    assert!(set.get(hole_center).next().is_none());
    assert!(set.get(hole_corner).next().is_some());
    assert!(set.get(exterior_ring_pt).next().is_some());
}

#[test]
fn polygon_rejects_hole_outside_exterior_and_self_intersection() {
    let outside: GeoJson = r#"{
        "type": "Polygon",
        "coordinates": [
            [[139.0, 35.0], [139.01, 35.0], [139.01, 35.01], [139.0, 35.01], [139.0, 35.0]],
            [[139.02, 35.02], [139.02, 35.03], [139.03, 35.03], [139.03, 35.02], [139.02, 35.02]]
        ]
    }"#
    .parse()
    .unwrap();
    assert_eq!(
        outside.cover_set(18).unwrap_err(),
        Error::Geometry(GeometryError::HoleOutsideExterior { index: 0 })
    );

    let self_intersecting: GeoJson = r#"{
        "type": "Polygon",
        "coordinates": [
            [[139.0, 35.0], [139.01, 35.01], [139.01, 35.0], [139.0, 35.01], [139.0, 35.0]]
        ]
    }"#
    .parse()
    .unwrap();
    assert_eq!(
        self_intersecting.cover_set(18).unwrap_err(),
        Error::Geometry(GeometryError::PolygonSelfIntersecting)
    );

    let overlapping_holes: GeoJson = r#"{
        "type": "Polygon",
        "coordinates": [
            [[139.0, 35.0], [139.02, 35.0], [139.02, 35.02], [139.0, 35.02], [139.0, 35.0]],
            [[139.002, 35.002], [139.002, 35.01], [139.01, 35.01], [139.01, 35.002], [139.002, 35.002]],
            [[139.005, 35.005], [139.005, 35.015], [139.015, 35.015], [139.015, 35.005], [139.005, 35.005]]
        ]
    }"#
    .parse()
    .unwrap();
    assert_eq!(
        overlapping_holes.cover_set(18).unwrap_err(),
        Error::Geometry(GeometryError::HoleOverlap {
            first: 0,
            second: 1
        })
    );
}
