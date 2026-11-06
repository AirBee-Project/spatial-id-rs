pub mod constructor;
pub mod convert;
pub mod impls;
pub mod random;
#[cfg(test)]
mod test;

use crate::SpatialId;
use crate::{
    Interval, SpatialIdError,
    error::Error,
    id::{helpers, zoom_level::ZoomLevel},
    time::span,
};
use alloc::vec::Vec;

/// 時空間IDの範囲を表す型。
///
/// 各インデックスを範囲で指定できる。各次元の範囲を表す配列の順序には意味を持たない。内部的には下記のような構造体で構成されており、各フィールドを非公開にすることで、ズームレベルに依存するインデックス範囲やその他のバリデーションを適切に適用できる。
///
/// この型は [`PartialOrd`] / [`Ord`] を実装しているが、これは主に [`BTreeSet`](alloc::collections::BTreeSet) や [`BTreeMap`](alloc::collections::BTreeMap) などの順序付きコレクションでの格納・探索用である。実際の空間的な「大小」を意味するものではない。
///
/// ```
/// # use spatial_id::SpatialId;
/// # use spatial_id::ZoomLevel;
/// pub struct RangeId {
///     z: ZoomLevel,
///     f: [i32; 2],
///     x: [u32; 2],
///     y: [u32; 2],
/// }
/// ```
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
pub struct RangeId {
    z: ZoomLevel,
    f: [i32; 2],
    x: [u32; 2],
    y: [u32; 2],
    i: Interval,
    #[cfg(feature = "temporal_id")]
    t: [u64; 2],
}

impl RangeId {
    /// 全時空間を覆う [`RangeId`]。
    pub const WHOLE: RangeId = RangeId {
        z: ZoomLevel::MIN,
        f: [-1, 0],
        x: [0, 0],
        y: [0, 0],
        i: Interval::WHOLE,
        #[cfg(feature = "temporal_id")]
        t: [0, 0],
    };

    /// この [`RangeId`] が保持しているズームレベル `z` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [-3,29], [8,9], [5,10]).unwrap();
    /// assert_eq!(id.z(), 5u8);
    /// ```
    pub fn z(&self) -> u8 {
        self.z.get()
    }

    /// この [`RangeId`] が保持している F 範囲 `[f1,f2]` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [-3,29], [8,9], [5,10]).unwrap();
    /// assert_eq!(id.f(), [-3i32,29i32]);
    /// ```
    pub fn f(&self) -> [i32; 2] {
        self.f
    }

    /// この [`RangeId`] が保持している X 範囲 `[x1,x2]` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [-3,29], [8,9], [5,10]).unwrap();
    /// assert_eq!(id.x(), [8u32,9u32]);
    /// ```
    pub fn x(&self) -> [u32; 2] {
        self.x
    }

    /// この [`RangeId`] が保持している Y 範囲 `[y1,y2]` を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [-3,29], [8,9], [5,10]).unwrap();
    /// assert_eq!(id.y(), [5u32,10u32]);
    /// ```
    pub fn y(&self) -> [u32; 2] {
        self.y
    }

    /// 時間間隔 `{i}` を返す。全時間のときは [`Interval::WHOLE`](crate::time::Interval::WHOLE)。
    pub fn i(&self) -> crate::Interval {
        self.i
    }

    /// この [`RangeId`] の時間インデックス範囲 `[min, max]`（両端含む）を返す。
    ///
    /// `temporal_id` feature 無効時は常に `[0, 0]`（全時間の唯一のSegment）。
    pub fn t(&self) -> [u64; 2] {
        #[cfg(feature = "temporal_id")]
        {
            self.t
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            [0, 0]
        }
    }

