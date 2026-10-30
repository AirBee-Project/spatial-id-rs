use alloc::collections::BTreeMap;

use super::{Falloff, FalloffAxis};
use crate::query::merge_policy::{Average, Max, Sum};
use crate::query::ops::unary::falloff::FalloffPattern;
use crate::query::traits::UnaryOperator;
use crate::{SingleId, Source, SpatialIdMap};

/// z=20, f=0, y=0 に固定した行から `x -> value` の対応を取り出す。
fn row(table: &SpatialIdMap<i32>) -> BTreeMap<u32, i32> {
    table
        .flat_single_ids(None)
        .filter(|(sid, _)| sid.f() == 0 && sid.y() == 0)
        .map(|(sid, v)| (sid.x(), *v))
        .collect()
}

fn time_segment(x: u32, v: i32) -> (SingleId, i32) {
    (SingleId::new(20, 0, x, 0).unwrap(), v)
}

/// 単一Segmentの X falloff（半径2）。減衰は value*(r-|d|)/r。重なりが無いので Sum/Max は同値。
#[test]
fn falloff_x_single_segment() {
    let mut table = SpatialIdMap::new();
    let (id, v) = time_segment(100, 4);
    table.insert(id, v);

    let out = table
        .query()
        .falloff_x(20, 2, None, FalloffPattern::Linear, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    // d=0:4, d=±1:2, d=±2:0
    assert_eq!(r.get(&98), Some(&0));
    assert_eq!(r.get(&99), Some(&2));
    assert_eq!(r.get(&100), Some(&4));
    assert_eq!(r.get(&101), Some(&2));
    assert_eq!(r.get(&102), Some(&0));
}

/// 重なる2Segmentの X falloff を Sum で合成。重なったSegmentは両寄与の和になる。
#[test]
fn falloff_x_overlap_sum() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(100, 4).0, 4);
    table.insert(time_segment(102, 4).0, 4);

    let out = table
        .query()
        .falloff_x(20, 2, None, FalloffPattern::Linear, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    // A(x100) → x98:0 x99:2 x100:4 x101:2 x102:0
    // B(x102) → x100:0 x101:2 x102:4 x103:2 x104:0
    // Sum     → x98:0 x99:2 x100:4 x101:4 x102:4 x103:2 x104:0
    assert_eq!(r.get(&98), Some(&0));
    assert_eq!(r.get(&99), Some(&2));
    assert_eq!(r.get(&100), Some(&4));
    assert_eq!(r.get(&101), Some(&4)); // 2+2
    assert_eq!(r.get(&102), Some(&4)); // 0+4
    assert_eq!(r.get(&103), Some(&2));
    assert_eq!(r.get(&104), Some(&0));
}

/// 同じ入力を Max で合成。重なりは最大値を取るため x101 が Sum と食い違う。
#[test]
fn falloff_x_overlap_max() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(100, 4).0, 4);
    table.insert(time_segment(102, 4).0, 4);

    let out = table
        .query()
        .falloff_x(20, 2, None, FalloffPattern::Linear, Max)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    // Max → x100:4 x101:2 x102:4（x101 は max(2,2)=2、Sum の 4 と異なる）
    assert_eq!(r.get(&100), Some(&4));
    assert_eq!(r.get(&101), Some(&2));
    assert_eq!(r.get(&102), Some(&4));
}

#[test]
fn falloff_x_reverse_target_wrapped_covers_needed_range() {
    let op = Falloff::<Sum>::new(FalloffAxis::X, 2, 1, None, FalloffPattern::Linear).unwrap();

    let bounds = crate::RangeId::new(1, [-2, 1], 0, [0, 1]).unwrap();
    let mut out = alloc::vec::Vec::new();
    <Falloff<Sum> as UnaryOperator<u32>>::reverse_target(&op, bounds, &mut out);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].z(), 2);
    assert_eq!(out[0].x(), [0, 3]);
}

