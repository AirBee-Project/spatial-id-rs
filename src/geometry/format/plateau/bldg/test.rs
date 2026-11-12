use alloc::string::String;
use alloc::vec::Vec;

use super::super::test::{city_model, sorted_ids};
use super::super::tran::test::road;
use super::{BldgFile, BldgShape, BldgUsage};
use crate::{CoverRangeIds, CoverSingleIds, SingleId, SpatialIdSet};

/// 緯度 `lat`、経度 139.0 付近の、底面が約 5 m 四方で高さ `h` の直方体の 6 面。
fn box_faces(lat: f64, h: f64) -> String {
    let (a, b) = (lat, lat + 0.00005);
    let (c, d) = (139.0, 139.00005);
    let ring = |pts: [(f64, f64, f64); 4]| {
        let mut s = String::from("<gml:Polygon><gml:exterior><gml:LinearRing><gml:posList>");
        for (la, lo, al) in pts.iter().chain(core::iter::once(&pts[0])) {
            s += &format!("{la} {lo} {al} ");
        }
        s + "</gml:posList></gml:LinearRing></gml:exterior></gml:Polygon>"
    };
    let faces = [
        [(a, c, 0.0), (a, d, 0.0), (b, d, 0.0), (b, c, 0.0)],
        [(a, c, h), (b, c, h), (b, d, h), (a, d, h)],
        [(a, c, 0.0), (b, c, 0.0), (b, c, h), (a, c, h)],
        [(a, d, 0.0), (a, d, h), (b, d, h), (b, d, 0.0)],
        [(a, c, 0.0), (a, c, h), (a, d, h), (a, d, 0.0)],
        [(b, c, 0.0), (b, d, 0.0), (b, d, h), (b, c, h)],
    ];
    faces.into_iter().map(ring).collect()
}

/// 直方体の 1 面だけ。閉じた立体にならない。
fn open_faces(lat: f64, h: f64) -> String {
    box_faces(lat, h)
        .split("</gml:Polygon>")
        .next()
        .unwrap()
        .to_owned()
        + "</gml:Polygon>"
}

fn lod2(faces: &str) -> String {
    format!(
        "<bldg:boundedBy><bldg:WallSurface><bldg:lod2MultiSurface><gml:MultiSurface>{faces}</gml:MultiSurface></bldg:lod2MultiSurface></bldg:WallSurface></bldg:boundedBy>"
    )
}

fn building(id: &str, lat: f64, lod1: &str, lod2: &str) -> String {
    format!(
        r#"<core:cityObjectMember><bldg:Building gml:id="{id}">
            <bldg:class codeSpace="x">3001</bldg:class>
            <bldg:usage codeSpace="x">411</bldg:usage>
            <bldg:measuredHeight uom="m">9.5</bldg:measuredHeight>
            <bldg:lod0FootPrint><gml:MultiSurface><gml:surfaceMember>
                <gml:Polygon><gml:exterior><gml:LinearRing><gml:posList>{lat} 139 0 {lat} 139.1 0 35.1 139.1 0 {lat} 139 0</gml:posList></gml:LinearRing></gml:exterior></gml:Polygon>
            </gml:surfaceMember></gml:MultiSurface></bldg:lod0FootPrint>
            <bldg:lod1Solid><gml:Solid><gml:exterior><gml:CompositeSurface>{lod1}</gml:CompositeSurface></gml:exterior></gml:Solid></bldg:lod1Solid>
            {lod2}
            <uro:buildingIDAttribute><uro:BuildingIDAttribute>
                <uro:buildingID>13101-bldg-1</uro:buildingID>
                <uro:prefecture codeSpace="x">13</uro:prefecture>
                <uro:city codeSpace="x">13101</uro:city>
            </uro:BuildingIDAttribute></uro:buildingIDAttribute>
        </bldg:Building></core:cityObjectMember>"#
    )
}

