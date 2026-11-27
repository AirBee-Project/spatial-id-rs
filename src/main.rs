use spatial_id::collection::mem::SpatialIdMap;
use spatial_id::query::Source;
use std::fs;

fn main() {
    let json_str = fs::read_to_string("sample/bldg_risk.json").unwrap();
    let table: SpatialIdMap<u32> = serde_json::from_str(&json_str).unwrap();

    let result = table.query().shift_x(25, 10).get(..).into_map().unwrap();

    fs::write("output.json", serde_json::to_string(&result).unwrap()).unwrap();
}