/// 3件以上の重なりも Average で平均になる。ズームの違う寄与が重なる場合も含める。
#[test]
fn falloff_x_overlap_average() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(100, 12).0, 12);
    table.insert(time_segment(101, 0).0, 0);
    // z=19 の x=51 は z=20 の x102..=103 を覆う
    table.insert(SingleId::new(19, 0, 51, 0).unwrap(), 0);

    let out = table
        .query()
        .falloff_x(20, 2, None, FalloffPattern::Linear, Average)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    // x100 への寄与は 12（x100, d=0）、0（x101, d=1）、0（x102..=103, d=-2）。2件ずつ合成すると 3 になる
    assert_eq!(r.get(&100), Some(&4));
}

#[test]
fn falloff_validate_rejects_radius_out_of_range() {
    let op = Falloff::<Sum>::new(FalloffAxis::F, 2, 4, None, FalloffPattern::Linear).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<i32>>::validate(&op).is_err());

    // 15^2 は u8 に収まるが、15^4 は収まらない
    let op = Falloff::<Sum>::new(FalloffAxis::X, 20, 15, None, FalloffPattern::Linear).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<u8>>::validate(&op).is_ok());
    let op =
        Falloff::<Sum>::new(FalloffAxis::X, 20, 15, None, FalloffPattern::QuadraticIn).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<u8>>::validate(&op).is_err());

    // 55108^4 は i64 に収まるが、55109^4 は収まらない
    let op =
        Falloff::<Sum>::new(FalloffAxis::X, 20, 55108, None, FalloffPattern::QuadraticIn).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<i64>>::validate(&op).is_ok());
    let op =
        Falloff::<Sum>::new(FalloffAxis::X, 20, 55109, None, FalloffPattern::QuadraticIn).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<i64>>::validate(&op).is_err());
}

/// 半径0の falloff は恒等（no-op）。
#[test]
fn falloff_x_radius_zero_is_noop() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(100, 7).0, 7);

    let out = table
        .query()
        .falloff_x(20, 0, None, FalloffPattern::Linear, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    assert_eq!(r.len(), 1);
    assert_eq!(r.get(&100), Some(&7));
}

/// 途中の積が `V` を超える値でも、あふれずに正しく減衰する。
#[test]
fn falloff_x_large_value_does_not_overflow() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(1000, 100_000).0, 100_000);

    // 100_000 * 200^2 は i32 を超える
    let out = table
        .query()
        .falloff_x(20, 200, None, FalloffPattern::QuadraticIn, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    assert_eq!(r.get(&1000), Some(&100_000));
    assert_eq!(r.get(&1100), Some(&75_000));
}

/// 分けた計算は、負の値でも `value * num / den` と一致する。
#[test]
fn attenuate_matches_direct_formula() {
    use super::Attenuator;
    for pattern in [
        FalloffPattern::Linear,
        FalloffPattern::QuadraticIn,
        FalloffPattern::QuadraticOut,
    ] {
        for r in 1..=7i64 {
            let attenuator = Attenuator::<i64>::new(r as u32, pattern).unwrap();
            for d in 0..=r {
                let (num, den) = match pattern {
                    FalloffPattern::Linear => (r - d, r),
                    FalloffPattern::QuadraticIn => (r * r - d * d, r * r),
                    FalloffPattern::QuadraticOut => ((r - d) * (r - d), r * r),
                };
                for value in -60..=60i64 {
                    assert_eq!(attenuator.attenuate(&value, d as u32), value * num / den);
                }
            }
        }
    }
}

