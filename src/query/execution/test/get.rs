#[cfg(test)]
use alloc::vec::Vec;

use crate::{FlexId, Source, SpatialIdMap};

/// `shift_x` の逆算が、shift のズームより粗いズームへ周期境界（経度方向の折り返し）を
/// 丸め込む際、以前は丸め込み後に隙間が消えているかどうかの判定を誤り、必要な入力領域を
/// 取りこぼすことがあった（x軸の半分しか入力として要求しない）。その結果、部分評価の `get` が
/// 全体の評価と食い違う値を返していた。
#[test]
fn get_shift_x_wrapped_coarsen_matches_whole() {
    let mut table = SpatialIdMap::<u32>::new();
    // z=2 の x=3（東西の折り返し境界寄り）にデータを置く。
    table.insert(FlexId::new(2, 0, 2, 3, 2, 0).unwrap(), 7);

    let expected: SpatialIdMap<u32> = table
        .clone()
        .query()
        .shift_x(2, -1)
        .get(..)
        .into_map()
        .unwrap();
    let expected_values: Vec<u32> = expected.flat_single_ids(None).map(|(_, v)| *v).collect();

    // shift のズーム(2)より粗い z=1 の x 全域をターゲットにする。
    let target = crate::RangeId::new(1, [-2, 1], [0, 1], [0, 1]).unwrap();
    let lazy_values: Vec<u32> = table
        .query()
        .shift_x(2, -1)
        .get(target)
        .map(Result::unwrap)
        .map(|(_, v)| v)
        .collect();

    assert_eq!(lazy_values, expected_values);
}

/// x系演算子を2つ連鎖すると、1つ目の演算子の`reverse_target`が返す中間`bounds`が
/// 折り返し状態（`x[0] > x[1]`）になることがある。木の枝刈り
/// （`Node::overlapping_children_range`）がこの規約を考慮していなかったため、
/// 部分評価の `get` が全体の評価と食い違う（データを取りこぼす）ことがあった。
#[test]
fn get_chained_shift_x_wrapped_intermediate_bounds_matches_whole() {
    let mut table = SpatialIdMap::<u32>::new();
    table.insert(FlexId::new(2, 0, 2, 2, 2, 0).unwrap(), 9);

    let expected: SpatialIdMap<u32> = table
        .clone()
        .query()
        .shift_x(2, -3)
        .shift_x(2, -2)
        .get(..)
        .into_map()
        .unwrap();
    let expected_values: Vec<u32> = expected.flat_single_ids(None).map(|(_, v)| *v).collect();

    // ターゲットは shift と同じズーム(2)の x 全域。中間 bounds が折り返し表現になる。
    let target = crate::RangeId::new(2, 0, [0, 3], 0).unwrap();
    let lazy_values: Vec<u32> = table
        .query()
        .shift_x(2, -3)
        .shift_x(2, -2)
        .get(target)
        .map(Result::unwrap)
        .map(|(_, v)| v)
        .collect();

    assert_eq!(lazy_values, expected_values);
}

#[test]
fn partial_get_matches_whole() {
    let mut table = SpatialIdMap::<u32>::new();
    let flex_id = FlexId::new(10, 10, 10, 10, 10, 10).unwrap();
    table.insert(flex_id, 42);

    // Normal run
    let expected_result: SpatialIdMap<u32> = table
        .clone()
        .query()
        .shift_x(10, 1)
        .shift_y(10, 2)
        .get(..)
        .into_map()
        .unwrap();

    let target = FlexId::new(10, 10, 10, 11, 10, 12).unwrap();

    let expected_val = expected_result.get(target).next().map(|(_, v)| *v);

    // LazyView get
    let query = table.query().shift_x(10, 1).shift_y(10, 2);

    let mut lazy_iter = query.get(target).map(Result::unwrap);
    let lazy_val = lazy_iter.next().map(|(_, v)| v);
    assert_eq!(lazy_iter.next(), None);
    assert_eq!(expected_val, lazy_val);
}

#[test]
fn get_returns_only_existing_values() {
    let mut table = SpatialIdMap::new();
    // 1箇所だけ値を入れる
    let id1 = FlexId::new(10, 10, 10, 10, 10, 10).unwrap();
    table.insert(id1, 100);

    let query = table.query();

    // id1 と、隣接する別のID (値がない) を含む RangeId
    let target = crate::RangeId::new(10, [10, 10], [10, 11], [10, 10]).unwrap();

    let results: Vec<_> = query.get(target).map(Result::unwrap).collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, 100);
}

