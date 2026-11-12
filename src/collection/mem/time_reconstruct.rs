use crate::SpatialId;
use crate::time::span::TimeSpan;
use crate::{AllowedIntervals, FlexId, RangeId};
use alloc::vec::Vec;

/// 空間成分のみを `u128` へパックし、高速な一致判定を行う。
#[inline(always)]
fn spatial_key_u128(id: &FlexId) -> u128 {
    ((id.f_zoomlevel() as u128) << 120)
        | ((id.f_index() as u128) << 96)
        | ((id.x_zoomlevel() as u128) << 88)
        | ((id.x_index() as u128) << 64)
        | ((id.y_zoomlevel() as u128) << 56)
        | (id.y_index() as u128)
}

/// 時間方向に隣接するSegmentを遅延評価で結合するイテレータ。
pub struct TimeReconstructor<'a, I, V>
where
    I: Iterator<Item = (FlexId, V)>,
    V: PartialEq,
{
    iter: I,
    pending: Option<(u128, FlexId, u64, u64, V)>,
    allowed_intervals: Option<&'a AllowedIntervals>,
}

impl<'a, I, V> Iterator for TimeReconstructor<'a, I, V>
where
    I: Iterator<Item = (FlexId, V)>,
    V: PartialEq,
{
    type Item = (RangeId, V);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, first_flex, start, mut end, value) = match self.pending.take() {
            Some(p) => p,
            None => {
                let (id, v) = self.iter.next()?;
                let (s, e) = id.seconds_range();
                (spatial_key_u128(&id), id, s, e, v)
            }
        };

        for (next_id, next_value) in self.iter.by_ref() {
            let next_key = spatial_key_u128(&next_id);
            let (next_start, next_end) = next_id.seconds_range();

            if next_key == key && next_value == value && next_start == end {
                end = next_end;
                continue;
            }

            self.pending = Some((next_key, next_id, next_start, next_end, next_value));
            return Some(finish(
                &first_flex,
                start,
                end,
                value,
                self.allowed_intervals,
            ));
        }

        Some(finish(
            &first_flex,
            start,
            end,
            value,
            self.allowed_intervals,
        ))
    }
}

/// 時間方向に隣接するSegmentを結合し、[`RangeId`] の列として返す。
///
/// `allowed_intervals` に[`AllowedIntervals`]を渡すと、結合後の秒区間を**その候補のうち最も粗い単位**で
/// 表し直す（Segment数が候補の中で最小になる）。[`None`] の場合は「その区間を表せる最も粗い単位」
/// （`gcd(開始秒, 幅)`）になり、Segment数は常に1になる。
///
/// [`AllowedIntervals`] は必ず全区間を表せる候補を含むので、**この関数は失敗しない**。
///
/// # 計算量
///
/// `O(n)`。入力はあらかじめ `(FlexId, 開始秒)` 順に並んでいる前提。
pub(crate) fn reconstruct_temporal<'a, I, V>(
    rows: I,
    allowed_intervals: Option<&'a AllowedIntervals>,
) -> impl Iterator<Item = (RangeId, V)> + 'a
where
    I: IntoIterator<Item = (FlexId, V)> + 'a,
    V: PartialEq + 'a,
{
    TimeReconstructor {
        iter: rows.into_iter(),
        pending: None,
        allowed_intervals,
    }
}

/// `rows` を時間方向に結合した [`RangeId`] として読み出す。
///
/// FlexId の Z-order 列では、同じ空間の時間Segmentが連続するとは限らない。時間で分割された
/// Segmentがあるときだけ `(空間, 開始秒)` 順に並べ直してから結合し、無ければ集めずに流す。
pub(crate) fn reconstruct<'a, V: PartialEq + 'a>(
    rows: impl Iterator<Item = (FlexId, V)> + 'a,
    has_temporal_split: bool,
    allowed_intervals: Option<&'a AllowedIntervals>,
) -> impl Iterator<Item = (RangeId, V)> + 'a {
    // WHY: FlexId の Z-order 列では空間領域で先に分割された場合、時間Segmentが交互に現れて連続しない。
    // 連続した時間ブロックへ結合するためには、(空間, 開始秒) の順にソートする必要がある。
    // そのため、時間方向の分割がある場合のみ O(N) のメモリを確保して全要素を Vec に集め、ソートしてから結合する。
    let (sorted, lazy) = if has_temporal_split {
        let mut sorted: Vec<(FlexId, V)> = rows.collect();
        sorted.sort_by_cached_key(|(id, _)| (spatial_key_u128(id), id.seconds_range().0));
        (Some(sorted), None)
    } else {
        (None, Some(rows))
    };

    reconstruct_temporal(
        sorted
            .into_iter()
            .flatten()
            .chain(lazy.into_iter().flatten()),
        allowed_intervals,
    )
}

