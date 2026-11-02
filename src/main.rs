use spatial_id::collection::SpatialIdMap;
use spatial_id::query::merge_policy::{Average, Max};
use spatial_id::query::{FalloffPattern, Source};
use spatial_id::{FlexId, RangeId, SingleId};
use std::fmt::Error;
use std::fs;

fn main() {
    let json_str = fs::read_to_string("sample/bldg_risk.json").unwrap();
    let table: SpatialIdMap<u32> = serde_json::from_str(&json_str).unwrap();

    let result = table
        .query()
        //-------------
        .extrude_f(25, 0, 50, Max)
        .falloff_y(25, 10, None, FalloffPattern::Linear, Max)
        .falloff_x(25, 10, None, FalloffPattern::Linear, Max)
        .zoom_out(23, Average)
        //-------------
        .get(..)
        .into_map()
        .unwrap();

    fs::write("output.json", serde_json::to_string(&result).unwrap()).unwrap();
}