/// `get` は時間軸でも絞り込む。
///
/// 木の走査は時間Segmentの2分割境界で枝刈りするが、ターゲットの秒区間とSegment境界は一致するとは
/// 限らないので、はみ出した葉が残りうる。最終フィルタ（[`FlexId::intersects_range`]）が
/// 時間を見ていないと、同じFlexIdの**別時刻の値**がそのまま返ってしまう。
#[cfg(feature = "temporal_id")]
#[test]
fn get_filters_by_time() {
    use crate::{Interval, SingleId, SpatialIdMap};
    use alloc::collections::BTreeSet;

    let time_segment = |t: u64| {
        SingleId::new(10, 0, 5, 5)
            .unwrap()
            .with_time(Interval::HOUR, t)
            .unwrap()
    };

    let mut table: SpatialIdMap<i32> = SpatialIdMap::new();
    table.insert(time_segment(0), 1); // [0, 3600)
    table.insert(time_segment(10), 2); // [36000, 39600)

    for (target, expected) in [(time_segment(0), 1), (time_segment(10), 2)] {
        let values: BTreeSet<i32> = table
            .clone()
            .query()
            .get(target)
            .map(Result::unwrap)
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            values,
            [expected].into_iter().collect::<BTreeSet<_>>(),
            "他の時刻の値が混ざっている"
        );
    }
}

#[test]
fn get_directional_falloff_matches_whole() {
    use crate::Side;
    use crate::query::merge_policy::Sum;
    use crate::query::ops::unary::falloff::FalloffPattern;

    let mut table = SpatialIdMap::<u32>::new();
    table.insert(FlexId::new(4, 2, 4, 2, 4, 2).unwrap(), 10);

    for side in [Some(Side::Upper), Some(Side::Lower), None] {
        let q_f = table
            .clone()
            .query()
            .falloff_f(4, 2, side, FalloffPattern::Linear, Sum);
        let expected_f = table
            .clone()
            .query()
            .falloff_f(4, 2, side, FalloffPattern::Linear, Sum)
            .get(..)
            .into_map()
            .unwrap();
        let target_f = FlexId::new(4, 3, 4, 2, 4, 2).unwrap();
        assert_eq!(
            q_f.get(target_f).map(Result::unwrap).next().map(|(_, v)| v),
            expected_f.get(target_f).next().map(|(_, v)| *v),
        );

        let q_x = table
            .clone()
            .query()
            .falloff_x(4, 2, side, FalloffPattern::Linear, Sum);
        let expected_x = table
            .clone()
            .query()
            .falloff_x(4, 2, side, FalloffPattern::Linear, Sum)
            .get(..)
            .into_map()
            .unwrap();
        let target_x = FlexId::new(4, 2, 4, 3, 4, 2).unwrap();
        assert_eq!(
            q_x.get(target_x).map(Result::unwrap).next().map(|(_, v)| v),
            expected_x.get(target_x).next().map(|(_, v)| *v),
        );

        let q_y = table
            .clone()
            .query()
            .falloff_y(4, 2, side, FalloffPattern::Linear, Sum);
        let expected_y = table
            .clone()
            .query()
            .falloff_y(4, 2, side, FalloffPattern::Linear, Sum)
            .get(..)
            .into_map()
            .unwrap();
        let target_y = FlexId::new(4, 2, 4, 2, 4, 1).unwrap();
        assert_eq!(
            q_y.get(target_y).map(Result::unwrap).next().map(|(_, v)| v),
            expected_y.get(target_y).next().map(|(_, v)| *v),
        );
    }
}

#[test]
fn get_zoom_out_wrapped_target_matches_whole() {
    use crate::query::merge_policy::Sum;

    let mut table = SpatialIdMap::<u32>::new();
    table.insert(FlexId::new(3, 0, 3, 0, 3, 0).unwrap(), 11);
    table.insert(FlexId::new(3, 0, 3, 7, 3, 0).unwrap(), 13);

    let expected = table
        .clone()
        .query()
        .zoom_out(2, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let expected_vals: Vec<u32> = expected.flat_single_ids(None).map(|(_, v)| *v).collect();

    let mut target = crate::RangeId::new(3, 0, 0, 0).unwrap();
    target.set_x([3, 2]).unwrap();

    let lazy_vals: Vec<u32> = table
        .query()
        .zoom_out(2, Sum)
        .get(target)
        .map(Result::unwrap)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(lazy_vals, expected_vals);
}

/// 1つ値を返したあと、読み出しに失敗する入力源。
struct FailingSource;

impl crate::Source for FailingSource {
    type Value = u32;

    fn read_flex_ids<'a>(
        &'a self,
        _targets: &[crate::RangeId],
        _token: &crate::CancellationToken,
    ) -> Result<crate::query::query_iter::QueryIter<'a, u32>, crate::Error> {
        let items = [
            Ok((FlexId::new(2, 0, 2, 0, 2, 0).unwrap(), 1)),
            Err(crate::Error::Query(crate::QueryError::SourceRead(
                "broken".into(),
            ))),
        ];
        Ok(crate::query::query_iter::QueryIter::new(items.into_iter()))
    }
}