/// 結合し終えた1件を [`RangeId`] に組み立てる。
#[inline]
fn finish<V>(
    key: &FlexId,
    start: u64,
    end: u64,
    value: V,
    allowed_intervals: Option<&AllowedIntervals>,
) -> (RangeId, V) {
    let base = RangeId::from(key);

    // 候補集合が指定されていれば、その中で最も粗い（＝Segment数が最小の）単位を直接選んで付与する。
    // 指定がなければ `TimeSpan::apply_to` で `gcd(start, end - start)` の最粗単位を計算する。
    let range = match allowed_intervals {
        Some(allowed) => {
            let interval = allowed.coarsest_dividing(start, end);
            let unit = interval.seconds();
            let t = if unit.is_power_of_two() {
                let shift = unit.trailing_zeros();
                [start >> shift, (end >> shift) - 1]
            } else {
                [start / unit, (end / unit) - 1]
            };
            base.time_with_unchecked(interval, t)
        }
        None => TimeSpan::new(start, end)
            .expect("結合した秒区間は空でない")
            .apply_to(base)
            .expect("結合した秒区間は元のSegmentの和なので常に有効"),
    };

    (range, value)
}

#[cfg(all(test, feature = "temporal_id"))]
mod tests {
    use super::*;
    use crate::{Interval, SingleId, SpatialId};

