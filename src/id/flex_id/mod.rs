pub mod constructor;
pub mod convert;
#[cfg(feature = "unstable")]
pub mod encode;
pub mod impls;
pub mod ops;

use crate::{
    Error, Side, SpatialIdError,
    id::{
        dimension::Dimension,
        zoom_level::{TZoomLevel, ZoomLevel},
    },
    time::span,
};

/// FlexId の PartialOrd（位置はルート、続いてズーム 1, 2, … の各深さで F→X→Y→T の順）における 2 つの [`FlexId`] の関係。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZOrderRel {
    /// 最初に食い違う位置で両方が割られており、左辺が下側（手前）、右辺が上側（奥）にある。互いに重ならない。
    Less,
    /// 2 つの [`FlexId`] が完全に一致している。
    Equal,
    /// 最初に食い違う位置で両方が割られており、左辺が上側（奥）、右辺が下側（手前）にある。互いに重ならない。
    Greater,
    /// 最初に食い違う位置で片方だけが割られている（包含関係にあるか、異方性により FlexId の PartialOrd で比較できない）。
    Overlap {
        /// 最初に食い違う位置の直前まで一致している共通の祖先領域。両方の [`FlexId`] を含む。
        enclosing: FlexId,
        /// `enclosing` の直後の位置で、片方だけが割っている次元。
        split_dim: Dimension,
        /// `split_dim` でまだ割られていない（粗い）側が左辺（`self`）なら `true`、右辺（`other`）なら `false`。
        a_is_unsplit: bool,
    },
}

/// 拡張時空間IDを表現する型。
///
/// 各次元がズームレベルとインデックス値を持つ。
#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash)]
pub struct FlexId {
    f_zoomlevel: ZoomLevel,
    f_index: i32,
    x_zoomlevel: ZoomLevel,
    x_index: u32,
    y_zoomlevel: ZoomLevel,
    y_index: u32,
    #[cfg(feature = "temporal_id")]
    t_zoomlevel: TZoomLevel,
    #[cfg(feature = "temporal_id")]
    t_index: u64,
}

impl FlexId {
    pub(crate) const UPPER_MAX: FlexId = FlexId {
        f_zoomlevel: ZoomLevel::MIN,
        f_index: 0,
        x_zoomlevel: ZoomLevel::MIN,
        x_index: 0,
        y_zoomlevel: ZoomLevel::MIN,
        y_index: 0,
        #[cfg(feature = "temporal_id")]
        t_zoomlevel: TZoomLevel::MIN,
        #[cfg(feature = "temporal_id")]
        t_index: 0,
    };

    pub(crate) const LOWER_MAX: FlexId = FlexId {
        f_zoomlevel: ZoomLevel::MIN,
        f_index: -1,
        x_zoomlevel: ZoomLevel::MIN,
        x_index: 0,
        y_zoomlevel: ZoomLevel::MIN,
        y_index: 0,
        #[cfg(feature = "temporal_id")]
        t_zoomlevel: TZoomLevel::MIN,
        #[cfg(feature = "temporal_id")]
        t_index: 0,
    };

    /// 全空間を重ならずに覆う最大の [`FlexId`]。`[UPPER_MAX, LOWER_MAX]` の順。
    pub(crate) const ROOTS: [FlexId; 2] = [Self::UPPER_MAX, Self::LOWER_MAX];

    /// [`ROOTS`](Self::ROOTS) のうち、自身を含むものの添字。
    pub(crate) fn root_index(&self) -> usize {
        usize::from(self.f_index.is_negative())
    }

    pub fn f_zoomlevel(&self) -> u8 {
        self.f_zoomlevel.get()
    }
    pub fn x_zoomlevel(&self) -> u8 {
        self.x_zoomlevel.get()
    }
    pub fn y_zoomlevel(&self) -> u8 {
        self.y_zoomlevel.get()
    }
    /// 時間軸のズームレベル。1Segmentは `2^(35 - t_zoomlevel)` 秒。
    ///
    /// `temporal_id` feature 無効時は常に `0`（全時間）。
    pub fn t_zoomlevel(&self) -> u8 {
        #[cfg(feature = "temporal_id")]
        {
            self.t_zoomlevel.get()
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            0
        }
    }

