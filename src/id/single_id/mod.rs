pub mod constructor;
pub mod convert;
#[cfg(feature = "unstable")]
pub mod encode;
pub mod impls;
pub mod ops;
pub mod random;
#[cfg(test)]
mod test;

use crate::{
    Interval, SpatialId, SpatialIdError, error::Error, id::zoom_level::ZoomLevel, time::span,
};

/// 標準的な時空間IDを表す型。
///
/// [Ouranos 4D 時空間ID仕様](https://www.ipa.go.jp/)の Spatio-temporal ID
/// `{z}/{f}/{x}/{y}_{i}/{t}` に一対一で対応する。空間部分が[`ZoomLevel`]でのf/x/y、
/// 時間部分が時間間隔 `i`（[`Interval`]）と**単一**インデックス `t` である。
///
/// 範囲を扱いたい場合は[`RangeId`](crate::RangeId)を使う。
///
/// 内部的には下記のような構造体で構成されている。
///
/// この型は [`PartialOrd`] / [`Ord`] を実装しているが、これは主に[`BTreeSet`](alloc::collections::BTreeSet) や [`BTreeMap`](alloc::collections::BTreeMap) などの順序付きコレクションでの格納・探索用であり、実際の空間的な「大小」を意味するものではない。
///
/// ```text
/// # use spatial_id::SpatialId;
/// pub struct SingleId {
///     z: ZoomLevel,
///     f: i32,
///     x: u32,
///     y: u32,
///
///     interval: Interval,    // 仕様の {i}
///     t: u64,                // 仕様の {t}（単一）
/// }
/// ```
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
pub struct SingleId {
    z: ZoomLevel,
    f: i32,
    x: u32,
    y: u32,
    i: Interval,
    #[cfg(feature = "temporal_id")]
    t: u64,
}

impl SingleId {
    /// この [`SingleId`] が保持しているズームレベル `z` を返す。
    ///
    /// ```no_run
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(5, 3, 2, 10).unwrap();
    /// assert_eq!(id.z(), 5u8);
    /// ```
    pub fn z(&self) -> u8 {
        self.z.get()
    }

    /// この [`SingleId`] が保持している F インデックス `f` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(5, 3, 2, 10).unwrap();
    /// assert_eq!(id.f(), 3i32);
    /// ```
    pub fn f(&self) -> i32 {
        self.f
    }

    /// この [`SingleId`] が保持している X インデックス `x` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(5, 3, 2, 10).unwrap();
    /// assert_eq!(id.x(), 2u32);
    /// ```
    pub fn x(&self) -> u32 {
        self.x
    }

    /// この [`SingleId`] が保持している Y インデックス `y` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(5, 3, 2, 10).unwrap();
    /// assert_eq!(id.y(), 10u32);
    /// ```
    pub fn y(&self) -> u32 {
        self.y
    }

    /// この [`SingleId`] の時間インデックス `{t}` を返す。
    ///
    /// `temporal_id` feature 無効時は常に `0`（全時間の唯一のSegment）。
    pub fn t(&self) -> u64 {
        #[cfg(feature = "temporal_id")]
        {
            self.t
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            0
        }
    }

