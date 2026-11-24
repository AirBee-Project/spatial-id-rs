use alloc::string::{String, ToString};

use crate::{SingleId, Source, SpatialIdMap};

fn single_at_x(x: u32) -> SingleId {
    SingleId::new(20, 0, x, 200).unwrap()
}

fn int_table() -> SpatialIdMap<i32> {
    let mut t = SpatialIdMap::new();
    t.insert(single_at_x(10), 1);
    t.insert(single_at_x(11), 15);
    t
}

/// 数値 → 真偽値のように、値の**型そのもの**を変えられる。
#[test]
fn map_values_changes_the_value_type() {
    let out: SpatialIdMap<bool> = int_table()
        .query()
        .map_values(|v: i32| v > 10)
        .get(..)
        .into_map()
        .unwrap();

    let mut rows: alloc::vec::Vec<(u32, bool)> = out
        .iter()
        .flat_map(|(id, v)| id.single_ids().map(move |s| (s.x(), *v)))
        .collect();
    rows.sort_by_key(|(x, _)| *x);

    assert_eq!(rows, alloc::vec![(10, false), (11, true)]);
}

/// 数値 → 文字列。変換表のような任意の写像が使える。
#[test]
fn map_values_to_text() {
    let out: SpatialIdMap<String> = int_table()
        .query()
        .map_values(|v: i32| if v > 10 { "high" } else { "low" }.to_string())
        .get(..)
        .into_map()
        .unwrap();

    let mut rows: alloc::vec::Vec<(u32, String)> = out
        .iter()
        .flat_map(|(id, v)| {
            let v = v.clone();
            id.single_ids().map(move |s| (s.x(), v.clone()))
        })
        .collect();
    rows.sort_by_key(|(x, _)| *x);

    assert_eq!(
        rows,
        alloc::vec![(10, "low".to_string()), (11, "high".to_string())]
    );
}

/// 変換の前後で他の演算子と組み合わせられる（変換前に絞り、変換後にも絞る）。
#[test]
fn map_values_composes_with_other_operators() {
    let out: SpatialIdMap<bool> = int_table()
        .query()
        .filter_in(10..) // 変換前: 10以上だけ残す
        .map_values(|v: i32| v > 10) // 型変換
        .filter_eq(true) // 変換後: true だけ残す
        .get(..)
        .into_map()
        .unwrap();

    let rows: alloc::vec::Vec<u32> = out
        .iter()
        .flat_map(|(id, _)| id.single_ids().map(|s| s.x()))
        .collect();

    assert_eq!(rows, alloc::vec![11]);
}

/// 部分評価でも型変換が効く（対象領域だけを読んで変換する）。
#[test]
fn map_values_via_partial_get() {
    let query = int_table().query().map_values(|v: i32| v > 10);

    let got: alloc::vec::Vec<bool> = query
        .get(single_at_x(11))
        .map(Result::unwrap)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(got, alloc::vec![true]);

    let got: alloc::vec::Vec<bool> = query
        .get(single_at_x(10))
        .map(Result::unwrap)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(got, alloc::vec![false]);
}

/// 値だけを条件にして絞り込める。
#[test]
fn filter_values_keeps_matching_values() {
    let out: SpatialIdMap<i32> = int_table()
        .query()
        .filter_values(|v| v % 2 == 1)
        .get(..)
        .into_map()
        .unwrap();

    let mut rows: alloc::vec::Vec<(u32, i32)> = out
        .iter()
        .flat_map(|(id, v)| id.single_ids().map(move |s| (s.x(), *v)))
        .collect();
    rows.sort_by_key(|(x, _)| *x);

    assert_eq!(rows, alloc::vec![(10, 1), (11, 15)]);

    let query = int_table().query().filter_values(|v| *v > 100);
    assert!(query.get(..).next().is_none());
}

/// 値だけを見て、`None` の要素を取り除きつつ別の型へ写せる。
#[test]
fn filter_map_values_drops_none_and_changes_the_type() {
    let out: SpatialIdMap<String> = int_table()
        .query()
        .filter_map_values(|v: i32| (v > 10).then(|| v.to_string()))
        .get(..)
        .into_map()
        .unwrap();

    let rows: alloc::vec::Vec<(u32, String)> = out
        .iter()
        .flat_map(|(id, v)| {
            let v = v.clone();
            id.single_ids().map(move |s| (s.x(), v.clone()))
        })
        .collect();

    assert_eq!(rows, alloc::vec![(11, "15".to_string())]);
}