    /// フィールドを宣言順に並べたキー。
    ///
    /// FlexId の PartialOrd は全順序ではないので、同じ FlexId を隣に集めるためだけに並べるときに使う。順序に空間的な意味はない。
    pub(crate) fn field_key(&self) -> (u8, i32, u8, u32, u8, u32, u8, u64) {
        (
            self.f_zoomlevel(),
            self.f_index(),
            self.x_zoomlevel(),
            self.x_index(),
            self.y_zoomlevel(),
            self.y_index(),
            self.t_zoomlevel(),
            self.t_index(),
        )
    }

    pub fn f_index(&self) -> i32 {
        self.f_index
    }
    pub fn x_index(&self) -> u32 {
        self.x_index
    }
    pub fn y_index(&self) -> u32 {
        self.y_index
    }
    /// この [`FlexId`] の時間インデックス `{t}`。[`time_interval`](crate::SpatialId::time_interval) を単位とする。
    ///
    /// [`t_zoomlevel`](Self::t_zoomlevel) と対で読めば、木の2分岐Segment `(zoom, index)` そのもの。
    /// `temporal_id` feature 無効時は常に `0`（全時間）。
    pub fn t_index(&self) -> u64 {
        #[cfg(feature = "temporal_id")]
        {
            self.t_index
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            0
        }
    }

    /// この [`FlexId`] が占める絶対秒区間 `[start, end)` を返す。
    /// 時間Segment（4軸目）を設定した自身を返す（ビルダー形式）。
    ///
    /// 引数は空間3軸と同じ「**ズームレベル＋インデックス**」で、[`new`](Self::new) の
    /// `f_zoomlevel` / `f_index` と同じく [`Into<u8>`](Into) を受ける
    /// （[`TZoomLevel`] をそのまま渡すこともできる）。1Segmentは `2^(35 - t_zoomlevel)` 秒で、
    /// [`t_zoomlevel`](Self::t_zoomlevel) / [`t_index`](Self::t_index) で読むのと同じ形である。
    ///
    /// # [`SingleId`](crate::SingleId) / [`RangeId`](crate::RangeId) の [`with_time`](crate::SingleId::with_time) との違い
    ///
    /// あちらは仕様の `{i}/{t}`、つまり**秒数**＋インデックスを取る。[`FlexId`] は木の
    /// ノードアドレスであり2分岐Segment1個しか持てないため、**ズーム**で受ける。
    ///
    /// ```text
    /// # use spatial_id::SpatialId;
    /// single.with_time(1800, 809712)  // 1800 「秒」単位の 809712 番目
    /// flex  .with_time_segment(25, 7) // 「ズーム」25（=1024秒幅）の 7 番目
    /// ```
    ///
    /// 秒数のつもりで渡した値は、`1800` や `3600` なら `u8` に収まらずコンパイルエラー、
    /// `60` や `86400` ならズーム範囲外（最大35）で [`SpatialIdError::ZOutOfRange`] になる。
    /// ただし `0..=35` の値は**どちらとも解釈できてしまう**ので、秒で考えたいときは
    /// [`with_time_span`](Self::with_time_span) / [`with_time_segment_at`](Self::with_time_segment_at)
    /// を使うこと。
    ///
    /// 任意秒数の間隔（`1800` など）を [`FlexId`] の集合として扱いたい場合は
    /// [`SingleId`](crate::SingleId) / [`RangeId`](crate::RangeId) に付けてから
    /// [`IntoIterator`] で展開する（必要な数のSegmentへ自動的に分解される）。
    ///
    /// # バリデーション
    /// - `t_zoomlevel` が `35` を超える場合は [`SpatialIdError::ZOutOfRange`] を返す。
    /// - `t_index` が `2^t_zoomlevel - 1` を超える場合は [`SpatialIdError::TOutOfRange`] を返す。
    /// - `temporal_id` feature 無効時は全時間以外を [`SpatialIdError::TIntervalError`] で拒否する。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{FlexId, time::TZoomLevel};
    /// // ズーム25のSegmentは 2^(35-25) = 1024 秒幅。
    /// let id = FlexId::new(5, 3, 2, 3, 10, 1).unwrap().with_time_segment(25, 7).unwrap();
    /// assert_eq!((id.t_zoomlevel(), id.t_index()), (25, 7));
    /// assert_eq!(id.time_interval().seconds(), 1024);
    ///
    /// // `TZoomLevel` をそのまま渡してもよい。
    /// let same = FlexId::new(5, 3, 2, 3, 10, 1)
    ///     .unwrap()
    ///     .with_time_segment(TZoomLevel::new(25).unwrap(), 7)
    ///     .unwrap();
    /// assert_eq!(same, id);
    ///
    /// // 秒数のつもりの値はズーム範囲外で弾かれる。
    /// assert!(FlexId::new(5, 3, 2, 3, 10, 1).unwrap().with_time_segment(60, 0).is_err());
    /// # }
    /// ```
    pub fn with_time_segment(
        self,
        t_zoomlevel: impl Into<u8>,
        t_index: u64,
    ) -> Result<Self, Error> {
        let tz = TZoomLevel::new(t_zoomlevel.into())?;
        tz.check_index(t_index)?;

        // 無効時の木はT軸を分割しないため、受け付けると挿入した時間区間がエラーも警告もなく
        // 全時間へ広がる（例: 1/4区間を入れると4倍になって戻る）。
        #[cfg(not(feature = "temporal_id"))]
        if tz.get() != 0 || t_index != 0 {
            return Err(SpatialIdError::TIntervalError {
                i: tz.segment_seconds(),
            }
            .into());
        }

        Ok(unsafe { self.with_time_segment_unchecked(tz.get(), t_index) })
    }

