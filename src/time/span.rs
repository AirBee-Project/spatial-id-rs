use super::segment::TimeSegment;
use crate::id::zoom_level::TZoomLevel;
use crate::{Interval, error::Error};

/// 絶対秒区間 `[start, end)`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TimeSpan {
    start: u64,
    end: u64,
}

impl TimeSpan {
    /// `id` が占有する絶対秒区間。
    pub(crate) fn of(id: &impl crate::SpatialId) -> Self {
        let (start, end) = id.time_range();
        Self::new_unchecked(start, end)
    }

    /// 新しい区間を生成する。start >= end や上限超過の場合は None を返す。
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start >= end || end > Interval::MAX_SECONDS {
            None
        } else {
            Some(Self { start, end })
        }
    }

    /// バリデーションを行わずに新しい区間を生成する。
    pub const fn new_unchecked(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// 開始秒
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// 終了秒 (排他)
    pub const fn end(&self) -> u64 {
        self.end
    }

    /// 区間の幅 (秒)
    pub const fn width(&self) -> u64 {
        self.end - self.start
    }

    /// 2つの絶対秒区間の交差（積集合）。重なりが無ければ [`None`]。
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        Self::new(start, end)
    }

    /// 絶対秒区間の差集合（`self - other`）。
    ///
    /// `other` が `self` の内側を刳り抜く場合は前後2つ、端を削る場合は1つ、完全に覆う場合は
    /// 0個を返す。重なりが無ければ `self` をそのまま返す。
    pub fn difference(&self, other: &Self) -> impl Iterator<Item = Self> + use<> {
        if other.end <= self.start || self.end <= other.start {
            return [Some(*self), None].into_iter().flatten();
        }

        let head_end = other.start.min(self.end);
        let head = Self::new(self.start, head_end);

        let tail_start = other.end.max(self.start);
        let tail = Self::new(tail_start, self.end);

        [head, tail].into_iter().flatten()
    }

    /// この秒区間を表せる最も粗い単位（秒数）を返す。
    /// 単位は `start` と区間幅の両方を割り切れなければならないので、最大公約数を求める。
    pub const fn coarsest_unit(&self) -> u64 {
        let (mut a, mut b) = (self.start, self.width());
        while b != 0 {
            let rem = a % b;
            a = b;
            b = rem;
        }
        a
    }

    /// この区間を `id` の時間として設定した [`RangeId`](crate::RangeId) を返す。
    ///
    /// `interval` が `None` のときは、区間をちょうど表せる最も粗い単位を使う。
    /// 指定した単位で区間が割り切れない場合は [`SpatialIdError::TIntervalError`](crate::SpatialIdError::TIntervalError) を返す。
    pub(crate) fn apply_to(
        self,
        mut id: crate::RangeId,
        interval: Option<Interval>,
    ) -> Result<crate::RangeId, Error> {
        let interval = match interval {
            Some(interval) => interval,
            None => Interval::new(self.coarsest_unit())?,
        };
        let unit = interval.seconds();
        if !self.start.is_multiple_of(unit) || !self.end.is_multiple_of(unit) {
            return Err(crate::SpatialIdError::TIntervalError { i: unit }.into());
        }
        id.set_time(interval, [self.start / unit, self.end / unit - 1])?;
        Ok(id)
    }

    /// 区間木的に高々 `O(log 幅)` 個の2分岐 TimeSegment へ分解する。
    pub fn into_segments(self) -> TimeSegments {
        let l = self.start;
        let r = self.end - 1;

        let trailing = |v: u64| {
            if v == 0 {
                u64::BITS
            } else {
                v.trailing_zeros()
            }
        };
        let k = trailing(l)
            .min(trailing(r.wrapping_add(1)))
            .min(TZoomLevel::MAX.get() as u32) as u8;

        TimeSegments {
            l: l >> k,
            r: r >> k,
            cur_z: (TZoomLevel::MAX.get() - k) as i8,
        }
    }
}

/// [`TimeSpan::into_segments`] が返す、[`TimeSegment`](crate::time::segment::TimeSegment) の列。
#[derive(Clone)]
pub(crate) struct TimeSegments {
    l: u64,
    r: u64,
    cur_z: i8,
}

impl Iterator for TimeSegments {
    type Item = TimeSegment;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.l > self.r {
                return None;
            }

            if self.cur_z == 0 {
                let v = self.l;
                self.l += 1;
                return Some(TimeSegment::new(unsafe { TZoomLevel::new_unchecked(0) }, v));
            }

            let z = self.cur_z as u8;
            if self.l == self.r {
                let v = self.l;
                self.l += 1;
                return Some(TimeSegment::new(unsafe { TZoomLevel::new_unchecked(z) }, v));
            }
            if self.l & 1 == 1 {
                let v = self.l;
                self.l += 1;
                return Some(TimeSegment::new(unsafe { TZoomLevel::new_unchecked(z) }, v));
            }
            if self.r & 1 == 0 {
                let v = self.r;
                self.r -= 1;
                return Some(TimeSegment::new(unsafe { TZoomLevel::new_unchecked(z) }, v));
            }
            self.l >>= 1;
            self.r >>= 1;
            self.cur_z -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::zoom_level::TZoomLevel;
    use alloc::vec::Vec;

