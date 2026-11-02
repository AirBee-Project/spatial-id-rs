//! 道路と建物のリスク値を Query で拡散・合成し、ドローンリスクマップを作る例。
//!
//! ```sh
//! cargo run --release --example riskmap --features unstable
//! ```

use spatial_id::collection::SpatialIdMap;
use spatial_id::query::merge_policy::{Average, Max};
use spatial_id::query::{FalloffPattern, Source};
use std::fs;

fn load(path: &str) -> SpatialIdMap<u32> {
    let json = fs::read_to_string(path).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn main() {
    let tran = load("sample/riskmap/tran_risk.json");
    let bldg = load("sample/riskmap/bldg_risk.json");

    let tran_risk = tran
        .query()
        .extrude_f(25, 0, 50, Max)
        .falloff_x(25, 10, None, FalloffPattern::QuadraticIn, Max)
        .falloff_y(25, 10, None, FalloffPattern::QuadraticIn, Max);

    let bldg_risk = bldg
        .query()
        .extrude_f(25, 0, 50, Max)
        .falloff_x(25, 10, None, FalloffPattern::QuadraticIn, Max)
        .falloff_y(25, 10, None, FalloffPattern::QuadraticIn, Max);

    let risk = bldg_risk
        .merge(tran_risk, Max)
        .zoom_out(23, Average)
        .filter_in(0..100)
        .get(..)
        .into_map()
        .unwrap();

    fs::write("riskmap.json", serde_json::to_string(&risk).unwrap()).unwrap();
}
