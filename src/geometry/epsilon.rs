use core::fmt;

use crate::{Error, GeometryError};

/// 幾何計算における同一点判定などの許容誤差（メートル単位）を表す型。
///
/// 浮動小数点数演算における誤差を吸収し、近接した頂点の同一判定や
/// 閉合性検証に用いる。
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Epsilon(f64);

impl Epsilon {
    /// デフォルトの許容誤差（1 µm = 1e-6 m）。
    ///
    /// 地球規模の ECEF 座標（約 6.4e6 m）では f64 の分解能が約 1e-9 m のため、
    /// それより十分大きく、かつ空間ID の最大ズームレベルの大きさより十分小さい値にしている。
    pub const DEFAULT: Self = Self(1e-6);

    /// 許容誤差ゼロ（0.0 m）。
    pub const ZERO: Self = Self(0.0);

    /// メートル単位の数値から許容誤差を生成する。
    ///
    /// 負数、NaN、無限大が指定された場合はエラーを返す。
    /// 0.0 は許容される。
    ///
    /// # 引数
    /// * `meters` - 許容誤差（メートル単位、0以上かつ有限数）
    ///
    /// # エラー
    /// `meters` が負数、NaN、または無限大の場合、[`GeometryError::EpsilonOutOfRange`] を返す。
    pub fn from_meters(meters: f64) -> Result<Self, Error> {
        if meters < 0.0 || !meters.is_finite() {
            Err(GeometryError::EpsilonOutOfRange { epsilon: meters }.into())
        } else {
            Ok(Self(meters))
        }
    }

    /// メートル単位の許容誤差の数値を返す。
    #[inline]
    pub const fn as_meters(self) -> f64 {
        self.0
    }
}

impl Default for Epsilon {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Epsilon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} m", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_epsilon() {
        let t = Epsilon::from_meters(0.01).unwrap();
        assert_eq!(t.as_meters(), 0.01);

        let t_zero = Epsilon::from_meters(0.0).unwrap();
        assert_eq!(t_zero.as_meters(), 0.0);
    }

    #[test]
    fn default_epsilon() {
        assert_eq!(Epsilon::default(), Epsilon::DEFAULT);
        assert_eq!(Epsilon::DEFAULT.as_meters(), 1e-6);
    }

    #[test]
    fn invalid_epsilon() {
        assert!(Epsilon::from_meters(-0.001).is_err());
        assert!(Epsilon::from_meters(f64::NAN).is_err());
        assert!(Epsilon::from_meters(f64::INFINITY).is_err());
        assert!(Epsilon::from_meters(f64::NEG_INFINITY).is_err());
    }
}
