use crate::id::zoom_level::ZoomLevel;
pub mod impls;

use crate::{
    SingleId,
    error::{Error, GeometryError},
};

/// 実数値のインデックス（Z, F, X, Y）で表される空間 ID の座標型。
///
/// 空間 ID ([`SingleId`]) が整数値のインデックスを持つのに対し、[`FractionalId`] は実数値（`f64`）の
/// F、X、Y インデックスを保持することで、グリッド内のより詳細な位置や端点などを表現できる。
///
/// ```
/// # use spatial_id::ZoomLevel;
/// pub struct FractionalId {
///     z: ZoomLevel,
///     f: f64,
///     x: f64,
///     y: f64,
/// }
/// ```
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct FractionalId {
    z: ZoomLevel,
    f: f64,
    x: f64,
    y: f64,
}

impl FractionalId {
    /// 指定されたズームレベル、F、X、Y の各成分（実数値）から [`FractionalId`] を生成する。
    ///
    /// 各引数は、空間 ID 上で扱える座標インデックスとして有効な範囲に収まっている必要がある。
    /// 範囲外の値が指定された場合、この関数は対応するエラーを返す。
    ///
    /// # 引数
    /// * `z` - 空間 ID のズームレベル（0 〜 [`crate::ZoomLevel::MAX`]）
    /// * `f` - F インデックス（`z.f_min()` 〜 `z.f_max() + 1`）
    /// * `x` - X インデックス（`0.0` 〜 `z.xy_max() + 1`）
    /// * `y` - Y インデックス（`0.0` 〜 `z.xy_max() + 1`）
    ///
    /// # 戻り値
    /// * 有効な値が指定された場合は `Ok(FractionalId)` を返す。
    /// * いずれかの値が範囲外の場合は、対応する [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// assert_eq!(fid.z(), 4);
    /// assert_eq!(fid.f(), 5.5);
    /// assert_eq!(fid.x(), 6.2);
    /// assert_eq!(fid.y(), 7.8);
    /// ```
    pub fn new(z: impl Into<u8>, f: f64, x: f64, y: f64) -> Result<Self, Error> {
        let zoom = ZoomLevel::new(z.into())?;

        // FractionalId は連続値のため、インデックス値の上限ではなく境界までを有効とする。
        // 上端（高度=2^25 / 経度=180° / 緯度の南端）は最後のインデックス値の上面 = 2^z に対応する。
        let f_min = zoom.f_min() as f64;
        let f_max = zoom.f_max() as f64 + 1.0;
        let xy_max = zoom.xy_max() as f64 + 1.0;

        if f < f_min || f > f_max || !f.is_finite() {
            return Err(GeometryError::FractionalFOutOfRange { f, z: zoom.get() }.into());
        }
        if x < 0.0 || x > xy_max || !x.is_finite() {
            return Err(GeometryError::FractionalXOutOfRange { x, z: zoom.get() }.into());
        }
        if y < 0.0 || y > xy_max || !y.is_finite() {
            return Err(GeometryError::FractionalYOutOfRange { y, z: zoom.get() }.into());
        }

        Ok(FractionalId { z: zoom, f, x, y })
    }

    /// ズームレベルを返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// assert_eq!(fid.z(), 4);
    /// ```
    pub fn z(&self) -> u8 {
        self.z.get()
    }

    /// F インデックス（実数）を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// assert_eq!(fid.f(), 5.5);
    /// ```
    pub fn f(&self) -> f64 {
        self.f
    }

    /// X インデックス（実数）を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// assert_eq!(fid.x(), 6.2);
    /// ```
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Y インデックス（実数）を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// assert_eq!(fid.y(), 7.8);
    /// ```
    pub fn y(&self) -> f64 {
        self.y
    }

    /// F インデックスを更新する。
    ///
    /// 与えられた `value` が、現在のズームレベル `z` に対応する
    /// `z.f_min()..=z.f_max()` の範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let mut fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// fid.set_f(6.0).unwrap();
    /// assert_eq!(fid.f(), 6.0);
    /// ```
    pub fn set_f(&mut self, value: f64) -> Result<(), Error> {
        let z = self.z.get();
        let min = self.z.f_min() as f64;
        let max = self.z.f_max() as f64;
        if value < min || value > max || !value.is_finite() {
            return Err(GeometryError::FractionalFOutOfRange { f: value, z }.into());
        }
        self.f = value;
        Ok(())
    }

    /// X インデックスを更新する。
    ///
    /// 与えられた `value` が、現在のズームレベル `z` に対応する
    /// `0.0..=z.xy_max()` の範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let mut fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// fid.set_x(8.0).unwrap();
    /// assert_eq!(fid.x(), 8.0);
    /// ```
    pub fn set_x(&mut self, value: f64) -> Result<(), Error> {
        let z = self.z.get();
        let max = self.z.xy_max() as f64;
        if value < 0.0 || value > max || !value.is_finite() {
            return Err(GeometryError::FractionalXOutOfRange { x: value, z }.into());
        }
        self.x = value;
        Ok(())
    }

