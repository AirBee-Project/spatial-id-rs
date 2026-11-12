use alloc::string::String;
use alloc::vec::Vec;

use super::super::test::city_model;
use super::{TranClass, TranFile, TranFunction};
use crate::{Coordinate, CoverSingleIds, Point};

pub(in super::super) fn road(id: &str, attributes: &str, traffic_area: &str) -> String {
    format!(
        r#"<core:cityObjectMember><tran:Road gml:id="{id}">
            {attributes}
            <tran:lod1MultiSurface><gml:MultiSurface><gml:surfaceMember>
                <gml:Polygon><gml:exterior><gml:LinearRing><gml:posList>35 139 0 35 139.0001 0 35.0001 139.0001 0 35.0001 139 0 35 139 0</gml:posList></gml:LinearRing></gml:exterior>
                <gml:interior><gml:LinearRing><gml:posList>35.00002 139.00002 0 35.00002 139.00008 0 35.00008 139.00008 0 35.00008 139.00002 0 35.00002 139.00002 0</gml:posList></gml:LinearRing></gml:interior></gml:Polygon>
            </gml:surfaceMember></gml:MultiSurface></tran:lod1MultiSurface>
            {traffic_area}
        </tran:Road></core:cityObjectMember>"#
    )
}

#[test]
fn tran_reads_road_and_keeps_parent_attributes() {
    let traffic_area = r#"<tran:trafficArea><tran:TrafficArea gml:id="ta">
        <tran:function>歩道</tran:function>
        <tran:lod3MultiSurface><gml:MultiSurface><gml:surfaceMember>
            <gml:Polygon><gml:exterior><gml:LinearRing><gml:posList>35 139 0 35 139.00002 0 35.00002 139.00002 0 35 139 0</gml:posList></gml:LinearRing></gml:exterior></gml:Polygon>
        </gml:surfaceMember></gml:MultiSurface></tran:lod3MultiSurface>
    </tran:TrafficArea></tran:trafficArea>"#;
    let file: TranFile = city_model(&road(
        "r1",
        "<tran:class>市町村道</tran:class><tran:function>車道</tran:function>",
        traffic_area,
    ))
    .parse()
    .unwrap();

    let roads = &file.transportations;
    assert_eq!(roads.len(), 1);
    assert_eq!(roads[0].attribute.gml_id, "r1");
    assert_eq!(roads[0].attribute.class, Some(TranClass::MunicipalRoad));
    assert_eq!(roads[0].attribute.function, Some(TranFunction::Carriageway));
    // LOD3 の TrafficArea の面だけが使われる。
    assert_eq!(roads[0].surfaces.len(), 1);
    assert_eq!(roads[0].surfaces[0].vertices().len(), 3);
}

#[test]
fn tran_excludes_holes() {
    // 穴は外周と同じ向きで書いてあり、つなぐ前に向きを反転する必要がある。
    let file: TranFile = city_model(&road("r1", "", "")).parse().unwrap();

    let surfaces = &file.transportations[0].surfaces;
    assert_eq!(surfaces.len(), 1);
    // 外周 4 点、穴 4 点、ブリッジの両端の重複 2 点。
    assert_eq!(surfaces[0].vertices().len(), 10);

    // 高さ 0 の面は f の境界上にあるので、水平位置 (x, y) だけで比べる。
    let xy: Vec<(u32, u32)> = file
        .cover_single_ids(24)
        .unwrap()
        .map(|id| (id.x(), id.y()))
        .collect();
    let cell = |lat, lon| {
        let id = Coordinate::new(lat, lon, 0.0)
            .unwrap()
            .single_id(24)
            .unwrap();
        (id.x(), id.y())
    };
    assert!(xy.contains(&cell(35.00001, 139.00001)));
    assert!(!xy.contains(&cell(35.00005, 139.00005)));

    let map = file.cover_map_with(24, |a| a.gml_id.clone()).unwrap();
    assert!(map.iter().all(|(_, v)| v == "r1"));
}
