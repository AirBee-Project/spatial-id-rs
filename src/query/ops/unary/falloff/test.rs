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
        .falloff_x(20, -2..=2, FalloffPattern::Linear, Sum)
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
        .falloff_x(20, -2..=2, FalloffPattern::Linear, Sum)
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
        .falloff_x(20, -2..=2, FalloffPattern::Linear, Max)
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
    let op = Falloff::<Sum>::new(FalloffAxis::X, 2, -1..=1, FalloffPattern::Linear).unwrap();

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
        .falloff_x(20, -2..=2, FalloffPattern::Linear, Average)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    // x100 への寄与は 12（x100, d=0）、0（x101, d=1）、0（x102..=103, d=-2）。2件ずつ合成すると 3 になる
    assert_eq!(r.get(&100), Some(&4));
}

#[test]
fn falloff_validate_rejects_radius_out_of_range() {
    let op = Falloff::<Sum>::new(FalloffAxis::F, 2, -4..=4, FalloffPattern::Linear).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<i32>>::validate(&op).is_err());

    // 15^2 は u8 に収まるが、15^4 は収まらない
    let op = Falloff::<Sum>::new(FalloffAxis::X, 20, -15..=15, FalloffPattern::Linear).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<u8>>::validate(&op).is_ok());
    let op =
        Falloff::<Sum>::new(FalloffAxis::X, 20, -15..=15, FalloffPattern::QuadraticIn).unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<u8>>::validate(&op).is_err());

    // 55108^4 は i64 に収まるが、55109^4 は収まらない
    let op = Falloff::<Sum>::new(
        FalloffAxis::X,
        20,
        -55108..=55108,
        FalloffPattern::QuadraticIn,
    )
    .unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<i64>>::validate(&op).is_ok());
    let op = Falloff::<Sum>::new(
        FalloffAxis::X,
        20,
        -55109..=55109,
        FalloffPattern::QuadraticIn,
    )
    .unwrap();
    assert!(<Falloff<Sum> as UnaryOperator<i64>>::validate(&op).is_err());
}

/// 半径0の falloff は恒等（no-op）。
#[test]
fn falloff_x_radius_zero_is_noop() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(100, 7).0, 7);

    let out = table
        .query()
        .falloff_x(20, 0..=0, FalloffPattern::Linear, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    assert_eq!(r.len(), 1);
    assert_eq!(r.get(&100), Some(&7));
}

/// 範囲の両側は、それぞれの側の距離を半径として減衰する。
#[test]
fn falloff_x_asymmetric_range() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(100, 12).0, 12);

    let out = table
        .query()
        .falloff_x(20, -2..=4, FalloffPattern::Linear, Sum)
        .get(..)
        .into_map()
        .unwrap();
    let r = row(&out);

    // 負の側は半径2: x98:0 x99:6、正の側は半径4: x101:9 x102:6 x103:3 x104:0
    let expected = [
        (98, 0),
        (99, 6),
        (100, 12),
        (101, 9),
        (102, 6),
        (103, 3),
        (104, 0),
    ];
    assert_eq!(r, expected.into_iter().collect());
}

#[test]
fn falloff_rejects_range_without_zero() {
    assert!(Falloff::<Sum>::new(FalloffAxis::X, 20, 1..=3, FalloffPattern::Linear).is_err());
    assert!(Falloff::<Sum>::new(FalloffAxis::X, 20, -3..=-1, FalloffPattern::Linear).is_err());
}

/// 途中の積が `V` を超える値でも、あふれずに正しく減衰する。
#[test]
fn falloff_x_large_value_does_not_overflow() {
    let mut table = SpatialIdMap::new();
    table.insert(time_segment(1000, 100_000).0, 100_000);

    // 100_000 * 200^2 は i32 を超える
    let out = table
        .query()
        .falloff_x(20, -200..=200, FalloffPattern::QuadraticIn, Sum)
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
