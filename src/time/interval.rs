//! 時間間隔 `{i}`を表す型に関する実装

use super::span::TimeSpan;
use crate::{SpatialIdError, error::Error};

/// 時間 ID の時間間隔 `{i}`（秒数）を表現する型。
///
/// よく使う値は関連定数として用意している。
///
/// | 定数 | 秒数 |
/// |---|---|
/// | [`WHOLE`](Self::WHOLE) | `2^35`（約1089年） |
/// | [`DAY`](Self::DAY) | 86400 |
/// | [`HOUR`](Self::HOUR) | 3600 |
/// | [`MINUTE`](Self::MINUTE) | 60 |
/// | [`SECOND`](Self::SECOND) | 1 |
#[cfg(feature = "temporal_id")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Interval(u64);

/// `temporal_id` feature 無効時の [`Interval`]。常に全時間を表すサイズ0のスタブ。
#[cfg(not(feature = "temporal_id"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Interval;

impl Interval {
    /// このライブラリが扱える最大の時間間隔＝全時間の秒数
    /// （`2^35` 秒 = 34,359,738,368 秒、約1089年）。[`new`](Self::new) はこれを超える値を拒む。
    pub const MAX_SECONDS: u64 = 1u64 << Self::MAX_POW;

    /// 最も粗い時間単位を表す二進層の指数。時間軸の最大ズームレベル。
    pub const MAX_POW: u8 = 35;

    /// 全時間（`2^35` 秒）。時間を指定していない ID はこの値を持つ。
    #[cfg(feature = "temporal_id")]
    pub const WHOLE: Interval = Interval(Self::MAX_SECONDS);
    /// 全時間。`temporal_id` feature 無効時に唯一有効な値。
    #[cfg(not(feature = "temporal_id"))]
    pub const WHOLE: Interval = Interval;

    /// 1日（86400秒）。
    #[cfg(feature = "temporal_id")]
    pub const DAY: Interval = Interval(86_400);
    /// 1時間（3600秒）。
    #[cfg(feature = "temporal_id")]
    pub const HOUR: Interval = Interval(3_600);
    /// 1分（60秒）。
    #[cfg(feature = "temporal_id")]
    pub const MINUTE: Interval = Interval(60);
    /// 1秒。
    #[cfg(feature = "temporal_id")]
    pub const SECOND: Interval = Interval(1);

    /// 秒数から [`Interval`] を作成する。
    ///
    /// # バリデーション
    /// - `seconds` が `0`、または [`MAX_SECONDS`](Self::MAX_SECONDS) を超える場合は
    ///   [`SpatialIdError::TIntervalError`] を返す。
    /// - `temporal_id` feature 無効時は [`MAX_SECONDS`](Self::MAX_SECONDS) 以外を拒否する。
    ///
    /// ```
    /// # use spatial_id::time::Interval;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// assert_eq!(Interval::new(3600).unwrap(), Interval::HOUR);
    /// // 仕様どおり、候補以外の秒数も許容する。
    /// assert_eq!(Interval::new(1800).unwrap().seconds(), 1800);
    /// assert!(Interval::new(0).is_err());
    /// # }
    /// ```
    pub const fn new(seconds: u64) -> Result<Interval, Error> {
        #[cfg(feature = "temporal_id")]
        {
            if seconds == 0 || seconds > Self::MAX_SECONDS {
                return Err(Error::SpatialId(SpatialIdError::TIntervalError {
                    i: seconds,
                }));
            }
            Ok(Interval(seconds))
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            if seconds == Self::MAX_SECONDS {
                Ok(Interval)
            } else {
                Err(Error::SpatialId(SpatialIdError::TIntervalError {
                    i: seconds,
                }))
            }
        }
    }

    /// この [`Interval`] の秒数。
    pub const fn seconds(self) -> u64 {
        #[cfg(feature = "temporal_id")]
        {
            self.0
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            Self::MAX_SECONDS
        }
    }

    /// この [`Interval`] の秒数が2の冪乗であるかを判定する。
    ///
    /// 時間付き ID を生成する際、時間間隔が2の冪秒であれば、木構造の中で必ず1つの時間Segment
    /// （[`FlexId`](crate::FlexId)）に収まるため、パフォーマンスが最適化される。
    /// 非2冪の秒数（例: 30分 = 1800秒）を指定した場合、このメソッドは `false` を返し、
    /// 内部で最大十数個のSegmentに分解されるため処理コストが比例して増大する。
    pub const fn is_power_of_two(self) -> bool {
        #[cfg(feature = "temporal_id")]
        {
            self.0.is_power_of_two()
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            true
        }
    }

