pub mod impls;

use crate::{
    Vec3, Vec3Ecef,
    error::Error,
    geometry::{constants::WGS84_B, point::coordinate::Coordinate},
};

/// 地心直交座標系（ECEF: Earth-Centered, Earth-Fixed）における座標を表す。
///
/// 原点は地球の重心にあり、
/// * X 軸は赤道面上で本初子午線方向
/// * Y 軸は赤道面上で東経 90 度方向
/// * Z 軸は北極方向
///
/// 単位はすべてメートル。
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Ecef {
    x: f64,
    y: f64,
    z: f64,
}

const NEAREDGE_P2: f64 = 336394929032.94604;
const NEAREDGE_Z2: f64 = 40074157011110.21;
const MAX_R2: f64 = (WGS84_B + 33_554_432.0) * (WGS84_B + 33_554_432.0);
impl Ecef {
    /// 指定された XYZ 成分から [`Ecef`] を生成する。
    ///
    /// 空間 ID で扱える範囲（緯度 ±85.0511°、高度 ±33,554,432 m）の外にある点を
    /// 指定した場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// // 赤道・本初子午線上の地表付近
    /// let ecef = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
    ///
    /// assert_eq!(ecef.x(), 6_378_137.0);
    /// assert_eq!(ecef.y(), 0.0);
    /// assert_eq!(ecef.z(), 0.0);
    ///
    /// // 地球の中心や高度範囲外の点はエラーになる
    /// assert!(Ecef::new(0.0, 0.0, 0.0).is_err());
    /// assert!(Ecef::new(1.0e9, 0.0, 0.0).is_err());
    /// ```
    pub fn new(x: f64, y: f64, z: f64) -> Result<Ecef, Error> {
        let p2 = x * x + y * y;
        let z2 = z * z;
        let r_ok = p2 + z2 <= MAX_R2;
        let lat_ok = (z2 < NEAREDGE_Z2 && p2 > NEAREDGE_P2)
            || (z2 > NEAREDGE_Z2 && z2 / p2 < NEAREDGE_Z2 / NEAREDGE_P2);
        //明らかに範囲内であるものを、四則演算のみ簡易検証により早期リターンする。
        //境界に近いもののみを厳密に検証する。
        if r_ok && lat_ok {
            Ok(Ecef { x, y, z })
        } else {
            Coordinate::try_from(Vec3Ecef::new(x, y, z))?;
            Ok(Ecef { x, y, z })
        }
    }

    /// 値の妥当性検証を行わずに [`Ecef`] を生成する。
    ///
    /// この関数は緯度・経度・高度に対する範囲チェックを一切行わない。
    /// 呼び出し側は、渡す値が空間 ID 上で扱える有効な範囲に収まっていることを
    /// 保証する責任を負う。
    ///
    /// # Safety
    /// この関数は `unsafe` である。
    /// 不正な値を指定した場合、[`Ecef`] が前提としている不変条件が破られ、
    /// 以降の処理で未定義な振る舞いまたは論理的な不整合を引き起こす可能性があるため、入力値の正当性が外部で十分に検証されている場合にのみ使用せよ。
    pub unsafe fn new_unchecked(x: f64, y: f64, z: f64) -> Ecef {
        Ecef { x, y, z }
    }
    /// X 成分を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let ecef = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
    /// assert_eq!(ecef.x(), 6_378_137.0);
    /// ```
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Y 成分を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let ecef = Ecef::new(0.0, 6_378_137.0, 0.0).unwrap();
    /// assert_eq!(ecef.y(), 6_378_137.0);
    /// ```
    pub fn y(&self) -> f64 {
        self.y
    }

    /// Z 成分を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let ecef = Ecef::new(6_378_137.0, 0.0, 100_000.0).unwrap();
    /// assert_eq!(ecef.z(), 100_000.0);
    /// ```
    pub fn z(&self) -> f64 {
        self.z
    }