    /// Y インデックスを更新する。
    ///
    /// 与えられた `value` が、現在のズームレベル `z` に対応する
    /// `0.0..=z.xy_max()` の範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let mut fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// fid.set_y(2.0).unwrap();
    /// assert_eq!(fid.y(), 2.0);
    /// ```
    pub fn set_y(&mut self, value: f64) -> Result<(), Error> {
        let z = self.z.get();
        let max = self.z.xy_max() as f64;
        if value < 0.0 || value > max || !value.is_finite() {
            return Err(GeometryError::FractionalYOutOfRange { y: value, z }.into());
        }
        self.y = value;
        Ok(())
    }

    /// この [`FractionalId`] が属する、整数値インデックスの [`SingleId`] を返す。
    ///
    /// 内部的には各実数値インデックス（F, X, Y）の床関数（`floor`）を取ることで
    /// 対応する [`SingleId`] を計算する。
    /// 上端ちょうどの値（`x = 2^z` など）は、最後のインデックス値に丸められる。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    /// # use spatial_id::SingleId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.2, 7.8).unwrap();
    /// let sid = fid.round_to_single_id();
    /// assert_eq!(sid.z(), 4);
    /// assert_eq!(sid.f(), 5);
    /// assert_eq!(sid.x(), 6);
    /// assert_eq!(sid.y(), 7);
    ///
    /// // 上端ちょうどの値は最後のインデックス値になる
    /// let top = FractionalId::new(4, 16.0, 16.0, 16.0).unwrap();
    /// let sid = top.round_to_single_id();
    /// assert_eq!(sid.f(), 15);
    /// assert_eq!(sid.x(), 15);
    /// assert_eq!(sid.y(), 15);
    /// ```
    pub fn round_to_single_id(&self) -> SingleId {
        unsafe {
            SingleId::new_unchecked(
                self.z.get(),
                (libm::floor(self.f) as i32).min(self.z.f_max()),
                (libm::floor(self.x) as u32).min(self.z.xy_max()),
                (libm::floor(self.y) as u32).min(self.z.xy_max()),
            )
        }
    }

    /// ズームレベルを `z` に変更した [`FractionalId`] を、同じ位置を指す値として返す。
    ///
    /// ズームレベルが 1 深くなるごとに各インデックスは 2 倍になり、浅くなるごとに半分になる。
    /// `z` が範囲外の場合は [`Error`] を返す。
    ///
    /// # Examples
    /// ```
    /// # use spatial_id::geometry::FractionalId;
    ///
    /// let fid = FractionalId::new(4, 5.5, 6.25, 7.75).unwrap();
    ///
    /// let deeper = fid.into_another_zoomlevel(6).unwrap();
    /// assert_eq!(deeper.z(), 6);
    /// assert_eq!(deeper.f(), 22.0);
    /// assert_eq!(deeper.x(), 25.0);
    /// assert_eq!(deeper.y(), 31.0);
    ///
    /// // 元のズームレベルに戻すと元の値になる
    /// assert_eq!(deeper.into_another_zoomlevel(4).unwrap(), fid);
    ///
    /// assert!(fid.into_another_zoomlevel(31).is_err());
    /// ```
    pub fn into_another_zoomlevel(self, z: impl Into<u8>) -> Result<FractionalId, Error> {
        let z = z.into();
        let zoom = ZoomLevel::new(z)?;
        let dz = z as i32 - self.z.get() as i32;
        if dz == 0 {
            return Ok(self);
        }
        let scale = libm::ldexp(1.0, dz); // 2^dz
        let new_f = self.f * scale;
        let new_x = self.x * scale;
        let new_y = self.y * scale;
        Ok(FractionalId {
            z: zoom,
            f: new_f,
            x: new_x,
            y: new_y,
        })
    }

    /// 値の妥当性検証を行わずに [`FractionalId`] を生成する。
    ///
    /// この関数はズームレベルや各実数インデックスに対する範囲チェックを一切行わない。
    /// 呼び出し側は、渡す値が空間 ID の有効な範囲に収まっていることを保証する責任を負う。
    ///
    /// # Safety
    /// この関数は `unsafe` である。
    ///
    /// 以下の制約が保証されない場合、パニック、不正メモリアクセス、未定義動作、
    /// または論理的な不整合を引き起こす可能性がある。
    /// * `z` が有効なズームレベル（0 〜 [`crate::ZoomLevel::MAX`]）であること
    /// * `f` が `z.f_min()..=z.f_max()` の範囲内であること
    /// * `x` および `y` が `0.0..=z.xy_max()` の範囲内であること
    pub unsafe fn new_unchecked(z: u8, f: f64, x: f64, y: f64) -> Self {
        FractionalId {
            z: ZoomLevel::new(z).unwrap(),
            f,
            x,
            y,
        }
    }
}

#[cfg(test)]
mod tests;
