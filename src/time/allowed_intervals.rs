use crate::Interval;
use alloc::borrow::Cow;
use alloc::vec::Vec;

/// 読み出しで `{i}` として使ってよい時間間隔の候補集合。
///
/// # 必ず含まれる2つの候補
///
/// どう構築しても [`Interval::WHOLE`] と（`temporal_id` feature 有効時は）
/// `Interval::SECOND` が自動的に加わる。この2つが無いと表現できない区間が生じるためである。
///
/// ```
/// # #[cfg(feature = "temporal_id")]
/// # {
/// # use spatial_id::time::{AllowedIntervals, Interval};
/// // 明示していなくても WHOLE と SECOND は入っている。
/// let intervals = AllowedIntervals::new([Interval::HOUR]);
/// assert!(intervals.contains(Interval::WHOLE));
/// assert!(intervals.contains(Interval::SECOND));
/// assert!(intervals.contains(Interval::HOUR));
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedIntervals {
    /// 秒数の降順。重複なし。先頭から見て最初に割り切れたものが「最も粗い」。
    ///
    /// [`Vec`]ではなく[`Cow`]なのは、[`calendar`](Self::calendar)を`static`として
    /// 用意して`&'static`で貸し出すためである。所有版（[`new`](Self::new)）は
    /// [`Cow::Owned`]、組み込みの候補集合は[`Cow::Borrowed`]になる。
    intervals: Cow<'static, [Interval]>,
}

/// [`AllowedIntervals::calendar`] の実体。
///
/// [`new`](AllowedIntervals::new) を通さないので、**降順・重複なしの不変条件は手で守ること**
/// （`calendar_is_sorted_descending_without_duplicates` が固定している）。
#[cfg(feature = "temporal_id")]
static CALENDAR: AllowedIntervals = AllowedIntervals {
    intervals: Cow::Borrowed(&[
        Interval::WHOLE,
        Interval::DAY,
        Interval::HOUR,
        Interval::MINUTE,
        Interval::SECOND,
    ]),
};

impl AllowedIntervals {
    /// 候補から作る。[`Interval::WHOLE`] と `Interval::SECOND` は自動的に加わる。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::time::{AllowedIntervals, Interval};
    /// // 30分・1時間で読みたい、という指定。
    /// let intervals = AllowedIntervals::new([Interval::new(1800).unwrap(), Interval::HOUR]);
    /// assert_eq!(intervals.coarsest_dividing(0, 7200), Interval::HOUR);
    /// assert_eq!(intervals.coarsest_dividing(0, 1800).seconds(), 1800);
    /// # }
    /// ```
    pub fn new(intervals: impl IntoIterator<Item = Interval>) -> Self {
        let mut intervals: Vec<Interval> = intervals.into_iter().collect();

        intervals.push(Interval::WHOLE);
        #[cfg(feature = "temporal_id")]
        intervals.push(Interval::SECOND);

        // 降順に並べておけば、探索は「先頭から最初に割り切れたもの」で済む。
        intervals.sort_unstable_by_key(|u| core::cmp::Reverse(u.seconds()));
        intervals.dedup();

        AllowedIntervals {
            intervals: Cow::Owned(intervals),
        }
    }