    /// 時間を設定した自身を返す（ビルダー形式）。
    ///
    /// [Ouranos 4D 時空間ID仕様](https://www.ipa.go.jp/)の `{i}/{t}` に対応する。`interval` は
    /// [`Interval`]、または秒数を表す整数（無注釈のリテラルを含む）のどちらでも渡せる。
    ///
    /// ## 動作コスト
    ///
    /// 木構造（[`SpatialIdSet`](crate::collection::SpatialIdSet) など）へ挿入する際、時間間隔が2の冪秒であれば必ず1つの時間Segment
    /// （[`FlexId`](crate::FlexId)）に収まる。しかし非2冪の秒数（例: 1800秒）を指定した場合、
    /// 内部で最大十数個のSegmentに分解されるため、**挿入コストと木の葉の数が比例して増大**する。
    /// パフォーマンスが重視される用途では、1800秒や3600秒の代わりに、**1024秒**や**4096秒**
    /// などの2の冪秒を利用することを強く推奨する（挿入処理が約10倍高速化される）。
    ///
    /// 仕様どおり任意の秒数を単位にできる。
    ///
    /// [`SingleId`] は空間が1Segmentなのと揃えて**時間も1Segment**しか持たない。時間の範囲を扱いたい
    /// 場合は [`RangeId::with_time`](crate::RangeId::with_time) を使う。
    ///
    /// FlexTreeが内部で必要とする2の冪秒のSegmentへの分解は、木へ挿入する段階
    /// （[`IntoIterator`]による[`FlexId`](crate::FlexId)への展開）で自動的に行われる。
    ///
    /// # バリデーション
    /// - `interval` が負、`0`、または [`Interval::MAX_SECONDS`] を超える場合は
    ///   [`SpatialIdError::TIntervalError`] を返す。
    /// - `(t + 1) * interval` が [`Interval::MAX_SECONDS`] を超える場合は
    ///   [`SpatialIdError::TOutOfRange`] を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, time::Interval};
    /// // 仕様書の例: 12/0/3638/1614_1800/809712（30分単位）
    /// let id = SingleId::new(12, 0, 3638, 1614).unwrap().with_time(1800, 809712).unwrap();
    /// assert_eq!(id.to_string(), "12/0/3638/1614_1800/809712");
    ///
    /// // 定数でも、秒数の `u64` でも指定できる。
    /// let id = SingleId::new(5, 3, 2, 10).unwrap().with_time(Interval::MINUTE, 3).unwrap();
    /// assert_eq!(id.to_string(), "5/3/2/10_60/3");
    /// let same = SingleId::new(5, 3, 2, 10).unwrap().with_time(60u64, 3).unwrap();
    /// assert_eq!(same, id);
    /// # }
    /// ```
    pub fn with_time<I>(self, interval: I, t: u64) -> Result<Self, Error>
    where
        I: TryInto<Interval>,
        Error: From<I::Error>,
    {
        let interval: Interval = interval.try_into()?;
        interval.validated_span(t, t)?;
        Ok(self.with_time_unchecked(interval, t))
    }

    /// Unix 時刻（秒）が属する時間Segmentを設定した自身を返す。
    ///
    /// 仕様書 1.5.3 (3) の `t = floor(u / i)` をこちらで計算するので、呼び出し側で
    /// インデックスを求める必要がない。
    ///
    /// ## 動作コスト
    ///
    /// 木構造（[`SpatialIdSet`](crate::collection::SpatialIdSet) など）へ挿入する際、時間間隔が2の冪秒であれば必ず1つの時間Segment
    /// （[`FlexId`](crate::FlexId)）に収まる。しかし非2冪の秒数（例: 1800秒）を指定した場合、
    /// 内部で最大十数個のSegmentに分解されるため、**挿入コストと木の葉の数が比例して増大**する。
    /// パフォーマンスが重視される用途では、1800秒や3600秒の代わりに、**1024秒**や**4096秒**
    /// などの2の冪秒を利用することを強く推奨する（挿入処理が約10倍高速化される）。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::SingleId;
    /// // 1457482000 秒は 30 分単位で 809712 番目のSegmentに入る。
    /// let id = SingleId::new(12, 0, 3638, 1614).unwrap().with_time_at(1800, 1_457_482_000).unwrap();
    /// assert_eq!(id.to_string(), "12/0/3638/1614_1800/809712");
    /// # }
    /// ```
    pub fn with_time_at<I>(self, interval: I, unix_seconds: u64) -> Result<Self, Error>
    where
        I: TryInto<Interval>,
        Error: From<I::Error>,
    {
        let interval: Interval = interval.try_into()?;
        self.with_time::<Interval>(interval, interval.index_of(unix_seconds))
    }

