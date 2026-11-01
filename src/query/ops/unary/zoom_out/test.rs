use crate::query::merge_policy::{Average, Max};
use crate::{SingleId, Source, SpatialIdMap, ZoomLevel};
use alloc::vec;
use alloc::vec::Vec;

#[test]
fn zoom_out_average_8_children() {
    let mut table = SpatialIdMap::<i32>::new();

    // Z=20 の同じ親（Z=19）に属する8つのボクセルを配置
    // 親IDは (z: 19, f: 0, x: 0, y: 0) になるように、子IDの x, y は 0 または 1 にする
    let id_000 = SingleId::new(20, 0, 0, 0).unwrap();
    let id_001 = SingleId::new(20, 0, 0, 1).unwrap();
    let id_010 = SingleId::new(20, 0, 1, 0).unwrap();
    let id_011 = SingleId::new(20, 0, 1, 1).unwrap();
    let id_100 = SingleId::new(20, 1, 0, 0).unwrap();
    let id_101 = SingleId::new(20, 1, 0, 1).unwrap();
    let id_110 = SingleId::new(20, 1, 1, 0).unwrap();
    let id_111 = SingleId::new(20, 1, 1, 1).unwrap();

    table.insert(id_000, 10);
    table.insert(id_001, 20);
    table.insert(id_010, 30);
    table.insert(id_011, 40);
    table.insert(id_100, 50);
    table.insert(id_101, 60);
    table.insert(id_110, 70);
    table.insert(id_111, 80);

    // ZoomOut to Z=19 (8 children -> 1 parent)
    let out = table
        .query()
        .zoom_out(ZoomLevel::new(19).unwrap(), Average)
        .get(..)
        .into_map()
        .unwrap();

    let parent = SingleId::new(19, 0, 0, 0).unwrap();
    let result = out.get(parent).next().unwrap().1;

    // (10+20+30+40+50+60+70+80) / 8 = 360 / 8 = 45
    assert_eq!(*result, 45);
}

#[test]
fn zoom_out_max_partial_children() {
    let mut table = SpatialIdMap::<i32>::new();

    // 親が同じ 3つのボクセルだけ配置
    let id_000 = SingleId::new(20, 0, 0, 0).unwrap();
    let id_011 = SingleId::new(20, 0, 1, 1).unwrap();
    let id_111 = SingleId::new(20, 1, 1, 1).unwrap();

    table.insert(id_000, 10);
    table.insert(id_011, 99);
    table.insert(id_111, 5);

    // ZoomOut to Z=19 using Max policy
    let out = table.query().zoom_out(19, Max).get(..).into_map().unwrap();

    let parent = SingleId::new(19, 0, 0, 0).unwrap();
    let result = out.get(parent).next().unwrap().1;

    // Max of {10, 99, 5} is 99
    assert_eq!(*result, 99);
}

#[test]
fn zoom_out_multiple_parents_commutative_and_non_commutative() {
    use crate::FlexId;
    use crate::query::merge_policy::Sum;

    let mut table = SpatialIdMap::<i32>::new();
    // 親1: (z: 1, f: 0, x: 0, y: 0) に属する子
    table.insert(FlexId::new(2, 0, 2, 0, 2, 0).unwrap(), 10);
    table.insert(FlexId::new(2, 0, 2, 1, 2, 0).unwrap(), 20);

    // 親2: (z: 1, f: 0, x: 1, y: 1) に属する子
    table.insert(FlexId::new(2, 0, 2, 2, 2, 2).unwrap(), 30);
    table.insert(FlexId::new(2, 0, 2, 3, 2, 3).unwrap(), 40);

    let sum_out: Vec<_> = table
        .clone()
        .query()
        .zoom_out(1, Sum)
        .get(..)
        .into_map()
        .unwrap()
        .iter()
        .map(|(id, v)| (id, *v))
        .collect();

    let avg_out: Vec<_> = table
        .query()
        .zoom_out(1, Average)
        .get(..)
        .into_map()
        .unwrap()
        .iter()
        .map(|(id, v)| (id, *v))
        .collect();

    let p1 = FlexId::new(1, 0, 1, 0, 1, 0).unwrap();
    let p2 = FlexId::new(1, 0, 1, 1, 1, 1).unwrap();

    assert_eq!(sum_out, vec![(p1, 30), (p2, 70)]);
    assert_eq!(avg_out, vec![(p1, 15), (p2, 35)]);
}

#[test]
fn zoom_out_lazily_short_circuits() {
    use crate::FlexId;
    use crate::query::ops::unary::zoom_out::ZoomOut;
    use crate::query::{query_iter::QueryIter, traits::UnaryOperator};

    struct PanicAfterFirst {
        count: usize,
    }

    impl Iterator for PanicAfterFirst {
        type Item = Result<(FlexId, i32), crate::Error>;
        fn next(&mut self) -> Option<Self::Item> {
            self.count += 1;
            match self.count {
                1 => Some(Ok((FlexId::new(2, 0, 2, 0, 2, 0).unwrap(), 10))),
                2 => Some(Ok((FlexId::new(2, 0, 2, 2, 2, 0).unwrap(), 20))),
                _ => panic!("lazy evaluation short-circuit failed"),
            }
        }
    }

    let op = ZoomOut::<i32, Max>::new(ZoomLevel::new(1).unwrap());
    let iter = QueryIter::from_ordered(PanicAfterFirst { count: 0 });
    let mut out = op.run(iter).unwrap();

    let first = out.next().unwrap().unwrap();
    assert_eq!(first.0, FlexId::new(1, 0, 1, 0, 1, 0).unwrap());
    assert_eq!(first.1, 10);
    // 最初の親を取得した段階でイテレータを drop
    drop(out);
}