    /// 時間を設定した自身を返す（ビルダー形式）。
    ///
    /// [`SingleId::time_with`](crate::SingleId::time_with)が単一Segmentしか受け取らないのに対し、
    /// こちらは空間と同じく**範囲**を受け取る。`t` は `[min, max]` の `[u64; 2]`、または
    /// 両端が等しい単一の `u64` のどちらでも渡せる（f/x/y 引数と同じ考え方）。
    ///
    /// FlexTreeが必要とする2の冪秒のSegmentへの分解は、木へ挿入する段階
    /// （[`IntoIterator`]による[`FlexId`](crate::FlexId)への展開）で自動的に行われる。
    ///
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
    /// # use spatial_id::{RangeId, time::Interval};
    /// let id = RangeId::new(4, [-3, 6], [8, 9], [5, 10]).unwrap()
    ///     .time_with(Interval::HOUR, [5, 8]).unwrap();
    /// assert_eq!(id.to_string(), "4/-3:6/8:9/5:10_3600/5:8");
    ///
    /// // 単一の時刻も渡せる。
    /// let single = RangeId::new(4, [-3, 6], [8, 9], [5, 10]).unwrap()
    ///     .time_with(Interval::HOUR, 5).unwrap();
    /// assert_eq!(single.to_string(), "4/-3:6/8:9/5:10_3600/5");
    /// # }
    /// ```
    pub fn time_with<I, T>(self, interval: I, t: T) -> Result<Self, Error>
    where
        I: TryInto<Interval>,
        Error: From<I::Error>,
        T: helpers::IntoRange<u64>,
    {
        let interval: Interval = interval.try_into()?;
        let mut t = t.into_range();
        if t[0] > t[1] {
            t.swap(0, 1);
        }

        interval.validated_span(t[0], t[1])?;
        Ok(self.time_with_unchecked(interval, t))
    }

    /// 同じ絶対秒区間を、別の単位で表し直した自身を返す。
    ///
    /// 単位が変わっても指す区間は変わらない。その単位で割り切れない場合は
    /// [`SpatialIdError::TIntervalError`] を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{RangeId, time::Interval};
    /// let id = RangeId::new(4, 0, 0, 0).unwrap().time_with(Interval::HOUR, [0, 1]).unwrap();
    /// assert_eq!(id.time_relabel(1800).unwrap().t(), [0, 3]);
    /// // 2時間ぶんは1日単位では表せない。
    /// assert!(id.time_relabel(Interval::DAY).is_err());
    /// # }
    /// ```
    pub fn time_relabel<I>(self, interval: I) -> Result<Self, Error>
    where
        I: TryInto<Interval>,
        Error: From<I::Error>,
    {
        let interval: Interval = interval.try_into()?;
        let (start, end) = self.seconds_range();
        let unit = interval.seconds();
        if !start.is_multiple_of(unit) || !end.is_multiple_of(unit) {
            return Err(SpatialIdError::TIntervalError { i: unit }.into());
        }
        Ok(self.time_with_unchecked(interval, [start / unit, end / unit - 1]))
    }

    /// 時間の指定を外し、全時間へ戻した自身を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{RangeId, SpatialId, time::Interval};
    /// let id = RangeId::new(4, 0, 0, 0).unwrap().time_with(Interval::HOUR, [0, 1]).unwrap();
    /// assert!(id.time_cleared().time_is_whole());
    /// # }
    /// ```
    pub fn time_cleared(self) -> Self {
        self.time_with_unchecked(Interval::WHOLE, [0, 0])
    }

    /// 検証済みの `{i}` / `{t}` を書き込む。
    fn time_with_unchecked(
        #[cfg_attr(not(feature = "temporal_id"), allow(unused_mut))] mut self,
        interval: Interval,
        t: [u64; 2],
    ) -> Self {
        self.i = interval;
        // `temporal_id` 無効時に検証を通るのは全時間だけで、そのとき `t` は `[0, 0]` に
        // しかならないので情報は落ちない。
        #[cfg(feature = "temporal_id")]
        {
            self.t = t;
        }
        #[cfg(not(feature = "temporal_id"))]
        debug_assert_eq!(t, [0, 0], "temporal_id 無効時に t != [0,0] が検証を通った");
        self
    }

    /// この [`RangeId`] の時間を、FlexTree が使う2分岐Segmentの列へ分解する。クレート内部専用。
    pub(crate) fn time_segments(&self) -> span::TimeSegments {
        span::TimeSpan::of(self).into_segments()
    }

    pub fn set_f(&mut self, value: [i32; 2]) -> Result<(), Error> {
        let z = self.z.get();
        let mut value = value;
        let f_min = self.z.f_min();
        let f_max = self.z.f_max();

        for &f_value in &value {
            if f_value < f_min || f_value > f_max {
                return Err(SpatialIdError::FOutOfRange { f: f_value, z }.into());
            }
        }

        if value[0] > value[1] {
            value.swap(0, 1);
        }

        self.f = value;
        Ok(())
    }