    /// 絶対秒区間 `[start, end)` を設定した自身を返す（1970-01-01 00:00 UTC 起点）。
    ///
    /// 単位は「その区間をちょうど表せる最も粗い秒数」が自動で選ばれる
    /// （`start` と区間幅の最大公約数）。[`SingleId`] は時間も1Segmentなので、
    /// **その単位でちょうど1Segmentにならない区間は受け付けない**。
    /// 複数Segmentにまたがる区間は [`RangeId::with_time_span`](crate::RangeId::with_time_span) を使う。
    ///
    /// ## 動作コスト
    ///
    /// 木構造（[`SpatialIdSet`](crate::collection::SpatialIdSet) など）へ挿入する際、区間幅が2の冪秒であれば必ず1つの時間Segment
    /// （[`FlexId`](crate::FlexId)）に収まる。しかし非2冪の幅（例: 1800秒）が選ばれた場合、
    /// 内部で最大十数個のSegmentに分解されるため、**挿入コストと木の葉の数が比例して増大**する。
    /// パフォーマンスが重視される用途では、区間幅が1800秒や3600秒になる代わりに、**1024秒**や**4096秒**
    /// などの2の冪秒になるように指定することを強く推奨する（挿入処理が約10倍高速化される）。
    ///
    /// # バリデーション
    /// - `start >= end`、または `end` が [`Interval::MAX_SECONDS`] を超える場合は
    ///   [`SpatialIdError::TOutOfRange`] を返す。
    /// - 区間が単一Segmentにならない場合は [`SpatialIdError::TIntervalError`] を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, 0, 0, 0).unwrap()
    ///     .with_time_span(1_457_481_600, 1_457_483_400).unwrap();
    /// assert_eq!(id.time_interval().seconds(), 1800);
    /// assert_eq!(id.t(), 809712);
    ///
    /// // 1Segmentに収まらない区間（1800秒 × 3Segment）は拒否する。
    /// assert!(SingleId::new(4, 0, 0, 0).unwrap().with_time_span(1800, 7200).is_err());
    /// # }
    /// ```
    pub fn with_time_span(self, start: u64, end: u64) -> Result<Self, Error> {
        let time_span =
            span::TimeSpan::new(start, end).ok_or(SpatialIdError::TOutOfRange { i: 1, t: end })?;
        let (interval, t_min, t_max) = time_span.to_interval_range()?;
        if t_min != t_max {
            return Err(SpatialIdError::TIntervalError {
                i: interval.seconds(),
            }
            .into());
        }
        Ok(self.with_time_unchecked(interval, t_min))
    }

    /// 同じ絶対秒区間を、別の単位で表し直した自身を返す。
    ///
    /// 単位が変わっても指す時刻は変わらない。その単位で単一Segmentにならない場合は
    /// [`SpatialIdError::TIntervalError`] を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, time::Interval};
    /// let id = SingleId::new(5, 3, 2, 10).unwrap().with_time(3600, 5).unwrap();
    /// // 1時間ぶんは 1800 秒単位では 2 Segmentになるので単一にできない。
    /// assert!(id.relabel_time(1800).is_err());
    /// assert_eq!(id.relabel_time(Interval::HOUR).unwrap().t(), 5);
    /// # }
    /// ```
    pub fn relabel_time<I>(self, interval: I) -> Result<Self, Error>
    where
        I: TryInto<Interval>,
        Error: From<I::Error>,
    {
        let interval: Interval = interval.try_into()?;
        let (start, end) = self.seconds_range();
        let unit = interval.seconds();
        if end - start != unit || !start.is_multiple_of(unit) {
            return Err(SpatialIdError::TIntervalError { i: unit }.into());
        }
        Ok(self.with_time_unchecked(interval, start / unit))
    }

    /// 時間の指定を外し、全時間へ戻した自身を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, SpatialId};
    /// let id = SingleId::new(5, 3, 2, 10).unwrap().with_time(3600, 5).unwrap();
    /// assert!(id.without_time().is_whole_time());
    /// # }
    /// ```
    pub fn without_time(self) -> Self {
        self.with_time_unchecked(Interval::WHOLE, 0)
    }

    /// 内部表現をそのまま取得する。クレート内部専用。
    pub(crate) fn time_segments(&self) -> span::TimeSegments {
        span::TimeSpan::of(self).into_segments()
    }

    /// 内部表現をそのまま設定する。クレート内部専用（検証済みの値を渡すこと）。
    pub(crate) fn with_time_unchecked(
        #[cfg_attr(not(feature = "temporal_id"), allow(unused_mut))] mut self,
        i: Interval,
        t: u64,
    ) -> Self {
        self.i = i;
        #[cfg(feature = "temporal_id")]
        {
            self.t = t;
        }
        #[cfg(not(feature = "temporal_id"))]
        debug_assert_eq!(t, 0, "temporal_id 無効時に t != 0 を設定しようとした");
        self
    }