#[test]
fn zoom_out_propagates_stream_error() {
    use crate::FlexId;
    use crate::query::ops::unary::zoom_out::ZoomOut;
    use crate::query::{query_iter::QueryIter, traits::UnaryOperator};

    let items = [
        Ok((FlexId::new(2, 0, 2, 0, 2, 0).unwrap(), 1)),
        Err(crate::Error::Query(crate::QueryError::SourceRead(
            "stream error".into(),
        ))),
    ];
    let iter = QueryIter::from_ordered(items.into_iter());
    let op = ZoomOut::<i32, Max>::new(ZoomLevel::new(1).unwrap());
    let mut out = op.run(iter).unwrap();

    let first = out.next().unwrap();
    // 最初の親の完了前にエラーが来た場合はエラーが直ちに返る
    assert!(first.is_err());
}

#[cfg(feature = "temporal_id")]
#[test]
fn zoom_out_merges_same_parent_split_by_finer_t() {
    use crate::FlexId;
    use crate::query::merge_policy::Sum;

    // 入力の列は a, b, c の順で、親が同じ a と c の間に、親が違う b が挟まる
    let a = FlexId::new(6, 0, 5, 0, 5, 0)
        .unwrap()
        .with_time_segment(6, 0)
        .unwrap();
    let b = FlexId::new(6, 0, 5, 0, 5, 0)
        .unwrap()
        .with_time_segment(6, 1)
        .unwrap();
    let c = FlexId::new(6, 1, 5, 0, 5, 0)
        .unwrap()
        .with_time_segment(6, 0)
        .unwrap();
    let mut table = SpatialIdMap::<i32>::new();
    table.insert(a, 1);
    table.insert(b, 10);
    table.insert(c, 100);

    for out in [
        table.clone().query().zoom_out(5, Sum).get(..),
        table.clone().query().zoom_out(5, Average).get(..),
    ] {
        let out: Vec<_> = out.collect::<Result<_, _>>().unwrap();
        let parent = FlexId::new(5, 0, 5, 0, 5, 0).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, parent.with_time_segment(6, 0).unwrap());
        assert_eq!(out[1].0, parent.with_time_segment(6, 1).unwrap());
        assert_eq!(out[1].1, 10);
    }
    let sum = table.query().zoom_out(5, Sum).get(..).into_map().unwrap();
    let parent = FlexId::new(5, 0, 5, 0, 5, 0).unwrap();
    assert_eq!(
        sum.get(parent.with_time_segment(6, 0).unwrap())
            .next()
            .unwrap()
            .1,
        &101
    );
}

#[cfg(feature = "temporal_id")]
#[test]
fn zoom_out_resolves_parents_overlapping_in_t() {
    use crate::FlexId;
    use crate::query::merge_policy::Sum;

    // 親の T が T6 idx0 と、それに含まれる T8 idx1 になり、親どうしが重なる
    let a = FlexId::new(6, 0, 5, 0, 5, 0)
        .unwrap()
        .with_time_segment(6, 0)
        .unwrap();
    let b = FlexId::new(6, 1, 5, 0, 5, 0)
        .unwrap()
        .with_time_segment(8, 1)
        .unwrap();
    let mut table = SpatialIdMap::<i32>::new();
    table.insert(a, 1);
    table.insert(b, 10);

    let out = table.query().zoom_out(5, Sum).get(..).into_map().unwrap();
    let parent = FlexId::new(5, 0, 5, 0, 5, 0).unwrap();
    let mut expected = SpatialIdMap::<i32>::new();
    expected.insert(parent.with_time_segment(6, 0).unwrap(), 1);
    expected.insert_with(parent.with_time_segment(8, 1).unwrap(), 10, |x, y| x + y);
    assert_eq!(out, expected);
}

#[test]
fn zoom_out_sum_matches_inserting_each_parent() {
    use crate::FlexId;
    use crate::query::merge_policy::Sum;

    let mut seed = 0x2001_u64;
    let mut next = move |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    for _ in 0..200 {
        let mut table = SpatialIdMap::<i32>::new();
        for _ in 0..20 {
            let (fz, xz, yz) = (next(8) as u8, next(8) as u8, next(8) as u8);
            let tz = if cfg!(feature = "temporal_id") {
                next(8) as u8
            } else {
                0
            };
            let id = FlexId::new(
                fz,
                next(2 << fz) as i32 - (1 << fz),
                xz,
                next(1 << xz) as u32,
                yz,
                next(1 << yz) as u32,
            )
            .unwrap()
            .with_time_segment(tz, next(1 << tz))
            .unwrap();
            table.insert(id, next(10) as i32 + 1);
        }
        let target_z = next(6) as u8;

        let out = table
            .clone()
            .query()
            .zoom_out(target_z, Sum)
            .get(..)
            .into_map()
            .unwrap();

        let mut expected = SpatialIdMap::<i32>::new();
        for (id, &v) in table.iter() {
            let parent = id.spatial_parent_at_zoom(target_z).unwrap();
            expected.insert_with(parent, v, |a, b| a + b);
        }
        assert_eq!(out, expected);
    }
}
