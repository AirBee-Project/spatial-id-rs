use crate::{Ecef, Error, FractionalId, RangeId, SingleId, geometry::Epsilon};

/// 現実空間の点に共通する性質。
pub trait Point: Into<Ecef> + Copy {
    /// この点を、指定されたズームレベルの [`SingleId`] に変換する。
    ///
    /// 高度・経度・緯度が空間 ID の範囲の上端ちょうどにある点は、最後のインデックス値に丸められる。
    /// ズームレベルが範囲外の場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::{Coordinate, Point};
    /// # use spatial_id::SingleId;
    /// let coord = Coordinate::new(34.9851603, 135.7584294, 20.0).unwrap();
    ///
    /// assert_eq!(
    ///     coord.single_id(24).unwrap(),
    ///     SingleId::new(24, 10, 14715409, 6646263).unwrap()
    /// );
    ///
    /// // ズームレベルが範囲外の場合はエラーになる
    /// assert!(coord.single_id(31).is_err());
    /// ```
    fn single_id(&self, z: impl Into<u8>) -> Result<SingleId, Error> {
        Ok(self.fractional_id(z)?.round_to_single_id())
    }

    /// この点を、指定されたズームレベルの [`RangeId`] に変換する。
    ///
    /// 結果は、[`Point::single_id`] で得た [`SingleId`] と同じ範囲を表す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::{Coordinate, Point};
    /// # use spatial_id::RangeId;
    /// let coord = Coordinate::new(35.681382, 139.766084, 10.3).unwrap();
    ///
    /// assert_eq!(
    ///     coord.range_id(20).unwrap(),
    ///     RangeId::from(coord.single_id(20).unwrap())
    /// );
    /// ```
    fn range_id(&self, z: impl Into<u8>) -> Result<RangeId, Error> {
        Ok(RangeId::from(self.single_id(z)?))
    }

    /// この点を、指定されたズームレベルの [`FractionalId`] に変換する。
    ///
    /// インデックスは切り捨てずに実数のまま保持する。[`Point::single_id`] はこの結果を
    /// [`FractionalId::round_to_single_id`] で丸めたものになる。
    /// ズームレベルが範囲外の場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::{Coordinate, Point};
    /// let coord = Coordinate::new(0.0, 90.0, 10.5).unwrap();
    ///
    /// // 小数部分はそのまま保持される
    /// let fid = coord.fractional_id(25).unwrap();
    /// assert_eq!(fid.f(), 10.5);
    ///
    /// // 丸めると single_id と一致する
    /// assert_eq!(fid.round_to_single_id(), coord.single_id(25).unwrap());
    ///
    /// // ズームレベルが範囲外の場合はエラーになる
    /// assert!(coord.fractional_id(31).is_err());
    /// ```
    fn fractional_id(&self, z: impl Into<u8>) -> Result<FractionalId, Error>;

    /// 2点間のユークリッド距離（メートル単位）が指定した許容誤差以内にあるかを判定する。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::{Coordinate, Epsilon, Point};
    /// let p1 = Coordinate::new(35.0, 139.0, 0.0).unwrap();
    /// let p2 = Coordinate::new(35.0, 139.0, 0.0001).unwrap();
    ///
    /// assert!(p1.eq_epsilon(&p2, Epsilon::from_meters(0.001).unwrap()));
    /// assert!(!p1.eq_epsilon(&p2, Epsilon::from_meters(0.00001).unwrap()));
    /// ```
    fn eq_epsilon(&self, other: &impl Point, epsilon: Epsilon) -> bool {
        let e1: Ecef = (*self).into();
        e1.distance(&(*other).into()) <= epsilon.as_meters()
    }
}
