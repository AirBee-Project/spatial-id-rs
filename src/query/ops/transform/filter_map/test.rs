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

/// `try_filter_map` は `Ok(None)` を除きつつ変換でき、`Err` で即座に評価を止める。
#[test]
fn try_filter_map_transforms_and_propagates_error() {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};

    let mut table = int_table();
    table.insert(single_at_x(12), 20);

    let ok_out: SpatialIdMap<u32> = table
        .clone()
        .query()
        .try_filter_map(|id, v| {
            let x = id.single_ids().next().unwrap().x();
            Ok::<_, crate::Error>((v >= 15).then_some(x))
        })
        .into_map()
        .unwrap();
    let mut xs: alloc::vec::Vec<u32> = ok_out.iter().map(|(_, v)| *v).collect();
    xs.sort();
    assert_eq!(xs, alloc::vec![11, 12]);

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_closure = calls.clone();
    let err = table
        .query()
        .try_filter_map(move |_, v| {
            calls_in_closure.fetch_add(1, Ordering::Relaxed);
            if v == 1 {
                Err(crate::QueryError::Transform("bad value".to_string()))
            } else {
                Ok(Some(v))
            }
        })
        .into_map()
        .unwrap_err();

    assert_eq!(
        err,
        crate::Error::Query(crate::QueryError::Transform("bad value".to_string()))
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

/// `Err` の前の要素までを順に返し、`Err` で変換を止めて以降は返さない。
#[test]
fn try_filter_map_yields_prior_ok_then_stops_at_error() {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};

    let mut table = SpatialIdMap::new();
    for x in 0..150u32 {
        table.insert(single_at_x(x), x);
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_closure = calls.clone();
    let query = table.query().try_filter_map(move |_, v| {
        calls_in_closure.fetch_add(1, Ordering::Relaxed);
        if v == 100 {
            Err(crate::QueryError::Transform("stop at 100".to_string()))
        } else {
            Ok(Some(v))
        }
    });
    let mut iter = query.get(..);

    let mut seen_ok = 0usize;
    let mut first_err = None;
    for item in iter.by_ref() {
        match item {
            Ok((_, v)) => {
                assert!(v < 100);
                seen_ok += 1;
            }
            Err(e) => {
                first_err = Some(e);
                break;
            }
        }
    }

    assert_eq!(seen_ok, 100);
    assert_eq!(
        first_err,
        Some(crate::Error::Query(crate::QueryError::Transform(
            "stop at 100".to_string()
        )))
    );
    assert!(iter.next().is_none());
    assert_eq!(calls.load(Ordering::Relaxed), 101);
}

/// `par_filter` / `par_map` / `par_filter_map` はチャンク境界を跨いでも逐次版と同じ結果を返す。
#[test]
fn par_filter_and_filter_map_match_sequential_across_chunks() {
    let mut table = SpatialIdMap::new();
    for x in 0..2500u32 {
        table.insert(single_at_x(x), x);
    }

    let expected: SpatialIdMap<u32> = table
        .clone()
        .query()
        .filter(|_, v| *v >= 1200 && *v % 3 == 0)
        .filter_map(|id, v| {
            let x = id.single_ids().next().unwrap().x();
            (v % 2 == 0).then_some(x + v)
        })
        .map(|_, v| v * 2)
        .get(..)
        .into_map()
        .unwrap();

    let actual: SpatialIdMap<u32> = table
        .query()
        .par_filter(|_, v| *v >= 1200 && *v % 3 == 0)
        .par_filter_map(|id, v| {
            let x = id.single_ids().next().unwrap().x();
            (v % 2 == 0).then_some(x + v)
        })
        .par_map(|_, v| v * 2)
        .get(..)
        .into_map()
        .unwrap();

    assert_eq!(actual, expected);
}

/// `par_try_filter_map` は複数要素が `Err` でも入力順で最初の `Err` を返し、以降の要素を返さない。
#[test]
fn par_try_filter_map_stops_at_first_error_in_order() {
    let mut table = SpatialIdMap::new();
    for x in 0..2500u32 {
        table.insert(single_at_x(x), x);
    }

    let query = table.query().par_try_filter_map(|id, v| {
        let x = id.single_ids().next().unwrap().x();
        if v == 1200 || v == 1300 {
            Err(crate::QueryError::Transform(format!("stop at {x}")))
        } else {
            Ok((v % 2 == 0).then_some(v))
        }
    });

    let mut iter = query.get(..);
    let mut seen_ok = 0usize;
    let mut first_err = None;
    for item in iter.by_ref() {
        match item {
            Ok((_, v)) => {
                assert!(v < 1200);
                seen_ok += 1;
            }
            Err(e) => {
                first_err = Some(e);
                break;
            }
        }
    }

    assert_eq!(seen_ok, 600);
    assert_eq!(
        first_err,
        Some(crate::Error::Query(crate::QueryError::Transform(
            "stop at 1200".to_string()
        )))
    );
    assert!(iter.next().is_none());
}

/// 上流で発生した `Err` の前までが並列変換された後、その `Err` が伝播する。
#[test]
fn par_filter_map_propagates_upstream_error_after_prior_items() {
    let mut table = SpatialIdMap::new();
    for x in 0..10u32 {
        table.insert(single_at_x(x), x);
    }

    let query = table
        .query()
        .try_filter_map(|_, v| {
            if v == 5 {
                Err(crate::QueryError::Transform("upstream err".to_string()))
            } else {
                Ok(Some(v))
            }
        })
        .par_map(|_, v| v * 10);

    let mut iter = query.get(..);
    let mut ok_values = alloc::vec::Vec::new();
    let mut err = None;
    for item in iter.by_ref() {
        match item {
            Ok((_, v)) => ok_values.push(v),
            Err(e) => {
                err = Some(e);
                break;
            }
        }
    }

    assert_eq!(ok_values, alloc::vec![0, 10, 20, 30, 40]);
    assert_eq!(
        err,
        Some(crate::Error::Query(crate::QueryError::Transform(
            "upstream err".to_string()
        )))
    );
    assert!(iter.next().is_none());
}
