pub mod impls;

#[cfg(test)]
mod tests;

use core::borrow::Borrow;

use crate::{
    ALT_ABS_MAX, Ecef, LAT_ABS_MAX, LON_ABS_MAX,
    error::{Error, GeometryError},
};

/// 緯度・経度・高度を表す型。
///
/// 内部的には下記のような構造体として定義されており、空間 ID 上で扱える座標に対する制約が常に満たされる。
///
/// この型は [`PartialOrd`] を実装しているが、これは主に [`BTreeSet`](alloc::collections::BTreeSet) や [`BTreeMap`](alloc::collections::BTreeMap)
/// といった順序付きコレクションにおける格納および探索を目的としたものであり、空間的な位置関係における「大小」を意味するものではない。
/// ```
/// pub struct Coordinate {
///     latitude: f64,
///     longitude: f64,
///     altitude: f64,
/// }
/// ```
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Coordinate {
    latitude: f64,
    longitude: f64,
    altitude: f64,
}

impl Coordinate {
    /// 指定された緯度・経度・高度から [Coordinate] を生成する。
    ///
    /// 各引数は、空間 ID 上で扱える座標として有効な範囲に収まっている必要がある。
    /// 範囲外の値が指定された場合、この関数は対応するエラーを返す。
    ///
    /// # 引数
    /// * `latitude` - 緯度（-85.0511 〜 85.0511）
    /// * `longitude` - 経度（-180.0 〜 180.0）
    /// * `altitude` - 高度（-33,554,432.0 〜 33,554,432.0）
    ///
    /// # 戻り値
    /// * 有効な値が指定された場合は `Ok(Coordinate)` を返す。
    /// * いずれかの値が範囲外の場合は、対応する [`Error`] を返す。
    ///
    /// # Example
    /// ```no_run
    /// # use spatial_id::geometry::Coordinate;
    /// let coord = Coordinate::new(35.0, 139.0, 10.0).unwrap();
    ///
    /// assert_eq!(coord.latitude(), 35.0);
    /// assert_eq!(coord.longitude(), 139.0);
    /// assert_eq!(coord.altitude(), 10.0);
    /// ```
    pub fn new(latitude: f64, longitude: f64, altitude: f64) -> Result<Self, Error> {
        if !(-LAT_ABS_MAX..=LAT_ABS_MAX).contains(&latitude) {
            return Err(GeometryError::LatitudeOutOfRange { latitude }.into());
        }

        if !(-LON_ABS_MAX..=LON_ABS_MAX).contains(&longitude) {
            return Err(GeometryError::LongitudeOutOfRange { longitude }.into());
        }

        if !(-ALT_ABS_MAX..=ALT_ABS_MAX).contains(&altitude) {
            return Err(GeometryError::AltitudeOutOfRange { altitude }.into());
        }

        Ok(Self {
            latitude,
            longitude,
            altitude,
        })
    }

    /// 値の妥当性検証を行わずに [`Coordinate`] を生成する。
    ///
    /// この関数は緯度・経度・高度に対する範囲チェックを一切行わない。
    /// 呼び出し側は、渡す値が空間 ID 上で扱える有効な範囲に収まっていることを
    /// 保証する責任を負う。
    ///
    /// # Safety
    /// この関数は `unsafe` である。
    /// 不正な値を指定した場合、[`Coordinate`] が前提としている不変条件が破られ、
    /// 以降の処理で未定義な振る舞いまたは論理的な不整合を引き起こす可能性があるため、入力値の正当性が外部で十分に検証されている場合にのみ使用せよ。
    pub unsafe fn new_unchecked(latitude: f64, longitude: f64, altitude: f64) -> Coordinate {
        Coordinate {
            latitude,
            longitude,
            altitude,
        }
    }

    /// 緯度を返す。
    ///
    /// 返される値は度数法（degree）で表され、
    /// 常にWEBメルカトル上で扱える有効な範囲（-85.0511 〜 85.0511）内に収まる。
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let coord = Coordinate::new(43.068564, 41.3507138, 30.0).unwrap();
    /// assert_eq!(coord.latitude(), 43.068564);
    /// ```
    pub fn latitude(&self) -> f64 {
        self.latitude
    }

    /// 経度を返す。
    ///
    /// 返される値は度数法（degree）で表され、
    /// 常に -180.0 〜 180.0 の範囲内に収まる。
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let coord = Coordinate::new(35.4095198,136.7566027, 0.0).unwrap();
    /// assert_eq!(coord.longitude(), 136.7566027);
    /// ```
    pub fn longitude(&self) -> f64 {
        self.longitude
    }

    /// 高度を返す。
    ///
    /// される値はメートル（m）で表され、
    /// 常に`-33,554,432.0 ..= 33,554,432.0`の範囲内に収まる。
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let coord = Coordinate::new(34.9851603, 135.7584294, 20.0).unwrap();
    /// assert_eq!(coord.altitude(), 20.0);
    /// ```
    pub fn altitude(&self) -> f64 {
        self.altitude
    }

    /// 緯度を設定する。
    ///
    /// 指定された値が有効な範囲外の場合、この関数はエラーを返し、
    /// 内部の値は変更されない。
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let mut coord = Coordinate::new(35.0, 41.3507138, 30.0).unwrap();
    /// coord.set_latitude(43.068564);
    /// assert_eq!(coord.latitude(), 43.068564);
    /// ```
    pub fn set_latitude(&mut self, latitude: f64) -> Result<(), Error> {
        if !(-LAT_ABS_MAX..=LAT_ABS_MAX).contains(&latitude) {
            return Err(GeometryError::LatitudeOutOfRange { latitude }.into());
        }
        self.latitude = latitude;
        Ok(())
    }