    /// F インデックスを更新する。
    ///
    /// 与えられた `value` が、現在のズームレベル `z` に対応する
    /// `ZoomLevel::new(z as u8)?.f_min()..=ZoomLevel::new(z as u8).unwrap().f_max()` の範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # パラメータ
    /// * `value` — 新しい F インデックス
    ///
    /// # バリデーション
    /// - `value` が許容範囲外の場合、[`SpatialIdError::FOutOfRange`] を返す。
    ///
    /// 正常な更新:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let mut id = SingleId::new(5, 3, 2, 10).unwrap();
    /// id.set_f(4).unwrap();
    /// assert_eq!(id.f(), 4);
    /// ```
    ///
    /// 範囲外の検知:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::SingleId;
    /// let mut id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let result = id.set_f(999);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::FOutOfRange { z: 3, f: 999 }))));
    /// ```
    pub fn set_f(&mut self, value: i32) -> Result<(), Error> {
        self.z.check_f(value)?;
        self.f = value;
        Ok(())
    }

    /// X インデックスを更新する。
    ///
    /// 与えられた `value` が、現在のズームレベル `z` に対応する
    /// `0..=ZoomLevel::new(z as u8).unwrap().xy_max()` の範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # パラメータ
    /// * `value` — 新しい X インデックス
    ///
    /// # バリデーション
    /// - `value` が許容範囲外の場合、[`SpatialIdError::XOutOfRange`] を返す。
    ///
    /// 正常な更新:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let mut id = SingleId::new(5, 3, 2, 10).unwrap();
    /// id.set_x(4).unwrap();
    /// assert_eq!(id.x(), 4);
    /// ```
    ///
    /// 範囲外の検知
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::SingleId;
    /// let mut id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let result = id.set_x(999);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::XOutOfRange { z: 3, x: 999 }))));
    /// ```
    pub fn set_x(&mut self, value: u32) -> Result<(), Error> {
        self.z.check_x(value)?;
        self.x = value;
        Ok(())
    }

    /// Y インデックスを更新する。
    ///
    /// 与えられた `value` が、現在のズームレベル `z` に対応する
    /// `0..=ZoomLevel::new(z as u8).unwrap().xy_max()` の範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # パラメータ
    /// * `value` — 新しい Y インデックス
    ///
    /// # バリデーション
    /// - `value` が許容範囲外の場合、[`SpatialIdError::YOutOfRange`] を返す。
    ///
    /// 正常な更新
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let mut id = SingleId::new(5, 3, 2, 10).unwrap();
    /// id.set_y(8).unwrap();
    /// assert_eq!(id.y(), 8);
    /// ```
    ///
    /// 範囲外の検知
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::SingleId;
    /// let mut id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let result = id.set_y(999);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::YOutOfRange { z: 3, y: 999 }))));
    /// ```
    pub fn set_y(&mut self, value: u32) -> Result<(), Error> {
        self.z.check_y(value)?;
        self.y = value;
        Ok(())
    }