    /// Unix 時刻（秒）が属する時間Segmentを設定した自身を返す。
    ///
    /// 粒度は [`with_time_segment`](Self::with_time_segment) と同じく**ズームレベル**で指定する
    /// （1Segmentは `2^(35 - t_zoomlevel)` 秒）。仕様 1.5.3 (3) の `t = floor(u / i)` に相当する
    /// 割り算はこちらで行うので、呼び出し側でインデックスを求める必要がない。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{FlexId, time::TZoomLevel};
    /// // 最深ズーム（35）は1秒幅。
    /// let id = FlexId::new(5, 3, 2, 3, 10, 1)
    ///     .unwrap()
    ///     .with_time_segment_at(TZoomLevel::MAX, 1_770_000_000)
    ///     .unwrap();
    /// assert_eq!(id.seconds_range(), (1_770_000_000, 1_770_000_001));
    /// # }
    /// ```
    pub fn with_time_segment_at(
        self,
        t_zoomlevel: impl Into<u8>,
        unix_seconds: u64,
    ) -> Result<Self, Error> {
        let tz = TZoomLevel::new(t_zoomlevel.into())?;
        self.with_time_segment(tz, unix_seconds / tz.segment_seconds())
    }

    /// 絶対秒区間 `[start, end)` を設定した自身を返す。
    ///
    /// 3型で共通の「実時間で指定する」入口。[`FlexId`] は2分岐Segment1個しか持てないので、
    /// **区間が2の冪秒でその境界に整列している場合**にのみ成功し、そうでなければ
    /// [`SpatialIdError::TIntervalError`] を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::FlexId;
    /// // [1024, 2048) はズーム25のSegment1個ちょうど。
    /// let id = FlexId::new(5, 3, 2, 3, 10, 1).unwrap().with_time_span(1024, 2048).unwrap();
    /// assert_eq!((id.t_zoomlevel(), id.t_index()), (25, 1));
    ///
    /// // 整列していない区間は1Segmentにならない。
    /// assert!(FlexId::new(5, 3, 2, 3, 10, 1).unwrap().with_time_span(1, 1025).is_err());
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
        let time_segment = interval.as_segment(t_min)?;
        let (zoom, index) = (time_segment.zoom().get(), time_segment.index());
        self.with_time_segment(zoom, index)
    }

    /// 時間の指定を外し、全時間へ戻した自身を返す。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{FlexId, SpatialId, time::TZoomLevel};
    /// let id = FlexId::new(5, 3, 2, 3, 10, 1)
    ///     .unwrap()
    ///     .with_time_segment(TZoomLevel::new(25).unwrap(), 7)
    ///     .unwrap();
    /// assert!(id.without_time().is_whole_time());
    /// # }
    /// ```
    pub fn without_time(self) -> Self {
        unsafe { self.with_time_segment_unchecked(0, 0) }
    }

    /// 検証を行わずに時間Segment（4軸目）を設定した自身を返す。
    ///
    /// # Safety
    ///
    /// - `zoom` は [`TZoomLevel::MAX`]（35）以下でなければならない。
    /// - `index` は `2^zoom - 1` 以下でなければならない。
    /// - `temporal_id` feature が無効な場合、`zoom == 0 && index == 0` でなければならない。
    pub(crate) unsafe fn with_time_segment_unchecked(
        #[cfg_attr(not(feature = "temporal_id"), allow(unused_mut))] mut self,
        zoom: u8,
        index: u64,
    ) -> Self {
        debug_assert!(TZoomLevel::new(zoom).is_ok_and(|z| z.check_index(index).is_ok()));

        #[cfg(feature = "temporal_id")]
        {
            self.t_zoomlevel = unsafe { TZoomLevel::new_unchecked(zoom) };
            self.t_index = index;
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            debug_assert!(
                zoom == 0 && index == 0,
                "temporal_id 無効時に時間Segment ({zoom}, {index}) を設定しようとした"
            );
            let _ = (zoom, index);
        }

        self
    }

    /// 指定したズームレベルでの空間的な親ID（包含する最小のFlexId）を返す。
    /// 指定されたズームが現在のズーム以上の場合は、各軸についてそのままのズームとインデックスを返す。
    pub fn spatial_parent_at_zoom(&self, target_z: u8) -> Result<FlexId, Error> {
        let f_zoomlevel = self.f_zoomlevel().min(target_z);
        let x_zoomlevel = self.x_zoomlevel().min(target_z);
        let y_zoomlevel = self.y_zoomlevel().min(target_z);

        let shift_f = self.f_zoomlevel() - f_zoomlevel;
        let shift_x = self.x_zoomlevel() - x_zoomlevel;
        let shift_y = self.y_zoomlevel() - y_zoomlevel;

        let f_index = self.f_index() >> shift_f;
        let x_index = self.x_index() >> shift_x;
        let y_index = self.y_index() >> shift_y;

        FlexId::new(
            f_zoomlevel,
            f_index,
            x_zoomlevel,
            x_index,
            y_zoomlevel,
            y_index,
        )
        .map(|id| unsafe { id.with_time_segment_unchecked(self.t_zoomlevel(), self.t_index()) })
    }

    /// F方向で二つに切り分ける。[`ZoomLevel::MAX`] なら [`None`]。
    ///
    /// 分割後も各軸が有効範囲に収まることは構築時に保証される（F は
    /// `[-2^z, 2^z-1]` を2倍して `side` を足すと過不足なく `z+1` の範囲になり、
    /// X/Y は値を変えずに引き継ぐ）ため、[`FlexId::new`] の再検証を通さない。
    pub(crate) fn split_f(&self, side: Side) -> Option<FlexId> {
        let f_zoomlevel = self.f_zoomlevel.deeper()?;
        let f_index = self.f_index * 2 + side as i32;
        debug_assert!(f_zoomlevel.check_f(f_index).is_ok(), "分割後のFが範囲外");

        Some(FlexId {
            f_zoomlevel,
            f_index,
            x_zoomlevel: self.x_zoomlevel,
            x_index: self.x_index,
            y_zoomlevel: self.y_zoomlevel,
            y_index: self.y_index,
            #[cfg(feature = "temporal_id")]
            t_zoomlevel: self.t_zoomlevel,
            #[cfg(feature = "temporal_id")]
            t_index: self.t_index,
        })
    }

    /// X方向で二つに切り分ける。[`ZoomLevel::MAX`] なら [`None`]。
    ///
    /// 再検証を省ける理由は [`split_f`](Self::split_f) と同じ。
    pub(crate) fn split_x(&self, side: Side) -> Option<FlexId> {
        let x_zoomlevel = self.x_zoomlevel.deeper()?;
        let x_index = self.x_index * 2 + side as u32;
        debug_assert!(x_zoomlevel.check_x(x_index).is_ok(), "分割後のXが範囲外");

        Some(FlexId {
            f_zoomlevel: self.f_zoomlevel,
            f_index: self.f_index,
            x_zoomlevel,
            x_index,
            y_zoomlevel: self.y_zoomlevel,
            y_index: self.y_index,
            #[cfg(feature = "temporal_id")]
            t_zoomlevel: self.t_zoomlevel,
            #[cfg(feature = "temporal_id")]
            t_index: self.t_index,
        })
    }

    /// Y方向で二つに切り分ける。[`ZoomLevel::MAX`] なら [`None`]。
    ///
    /// 再検証を省ける理由は [`split_f`](Self::split_f) と同じ。
    pub(crate) fn split_y(&self, side: Side) -> Option<FlexId> {
        let y_zoomlevel = self.y_zoomlevel.deeper()?;
        let y_index = self.y_index * 2 + side as u32;
        debug_assert!(y_zoomlevel.check_y(y_index).is_ok(), "分割後のYが範囲外");

        Some(FlexId {
            f_zoomlevel: self.f_zoomlevel,
            f_index: self.f_index,
            x_zoomlevel: self.x_zoomlevel,
            x_index: self.x_index,
            y_zoomlevel,
            y_index,
            #[cfg(feature = "temporal_id")]
            t_zoomlevel: self.t_zoomlevel,
            #[cfg(feature = "temporal_id")]
            t_index: self.t_index,
        })
    }

    /// 時間軸で二つに切り分ける。[`TZoomLevel::MAX`] なら [`None`]。
    ///
    /// 再検証を省ける理由は [`split_f`](Self::split_f) と同じ
    /// （インデックスを2倍して `side` を足すと過不足なく1段深い範囲になる）。
    #[cfg(feature = "temporal_id")]
    pub(crate) fn split_t(&self, side: Side) -> Option<FlexId> {
        let t_zoomlevel = self.t_zoomlevel.deeper()?;
        let t_index = self.t_index * 2 + side as u64;
        debug_assert!(
            t_zoomlevel.check_index(t_index).is_ok(),
            "分割後のTが範囲外"
        );

        Some(FlexId {
            f_zoomlevel: self.f_zoomlevel,
            f_index: self.f_index,
            x_zoomlevel: self.x_zoomlevel,
            x_index: self.x_index,
            y_zoomlevel: self.y_zoomlevel,
            y_index: self.y_index,
            t_zoomlevel,
            t_index,
        })
    }

    /// 時間軸で二つに切り分ける。
    ///
    /// `temporal_id` feature 無効時は時間軸が存在せず、全時間は分割できないので常に
    /// [`None`]。木も3軸なのでこの経路には来ない。
    #[cfg(not(feature = "temporal_id"))]
    pub(crate) fn split_t(&self, _side: Side) -> Option<FlexId> {
        None
    }

    /// `dimension` 方向のズームレベルを返す。軸を値で選ぶ必要があるFlexTreeの実装用。
    pub(crate) fn zoomlevel_on(&self, dimension: Dimension) -> u8 {
        match dimension {
            Dimension::F => self.f_zoomlevel(),
            Dimension::X => self.x_zoomlevel(),
            Dimension::Y => self.y_zoomlevel(),
            Dimension::T => self.t_zoomlevel(),
        }
    }

    /// `dimension` 方向のインデックスを返す。次元ごとに幅が違うので `i64` に揃える。
    pub(crate) fn index_on(&self, dimension: Dimension) -> i64 {
        match dimension {
            Dimension::F => i64::from(self.f_index()),
            Dimension::X => i64::from(self.x_index()),
            Dimension::Y => i64::from(self.y_index()),
            Dimension::T => self.t_index() as i64,
        }
    }

    /// 次元ごとのズームレベル `zoom`（[`Dimension::ALL`] の順）での自身の祖先を返す。
    /// `zoom` の各値は自身のその次元のズーム以下であること。
    pub(crate) fn ancestor_at(&self, zoom: [u8; 4]) -> FlexId {
        let [f, x, y, t] = zoom;
        debug_assert!(
            Dimension::ALL
                .iter()
                .all(|&d| zoom[d as usize] <= self.zoomlevel_on(d))
        );
        #[cfg(not(feature = "temporal_id"))]
        let _ = t;
        // SAFETY: 祖先のズームは自身のズーム以下なので、どれも有効なズームレベル
        unsafe {
            FlexId {
                f_zoomlevel: ZoomLevel::new_unchecked(f),
                f_index: self.f_index >> (self.f_zoomlevel() - f),
                x_zoomlevel: ZoomLevel::new_unchecked(x),
                x_index: self.x_index >> (self.x_zoomlevel() - x),
                y_zoomlevel: ZoomLevel::new_unchecked(y),
                y_index: self.y_index >> (self.y_zoomlevel() - y),
                #[cfg(feature = "temporal_id")]
                t_zoomlevel: TZoomLevel::new_unchecked(t),
                #[cfg(feature = "temporal_id")]
                t_index: self.t_index >> (self.t_zoomlevel() - t),
            }
        }
    }

    /// `dimension` 方向で二つに切り分けた `side` 側を返す。その軸が最大ズームなら [`None`]。軸を値で選ぶ必要があるFlexTreeの実装用。
    pub(crate) fn split_on(&self, dimension: Dimension, side: Side) -> Option<FlexId> {
        match dimension {
            Dimension::F => self.split_f(side),
            Dimension::X => self.split_x(side),
            Dimension::Y => self.split_y(side),
            Dimension::T => self.split_t(side),
        }
    }

    /// `dimension` 方向で二つに切り分けた側のうち、`target` を含む側を返す。その次元が最大ズームなら [`None`]。
    #[cfg(test)]
    pub(crate) fn split_toward(&self, dimension: Dimension, target: &FlexId) -> Option<FlexId> {
        let upper = self.split_on(dimension, Side::Upper)?;
        if upper.contains(target) {
            Some(upper)
        } else {
            self.split_on(dimension, Side::Lower)
        }
    }

    /// `dimension` 方向で二つに切り分けた側のうち、`target` を含む側。
    /// `target` は自身に含まれ、その次元で自身より細かいこと。
    pub(crate) fn side_toward(&self, dimension: Dimension, target: &FlexId) -> Side {
        let shift = target.zoomlevel_on(dimension) - self.zoomlevel_on(dimension) - 1;
        if (target.index_on(dimension) >> shift) & 1 == 1 {
            Side::Upper
        } else {
            Side::Lower
        }
    }

    /// 自身が `other` より細かい次元の集合（[`Dimension::bit`] の OR）。
    pub(crate) fn finer_dimensions_than(&self, other: &FlexId) -> u8 {
        Dimension::mask(|d| self.zoomlevel_on(d) > other.zoomlevel_on(d))
    }

    /// FlexId の PartialOrd の各位置について、自身がどちら側にあるかを、先に割る位置から順に返す。
    ///
    /// 位置はルート（F の符号）、続いてズーム 1, 2, … の各ズームで F→X→Y→T の順。
    /// 自身がその位置で割られていなければ [`None`]、割られていれば上側なら `true`。
    #[cfg(any(feature = "unstable", test))]
    fn zorder_sides(&self) -> impl Iterator<Item = Option<bool>> + '_ {
        let levels = (1..=TZoomLevel::MAX.get()).flat_map(move |level| {
            Dimension::ALL
                .into_iter()
                .filter(move |&d| d == Dimension::T || level <= ZoomLevel::MAX.get())
                .map(move |d| {
                    let z = self.zoomlevel_on(d);
                    (level <= z).then(|| (self.index_on(d) >> (z - level)) & 1 == 1)
                })
        });
        core::iter::once(Some(self.root_index() == 1)).chain(levels)
    }

    /// FlexId の PartialOrd に沿って並べ替えるためのキーを返す。
    ///
    /// 先に割る位置を上位ビットとし、各位置の側（上側なら 1、割られていなければ 0）を並べた値。
    /// FlexId の Z-order 列（[`QueryIter`](crate::query::QueryIter) が満たす列）は、
    /// このキーの狭義の昇順に並ぶ。
    #[cfg(any(feature = "unstable", test))]
    pub fn zorder_key(&self) -> u128 {
        self.zorder_sides()
            .fold(0, |key, side| key << 1 | u128::from(side == Some(true)))
    }

    /// FlexId の PartialOrd における `self` と `other` の関係を求める。
    ///
    /// 両方が割られた位置で先に食い違えば [`ZOrderRel::Less`] または [`ZOrderRel::Greater`]、
    /// 完全一致なら [`ZOrderRel::Equal`]、片方だけが割られた位置で先に食い違えば
    /// 揃えるために割るべき共通祖先領域と次元を持つ [`ZOrderRel::Overlap`] を返す。
    pub(crate) fn zorder_rel(&self, other: &FlexId) -> ZOrderRel {
        match self.root_index().cmp(&other.root_index()) {
            core::cmp::Ordering::Less => return ZOrderRel::Less,
            core::cmp::Ordering::Greater => return ZOrderRel::Greater,
            core::cmp::Ordering::Equal => {}
        }

        let Some((agree, split_dim)) = self.first_zorder_diff(other) else {
            return ZOrderRel::Equal;
        };

        let (za, zb) = (self.zoomlevel_on(split_dim), other.zoomlevel_on(split_dim));
        if agree < za.min(zb) {
            if (self.index_on(split_dim) >> (za - 1 - agree)) & 1 == 0 {
                ZOrderRel::Less
            } else {
                ZOrderRel::Greater
            }
        } else {
            ZOrderRel::Overlap {
                enclosing: self.common_zorder_ancestor(other, agree, split_dim),
                split_dim,
                a_is_unsplit: za < zb,
            }
        }
    }

    /// 同じルート内で `self` が FlexId の PartialOrd で `other` より前にあるなら、
    /// 分岐する共通の祖先領域・割る次元・その次元でのズーム深さを返す。
    pub(crate) fn zorder_fork(&self, other: &FlexId) -> Option<(FlexId, Dimension, u8)> {
        if self.root_index() != other.root_index() {
            return None;
        }
        let (agree, split_dim) = self.first_zorder_diff(other)?;
        let (za, zb) = (self.zoomlevel_on(split_dim), other.zoomlevel_on(split_dim));
        if agree >= za.min(zb) || (self.index_on(split_dim) >> (za - 1 - agree)) & 1 != 0 {
            return None;
        }
        Some((
            self.common_zorder_ancestor(other, agree, split_dim),
            split_dim,
            agree,
        ))
    }

    /// 同じルート内で、FlexId の PartialOrd において `self` と `other` が最初に食い違う `(一致ズーム深さ, 次元)` を返す。
    /// `self == other` なら [`None`]。
    fn first_zorder_diff(&self, other: &FlexId) -> Option<(u8, Dimension)> {
        let mut best: Option<(u8, Dimension)> = None;
        for d in Dimension::ALL {
            let (za, zb) = (self.zoomlevel_on(d), other.zoomlevel_on(d));
            let deepest = za.min(zb);
            let diff = (self.index_on(d) >> (za - deepest)) ^ (other.index_on(d) >> (zb - deepest));
            if diff == 0 && za == zb {
                continue;
            }
            let agree = deepest - (u64::BITS - (diff as u64).leading_zeros()) as u8;
            if best.is_none_or(|(best_agree, _)| agree < best_agree) {
                best = Some((agree, d));
            }
        }
        best
    }

    /// 最初に食い違う `(agree, split_dim)` の直前まで一致している共通の祖先領域を返す。
    fn common_zorder_ancestor(&self, other: &FlexId, agree: u8, split_dim: Dimension) -> FlexId {
        let zoom = Dimension::ALL.map(|d| {
            self.zoomlevel_on(d)
                .min(other.zoomlevel_on(d))
                .min(agree + u8::from(d < split_dim))
        });
        self.ancestor_at(zoom)
    }

    /// 次元の集合 `dimensions`（[`Dimension::bit`] の OR）のうち、自身のズームが一番粗い次元を返す。
    /// 同じズームなら F→X→Y→T の順。空なら [`None`]。
    pub(crate) fn coarsest_dimension_in(&self, dimensions: u8) -> Option<Dimension> {
        Dimension::ALL
            .into_iter()
            .filter(|&d| dimensions & d.bit() != 0)
            .min_by_key(|&d| (self.zoomlevel_on(d), d as u8))
    }

    /// この [`FlexId`] が `other` と **面を共有** しているかを判定する。X 軸は循環（対蹠経度で東西端が接続）を考慮する。辺・頂点だけで接する場合、領域が重なる場合、離れている場合はいずれも `false` を返す。判定は空間 3 軸（F / X / Y）のみで行い、時間 ID は考慮しない。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::FlexId;
    /// let a = FlexId::new(4, 5, 4, 5, 4, 5).unwrap();
    /// let east = FlexId::new(4, 5, 4, 6, 4, 5).unwrap(); // X+1（面で接する）
    /// let diag = FlexId::new(4, 5, 4, 6, 4, 6).unwrap(); // X+1,Y+1（辺で接する）
    /// assert!(a.is_spatial_face_neighbor(&east));
    /// assert!(!a.is_spatial_face_neighbor(&diag));
    /// assert!(!a.is_spatial_face_neighbor(&a)); // 重なり（自身）は面共有ではない
    /// ```
    pub fn is_spatial_face_neighbor(&self, other: &FlexId) -> bool {
        #[derive(PartialEq)]
        enum Rel {
            Overlap,
            Adjacent,
            Separated,
        }

        fn dimension_range(zoom: u8, index: i64, common: u8) -> (i64, i64) {
            let shift = (common - zoom) as i64;
            (index << shift, ((index + 1) << shift) - 1)
        }

        fn classify(a: (i64, i64), b: (i64, i64), cyclic_width: Option<i64>) -> Rel {
            if a.0.max(b.0) <= a.1.min(b.1) {
                return Rel::Overlap;
            }
            let mut adjacent = a.1 + 1 == b.0 || b.1 + 1 == a.0;
            if let Some(w) = cyclic_width
                && ((a.1 + 1).rem_euclid(w) == b.0.rem_euclid(w)
                    || (b.1 + 1).rem_euclid(w) == a.0.rem_euclid(w))
            {
                adjacent = true;
            }
            if adjacent {
                Rel::Adjacent
            } else {
                Rel::Separated
            }
        }

        let cf = self.f_zoomlevel().max(other.f_zoomlevel());
        let rf = classify(
            dimension_range(self.f_zoomlevel(), self.f_index() as i64, cf),
            dimension_range(other.f_zoomlevel(), other.f_index() as i64, cf),
            None,
        );
        let cx = self.x_zoomlevel().max(other.x_zoomlevel());
        let rx = classify(
            dimension_range(self.x_zoomlevel(), self.x_index() as i64, cx),
            dimension_range(other.x_zoomlevel(), other.x_index() as i64, cx),
            Some(1i64 << cx),
        );
        let cy = self.y_zoomlevel().max(other.y_zoomlevel());
        let ry = classify(
            dimension_range(self.y_zoomlevel(), self.y_index() as i64, cy),
            dimension_range(other.y_zoomlevel(), other.y_index() as i64, cy),
            None,
        );

        let rels = [rf, rx, ry];
        rels.iter().filter(|r| **r == Rel::Adjacent).count() == 1
            && rels.iter().filter(|r| **r == Rel::Overlap).count() == 2
    }
}

