pub(crate) mod cancellation;
pub(crate) mod get;

use crate::{SingleId, Source, SpatialIdMap};

/// 同じ(X,Y)・異なるFの2Segmentにextrude_fを適用すると、Max方針で全出力Segmentが最大値になる。
#[test]
fn extrude_f_same_xy_diff_f_resolves_via_policy() {
    let mut table = SpatialIdMap::new();
    table.insert(SingleId::new(10, 0, 100, 100).unwrap(), 10);
    table.insert(SingleId::new(10, 1, 100, 100).unwrap(), 20);

    let out: SpatialIdMap<i32> = table
        .query()
        .extrude_f(10, -1, 1, crate::query::merge_policy::Max)
        .get(..)
        .into_map()
        .unwrap();

    for (_, v) in out.flat_single_ids(None) {
        assert_eq!(*v, 20, "Max(10,20)は全出力Segmentで20になるはず");
    }
}

/// 回帰テスト: extrude_f/x/yは、列（他2軸のfootprint）でまとめて展開してから木を再構築する。
/// 列同士は「他軸で区別されていた粗いSegmentとその内側のネストした細かいSegment」により
/// 互いに素とは限らないため、`from_flexids` へ渡す前の順序を決定的にしておかないと、
/// 同一入力でも実行のたびに結果が変わりうる（実データで実際に再現したバグ）。
#[cfg(feature = "unstable")]
#[test]
fn extrude_result_is_deterministic_across_runs() {
    let bldg_risk: SpatialIdMap<u32> =
        serde_json::from_str(&std::fs::read_to_string("sample/bldg_risk.json").unwrap()).unwrap();

    let mut subset = SpatialIdMap::new();
    for (id, &v) in bldg_risk.iter().take(200) {
        subset.insert(id, v);
    }

    let run_f = |t: SpatialIdMap<u32>| {
        t.query()
            .extrude_f(25, 0, 3, crate::query::merge_policy::Max)
            .get(..)
            .into_map()
            .unwrap()
    };
    let run_x = |t: SpatialIdMap<u32>| {
        t.query()
            .extrude_x(25, 7450000, 7450100, crate::query::merge_policy::Max)
            .get(..)
            .into_map()
            .unwrap()
    };
    let run_y = |t: SpatialIdMap<u32>| {
        t.query()
            .extrude_y(25, 3301000, 3301100, crate::query::merge_policy::Max)
            .get(..)
            .into_map()
            .unwrap()
    };

    for (label, run) in [
        (
            "extrude_f",
            &run_f as &dyn Fn(SpatialIdMap<u32>) -> SpatialIdMap<u32>,
        ),
        ("extrude_x", &run_x),
        ("extrude_y", &run_y),
    ] {
        let a = run(subset.clone());
        let b = run(subset.clone());
        assert_eq!(
            a.flat_single_ids(None).collect::<alloc::vec::Vec<_>>(),
            b.flat_single_ids(None).collect::<alloc::vec::Vec<_>>(),
            "{label}: 同一入力なのに結果が毎回変わる（非決定性あり）",
        );
    }
}