    /// 指定したズームレベル `target_z` に細分化した、この [`SingleId`] を含むすべての子 [`SingleId`] を生成する。
    ///
    /// # パラメータ
    /// * `target_z` — 生成したい子 [`SingleId`] のズームレベル
    ///
    /// # バリデーション
    /// - `target_z` が現在のズームレベルより浅い場合は、[`SpatialIdError::ZoomLevelTransitionOutOfRange`] を返す。
    /// - `target_z` が本クレートで扱える最大ズームレベルを超える場合は、[`SpatialIdError::ZOutOfRange`] を返す。
    ///
    /// 1段深いズームへの細分化
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(3, 3, 2, 7).unwrap();
    ///
    /// // target_z = 4 のため F, X, Y はそれぞれ 2 分割される
    /// let children: Vec<_> = id.spatial_children_at_zoom(4).unwrap().collect();
    ///
    /// assert_eq!(children.len(), 8); // 2 × 2 × 2
    ///
    /// // 最初の要素を確認（f, x, y の下限側）
    /// let first = &children[0];
    /// assert_eq!(first.z(), 4);
    /// assert_eq!(first.f(), 3 * 2);   // 2
    /// assert_eq!(first.x(), 2 * 2);   // 6
    /// assert_eq!(first.y(), 7 * 2);   // 8
    /// ```
    ///
    /// 現在より浅いズームを指定した場合
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let result = id.spatial_children_at_zoom(2);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::ZoomLevelTransitionOutOfRange { current_z: 3, target_z: 2 }))));
    /// ```
    pub fn spatial_children_at_zoom(
        &self,
        target_z: u8,
    ) -> Result<impl Iterator<Item = SingleId>, Error> {
        let target_zoom = ZoomLevel::new(target_z)?;

        if target_z < self.z() {
            return Err(SpatialIdError::ZoomLevelTransitionOutOfRange {
                current_z: self.z(),
                target_z,
            }
            .into());
        }

        let difference = target_z - self.z();

        let scale_f = 1_i32 << difference as u32;
        let scale_xy = 1_u32 << difference as u32;

        let f_start = self.f * scale_f;
        let x_start = self.x * scale_xy;
        let y_start = self.y * scale_xy;

        let f_range = f_start..=f_start + scale_f - 1;
        let x_range = x_start..=x_start + scale_xy - 1;
        let y_range = y_start..=y_start + scale_xy - 1;

        Ok(f_range.flat_map(move |f| {
            let x_range = x_range.clone();
            let y_range = y_range.clone();

            x_range.flat_map(move |x| {
                y_range.clone().map(move |y| SingleId {
                    z: target_zoom,
                    f,
                    x,
                    y,

                    i: self.i,
                    #[cfg(feature = "temporal_id")]
                    t: self.t,
                })
            })
        }))
    }

    /// 1 段深いズームレベル（`z + 1`）に細分化した、この [`SingleId`] を含む 8 個の子 [`SingleId`] を生成する。
    ///
    /// # バリデーション
    /// - 現在のズームレベルが最大ズームレベル [`ZoomLevel::MAX`] の場合は、[`SpatialIdError::ZOutOfRange`] を返す。
    pub fn spatial_children(&self) -> Result<impl Iterator<Item = SingleId>, Error> {
        self.spatial_children_at_zoom(self.z() + 1)
    }

    /// 指定したズームレベル `target_z` に縮約した、この [`SingleId`] の親 [`SingleId`] を返す。
    ///
    /// # パラメータ
    /// * `target_z` — 取得したい親 [`SingleId`] のズームレベル
    ///
    /// # バリデーション
    /// - `target_z` が現在のズームレベルより深い場合は、[`SpatialIdError::ZoomLevelTransitionOutOfRange`] を返す。
    /// - `target_z` が本クレートで扱える最大ズームレベルを超える場合は、[`SpatialIdError::ZOutOfRange`] を返す。
    ///
    /// 1段浅いズームへの縮約
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, 6, 9, 14).unwrap();
    ///
    /// let parent = id.spatial_parent_at_zoom(3).unwrap();
    ///
    /// assert_eq!(parent.z(), 3u8);
    /// assert_eq!(parent.f(), 3i32);
    /// assert_eq!(parent.x(), 4u32);
    /// assert_eq!(parent.y(), 7u32);
    /// ```
    ///
    /// Fが負の場合の挙動
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, -1, 8, 12).unwrap();
    ///
    /// let parent = id.spatial_parent_at_zoom(3).unwrap();
    ///
    /// assert_eq!(parent.z(), 3u8);
    /// assert_eq!(parent.f(), -1i32);
    /// assert_eq!(parent.x(), 4u32);
    /// assert_eq!(parent.y(), 6u32);
    /// ```
    ///
    /// 現在より深いズームを指定した場合:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let result = id.spatial_parent_at_zoom(4);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::ZoomLevelTransitionOutOfRange { current_z: 3, target_z: 4 }))));
    /// ```
    pub fn spatial_parent_at_zoom(&self, target_z: u8) -> Result<SingleId, Error> {
        let target_zoom = ZoomLevel::new(target_z)?;

        if target_z > self.z() {
            return Err(SpatialIdError::ZoomLevelTransitionOutOfRange {
                current_z: self.z(),
                target_z,
            }
            .into());
        }

        let difference = self.z() - target_z;
        let f = self.f >> difference;
        let x = self.x >> difference;
        let y = self.y >> difference;

        Ok(SingleId {
            z: target_zoom,
            f,
            x,
            y,

            i: self.i,
            #[cfg(feature = "temporal_id")]
            t: self.t,
        })
    }

    /// 1 段浅いズームレベル（`z - 1`）にある親 [`SingleId`] を返す。`z = 0` の場合は [`None`] を返す。
    pub fn spatial_parent(&self) -> Option<SingleId> {
        if self.z() == 0 {
            None
        } else {
            self.spatial_parent_at_zoom(self.z() - 1).ok()
        }
    }

    /// この [`SingleId`] と同じ親を共有する兄弟 [`SingleId`] を 7 個返す。
    ///
    /// 返される 7 個と自分自身を合わせると、1つ上のズームレベルにある親 [`SingleId`] を構成する。
    /// `z = 0` の場合は親を持たないため [`None`] を返す。
    ///
    /// # 動作例
    ///
    /// 兄弟の列挙:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let siblings = id.spatial_siblings().unwrap();
    ///
    /// assert_eq!(siblings.len(), 7);
    /// assert!(siblings.iter().all(|s| s.spatial_parent_at_zoom(2).unwrap() == id.spatial_parent_at_zoom(2).unwrap()));
    /// assert!(!siblings.contains(&id));
    /// ```
    ///
    /// 最上位の ID の場合:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(0, 0, 0, 0).unwrap();
    /// assert!(id.spatial_siblings().is_none());
    /// ```
    pub fn spatial_siblings(&self) -> Option<[SingleId; 7]> {
        if self.z() == 0 {
            return None;
        }

        let parent_z = self.z() - 1;
        let parent = self.spatial_parent_at_zoom(parent_z).ok()?;
        let next_zoom = self.z;
        let f_start = parent.f() * 2;
        let x_start = parent.x() * 2;
        let y_start = parent.y() * 2;

        let candidates = [
            SingleId {
                z: next_zoom,
                f: f_start,
                x: x_start,
                y: y_start,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start,
                x: x_start,
                y: y_start + 1,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start,
                x: x_start + 1,
                y: y_start,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start,
                x: x_start + 1,
                y: y_start + 1,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start + 1,
                x: x_start,
                y: y_start,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start + 1,
                x: x_start,
                y: y_start + 1,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start + 1,
                x: x_start + 1,
                y: y_start,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
            SingleId {
                z: next_zoom,
                f: f_start + 1,
                x: x_start + 1,
                y: y_start + 1,
                i: self.i,
                #[cfg(feature = "temporal_id")]
                t: self.t,
            },
        ];

        let mut siblings: [Option<SingleId>; 7] = [None, None, None, None, None, None, None];
        let mut index = 0;

        for child in candidates {
            if child != *self {
                siblings[index] = Some(child);
                index += 1;
            }
        }

        Some(siblings.map(|child| child.expect("sibling set should contain exactly 7 items")))
    }

    /// この [`SingleId`] の全ての親を、直近の親から順に列挙する。
    ///
    /// 返す順序は `z - 1`, `z - 2`, ..., `0` であり、元の [`SingleId`] 自身は含まない。
    /// `z = 0` の場合は空のイテレータを返す。
    ///
    /// # 動作例
    ///
    /// 親の列挙:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(3, 3, 2, 7).unwrap();
    /// let parents: Vec<_> = id.spatial_parents().collect();
    ///
    /// assert_eq!(parents.len(), 3);
    /// assert_eq!(parents[0], SingleId::new(2, 1, 1, 3).unwrap());
    /// assert_eq!(parents[1], SingleId::new(1, 0, 0, 1).unwrap());
    /// assert_eq!(parents[2], SingleId::new(0, 0, 0, 0).unwrap());
    /// ```
    pub fn spatial_parents(&self) -> impl Iterator<Item = SingleId> + '_ {
        (0..self.z()).rev().map(move |target_z| {
            self.spatial_parent_at_zoom(target_z)
                .expect("target_z must be valid")
        })
    }

    /// この [`SingleId`] の面を共有する 6 近傍を列挙する。
    ///
    /// 近傍は `f` / `x` / `y` の各軸について `±1` だけ動かした 6 個である。
    /// `x` は循環するため常に有効だが、`f` と `y` が範囲外になる方向は除外される。
    /// そのため、境界上の [`SingleId`] では 6 個未満になることがある。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, 6, 9, 10).unwrap();
    /// let neighbors: Vec<_> = id.spatial_face_neighbors().collect();
    /// assert_eq!(neighbors.len(), 6);
    /// ```
    pub fn spatial_face_neighbors(&self) -> impl Iterator<Item = SingleId> + '_ {
        const OFFSETS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];

        OFFSETS.into_iter().filter_map(move |(df, dx, dy)| {
            let mut neighbor = *self;

            if df != 0 && neighbor.move_f(df).is_err() {
                return None;
            }

            if dx != 0 {
                neighbor.move_x(dx);
            }

            if dy != 0 && neighbor.move_y(dy).is_err() {
                return None;
            }

            Some(neighbor)
        })
    }

    /// この [`SingleId`] の辺を共有する 12 近傍を列挙する。
    ///
    /// 近傍は `f` / `x` / `y` のうちちょうど 2 軸について `±1` だけ動かした 12 個である。
    /// `x` は循環するため常に有効だが、`f` と `y` が
    /// 範囲外になる方向は除外される。
    /// そのため、境界上の [`SingleId`] では 12 個未満になることがある。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, 6, 9, 10).unwrap();
    /// let neighbors: Vec<_> = id.spatial_edge_neighbors().collect();
    /// assert_eq!(neighbors.len(), 12);
    /// ```
    pub fn spatial_edge_neighbors(&self) -> impl Iterator<Item = SingleId> + '_ {
        const OFFSETS: [(i32, i32, i32); 12] = [
            // f-x 平面 (dy=0)
            (1, 1, 0),
            (1, -1, 0),
            (-1, 1, 0),
            (-1, -1, 0),
            // f-y 平面 (dx=0)
            (1, 0, 1),
            (1, 0, -1),
            (-1, 0, 1),
            (-1, 0, -1),
            // x-y 平面 (df=0)
            (0, 1, 1),
            (0, 1, -1),
            (0, -1, 1),
            (0, -1, -1),
        ];

        OFFSETS.into_iter().filter_map(move |(df, dx, dy)| {
            let mut neighbor = *self;

            if df != 0 && neighbor.move_f(df).is_err() {
                return None;
            }

            if dx != 0 {
                neighbor.move_x(dx);
            }

            if dy != 0 && neighbor.move_y(dy).is_err() {
                return None;
            }

            Some(neighbor)
        })
    }

    /// この [`SingleId`] の頂点を共有する 8 近傍を列挙する。
    ///
    /// 近傍は `f` / `x` / `y` の全 3 軸について `±1` だけ動かした 8 個である。
    /// `x` は循環するため常に有効だが、`f` と `y` が
    /// 範囲外になる方向は除外される。
    /// そのため、境界上の [`SingleId`] では 8 個未満になることがある。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, 6, 9, 10).unwrap();
    /// let neighbors: Vec<_> = id.spatial_vertex_neighbors().collect();
    /// assert_eq!(neighbors.len(), 8);
    /// ```
    pub fn spatial_vertex_neighbors(&self) -> impl Iterator<Item = SingleId> + '_ {
        const OFFSETS: [(i32, i32, i32); 8] = [
            (1, 1, 1),
            (1, 1, -1),
            (1, -1, 1),
            (1, -1, -1),
            (-1, 1, 1),
            (-1, 1, -1),
            (-1, -1, 1),
            (-1, -1, -1),
        ];

        OFFSETS.into_iter().filter_map(move |(df, dx, dy)| {
            let mut neighbor = *self;

            if neighbor.move_f(df).is_err() {
                return None;
            }

            neighbor.move_x(dx);

            if neighbor.move_y(dy).is_err() {
                return None;
            }

            Some(neighbor)
        })
    }

    /// この [`SingleId`] の 26 近傍を列挙する。
    ///
    /// 近傍は `f` / `x` / `y` の各軸について `-1, 0, 1` の組み合わせから
    /// 自身を除いた 26 個である。`x` は循環するため常に有効だが、`f` と `y` が
    /// 範囲外になる方向は除外される。
    /// そのため、境界上の [`SingleId`] では 26 個未満になることがある。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(4, 6, 9, 10).unwrap();
    /// let neighbors: Vec<_> = id.spatial_all_neighbors().collect();
    /// assert_eq!(neighbors.len(), 26);
    /// ```
    pub fn spatial_all_neighbors(&self) -> impl Iterator<Item = SingleId> + '_ {
        (-1..=1).flat_map(move |df| {
            (-1..=1).flat_map(move |dy| {
                (-1..=1).filter_map(move |dx| {
                    if df == 0 && dx == 0 && dy == 0 {
                        return None;
                    }

                    let mut neighbor = *self;

                    if df != 0 && neighbor.move_f(df).is_err() {
                        return None;
                    }

                    if dx != 0 {
                        neighbor.move_x(dx);
                    }

                    if dy != 0 && neighbor.move_y(dy).is_err() {
                        return None;
                    }

                    Some(neighbor)
                })
            })
        })
    }
}
