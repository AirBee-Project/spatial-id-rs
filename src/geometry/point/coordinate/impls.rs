use crate::{
    Coordinate, Ecef, Error, FractionalId, LON_ABS_MAX, Point, WGS84_A, WGS84_E2, ZoomLevel,
    geometry::traits::CoverSingleIds,
};
use core::fmt;

impl fmt::Debug for Coordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Coordinate")
            .field("latitude", &self.latitude)
            .field("longitude", &self.longitude)
            .field("altitude", &self.altitude)
            .finish()
    }
}

impl From<Coordinate> for Ecef {
    /// [`Coordinate`]を[`Ecef`]への変換。
    /// ```
    /// # use spatial_id::geometry::{Coordinate, Ecef};
    /// let coord = Coordinate::new(43.068564, 41.3507138, 30.0).unwrap();
    /// let ecef: Ecef = coord.into();
    /// print!("{},{},{}", ecef.x(), ecef.y(), ecef.z());
    /// assert_eq!(ecef.x(), 3503254.6369501497);
    /// assert_eq!(ecef.y(), 3083182.6924748584);
    /// assert_eq!(ecef.z(), 4333089.862951963);
    /// ```
    fn from(value: Coordinate) -> Self {
        let lat = value.latitude.to_radians();
        let lon = value.longitude.to_radians();
        let h = value.altitude;

        let sin_lat = libm::sin(lat);
        let cos_lat = libm::cos(lat);
        let sin_lon = libm::sin(lon);
        let cos_lon = libm::cos(lon);

        let n = WGS84_A / libm::sqrt(1.0 - WGS84_E2 * sin_lat * sin_lat);

        let x = (n + h) * cos_lat * cos_lon;
        let y = (n + h) * cos_lat * sin_lon;
        let z = (n * (1.0 - WGS84_E2) + h) * sin_lat;

        unsafe { Ecef::new_unchecked(x, y, z) }
    }
}

/// 緯度・経度・高度のすべてが `0.0` に設定された座標を返す。
impl Default for Coordinate {
    fn default() -> Self {
        Self {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
        }
    }
}

impl Point for Coordinate {
    /// この座標を、指定されたズームレベルに対応する [`FractionalId`] に変換する。
    ///
    /// 各インデックスは切り捨てずに実数のまま保持する。範囲の上端ちょうどの座標は、
    /// 最後のインデックス値の上面（`2^z`）に対応する。
    ///
    /// # 引数
    /// * `z` - 空間 ID のズームレベル
    ///
    /// # 戻り値
    /// * 指定されたズームレベルに対応する [`FractionalId`]
    /// * ズームレベルが範囲外の場合は [`Error`]
    ///
    /// # Example
    /// ```
    /// # use spatial_id::geometry::{Coordinate, FractionalId, Point};
    /// let coord = Coordinate::new(34.9851603, 135.7584294, 20.0).unwrap();
    /// assert_eq!(
    ///     coord.fractional_id(24).unwrap(),
    ///     FractionalId::new(24, 10.0, 14715409.371845974, 6646263.059889234).unwrap()
    /// )
    /// ```
    fn fractional_id(&self, z: impl Into<u8>) -> Result<FractionalId, Error> {
        let zoom = ZoomLevel::new(z.into())?;
        let z = zoom.get();

        let lat = self.latitude;
        let lon = self.longitude;
        let alt = self.altitude;

        //Z=25のとき高さはちょうど1m
        let factor = libm::pow(2_f64, (z as i32 - 25) as f64);
        let f = factor * alt;

        let n = 2u64.pow(z as u32) as f64;
        let x = (lon + LON_ABS_MAX) / (2.0 * LON_ABS_MAX) * n;

        let lat_rad = lat.to_radians();
        let y = (1.0
            - libm::log(libm::tan(lat_rad) + 1.0 / libm::cos(lat_rad)) / core::f64::consts::PI)
            / 2.0
            * n;

        FractionalId::new(z, f, x, y)
    }
}

impl CoverSingleIds for Coordinate {
    type Value<'a> = ();

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (crate::SingleId, V)> + 'a, crate::Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        Ok(core::iter::once((self.single_id(z)?, value)))
    }
}
