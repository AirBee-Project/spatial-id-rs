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

    /// 時間方向（T）に指定インデックスだけ移動する。
    ///
    /// 単位は `{i}`（[`FlexId`] はズームが決める Segment 幅）。時間は循環しないので、
    /// 範囲外へ出る場合は [`SpatialIdError::TOutOfRange`](crate::SpatialIdError::TOutOfRange) を返し、元の状態を保つ。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, SpatialId};
    /// let mut id = SingleId::new(5, 3, 2, 10).unwrap();
    /// id.set_time(3600, 5).unwrap();
    /// id.move_t(-2).unwrap();
    /// assert_eq!(id.t(), 3);
    /// assert!(id.move_t(-4).is_err());
    /// assert_eq!(id.t(), 3);
    /// # }
    /// ```
    fn move_t(&mut self, by: i64) -> Result<(), Error>;

    /// F 方向の長さをメートル単位で返す。
    fn length_f_meters(&self) -> f64;

    /// 時間方向の長さを秒単位で返す。時間を指定していないときは全時間の長さになる。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{RangeId, SpatialId};
    /// let mut id = RangeId::new(5, 3, 2, 10).unwrap();
    /// id.set_time(1800, [0, 3]).unwrap();
    /// assert_eq!(id.length_t_seconds(), 7200);
    /// # }
    /// ```
    fn length_t_seconds(&self) -> u64 {
        let (start, end) = self.time_range();
        end - start
    }

    /// X 方向の長さをメートル単位で返す。
    fn length_x_meters(&self) -> f64;

    /// Y 方向の長さをメートル単位で返す。
    fn length_y_meters(&self) -> f64;

    /// 時間情報を含まない、空間3次元（F/X/Y）の中心座標を返す。
    fn spatial_center(&self) -> Coordinate;

    /// 時間情報を含まない、空間3次元（F/X/Y）の直方体を構成する8頂点の座標を返す。
    fn spatial_vertices(&self) -> [Coordinate; 8];

    /// 占有する絶対秒区間 `[start, end)` を返す。
    fn time_range(&self) -> (u64, u64);

    /// 時間を指定していない（全時間を覆う）かを判定する。
    ///
    /// ```
    /// # use spatial_id::{SingleId, SpatialId};
    /// assert!(SingleId::new(12, 0, 3638, 1614).unwrap().time_is_whole());
    /// ```
    fn time_is_whole(&self) -> bool {
        self.time_range() == (0, Interval::MAX_SECONDS)
    }

    /// 時間の指定を外し、全時間へ戻す。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, SpatialId};
    /// let mut id = SingleId::new(5, 3, 2, 10).unwrap();
    /// id.set_time(3600, 5).unwrap();
    /// id.time_clear();
    /// assert!(id.time_is_whole());
    /// # }
    /// ```
    fn time_clear(&mut self);

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