    /// 暦の単位だけに正規化する候補集合。
    ///
    /// `{`[`WHOLE`](Interval::WHOLE)`,` [`DAY`](Interval::DAY)`,` [`HOUR`](Interval::HOUR)`,`
    /// [`MINUTE`](Interval::MINUTE)`,` [`SECOND`](Interval::SECOND)`}`。
    ///
    /// `temporal_id` feature が無効なビルドでは、暦の定数（[`Interval::DAY`] など）
    /// 自体が存在しないため、この関数も**存在しない**（[`Interval::HOUR`] などと同じ扱い）。
    /// `{WHOLE}` だけの集合へ黙って縮退させることはしない。無効時でも候補集合が欲しい場合は
    /// [`AllowedIntervals::default`] を使う（`{WHOLE}` を明示的に返す）。
    ///
    /// # なぜ `&'static` を返すのか
    ///
    /// 読み出し API（[`crate::SpatialIdSet::reconstructed_time_ranges`] など）へ
    /// **一時値のまま直接渡せる**ようにするためである。所有値を返していた頃は
    ///
    /// ```text
    /// let ids = set.reconstructed_time_ranges(Some(&AllowedIntervals::calendar()));  // 一時値が文末で死ぬ
    /// ```
    ///
    /// が書けず、コレクションごとに `range_ids_calendar()` のような別名を生やして
    /// 内側で `collect` する必要があった（＝遅延評価が失われていた）。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::time::{AllowedIntervals, Interval};
    /// let intervals = AllowedIntervals::calendar();
    /// // 2時間ぶんは「1時間 × 2Segment」として表される（`gcd` なら `7200秒 × 1Segment`）。
    /// assert_eq!(intervals.coarsest_dividing(0, 7200), Interval::HOUR);
    /// // 暦で割り切れない区間は秒まで落ちる。
    /// assert_eq!(intervals.coarsest_dividing(30, 90), Interval::SECOND);
    /// # }
    /// ```
    #[cfg(feature = "temporal_id")]
    pub fn calendar() -> &'static Self {
        &CALENDAR
    }

    /// 候補に含まれるか。
    pub fn contains(&self, unit: Interval) -> bool {
        self.intervals.contains(&unit)
    }

    /// 候補を秒数の降順で列挙する。
    pub fn iter(&self) -> impl Iterator<Item = Interval> + '_ {
        self.intervals.iter().copied()
    }

    /// 絶対秒区間 `[start, end)` を表せる候補のうち、**最も粗いもの**を返す。
    ///
    /// 単位 `u` でこの区間を表せる条件は `start % u == 0 && end % u == 0` である
    /// （`t = [start/u, end/u - 1]` になる）。粗いほどSegment数
    /// （`t_max - t_min + 1`）が少なくなるので、これが候補の中でのSegment数最小を与える。
    ///
    /// `Interval::SECOND` が必ず候補にあるため、**この関数は必ず値を返す**
    /// （`temporal_id` feature 無効時は全時間しか存在せず、[`WHOLE`](Interval::WHOLE) が返る）。
    pub fn coarsest_dividing(&self, start: u64, end: u64) -> Interval {
        for unit in self.intervals.iter().copied() {
            let seconds = unit.seconds();
            if start.is_multiple_of(seconds) && end.is_multiple_of(seconds) {
                return unit;
            }
        }

        // 降順の末尾は必ず SECOND（feature 無効時は WHOLE で、区間も全時間しかない）。
        unreachable!("AllowedIntervals は必ず全区間を表せる候補を含む")
    }
}

impl Default for AllowedIntervals {
    /// `temporal_id` feature 有効時は `calendar()` と同じ。
    ///
    /// 無効時は `{`[`WHOLE`](Interval::WHOLE)`}` のみの集合を返す。`calendar()` と違い
    /// これは縮退ではない：無効時は他に表現しうる候補（`DAY` 等）が定義上存在しないため、
    /// `{WHOLE}` が取りうる唯一かつ正しい値である。
    fn default() -> Self {
        #[cfg(feature = "temporal_id")]
        {
            CALENDAR.clone()
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            AllowedIntervals::new([])
        }
    }
}

#[cfg(all(test, feature = "temporal_id"))]
mod tests {
    use super::*;

    #[test]
    fn whole_and_second_are_always_present() {
        for intervals in [
            &AllowedIntervals::new([]),
            &AllowedIntervals::new([Interval::HOUR]),
            AllowedIntervals::calendar(),
        ] {
            assert!(intervals.contains(Interval::WHOLE));
            assert!(intervals.contains(Interval::SECOND));
        }
    }

    #[test]
    fn candidates_are_sorted_descending_without_duplicates() {
        let intervals = AllowedIntervals::new([Interval::HOUR, Interval::MINUTE, Interval::HOUR]);
        let seconds: Vec<u64> = intervals.iter().map(|u| u.seconds()).collect();
        assert_eq!(seconds, [Interval::MAX_SECONDS, 3600, 60, 1]);
    }

    /// [`CALENDAR`] は `new` を通さない `static` なので、降順・重複なしの不変条件を
    /// ここで固定する。`coarsest_dividing` は「先頭から最初に割り切れたもの」を返すため、
    /// 順序が崩れると最も粗い単位が選ばれなくなる。
    #[test]
    fn calendar_is_sorted_descending_without_duplicates() {
        let seconds: Vec<u64> = AllowedIntervals::calendar()
            .iter()
            .map(|u| u.seconds())
            .collect();
        assert_eq!(seconds, [Interval::MAX_SECONDS, 86_400, 3_600, 60, 1]);

        // `new` 経由で同じ候補を組んだものと一致する。
        assert_eq!(
            AllowedIntervals::calendar(),
            &AllowedIntervals::new([Interval::DAY, Interval::HOUR, Interval::MINUTE])
        );
    }