/// 大量データに対して take(1) を行った際、ストリームが全件走査せずに短絡評価されることを確認する。
#[test]
fn falloff_lazily_short_circuits() {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    let mut table = SpatialIdMap::new();
    for i in 0..10_000u32 {
        table.insert(SingleId::new(20, 0, i * 10, 0).unwrap(), 100i32);
    }

    let read_count = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&read_count);
    let first = table
        .query()
        .filter_values(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            true
        })
        .falloff_x(20, 5, None, FalloffPattern::Linear, Max)
        .get(..)
        .next()
        .unwrap()
        .unwrap();

    assert_eq!(first.1, 100);
    let consumed = read_count.load(Ordering::Relaxed);
    assert!(consumed < 100, "消費件数が想定より多い: {consumed}");
}

use alloc::vec::Vec;
use crate::Side;
use crate::ZoomLevel;

fn expected_batch<P: crate::query::merge_policy::MergePolicy<i32>>(
    table: &SpatialIdMap<i32>,
    axis: FalloffAxis,
    z: u8,
    radius: u32,
    direction: Option<Side>,
    pattern: FalloffPattern,
) -> SpatialIdMap<i32> {
    use super::Attenuator;
    let attenuator = Attenuator::new(radius, pattern).unwrap();
    let mut tree = crate::collection::core::FlexTreeCore::default();
    let mut moved = Vec::new();
    let zoom = ZoomLevel::new(z).unwrap();

    if P::IS_COMMUTATIVE {
        let resolve = |a: &i32, b: &i32| P::resolve(*a, *b);
        for (id, val) in table.clone() {
            id.falloff_each(
                axis,
                zoom,
                radius,
                direction,
                &attenuator,
                &val,
                &mut moved,
                |ids, v| tree.insert_with(ids.iter().copied(), v, &resolve),
            );
        }
        SpatialIdMap { inner: tree }
    } else {
        let mut gathered = crate::collection::core::FlexTreeCore::<
            Vec<i32>,
            crate::collection::core::NoSummary,
        >::default();
        for (id, val) in table.clone() {
            id.falloff_each(
                axis,
                zoom,
                radius,
                direction,
                &attenuator,
                &val,
                &mut moved,
                |ids, v| {
                    gathered.insert_with(ids.iter().copied(), alloc::vec![v], &|a: &Vec<i32>, b: &Vec<i32>| {
                        [a.as_slice(), b].concat()
                    })
                },
            );
        }
        let inner: crate::collection::core::FlexTreeCore<
            i32,
            crate::collection::core::NoSummary,
        > = gathered
            .iter()
            .filter_map(|(id, values)| Some((id, P::resolve_many(values.iter().copied())?)))
            .collect();
        SpatialIdMap { inner }
    }
}

fn assert_stream_matches_expected<P: crate::query::merge_policy::MergePolicy<i32> + 'static>(
    table: &SpatialIdMap<i32>,
    axis: FalloffAxis,
    z: u8,
    radius: u32,
    direction: Option<Side>,
    pattern: FalloffPattern,
    policy: P,
) {
    let expected = expected_batch::<P>(table, axis, z, radius, direction, pattern);

    let query_iter = match axis {
        FalloffAxis::X => table
            .clone()
            .query()
            .falloff_x(z, radius, direction, pattern, policy),
        FalloffAxis::Y => table
            .clone()
            .query()
            .falloff_y(z, radius, direction, pattern, policy),
        FalloffAxis::F => table
            .clone()
            .query()
            .falloff_f(z, radius, direction, pattern, policy),
    };

    let items: Vec<_> = query_iter.get(..).collect::<Result<_, _>>().unwrap();
    for window in items.windows(2) {
        assert!(
            window[0].0 < window[1].0,
            "ストリームの順序が狭義単調増加でない: {:?} >= {:?}",
            window[0].0,
            window[1].0
        );
    }

    let streamed: SpatialIdMap<i32> = items.into_iter().collect();
    assert_eq!(streamed, expected);
}