#[test]
fn bldg_reads_attributes_and_lod1_solid() {
    let file: BldgFile = city_model(&building("b1", 35.0, &box_faces(35.0, 10.0), ""))
        .parse()
        .unwrap();

    assert_eq!(file.buildings.len(), 1);
    let attribute = &file.buildings[0].attribute;
    assert_eq!(attribute.gml_id, "b1");
    assert_eq!(attribute.class_code, "3001");
    assert_eq!(attribute.usage, Some(BldgUsage::Retail));
    assert_eq!(attribute.measured_height, Some(9.5));
    assert_eq!(attribute.uro_building_id, "13101-bldg-1");
    assert_eq!(attribute.uro_prefecture_code.as_deref(), Some("13"));
    assert_eq!(attribute.uro_city_code, "13101");
    assert!(matches!(file.buildings[0].shape, BldgShape::Solid(_)));
    assert!(file.cover_single_ids(24).unwrap().next().is_some());
}

#[test]
fn bldg_falls_back_to_lod1_when_lod2_is_open() {
    let lod1_only: BldgFile = city_model(&building("b1", 35.0, &box_faces(35.0, 10.0), ""))
        .parse()
        .unwrap();
    let with_open_lod2: BldgFile = city_model(&building(
        "b1",
        35.0,
        &box_faces(35.0, 10.0),
        &lod2(&open_faces(35.0, 20.0)),
    ))
    .parse()
    .unwrap();

    assert_eq!(
        sorted_ids(with_open_lod2.cover_single_ids(24).unwrap()),
        sorted_ids(lod1_only.cover_single_ids(24).unwrap())
    );
}

#[test]
fn bldg_covers_surfaces_when_no_lod_is_closed() {
    let file: BldgFile = city_model(&building(
        "open",
        35.0,
        &open_faces(35.0, 10.0),
        &lod2(&open_faces(35.0, 20.0)),
    ))
    .parse()
    .unwrap();

    // 最も高い LOD（LOD2）の面だけが残る。
    let BldgShape::Surfaces(surfaces) = &file.buildings[0].shape else {
        panic!("closed solid was not expected");
    };
    assert_eq!(surfaces.len(), 1);
    let single = sorted_ids(file.cover_single_ids(24).unwrap());
    assert!(!single.is_empty());
    let range = sorted_ids(
        file.cover_range_ids(24)
            .unwrap()
            .flat_map(|r| r.single_ids().collect::<Vec<_>>()),
    );
    assert_eq!(range, single);
}

#[test]
fn bldg_ignores_other_features() {
    let file: BldgFile = city_model(&road("r1", "<tran:class>市町村道</tran:class>", ""))
        .parse()
        .unwrap();
    assert!(file.buildings.is_empty());
}

#[test]
fn bldg_file_covers_every_building() {
    let file: BldgFile = city_model(
        &(building("b1", 35.0, &box_faces(35.0, 10.0), "")
            + &building("b2", 35.001, &box_faces(35.001, 30.0), "")),
    )
    .parse()
    .unwrap();

    let mut expected: Vec<(SingleId, String)> = file
        .buildings
        .iter()
        .flat_map(|b| {
            let id = b.attribute.gml_id.clone();
            b.shape
                .cover_single_ids(24)
                .unwrap()
                .map(move |s| (s, id.clone()))
        })
        .collect();
    expected.sort();
    let mut actual: Vec<(SingleId, String)> = file
        .cover_single_ids_with(24, |a| a.gml_id.clone())
        .unwrap()
        .collect();
    actual.sort();
    assert_eq!(actual, expected);

    // 2 棟は離れていて値が衝突しないので、各 ID の値は 1 通りに決まる。
    let map = file.cover_map_with(24, |a| a.gml_id.clone()).unwrap();
    for (id, value) in &expected {
        assert!(map.get(*id).all(|(_, v)| v == value));
    }
    let covered: SpatialIdSet = map.into();
    let expected_set: SpatialIdSet = expected.iter().map(|(id, _)| *id).collect();
    assert_eq!(covered, expected_set);

    let from_ranges = sorted_ids(
        file.cover_range_ids(24)
            .unwrap()
            .flat_map(|r| r.single_ids().collect::<Vec<_>>()),
    );
    assert_eq!(
        from_ranges,
        sorted_ids(expected.into_iter().map(|(id, _)| id))
    );
}