    /// 全時間は `WHOLE` で1Segmentに収まる。
    ///
    /// 暦の単位はどれも `2^35` を割り切らないので、`WHOLE` を必須にしていないと
    /// 時間なしの ID が `_1/0:34359738367`（343億Segment）になってしまう。
    #[test]
    fn whole_time_stays_a_single_segment() {
        let intervals = AllowedIntervals::calendar();
        assert_eq!(
            intervals.coarsest_dividing(0, Interval::MAX_SECONDS),
            Interval::WHOLE
        );

        // 前提の確認: 暦の単位では 2^35 を割り切れない。
        for u in [86_400u64, 3_600, 60] {
            assert_ne!(
                Interval::MAX_SECONDS % u,
                0,
                "2^35 が {u} で割り切れてしまう"
            );
        }
    }

    #[test]
    fn picks_the_coarsest_candidate() {
        let intervals = AllowedIntervals::calendar();
        // ちょうど1日
        assert_eq!(intervals.coarsest_dividing(86_400, 172_800), Interval::DAY);
        // 2時間ぶん（`gcd` なら 7200 秒だが、候補にないので 1 時間 × 2 TimeSegment）
        assert_eq!(intervals.coarsest_dividing(0, 7_200), Interval::HOUR);
        // 30分ぶん
        assert_eq!(intervals.coarsest_dividing(0, 1_800), Interval::MINUTE);
        // 暦で表せない端数は秒まで落ちる
        assert_eq!(intervals.coarsest_dividing(30, 90), Interval::SECOND);
    }

    /// 候補を足せば、その単位が選ばれる。
    #[test]
    fn custom_candidates_are_honoured() {
        let half_hour = Interval::new(1_800).unwrap();
        let intervals = AllowedIntervals::new([half_hour]);
        assert_eq!(intervals.coarsest_dividing(1_800, 3_600), half_hour);
        // 候補に無い 3600 は選ばれず、1800 × 2 Segmentになる。
        assert_eq!(intervals.coarsest_dividing(0, 3_600), half_hour);
    }
}

/// 各コレクションから候補集合つきで読み出せることの疎通確認。
///
/// 単位の選択ロジックそのものは上の `tests` が押さえているので、ここでは
/// 「`SpatialIdSet` / `SpatialIdMap` から同じ形で使えること」と
/// 「暦の正規化が既定（`gcd`）と実際に違う結果になること」を固定する。
#[cfg(all(test, feature = "temporal_id"))]
mod collection_api {
    use crate::{AllowedIntervals, Interval, SingleId, SpatialId, SpatialIdMap, SpatialIdSet};
    use alloc::string::ToString;
    use alloc::vec::Vec;

    /// 同じFlexIdの隣り合う2時間ぶん。値は同じなので結合される。
    fn two_hours() -> [SingleId; 2] {
        let base = SingleId::new(12, 0, 3638, 1614).unwrap();
        [
            base.time_with(Interval::HOUR, 0).unwrap(),
            base.time_with(Interval::HOUR, 1).unwrap(),
        ]
    }

    #[test]
    fn set_reads_back_with_calendar_units() {
        let mut set = SpatialIdSet::new();
        for id in two_hours() {
            set.insert(id);
        }

        // 既定は gcd なので「7200 秒 × 1 TimeSegment」。
        let natural: Vec<_> = set
            .reconstructed_time_ranges(None)
            .map(|r| r.to_string())
            .collect();
        assert_eq!(natural, ["12/0/3638/1614_7200/0"]);

        // 暦に正規化すると「3600 秒 × 2 TimeSegment」。
        let calendar: Vec<_> = set
            .reconstructed_time_ranges(Some(AllowedIntervals::calendar()))
            .map(|r| r.to_string())
            .collect();
        assert_eq!(calendar, ["12/0/3638/1614_3600/0:1"]);

        // 秒区間はどの表現でも変わらない。
        assert_eq!(
            set.reconstructed_time_ranges(None)
                .next()
                .unwrap()
                .seconds_range(),
            set.reconstructed_time_ranges(Some(AllowedIntervals::calendar()))
                .next()
                .unwrap()
                .seconds_range()
        );
    }

    /// `AllowedIntervals::calendar()` は `&'static` なので、一時値のまま渡しても
    /// 戻り値のイテレータを束縛できる（所有値を返していた頃は書けなかった形）。
    #[test]
    fn calendar_can_be_passed_inline_and_the_iterator_outlives_it() {
        let mut set = SpatialIdSet::new();
        for id in two_hours() {
            set.insert(id);
        }

        let iter = set.reconstructed_time_ranges(Some(AllowedIntervals::calendar()));
        let got: Vec<_> = iter.map(|r| r.to_string()).collect();
        assert_eq!(got, ["12/0/3638/1614_3600/0:1"]);

        // 一時値の候補集合でも同じ（`intervals` は `&self` とライフタイムを共有しない）。
        let intervals = AllowedIntervals::new([Interval::MINUTE]);
        let iter = set.reconstructed_time_ranges(Some(&intervals));
        assert_eq!(iter.count(), 1);
    }