#[test]
fn get_yields_source_errors() {
    let target = crate::RangeId::new(1, [-2, 1], [0, 1], [0, 1]).unwrap();
    let items: Vec<_> = FailingSource.query().get(target).collect();
    assert!(matches!(
        items.as_slice(),
        [
            Ok(_),
            Err(crate::Error::Query(crate::QueryError::SourceRead(_)))
        ]
    ));
}

/// 複数の範囲を渡すと、どれかと交差する要素を1回ずつ返す。配列・Vec・スライスのどれで渡しても同じ。
#[test]
fn get_with_multiple_targets_returns_union() {
    let mut table = SpatialIdMap::<u32>::new();
    for x in 0..8 {
        table.insert(FlexId::new(3, 0, 3, x, 3, 0).unwrap(), x);
    }
    let query = table.query();
    let a = crate::RangeId::new(3, [0, 0], [1, 2], [0, 0]).unwrap();
    let b = crate::RangeId::new(3, [0, 0], [2, 4], [0, 0]).unwrap();

    let values = |table: SpatialIdMap<u32>| -> Vec<u32> { table.iter().map(|(_, v)| *v).collect() };
    let from_array = values(query.get([a, b]).into_map().unwrap());
    assert_eq!(from_array, alloc::vec![1, 2, 3, 4]);
    assert_eq!(
        values(query.get(alloc::vec![a, b]).into_map().unwrap()),
        from_array
    );
    assert_eq!(
        values(query.get(&[a, b][..]).into_map().unwrap()),
        from_array
    );
}

/// `..` は全体を評価し、範囲で絞り込まない。
#[test]
fn get_whole_returns_everything() {
    let mut table = SpatialIdMap::<u32>::new();
    table.insert(FlexId::new(3, -8, 3, 0, 3, 0).unwrap(), 1);
    table.insert(FlexId::new(3, 7, 3, 7, 3, 7).unwrap(), 2);

    let got = table.clone().query().get(..).into_map().unwrap();
    assert_eq!(got, table);
}

/// 検証に失敗すると、最初の要素として `Err` を返して終わる。
#[test]
fn get_yields_validation_error_first() {
    let query = SpatialIdMap::<u32>::new().query().shift_x(100, 3);
    let items: Vec<_> = query.get(..).collect();
    assert!(matches!(items.as_slice(), [Err(_)]));
}

/// `get` は要素を範囲で切り取り、`get_overlapping` は交差する要素をそのまま返す。
#[test]
fn get_clips_and_get_overlapping_does_not() {
    let mut table = SpatialIdMap::<u32>::new();
    // z=18 の要素は z=20 の 4×4×4 を覆う
    let coarse = FlexId::new(18, 0, 18, 25, 18, 0).unwrap();
    table.insert(coarse, 7);
    let query = table.query();
    let target = crate::SingleId::new(20, 1, 101, 2).unwrap();
    let target_id = target.into_iter().next().unwrap();

    let clipped: Vec<_> = query.get(target).map(Result::unwrap).collect();
    assert_eq!(clipped, alloc::vec![(target_id, 7)]);

    let overlapping: Vec<_> = query.get_overlapping(target).map(Result::unwrap).collect();
    assert_eq!(overlapping, alloc::vec![(coarse, 7)]);
}

/// 範囲が互いに重なっていても、切り取った断片は互いに素で、FlexId の Z-order 列に並ぶ。
#[test]
fn get_with_overlapping_targets_yields_each_place_once_in_order() {
    let mut table = SpatialIdMap::<u32>::new();
    table.insert(FlexId::new(16, 0, 16, 6, 16, 0).unwrap(), 1);
    table.insert(FlexId::new(16, 0, 16, 7, 16, 0).unwrap(), 2);
    let query = table.query();
    let big = crate::RangeId::new(18, [0, 1], [26, 29], [0, 1]).unwrap();
    let inside = crate::SingleId::new(19, 1, 54, 2).unwrap();

    let pieces: Vec<(FlexId, u32)> = query
        .get(alloc::vec![big, inside.into()])
        .map(Result::unwrap)
        .collect();
    for pair in pieces.windows(2) {
        assert!(
            pair[0].0 < pair[1].0,
            "{:?} の次に {:?}",
            pair[0].0,
            pair[1].0
        );
    }
    let from_pieces: SpatialIdMap<u32> = pieces.into_iter().collect();
    assert_eq!(from_pieces, query.get(big).into_map().unwrap());
}

#[test]
fn into_set_returns_spatial_id_set() {
    let mut table = SpatialIdMap::<u32>::new();
    let id = FlexId::new(16, 0, 16, 6, 16, 0).unwrap();
    table.insert(id, 42);

    let set = table.query().get(..).into_set().unwrap();
    let mut expected = crate::SpatialIdSet::new();
    expected.insert(id);
    assert_eq!(set, expected);
}

#[test]
fn into_set_yields_validation_error() {
    let query = SpatialIdMap::<u32>::new().query().shift_x(100, 3);
    assert!(query.get(..).into_set().is_err());
}