    /// 経度を設定する。
    ///
    /// 指定された値が有効な範囲外の場合、この関数はエラーを返し、
    /// 内部の値は変更されない。
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let mut coord = Coordinate::new(35.4095198,130.0, 0.0).unwrap();
    /// coord.set_longitude(136.7566027);
    /// assert_eq!(coord.longitude(), 136.7566027);
    /// ```
    pub fn set_longitude(&mut self, longitude: f64) -> Result<(), Error> {
        if !(-LON_ABS_MAX..=LON_ABS_MAX).contains(&longitude) {
            return Err(GeometryError::LongitudeOutOfRange { longitude }.into());
        }
        self.longitude = longitude;
        Ok(())
    }

    /// 高度を設定する。
    ///
    /// 単位はメートル（m）である。
    /// 指定された値が有効な範囲外の場合、この関数はエラーを返し、
    /// 内部の値は変更されない。
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let mut coord = Coordinate::new(34.9851603, 135.7584294, -10.0).unwrap();
    /// coord.set_altitude(20.0);
    /// assert_eq!(coord.altitude(), 20.0);
    /// ```
    pub fn set_altitude(&mut self, altitude: f64) -> Result<(), Error> {
        if !(-ALT_ABS_MAX..=ALT_ABS_MAX).contains(&altitude) {
            return Err(GeometryError::AltitudeOutOfRange { altitude }.into());
        }
        self.altitude = altitude;
        Ok(())
    }

    /// 他の [`Coordinate`] との距離をメートル単位で返す。
    ///
    /// * 単位はメートル（m）である。
    /// * WGS84 楕円体を前提とした近似距離。
    ///
    /// # 引数
    /// * `other` - 距離を計算する対象の座標
    ///
    /// # 戻り値
    /// * 2 点間の距離（メートル）
    ///
    ///# Example
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let coord_tokyo = Coordinate::new(35.681382, 139.76608399999998, 0.0).unwrap();
    /// let coord_sinagawa = Coordinate::new(35.630152, 139.74044000000004, 10.0).unwrap();
    /// let d = coord_tokyo.distance(&coord_sinagawa);
    /// assert_eq!(&d, &6140.165513581259);
    /// ```
    pub fn distance(&self, other: &Coordinate) -> f64 {
        let e1: Ecef = (*self).into();
        let e2: Ecef = (*other).into();
        e1.distance(&e2)
    }

    /// 与えられた座標群の重心を計算する。
    ///
    /// この関数は [`IntoIterator`] を受け入れるため、スライス `&[Coordinate]`、
    /// `Vec<Coordinate>`、または `iter_coords()` などのイテレータを直接渡すことができる。
    /// 内部で [`Borrow`](core::borrow::Borrow) トレイトを利用しているため、要素が実体か参照かを問わず動作する。
    ///
    /// 座標群が空の場合は、[`Coordinate::default()`] を返す。
    ///
    /// # パラメーター
    /// * `coordinates` — 重心を計算する対象となる座標の集合（イテレータ、スライス、Vecなど）
    ///
    /// # 動作例
    ///
    /// スライスからの計算:
    /// ```
    /// # use spatial_id::geometry::Coordinate;
    /// let points = [
    ///     Coordinate::new(0.0, 0.0, 10.0).unwrap(),
    ///     Coordinate::new(10.0, 10.0, 20.0).unwrap(),
    /// ];
    /// let center = Coordinate::centroid(&points);
    /// assert_eq!(center.latitude(), 5.0);
    /// ```
    pub fn centroid<I>(coordinates: I) -> Coordinate
    where
        I: IntoIterator,
        I::Item: Borrow<Coordinate>,
    {
        let (mut sum_lat, mut sum_lon, mut sum_alt) = (0.0, 0.0, 0.0);

        let mut sum_lon_360 = 0.0;
        let (mut min_lon, mut max_lon) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_lon_360, mut max_lon_360) = (f64::INFINITY, f64::NEG_INFINITY);

        let mut count = 0usize;

        for coord in coordinates {
            let c = coord.borrow();
            let lon_360 = if c.longitude < 0.0 {
                c.longitude + 360.0
            } else {
                c.longitude
            };

            sum_lat += c.latitude;
            sum_lon += c.longitude;
            sum_alt += c.altitude;

            sum_lon_360 += lon_360;
            min_lon = min_lon.min(c.longitude);
            max_lon = max_lon.max(c.longitude);
            min_lon_360 = min_lon_360.min(lon_360);
            max_lon_360 = max_lon_360.max(lon_360);

            count += 1;
        }

        if count == 0 {
            return Self::default();
        }

        let n = count as f64;

        let spread = max_lon - min_lon;
        let lon = if spread > 180.0 && max_lon_360 - min_lon_360 < spread {
            let mean = sum_lon_360 / n;
            if mean > 180.0 { mean - 360.0 } else { mean }
        } else {
            sum_lon / n
        };

        // 浮動小数点により緯度が範囲に収まる保証がないため、`clamp`する。
        Self::new((sum_lat / n).clamp(-85.0511, 85.0511), lon, sum_alt / n).unwrap()
    }
}