    /// 時間を持たない木では、候補集合を渡しても遅延イテレータのまま返る。
    ///
    /// 候補集合を一時値として渡せなかった頃は、公開側で `collect::<Vec<_>>()` して
    /// 借用を切っていたため、時間を持たない木でも全葉が先行して積まれていた。
    /// `Vec::into_iter` は要素数を正確に申告するので、`size_hint` の下限が `0` のままで
    /// あることを見れば `collect` の再導入を検出できる。
    #[test]
    fn reading_a_time_free_tree_does_not_materialise() {
        let mut set = SpatialIdSet::new();
        for x in 0..32u32 {
            set.insert(SingleId::new(20, 0, x * 2, 0).unwrap());
        }
        assert_eq!(set.count(), 32);

        for hint in [
            set.reconstructed_time_ranges(None).size_hint(),
            set.reconstructed_time_ranges(Some(AllowedIntervals::calendar()))
                .size_hint(),
            set.flat_single_ids(None).size_hint(),
            set.flat_single_ids(Some(AllowedIntervals::calendar()))
                .size_hint(),
        ] {
            assert_eq!(hint, (0, None), "先行して全件を積んでいる");
        }
    }

    #[test]
    fn table_reads_back_with_calendar_units() {
        let mut table: SpatialIdMap<u32> = SpatialIdMap::new();
        for id in two_hours() {
            table.insert(id, 7);
        }

        let got: Vec<_> = table
            .reconstructed_time_ranges(Some(AllowedIntervals::calendar()))
            .map(|(r, v)| (r.to_string(), *v))
            .collect();
        assert_eq!(got, [("12/0/3638/1614_3600/0:1".into(), 7u32)]);
    }

    /// `flat_single_ids` 側でも単位を選べる。暦にすると2Segmentなので2件へ展開される。
    #[test]
    fn flat_single_ids_honours_the_unit() {
        let mut set = SpatialIdSet::new();
        for id in two_hours() {
            set.insert(id);
        }

        // 既定（gcd, 7200秒 × 1Segment）は1件。
        assert_eq!(set.flat_single_ids(None).count(), 1);

        // 暦（3600秒 × 2Segment）は2件へ展開される。
        let calendar: Vec<_> = set
            .flat_single_ids(Some(AllowedIntervals::calendar()))
            .map(|s| s.to_string())
            .collect();
        assert_eq!(calendar, ["12/0/3638/1614_3600/0", "12/0/3638/1614_3600/1"]);
    }

    /// 時間を持たない ID は、候補集合を渡しても全時間のまま。
    ///
    /// 暦の単位はどれも `2^35` を割り切らないので、`WHOLE` が候補に無いと
    /// `_1/0:34359738367`（343億Segment）に化ける。
    #[test]
    fn whole_time_ids_are_untouched() {
        let mut set = SpatialIdSet::new();
        set.insert(SingleId::new(12, 0, 3638, 1614).unwrap());

        for got in [
            set.reconstructed_time_ranges(None).next().unwrap(),
            set.reconstructed_time_ranges(Some(AllowedIntervals::calendar()))
                .next()
                .unwrap(),
        ] {
            assert_eq!(got.time_interval(), Interval::WHOLE);
            assert_eq!(got.t(), [0, 0]);
            assert_eq!(got.to_string(), "12/0/3638/1614");
        }
    }

    /// 仕様書の例（1800秒）は、既定なら往復し、暦に正規化すると分（60秒）へ落ちる。
    #[test]
    fn spec_example_under_each_policy() {
        let original = SingleId::new(12, 0, 3638, 1614)
            .unwrap()
            .time_with(1800, 809712)
            .unwrap();
        let mut set = SpatialIdSet::new();
        set.insert(original);

        assert_eq!(
            set.reconstructed_time_ranges(None)
                .next()
                .unwrap()
                .to_string(),
            "12/0/3638/1614_1800/809712"
        );
        // 1800 は暦の候補に無いので、割り切れる最も粗い候補＝分になる。
        let calendar = set
            .reconstructed_time_ranges(Some(AllowedIntervals::calendar()))
            .next()
            .unwrap();
        assert_eq!(calendar.time_interval(), Interval::MINUTE);
        assert_eq!(calendar.t(), [24_291_360, 24_291_389]);
        // 秒区間は変わらない。
        assert_eq!(calendar.seconds_range(), original.seconds_range());
    }
}