    /// この単位で、Unix 時刻（秒）が属するSegmentのインデックス `{t}` を返す。
    ///
    /// 仕様書 1.5.3 (3) の `t = floor(u / i)`。
    ///
    /// ```
    /// # use spatial_id::time::Interval;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// assert_eq!(Interval::HOUR.index_of(7_300), 2);
    /// assert_eq!(Interval::new(1800).unwrap().index_of(1_457_482_000), 809_712);
    /// # }
    /// ```
    pub const fn index_of(self, unix_seconds: u64) -> u64 {
        unix_seconds / self.seconds()
    }

    /// `{i}` と `{t}` の範囲（両端含む）が占める絶対秒区間 `[start, end)` を、検証して返す。
    ///
    /// 単一Segmentの型（[`SingleId`](crate::SingleId) / [`FlexId`](crate::FlexId)）は
    /// `t_min == t_max` で呼ぶ。仕様は `{i}` に任意の秒数を認めるが、区間の終端が
    /// [`Interval::MAX_SECONDS`] を超えてはならない。
    pub(crate) fn validated_span(&self, t_min: u64, t_max: u64) -> Result<TimeSpan, Error> {
        let unit = self.seconds();
        let end = t_max
            .checked_add(1)
            .and_then(|v| v.checked_mul(unit))
            .ok_or(SpatialIdError::TOutOfRange { i: unit, t: t_max })?;

        if end > Interval::MAX_SECONDS {
            return Err(SpatialIdError::TOutOfRange { i: unit, t: t_max }.into());
        }

        TimeSpan::new(t_min * unit, end)
            .ok_or_else(|| SpatialIdError::TOutOfRange { i: unit, t: t_max }.into())
    }
}

impl Default for Interval {
    /// [`WHOLE`](Self::WHOLE)（時間指定なし）。
    ///
    /// `temporal_id` feature の有無に関わらず定義する（無効時は derive していた頃、
    /// 有効時には無かった。feature の有無で trait 実装が増減すると、ジェネリックコードが
    /// 一方の構成でだけ壊れるため揃えている）。
    fn default() -> Self {
        Self::WHOLE
    }
}

impl core::fmt::Display for Interval {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.seconds())
    }
}

impl TryFrom<u64> for Interval {
    type Error = Error;

    fn try_from(seconds: u64) -> Result<Self, Self::Error> {
        Interval::new(seconds)
    }
}

/// `u64` へ収まらない値は**必ず失敗させる**。`as u64` で丸めると、`u128` の
/// `2^64 + 3600` や `i128` の負値が `Ok(Interval::HOUR)` として通ってしまう。
macro_rules! impl_try_from_integer {
    ($($t:ty),*) => {
        $(
            impl TryFrom<$t> for Interval {
                type Error = Error;

                fn try_from(seconds: $t) -> Result<Self, Self::Error> {
                    let seconds = u64::try_from(seconds)
                        .map_err(|_| SpatialIdError::TIntervalError { i: seconds as u64 })?;
                    Interval::new(seconds)
                }
            }
        )*
    };
}

impl_try_from_integer!(u8, u16, u32, u128, usize, i8, i16, i32, i64, i128, isize);

#[cfg(all(test, feature = "temporal_id"))]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_seconds() {
        assert!(Interval::new(0).is_err());
        assert!(Interval::new(Interval::MAX_SECONDS + 1).is_err());
        assert!(Interval::new(Interval::MAX_SECONDS).is_ok());
    }

    /// `as u64` による丸めを許すと、範囲外の値が別の有効な [`Interval`] に化けてしまう。
    #[test]
    fn try_from_wide_integers_never_truncates() {
        // 2^64 + 3600。`as u64` だと 3600（＝1時間）になっていた。
        assert!(Interval::try_from((1u128 << 64) + 3600).is_err());
        // 負の巨大値。`as u64` だと同じく 3600 として通っていた。
        assert!(Interval::try_from(-((1i128 << 64) - 3600)).is_err());

        assert!(Interval::try_from(-1i32).is_err());
        assert!(Interval::try_from(-1i64).is_err());
        assert!(Interval::try_from(u128::MAX).is_err());

        // 範囲内は従来どおり通る。`i64` は実装漏れしていた。
        assert_eq!(Interval::try_from(3600i64).unwrap(), Interval::HOUR);
        assert_eq!(Interval::try_from(3600u32).unwrap(), Interval::HOUR);
        assert_eq!(Interval::try_from(3600i128).unwrap(), Interval::HOUR);
    }

    #[test]
    fn index_of_follows_the_spec_formula() {
        // t = floor(u / i)
        for (i, u, expected) in [
            (3600u64, 7_300u64, 2u64),
            (1800, 1_457_482_000, 809_712),
            (1, 1_770_000_000, 1_770_000_000),
            (86_400, 86_399, 0),
        ] {
            assert_eq!(Interval::new(i).unwrap().index_of(u), expected);
        }
    }
}

