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

/// `FlexId` を見て値を決められる。
#[test]
fn map_sees_the_flex_id() {
    let out: SpatialIdMap<u32> = int_table()
        .query()
        .map(|id, _| id.single_ids().next().unwrap().x())
        .get(..)
        .into_map()
        .unwrap();

    let mut rows: alloc::vec::Vec<(u32, u32)> = out
        .iter()
        .flat_map(|(id, v)| id.single_ids().map(move |s| (s.x(), *v)))
        .collect();
    rows.sort_by_key(|(x, _)| *x);

    assert_eq!(rows, alloc::vec![(10, 10), (11, 11)]);
}

/// `FlexId` と値の両方を条件にして絞り込める。
#[test]
fn filter_sees_the_flex_id_and_value() {
    let mut table = int_table();
    table.insert(single_at_x(12), 20);

    let out: SpatialIdMap<i32> = table
        .query()
        .filter(|id, v| *v > 10 && id.single_ids().all(|s| s.x() != 12))
        .get(..)
        .into_map()
        .unwrap();

    let rows: alloc::vec::Vec<(u32, i32)> = out
        .iter()
        .flat_map(|(id, v)| id.single_ids().map(move |s| (s.x(), *v)))
        .collect();

    assert_eq!(rows, alloc::vec![(11, 15)]);
}

/// `None` を返した要素は取り除かれ、残った要素の値は別の型へ写る。
#[test]
fn filter_map_drops_none_and_changes_the_type() {
    let out: SpatialIdMap<String> = int_table()
        .query()
        .filter_map(|id, v: i32| {
            (v > 10).then(|| format!("{}:{v}", id.single_ids().next().unwrap().x()))
        })
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

    assert_eq!(rows, alloc::vec![(11, "11:15".to_string())]);
}