/// 乱数で配置したボクセルに対するストリーム Falloff の結果が、一括挿入の期待結果と完全に一致することを確認する。
#[test]
fn falloff_streaming_matches_batch() {
    let mut table = SpatialIdMap::new();
    for i in 0..200u32 {
        let x = (i * 37) % 500;
        let y = (i * 53) % 500;
        let f = (i % 7) as i32 - 3; // -3..=3 で Upper と Lower に跨る
        table.insert(SingleId::new(20, f, x, y).unwrap(), (i % 30 + 1) as i32);
    }

    assert_stream_matches_expected(&table, FalloffAxis::X, 20, 3, None, FalloffPattern::Linear, Max);
    assert_stream_matches_expected(&table, FalloffAxis::Y, 20, 3, None, FalloffPattern::Linear, Max);
    assert_stream_matches_expected(&table, FalloffAxis::F, 20, 2, None, FalloffPattern::Linear, Max);
}

/// z=26 や z=30（最大ズーム）でアンダーフローやオーバーフローを起こさず正しく動作することを確認する。
#[test]
fn falloff_high_zoom() {
    let mut table = SpatialIdMap::new();
    table.insert(SingleId::new(26, 0, 100, 0).unwrap(), 100i32);
    table.insert(SingleId::new(26, 0, 104, 0).unwrap(), 50i32);
    assert_stream_matches_expected(&table, FalloffAxis::X, 26, 3, None, FalloffPattern::Linear, Max);

    let mut table_max = SpatialIdMap::new();
    table_max.insert(SingleId::new(ZoomLevel::MAX.get(), 0, 100, 0).unwrap(), 100i32);
    assert_stream_matches_expected(
        &table_max,
        FalloffAxis::X,
        ZoomLevel::MAX.get(),
        2,
        None,
        FalloffPattern::Linear,
        Max,
    );
}

/// F 軸の境界跨ぎ（Lower <-> Upper）や X 軸の折り返しが正しくバッチ処理と一致することを確認する。
#[test]
fn falloff_boundary_crossing_and_wrapping() {
    // F 軸で Lower (-1) から Upper (0, 1) へ跨ぐケース
    let mut table_f_cross = SpatialIdMap::new();
    table_f_cross.insert(SingleId::new(10, -1, 10, 10).unwrap(), 100i32);
    table_f_cross.insert(SingleId::new(10, 0, 10, 10).unwrap(), 50i32);
    assert_stream_matches_expected(&table_f_cross, FalloffAxis::F, 10, 2, None, FalloffPattern::Linear, Max);

    // X 軸で東端 (2^z - 1) を超えて折り返すケース
    let limit = (1u32 << 10) - 1;
    let mut table_x_wrap = SpatialIdMap::new();
    table_x_wrap.insert(SingleId::new(10, 0, limit - 1, 10).unwrap(), 80i32);
    assert_stream_matches_expected(&table_x_wrap, FalloffAxis::X, 10, 3, None, FalloffPattern::Linear, Max);
}

/// 非可換かつ resolve_many を持つ Policy（Average）でストリーム結果とバッチ結果が一致することを確認する。
#[test]
fn falloff_non_commutative_policy() {
    let mut table = SpatialIdMap::new();
    table.insert(SingleId::new(15, 0, 10, 10).unwrap(), 100i32);
    table.insert(SingleId::new(15, 0, 12, 10).unwrap(), 40i32);
    assert_stream_matches_expected(&table, FalloffAxis::X, 15, 2, None, FalloffPattern::Linear, Average);
}

/// 方向付き（direction: Some）でストリーム結果とバッチ結果が一致することを確認する。
#[test]
fn falloff_directional() {
    let mut table = SpatialIdMap::new();
    table.insert(SingleId::new(15, 0, 50, 50).unwrap(), 100i32);
    assert_stream_matches_expected(
        &table,
        FalloffAxis::X,
        15,
        3,
        Some(Side::Upper),
        FalloffPattern::Linear,
        Max,
    );
    assert_stream_matches_expected(
        &table,
        FalloffAxis::X,
        15,
        3,
        Some(Side::Lower),
        FalloffPattern::Linear,
        Max,
    );
}
