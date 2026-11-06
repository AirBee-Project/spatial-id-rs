use core::{
    fmt::{Debug, Display},
    hash::Hash,
    str::FromStr,
};

use crate::{Coordinate, FlexId, Interval, RangeId, SingleId, error::Error};

mod private {
    pub trait Sealed {}
    impl Sealed for crate::SingleId {}
    impl Sealed for crate::RangeId {}
    impl Sealed for crate::FlexId {}
}

/// [SingleId]、[RangeId]、[FlexId] に共通するTrait。
pub trait SpatialId:
    private::Sealed
    + IntoIterator<Item = FlexId>
    + Into<RangeId>
    + Debug
    + Display
    + Clone
    + Copy
    + Eq
    + Hash
    + FromStr
{
    /// F 方向に指定インデックスだけ移動する。
    fn move_f(&mut self, by: i32) -> Result<(), Error>;

    /// X 方向に指定インデックスだけ移動する。
    ///
    /// 経度（X）は地球を東西に循環するため、移動量は常に有効であり失敗しない。
    fn move_x(&mut self, by: i32);

    /// Y 方向に指定インデックスだけ移動する。
    fn move_y(&mut self, by: i32) -> Result<(), Error>;

    /// 3 軸（F, X, Y）をまとめて移動する。
    ///
    /// 移動途中で失敗した場合は元の状態を保つ。
    fn spatial_shift(&mut self, df: i32, dx: i32, dy: i32) -> Result<(), Error> {
        let mut moved = *self;
        moved.move_f(df)?;
        moved.move_x(dx);
        moved.move_y(dy)?;
        *self = moved;
        Ok(())
    }

    /// F 方向の長さをメートル単位で返す。
    fn length_f_meters(&self) -> f64;

    /// X 方向の長さをメートル単位で返す。
    fn length_x_meters(&self) -> f64;

    /// Y 方向の長さをメートル単位で返す。
    fn length_y_meters(&self) -> f64;

    /// 時間情報を含まない、空間3次元（F/X/Y）の中心座標を返す。
    fn spatial_center(&self) -> Coordinate;

    /// 時間情報を含まない、空間3次元（F/X/Y）の直方体を構成する8頂点の座標を返す。
    fn spatial_vertices(&self) -> [Coordinate; 8];

    /// 時間間隔 `{i}`を返す。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, SpatialId, time::Interval};
    /// let id = SingleId::new(12, 0, 3638, 1614).unwrap().with_time(1800, 809712).unwrap();
    /// assert_eq!(id.time_interval().seconds(), 1800);
    ///
    /// let plain = SingleId::new(12, 0, 3638, 1614).unwrap();
    /// assert_eq!(plain.time_interval(), Interval::WHOLE);
    /// # }
    /// ```
    fn time_interval(&self) -> Interval;

    /// 占有する絶対秒区間 `[start, end)` を返す。
    fn seconds_range(&self) -> (u64, u64);

    /// 時間を指定していない（全時間を覆う）かを判定する。
    ///
    /// ```
    /// # use spatial_id::{SingleId, SpatialId};
    /// assert!(SingleId::new(12, 0, 3638, 1614).unwrap().is_whole_time());
    /// ```
    fn is_whole_time(&self) -> bool {
        self.seconds_range() == (0, Interval::MAX_SECONDS)
    }

    /// 時間の指定を外し、全時間へ戻した自身を返す。
    fn without_time(self) -> Self;

    /// 絶対秒区間 `[start, end)` を設定した自身を返す。
    fn with_time_span(self, start: u64, end: u64) -> Result<Self, Error>;

    /// 指定された Unix 時刻（秒）を時間区間に含むかを判定する。
    fn contains_time(&self, unix_seconds: u64) -> bool {
        let (start, end) = self.seconds_range();
        (start..end).contains(&unix_seconds)
    }

    /// 指定したズームレベルでの空間的な親ID（包含する最小の空間ID）を返す。
    fn spatial_parent_at_zoom(&self, target_z: u8) -> Result<Self, Error>;

    /// 自身が [`SingleId`] なら `Some(自身)`、そうでなければ `None` を返す。
    ///
    /// ```
    /// # use spatial_id::{FlexId, RangeId, SingleId, SpatialId};
    /// assert!(SingleId::new(4, 0, 1, 1).unwrap().as_single_id().is_some());
    /// assert!(RangeId::new(4, 0, 1, 1).unwrap().as_single_id().is_none());
    /// assert!(FlexId::new(4, 0, 4, 1, 4, 1).unwrap().as_single_id().is_none());
    /// ```
    fn as_single_id(&self) -> Option<SingleId> {
        None
    }

    /// 自身が [`RangeId`] なら `Some(自身)`、そうでなければ `None` を返す。
    ///
    /// [`SingleId`] や [`FlexId`] を [`RangeId`] として扱いたい場合は [`Into`] を使う。
    ///
    /// ```
    /// # use spatial_id::{FlexId, RangeId, SingleId, SpatialId};
    /// assert!(RangeId::new(4, 0, 1, 1).unwrap().as_range_id().is_some());
    /// assert!(SingleId::new(4, 0, 1, 1).unwrap().as_range_id().is_none());
    /// assert!(FlexId::new(4, 0, 4, 1, 4, 1).unwrap().as_range_id().is_none());
    /// ```
    fn as_range_id(&self) -> Option<RangeId> {
        None
    }

    /// 自身が [`FlexId`] なら `Some(自身)`、そうでなければ `None` を返す。
    ///
    /// ```
    /// # use spatial_id::{FlexId, RangeId, SingleId, SpatialId};
    /// assert!(FlexId::new(4, 0, 4, 1, 4, 1).unwrap().as_flex_id().is_some());
    /// assert!(SingleId::new(4, 0, 1, 1).unwrap().as_flex_id().is_none());
    /// assert!(RangeId::new(4, 0, 1, 1).unwrap().as_flex_id().is_none());
    /// ```
    fn as_flex_id(&self) -> Option<FlexId> {
        None
    }

    /// この空間 ID を構成する [`SingleId`] のイテレータを返す。
    fn single_ids(self) -> impl Iterator<Item = SingleId>;
}