    #[test]
    fn split_covers_every_range_exactly() {
        let cases: &[(u64, u64)] = &[
            (0, Interval::MAX_SECONDS),
            (0, 1),
            (1, 2),
            (3600, 7200),
            (1800 * 809_712, 1800 * 809_713),
            (1_770_000_000, 1_770_000_001),
            (5, 12),
            (7, 8),
            (86_400 * 20_486, 86_400 * 20_487),
            (1, Interval::MAX_SECONDS),
        ];

        for &(start, end) in cases {
            let time_span = TimeSpan::new(start, end).unwrap();
            let mut ranges: Vec<_> = time_span
                .into_segments()
                .map(|seg| seg.time_span())
                .collect();
            ranges.sort_unstable_by_key(|s| s.start());

            let mut cursor = start;
            for s in &ranges {
                assert_eq!(
                    s.start(),
                    cursor,
                    "[{start}, {end}) でSegmentが連続していない"
                );
                cursor = s.end();
            }
            assert_eq!(cursor, end, "[{start}, {end}) の合計が一致しない");
        }
    }

    #[test]
    fn whole_range_is_a_single_segment() {
        let time_span = TimeSpan::new(0, Interval::MAX_SECONDS).unwrap();
        let time_segments: Vec<_> = time_span.into_segments().collect();
        assert_eq!(time_segments.len(), 1);
        assert_eq!(time_segments[0].zoom().get(), 0);
        assert_eq!(time_segments[0].index(), 0);
    }

    #[test]
    fn empty_range_yields_nothing() {
        assert!(TimeSpan::new(5, 5).is_none());
        assert!(TimeSpan::new(9, 5).is_none());
    }

    #[test]
    fn a_single_segment_restores_its_own_width() {
        for zoom in 0..=TZoomLevel::MAX.get() {
            let max_index = (1u64 << zoom) - 1;
            for index in [0u64, 1, max_index].into_iter().filter(|i| *i <= max_index) {
                let seg = TimeSegment::new(TZoomLevel::new(zoom).unwrap(), index);
                let time_span = seg.time_span();
                assert_eq!(time_span.coarsest_unit(), time_span.width(), "z={zoom}");
            }
        }
    }

    #[test]
    fn coarsest_unit_restores_arbitrary_intervals() {
        for (start, end, expected) in [
            (0u64, Interval::MAX_SECONDS, Interval::MAX_SECONDS),
            (86_400 * 20_486, 86_400 * 20_487, 86_400),
            (3_600 * 491_666, 3_600 * 491_667, 3_600),
            (60 * 3, 60 * 4, 60),
            (1800 * 809_712, 1800 * 809_713, 1800),
            (7_200, 14_400, 7_200),
            (300 * 4_858_272, 300 * 4_858_273, 300),
            (1_770_000_000, 1_770_000_001, 1),
        ] {
            let time_span = TimeSpan::new(start, end).unwrap();
            assert_eq!(time_span.coarsest_unit(), expected, "[{start}, {end})");
        }
    }

    #[test]
    fn intersect_and_difference_agree_with_the_set_operations() {
        let a = TimeSpan::new(3600, 7200).unwrap();
        let b = TimeSpan::new(5400, 9000).unwrap();
        assert_eq!(a.intersect(&b), TimeSpan::new(5400, 7200));

        let a2 = TimeSpan::new(0, 100).unwrap();
        let b2 = TimeSpan::new(100, 200).unwrap();
        assert_eq!(a2.intersect(&b2), None);

        let a3 = TimeSpan::new(0, 86_400).unwrap();
        let b3 = TimeSpan::new(3_600, 7_200).unwrap();
        let pieces: Vec<_> = a3.difference(&b3).collect();
        assert_eq!(
            pieces,
            [
                TimeSpan::new(0, 3_600).unwrap(),
                TimeSpan::new(7_200, 86_400).unwrap()
            ]
        );

        let a4 = TimeSpan::new(0, 86_400).unwrap();
        let b4 = TimeSpan::new(0, 3_600).unwrap();
        let pieces: Vec<_> = a4.difference(&b4).collect();
        assert_eq!(pieces, [TimeSpan::new(3_600, 86_400).unwrap()]);

        let a5 = TimeSpan::new(10, 20).unwrap();
        let b5 = TimeSpan::new(0, 100).unwrap();
        assert_eq!(a5.difference(&b5).count(), 0);

        let a6 = TimeSpan::new(0, 10).unwrap();
        let b6 = TimeSpan::new(50, 60).unwrap();
        let pieces: Vec<_> = a6.difference(&b6).collect();
        assert_eq!(pieces, [TimeSpan::new(0, 10).unwrap()]);
    }
}