    pub fn set_x(&mut self, value: [u32; 2]) -> Result<(), Error> {
        let z = self.z.get();
        let xy_max = self.z.xy_max();

        for &x_value in &value {
            if x_value > xy_max {
                return Err(SpatialIdError::XOutOfRange { x: x_value, z }.into());
            }
        }

        self.x = value;
        Ok(())
    }

    pub fn set_y(&mut self, value: [u32; 2]) -> Result<(), Error> {
        let z = self.z.get();
        let mut value = value;
        let xy_max = self.z.xy_max();

        for &y_value in &value {
            if y_value > xy_max {
                return Err(SpatialIdError::YOutOfRange { y: y_value, z }.into());
            }
        }

        if value[0] > value[1] {
            value.swap(0, 1);
        }

        self.y = value;
        Ok(())
    }

    /// 各軸の区間の両端を、ズーム `z` のインデックス単位で `f`・`x`・`y`（`[min, max]`）だけずらした範囲を `out` へ追加する。
    pub(crate) fn shift_range(
        &self,
        z: u8,
        f: [i64; 2],
        x: [i64; 2],
        y: [i64; 2],
        out: &mut Vec<RangeId>,
    ) {
        let target_z = z.max(self.z.get());
        let Ok(mut b) = self.spatial_children_at_zoom(target_z) else {
            return;
        };
        let scale = 1i64 << (target_z - z);

        let f_min = (b.f[0] as i64 + f[0] * scale).max(b.z.f_min() as i64);
        let f_max = (b.f[1] as i64 + f[1] * scale).min(b.z.f_max() as i64);
        if f_min > f_max {
            return;
        }
        b.f = [f_min as i32, f_max as i32];

        let xy_max = b.z.xy_max();
        let y_min = (b.y[0] as i64 + y[0] * scale).max(0);
        let y_max = (b.y[1] as i64 + y[1] * scale).min(xy_max as i64);
        if y_min > y_max {
            return;
        }
        b.y = [y_min as u32, y_max as u32];

        let len = 1i64 << target_z;
        let cur_span = (b.x[1] as i64 - b.x[0] as i64).rem_euclid(len);
        let new_span = cur_span + (x[1] - x[0]) * scale;
        if new_span < 0 {
            return;
        }
        if new_span >= len - 1 {
            b.x = [0, xy_max];
            out.push(b);
            return;
        }

        let x_min = (b.x[0] as i64 + x[0] * scale).rem_euclid(len) as u32;
        let x_max = (b.x[1] as i64 + x[1] * scale).rem_euclid(len) as u32;
        // 折り返し区間を昇順の2区間に分割しておくことで、後続の逆算や木探索の単調性を保つ
        if x_min <= x_max {
            b.x = [x_min, x_max];
            out.push(b);
        } else {
            let mut r1 = b;
            r1.x = [x_min, xy_max];
            out.push(r1);

            b.x = [0, x_max];
            out.push(b);
        }
    }