/// 3型（[`SingleId`](crate::SingleId) / [`RangeId`](crate::RangeId) / [`FlexId`](crate::FlexId)）
/// で、時間 API の名前・引数・戻り値の形が揃っていることを固定する。
///
/// 型ごとに「表せる時間の形」は違う（1Segment / 範囲 / 2分岐Segment1個）が、**入口の名前と失敗の
/// 仕方は同じ**であるべき。片方だけに生えた補助コンストラクタや、[`Option`] と [`Result`] の
/// 混在を防ぐための回帰テストである。
#[cfg(all(test, feature = "temporal_id"))]
mod api_symmetry {
    use crate::{FlexId, Interval, RangeId, SingleId, SpatialId};

    /// `time_with` は3型とも同じ名前・同じ [`Result`] で、`{i}/{t}` を受ける。
    #[test]
    fn with_time_is_available_on_every_type() {
        let single = SingleId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(1024, 7)
            .unwrap();
        let range = RangeId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(1024, 7)
            .unwrap();
        // FlexId だけは母語（ズーム）で指定する。ズーム25 = 2^(35-25) = 1024 秒幅。
        let flex = FlexId::new(4, 0, 4, 1, 4, 1)
            .unwrap()
            .time_with_segment(25, 7)
            .unwrap();

        // 同じ `{i}/{t}` を指定したので、占める秒区間も一致する。
        let expected = (1024 * 7, 1024 * 8);
        assert_eq!(single.seconds_range(), expected);
        assert_eq!(range.seconds_range(), expected);
        assert_eq!(flex.seconds_range(), expected);
    }

    /// `interval` 引数は [`Interval`] でも整数リテラルでも `u64` でも通る。
    #[test]
    fn interval_argument_accepts_every_spelling() {
        let by_const = SingleId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(Interval::HOUR, 3)
            .unwrap();
        let by_literal = SingleId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(3600, 3)
            .unwrap();
        let by_u64 = SingleId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(3600u64, 3)
            .unwrap();

        assert_eq!(by_const, by_literal);
        assert_eq!(by_const, by_u64);
    }

    /// Unix 時刻から時間を決める入口。
    #[test]
    fn time_with_at_contains_the_given_instant() {
        const UNIX: u64 = 1_770_000_000;
        let single = SingleId::new(4, 0, 1, 1)
            .unwrap()
            .time_with_at(1024, UNIX)
            .unwrap();

        assert!(single.seconds_range().0 <= UNIX && UNIX < single.seconds_range().1);
    }

    /// `time_relabel` は3型とも [`Result`]（[`Option`] との混在を許さない）。
    #[test]
    fn relabel_time_returns_result_on_every_type() {
        let single = SingleId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(3600, 5)
            .unwrap();
        assert_eq!(single.time_relabel(Interval::HOUR).unwrap(), single);
        assert!(single.time_relabel(1800).is_err());

        let range = RangeId::new(4, 0, 1, 1)
            .unwrap()
            .time_with(3600, [0, 1])
            .unwrap();
        assert_eq!(range.time_relabel(1800).unwrap().t(), [0, 3]);
        assert!(range.time_relabel(Interval::DAY).is_err());

        // `FlexId` には `time_relabel` を持たせない。Segmentが秒区間を一意に決めるので、
        // 「同じ区間を別の単位で」は定義上ありえず、成功しても必ず元と同じ値になる
        // （常に no-op か Err にしかならないAPIは、対称性のためだけに置く価値がない）。
    }
}
