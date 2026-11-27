use ::geojson::GeoJson;

use crate::CoverSingleIds;

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