/// `temporal_id` feature 無効時に時間軸ぶんのメモリが本当に消えていることを固定する。
///
/// `t_zoomlevel` / `t_index` を `cfg` で落とし忘れると、時間を使わない利用者が
/// [`FlexId`] 1個あたり8バイト（＋木の走査スタックぶん）を払い続けることになる。
/// 有効時の24バイトは `3軸(4+4+4) + ズーム3 + Tズーム1 + パディング + t_index(8)`。
#[cfg(not(feature = "temporal_id"))]
const _: () = assert!(
    core::mem::size_of::<FlexId>() == 16,
    "temporal_id 無効時の FlexId は空間3軸ぶん（16バイト）でなければならない"
);
#[cfg(feature = "temporal_id")]
const _: () = assert!(
    core::mem::size_of::<FlexId>() == 24,
    "FlexId のサイズが変わった。木の走査スタックに直結するので意図した変更か確認すること"
);

/// FlexId の PartialOrd。最初に食い違う位置で両方が割られていれば、その位置の側で順序が決まる。
/// 重なっている組と、片方だけが割られた位置で先に食い違う組は比較できない（[`None`]）。
impl PartialOrd for FlexId {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match self.zorder_rel(other) {
            ZOrderRel::Less => Some(core::cmp::Ordering::Less),
            ZOrderRel::Equal => Some(core::cmp::Ordering::Equal),
            ZOrderRel::Greater => Some(core::cmp::Ordering::Greater),
            ZOrderRel::Overlap { .. } => None,
        }
    }
}