    /// 指定したより深いズームレベル `target_z` にスケール変換した、この [`RangeId`] を生成する。
    ///
    /// # パラメータ
    /// * `target_z` — 変換先のズームレベル
    ///
    /// # バリデーション
    /// - `target_z` が現在のズームレベルより浅い場合は、[`SpatialIdError::ZoomLevelTransitionOutOfRange`] を返す。
    /// - `target_z` が本クレートで扱える最大ズームレベルを超える場合は、[`SpatialIdError::ZOutOfRange`] を返す。
    ///
    /// 1段深いズームへのスケール変換
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [-3,29], [8,9], [5,10]).unwrap();
    /// let result = id.spatial_children_at_zoom(6).unwrap();
    /// assert_eq!(result, RangeId::new(6, [-6, 59], [16, 19], [10, 21]).unwrap());
    /// ```
    ///
    /// 現在より浅いズームを指定した場合
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::RangeId;
    /// let id = RangeId::new(5, [-3,29], [8,9], [5,10]).unwrap();
    /// let result = id.spatial_children_at_zoom(4);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::ZoomLevelTransitionOutOfRange { current_z: 5, target_z: 4 }))));
    /// ```
    pub fn spatial_children_at_zoom(&self, target_z: u8) -> Result<RangeId, Error> {
        let z = self.z.get();
        if target_z < z {
            return Err(SpatialIdError::ZoomLevelTransitionOutOfRange {
                current_z: z,
                target_z,
            }
            .into());
        }

        if ZoomLevel::new(target_z).is_err() {
            return Err(SpatialIdError::ZOutOfRange { z: target_z }.into());
        }

        let difference = target_z - z;
        let scale_f = 1_i32 << difference as u32;
        let scale_xy = 1_u32 << difference as u32;

        let f = helpers::scale_range_i32(self.f[0], self.f[1], scale_f);
        let x = helpers::scale_range_u32(self.x[0], self.x[1], scale_xy);
        let y = helpers::scale_range_u32(self.y[0], self.y[1], scale_xy);

        Ok(RangeId {
            z: ZoomLevel::new(target_z).unwrap(),
            f,
            x,
            y,

            i: self.i,
            #[cfg(feature = "temporal_id")]
            t: self.t,
        })
    }

    /// 指定したズームレベル `target_z` に縮約した、この [`RangeId`] の親 [`RangeId`] を返す。
    ///
    /// # パラメータ
    /// * `target_z` — 取得したい親 [`RangeId`] のズームレベル
    ///
    /// # バリデーション
    /// - `target_z` が現在のズームレベルより深い場合は、[`SpatialIdError::ZoomLevelTransitionOutOfRange`] を返す。
    /// - `target_z` が本クレートで扱える最大ズームレベルを超える場合は、[`SpatialIdError::ZOutOfRange`] を返す。
    ///
    /// 1段浅いズームへの縮約
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [1,29], [8,9], [5,10]).unwrap();
    /// let parent = id.spatial_parent_at_zoom(4).unwrap();
    ///
    /// assert_eq!(parent.z(), 4);
    /// assert_eq!(parent.f(), [0,14]);
    /// assert_eq!(parent.x(), [4,4]);
    /// assert_eq!(parent.y(), [2,5]);
    /// ```
    ///
    /// Fが負の場合の挙動:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::RangeId;
    /// # use spatial_id::Error;
    /// let id = RangeId::new(5, [-10,-5], [8,9], [5,10]).unwrap();
    ///
    /// let parent = id.spatial_parent_at_zoom(4).unwrap();
    ///
    /// assert_eq!(parent.z(), 4);
    /// assert_eq!(parent.f(), [-5,-3]);
    /// assert_eq!(parent.x(), [4,4]);
    /// assert_eq!(parent.y(), [2,5]);
    /// ```
    ///
    /// 現在より深いズームを指定した場合:
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::Error;
    /// # use spatial_id::SpatialIdError;
    /// # use spatial_id::RangeId;
    /// let id = RangeId::new(5, [-10,-5], [8,9], [5,10]).unwrap();
    /// let result = id.spatial_parent_at_zoom(6);
    /// assert!(matches!(result, Err(Error::SpatialId(SpatialIdError::ZoomLevelTransitionOutOfRange { current_z: 5, target_z: 6 }))));
    /// ```
    pub fn spatial_parent_at_zoom(&self, target_z: u8) -> Result<RangeId, Error> {
        let z = self.z.get();
        if target_z > z {
            return Err(SpatialIdError::ZoomLevelTransitionOutOfRange {
                current_z: z,
                target_z,
            }
            .into());
        }

        let Ok(target_zoom) = ZoomLevel::new(target_z) else {
            return Err(SpatialIdError::ZOutOfRange { z: target_z }.into());
        };

        let shift = (z - target_z) as u32;

        let f = [self.f[0] >> shift, self.f[1] >> shift];
        let x0 = self.x[0] >> shift;
        let x1 = self.x[1] >> shift;
        let x = if self.x[0] > self.x[1] && x0 <= x1 {
            // 折り返し区間を浅いズームへ丸めた結果、両端が重なって隙間が消えた場合は全周を覆う
            [0, target_zoom.xy_max()]
        } else {
            [x0, x1]
        };
        let y = [self.y[0] >> shift, self.y[1] >> shift];

        Ok(RangeId {
            z: target_zoom,
            f,
            x,
            y,

            i: self.i,
            #[cfg(feature = "temporal_id")]
            t: self.t,
        })
    }
}
