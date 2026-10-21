use core::fmt;

use crate::{
    ALT_ABS_MAX, Coordinate, Ecef, Error, FractionalId, LAT_ABS_MAX, Point, WGS84_A, WGS84_E2,
    WGS84_F, geometry::traits::CoverSingleIds,
};

impl fmt::Debug for Ecef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ecef")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("z", &self.z)
            .finish()
    }
}

impl From<Ecef> for Coordinate {
    /// 地心直交座標系（ECEF）から地理座標（緯度・経度・高度）への変換。
    fn from(value: Ecef) -> Self {
        let (lat, lon, h) = ecef_to_geodetic(value.x, value.y, value.z);
        unsafe { Coordinate::new_unchecked(lat, lon, h) }
    }
}

/// 往復変換（地理座標 → ECEF → 地理座標）で生じる緯度の浮動小数点誤差の許容幅 \[deg\]。
///
/// 地表付近から高度上限までの実測誤差は 1e-12 度未満であり、十分な余裕を持たせている。
const LATITUDE_TOLERANCE: f64 = 1e-9;

/// 往復変換で生じる高度の浮動小数点誤差の許容幅 \[m\]。
///
/// 地表付近から高度上限までの実測誤差は 1e-3 m 未満であり、十分な余裕を持たせている。
const ALTITUDE_TOLERANCE: f64 = 1e-2;

/// ECEF の XYZ 成分 \[m\] から、緯度 \[deg\]・経度 \[deg\]・高度 \[m\] を計算する。
///
/// 範囲内の地理座標を ECEF に変換して戻すと、浮動小数点誤差によって境界値を
/// わずかに超えることがある。許容幅以内の超過は誤差とみなして境界値に丸める。
/// 許容幅を超える値や NaN はそのまま返すので、範囲の検証は呼び出し側で行う。
pub(crate) fn ecef_to_geodetic(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let lon = libm::atan2(y, x);
    let p = libm::sqrt(x * x + y * y);

    // 緯度の初期値（Bowring）
    let mut lat = libm::atan2(z / p, 1.0 - WGS84_F);
    let mut h = 0.0;

    for _ in 0..10 {
        let sin_lat = libm::sin(lat);
        let n = WGS84_A / libm::sqrt(1.0 - WGS84_E2 * sin_lat * sin_lat);
        h = p / libm::cos(lat) - n;

        let new_lat = libm::atan2(z + WGS84_E2 * n * sin_lat, p);

        if libm::fabs(new_lat - lat) < 1e-12 {
            lat = new_lat;
            break;
        }
        lat = new_lat;
    }

    (
        snap_to_bound(lat.to_degrees(), LAT_ABS_MAX, LATITUDE_TOLERANCE),
        lon.to_degrees(),
        snap_to_bound(h, ALT_ABS_MAX, ALTITUDE_TOLERANCE),
    )
}

/// `value` が `±bound` を `tolerance` 以内だけ超えている場合に限り、`±bound` に丸める。
fn snap_to_bound(value: f64, bound: f64, tolerance: f64) -> f64 {
    if value > bound && value <= bound + tolerance {
        bound
    } else if value < -bound && value >= -bound - tolerance {
        -bound
    } else {
        value
    }
}

impl Point for Ecef {
    fn fractional_id(&self, z: impl Into<u8>) -> Result<FractionalId, Error> {
        Coordinate::from(*self).fractional_id(z)
    }
}

impl CoverSingleIds for Ecef {
    fn cover_single_ids_with<V>(
        &self,
        z: impl Into<u8>,
        value: V,
    ) -> Result<impl Iterator<Item = (crate::SingleId, V)>, crate::Error>
    where
        V: Clone + 'static,
    {
        Ok(core::iter::once((self.single_id(z)?, value)))
    }
}