    fn reconstruct_temporal_vec<V: Clone + PartialEq + Send>(
        items: impl IntoIterator<Item = (FlexId, V)>,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Vec<(RangeId, V)> {
        let mut items: Vec<_> = items.into_iter().collect();
        items.sort_by(|a, b| {
            let key_a = spatial_key_u128(&a.0);
            let key_b = spatial_key_u128(&b.0);
            key_a
                .cmp(&key_b)
                .then_with(|| a.0.seconds_range().0.cmp(&b.0.seconds_range().0))
        });
        reconstruct_temporal(items, allowed_intervals).collect()
    }

    /// 時間成分を全時間へ落とした [`FlexId`]。元のテストの `sorted_reference` 用。
    #[allow(dead_code)]
    pub(crate) fn spatial_key(flex_id: &FlexId) -> FlexId {
        (*flex_id).time_cleared()
    }

    /// 仕様書 1.5.3 の例（`1800/809712`）が、分解 → 結合を経て元の表記へ戻る。
    #[test]
    fn round_trips_the_spec_example() {
        let original = SingleId::new(12, 0, 3638, 1614)
            .unwrap()
            .time_with(1800, 809712)
            .unwrap();

        let time_segments: Vec<_> = original.flex_ids().map(|id| (id, 1u8)).collect();
        assert!(
            time_segments.len() > 1,
            "1800秒は複数の2分岐Segmentへ分解されるはず"
        );

        let merged = reconstruct_temporal_vec(time_segments, None);
        assert_eq!(merged.len(), 1, "結合されて1件になるはず");
        assert_eq!(merged[0].0.to_string(), "12/0/3638/1614_1800/809712");
    }

    /// 値が違うSegmentは結合しない。
    #[test]
    fn does_not_merge_across_different_values() {
        let base = SingleId::new(12, 0, 3638, 1614).unwrap();
        let a = base.time_with(Interval::HOUR, 0).unwrap();
        let b = base.time_with(Interval::HOUR, 1).unwrap();

        let mut time_segments: Vec<(FlexId, u8)> = Vec::new();
        time_segments.extend(a.flex_ids().map(|id| (id, 1u8)));
        time_segments.extend(b.flex_ids().map(|id| (id, 2u8)));

        let merged = reconstruct_temporal_vec(time_segments, None);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].0.seconds_range(), (0, 3600));
        assert_eq!(merged[1].0.seconds_range(), (3600, 7200));
    }

    /// 同じ値なら、隣接する2つの時間Segmentは1つに融合する。
    #[test]
    fn merges_adjacent_segments_with_the_same_value() {
        let base = SingleId::new(12, 0, 3638, 1614).unwrap();
        let a = base.time_with(Interval::HOUR, 0).unwrap();
        let b = base.time_with(Interval::HOUR, 1).unwrap();

        let mut time_segments: Vec<(FlexId, u8)> = Vec::new();
        time_segments.extend(a.flex_ids().map(|id| (id, 7u8)));
        time_segments.extend(b.flex_ids().map(|id| (id, 7u8)));

        let merged = reconstruct_temporal_vec(time_segments, None);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].0.seconds_range(), (0, 7200));
        // gcd(0, 7200) = 7200 なので「2時間」という単位で1Segmentになる。
        assert_eq!((merged[0].0.i().seconds(), merged[0].0.t()), (7200, [0, 0]));
    }

    /// 候補集合を渡すと、その中で最も粗い（＝Segment数が最小の）単位が選ばれる。
    #[test]
    fn candidate_set_picks_the_coarsest_unit() {
        let base = SingleId::new(12, 0, 3638, 1614).unwrap();
        let a = base.time_with(Interval::HOUR, 0).unwrap();
        let b = base.time_with(Interval::HOUR, 1).unwrap();

        let mut time_segments: Vec<(FlexId, u8)> = Vec::new();
        time_segments.extend(a.flex_ids().map(|id| (id, 7u8)));
        time_segments.extend(b.flex_ids().map(|id| (id, 7u8)));

        let merged = reconstruct_temporal_vec(time_segments, Some(AllowedIntervals::calendar()));
        assert_eq!(merged.len(), 1);
        assert_eq!((merged[0].0.i().seconds(), merged[0].0.t()), (3600, [0, 1]));

        // 候補が区間を割り切れないときは、必ず含まれる SECOND まで落ちる。
        // 候補集合が貧しいとSegment数が爆発することを示す例でもある
        // （1時間 = 3600 TimeSegment）。実用では `AllowedIntervals::calendar()` のように
        // 粒度の階段を用意しておくこと。
        let mut time_segments: Vec<(FlexId, u8)> = Vec::new();
        time_segments.extend(
            SingleId::new(12, 0, 3638, 1614)
                .unwrap()
                .time_with(Interval::HOUR, 0)
                .unwrap()
                .flex_ids()
                .map(|id| (id, 7u8)),
        );
        let merged =
            reconstruct_temporal_vec(time_segments, Some(&AllowedIntervals::new([Interval::DAY])));
        assert_eq!((merged[0].0.i().seconds(), merged[0].0.t()), (1, [0, 3599]));
        // 秒区間そのものは変わらない。
        assert_eq!(merged[0].0.seconds_range(), (0, 3600));
    }

    /// FlexIdが違えば、時間が連続していても結合しない。
    #[test]
    fn does_not_merge_across_different_segments() {
        let a = SingleId::new(12, 0, 3638, 1614)
            .unwrap()
            .time_with(Interval::HOUR, 0)
            .unwrap();
        let b = SingleId::new(12, 0, 3639, 1614)
            .unwrap()
            .time_with(Interval::HOUR, 1)
            .unwrap();

        let mut time_segments: Vec<(FlexId, u8)> = Vec::new();
        time_segments.extend(a.flex_ids().map(|id| (id, 7u8)));
        time_segments.extend(b.flex_ids().map(|id| (id, 7u8)));

        assert_eq!(reconstruct_temporal_vec(time_segments, None).len(), 2);
    }

    /// 時間を使っていない場合は入力と1対1で対応する（全時間Segmentはそのまま）。
    #[test]
    fn passes_through_when_no_temporal_information() {
        let time_segments: Vec<(FlexId, u8)> = (0..4u32)
            .map(|x| (FlexId::new(3, 0, 3, x, 3, 0).unwrap(), 1u8))
            .collect();

        let merged = reconstruct_temporal_vec(time_segments, None);
        assert_eq!(merged.len(), 4);
        assert!(merged.iter().all(|(id, _)| id.time_is_whole()));
    }
}