    /// X 成分を設定する。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let mut ecef = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
    /// ecef.set_x(6_400_000.0).unwrap();
    /// assert_eq!(ecef.x(), 6_400_000.0);
    ///
    /// // 範囲外になる値はエラーとなり、元の値が保たれる
    /// assert!(ecef.set_x(1.0e9).is_err());
    /// assert_eq!(ecef.x(), 6_400_000.0);
    /// ```
    pub fn set_x(&mut self, x: f64) -> Result<(), Error> {
        let p2 = self.y * self.y + x * x;
        let z2 = self.z * self.z;
        let r_ok = p2 + z2 <= MAX_R2;
        let lat_ok = (z2 < NEAREDGE_Z2 && p2 > NEAREDGE_P2)
            || (z2 > NEAREDGE_Z2 && z2 / p2 < NEAREDGE_Z2 / NEAREDGE_P2);
        if r_ok && lat_ok {
            self.x = x;
            Ok(())
        } else {
            Coordinate::try_from(Vec3Ecef::new(x, self.y, self.z))?;
            self.x = x;
            Ok(())
        }
    }

    /// Y 成分を設定する。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let mut ecef = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
    /// ecef.set_y(100_000.0).unwrap();
    /// assert_eq!(ecef.y(), 100_000.0);
    ///
    /// // 範囲外になる値はエラーとなり、元の値が保たれる
    /// assert!(ecef.set_y(1.0e9).is_err());
    /// assert_eq!(ecef.y(), 100_000.0);
    /// ```
    pub fn set_y(&mut self, y: f64) -> Result<(), Error> {
        let p2 = self.x * self.x + y * y;
        let z2 = self.z * self.z;
        let r_ok = p2 + z2 <= MAX_R2;
        let lat_ok = (z2 < NEAREDGE_Z2 && p2 > NEAREDGE_P2)
            || (z2 > NEAREDGE_Z2 && z2 / p2 < NEAREDGE_Z2 / NEAREDGE_P2);
        if r_ok && lat_ok {
            self.y = y;
            Ok(())
        } else {
            Coordinate::try_from(Vec3Ecef::new(self.x, y, self.z))?;
            self.y = y;
            Ok(())
        }
    }

    /// Z 成分を設定する。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let mut ecef = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
    /// ecef.set_z(100_000.0).unwrap();
    /// assert_eq!(ecef.z(), 100_000.0);
    ///
    /// // 範囲外になる値（ここでは極に近すぎる点）はエラーとなり、元の値が保たれる
    /// assert!(ecef.set_z(1.0e8).is_err());
    /// assert_eq!(ecef.z(), 100_000.0);
    /// ```
    pub fn set_z(&mut self, z: f64) -> Result<(), Error> {
        let p2 = self.x * self.x + self.y * self.y;
        let z2 = z * z;
        let r_ok = p2 + z2 <= MAX_R2;
        let lat_ok = (z2 < NEAREDGE_Z2 && p2 > NEAREDGE_P2)
            || (z2 > NEAREDGE_Z2 && z2 / p2 < NEAREDGE_Z2 / NEAREDGE_P2);
        if r_ok && lat_ok {
            self.z = z;
            Ok(())
        } else {
            Coordinate::try_from(Vec3Ecef::new(self.x, self.y, z))?;
            self.z = z;
            Ok(())
        }
    }

    /// 他の [`Ecef`] 座標との距離をメートル単位で返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::Ecef;
    ///
    /// let a = Ecef::new(6_378_137.0, 0.0, 0.0).unwrap();
    /// let b = Ecef::new(6_378_140.0, 4.0, 0.0).unwrap();
    ///
    /// assert_eq!(a.distance(&b), 5.0);
    /// ```
    pub fn distance(&self, other: &Ecef) -> f64 {
        libm::sqrt(
            ((self.x() - other.x()) * (self.x() - other.x()))
                + ((self.y() - other.y()) * (self.y() - other.y()))
                + ((self.z() - other.z()) * (self.z() - other.z())),
        )
    }
    /// 原点からの距離の2乗を取得する。
    pub fn norm_squared(&self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// 指定された軸インデックス (0=x, 1=y, 2=z) の成分を返す。
    /// 投影計算などで軸を動的に扱いたい場合に有用。
    pub(crate) fn get_component(&self, index: usize) -> f64 {
        match index {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }

    /// 指定された2つの軸（u, v）に基づいて 2D 平面に投影した座標を返す。
    pub(crate) fn project_2d(&self, u_axis: usize, v_axis: usize) -> (f64, f64) {
        (self.get_component(u_axis), self.get_component(v_axis))
    }
}

#[cfg(test)]
mod tests;
