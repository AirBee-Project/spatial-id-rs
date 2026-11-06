use crate::SingleId;
use crate::id::zoom_level::ZoomLevel;

use crate::{Interval, error::Error};

impl SingleId {
    /// 指定された値から [`SingleId`] を作成する。このコンストラクタは、与えられた `z`, `f`, `x`, `y` が  各ズームレベルにおける範囲内にあるかを検証し、範囲外の場合は [`Error`] を返す。
    ///
    /// # パラメータ
    /// * `z` — ズームレベル（0–[`crate::ZoomLevel::MAX`]の範囲が有効）
    /// * `f` — Fインデックス（鉛直方向）
    /// * `x` — Xインデックス（東西方向）
    /// * `y` — Yインデックス（南北方向）
    ///
    /// # バリデーション
    /// - `z` が [`ZoomLevel::MAX`] を超える場合、[`SpatialIdError::ZOutOfRange`](crate::SpatialIdError::ZOutOfRange) を返す。
    /// - `f` がズームレベル `z` に対する `ZoomLevel::new(z as u8)?.f_min()..=ZoomLevel::new(z as u8)?.f_max()` の範囲外の場合、
    ///   [`SpatialIdError::FOutOfRange`](crate::SpatialIdError::FOutOfRange) を返す。
    /// - `x` または `y` が `0..=ZoomLevel::new(z as u8)?.xy_max()` の範囲外の場合、
    ///   それぞれ [`SpatialIdError::XOutOfRange`](crate::SpatialIdError::XOutOfRange)、[`SpatialIdError::YOutOfRange`](crate::SpatialIdError::YOutOfRange) を返す。
    ///
    ///
    /// IDの作成:
    /// ```no_run
    /// # use spatial_id::SingleId;
    /// let id = SingleId::new(5, 3, 2, 10).unwrap();
    /// assert_eq!(id.to_string(), "5/3/2/10".to_string());
    /// ```
    ///
    /// 次元の範囲外の検知:
    /// ```no_run
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialIdError;
    /// let id = SingleId::new(3, 3, 2, 10);
    /// assert_eq!(id, Err(SpatialIdError::YOutOfRange{z:3,y:10}.into()));
    /// ```
    ///
    /// ズームレベルの範囲外の検知:
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialIdError;
    /// let id = SingleId::new(68, 3, 2, 10);
    /// assert_eq!(id, Err(SpatialIdError::ZOutOfRange { z:68 }.into()));
    /// ```
    ///
    /// 時間を付与したい場合は [`SingleId::set_time`] を組み合わせる:
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::{SingleId, time::Interval};
    /// let mut id = SingleId::new(5, 3, 2, 10).unwrap();
    /// id.set_time(Interval::MINUTE, 3).unwrap();
    /// assert_eq!(id.to_string(), "5/3/2/10_60/3".to_string());
    /// # }
    /// ```
    pub fn new(z: impl Into<u8>, f: i32, x: u32, y: u32) -> Result<SingleId, Error> {
        let zoom = ZoomLevel::new(z.into())?;
        zoom.check_f(f)?;
        zoom.check_x(x)?;
        zoom.check_y(y)?;

        Ok(SingleId {
            z: zoom,
            f,
            x,
            y,
            i: Interval::WHOLE,
            #[cfg(feature = "temporal_id")]
            t: 0,
        })
    }

    /// 検証を行わずに [`SingleId`] を作成する。
    ///
    /// この関数は [`SingleId::new`] と異なり、与えられた `z`, `f`, `x`, `y` に対して一切の範囲チェックや整合性チェックを行わない。そのため、高速に ID を生成できるが、**不正なパラメータを与えた場合の動作は未定義である**。
    ///
    /// # 注意
    /// 呼び出し側は、以下をすべて満たすことを保証しなければならない。
    ///
    /// * `z` が有効なズームレベル（0–[`crate::ZoomLevel::MAX`]）であること
    /// * `f` が与えられた `z` に応じて `ZoomLevel::new(z as u8)?.f_min()..=unsafe { ZoomLevel::new_unchecked(z as u8) }.f_max()` の範囲内であること
    /// * `x` および `y` が `0..=unsafe { ZoomLevel::new_unchecked(z as u8) }.xy_max()` の範囲内であること
    ///
    /// これらが保証されない場合、パニック・不正メモリアクセス・未定義動作を引き起こす可能性がある。
    ///
    /// ```
    /// # use spatial_id::SingleId;
    /// // パラメータが妥当であることを呼び出し側が保証する必要がある
    /// let id = unsafe { SingleId::new_unchecked(5, 3, 2, 10) };
    ///
    /// assert_eq!(id.z(), 5u8);
    /// assert_eq!(id.f(), 3i32);
    /// assert_eq!(id.x(), 2u32);
    /// assert_eq!(id.y(), 10u32);
    /// ```
    ///
    /// # Safety
    /// 呼び出し側は、`z` / `f` / `x` / `y` が各ズームレベルの有効範囲内であることを保証しなければならない。
    pub unsafe fn new_unchecked(z: u8, f: i32, x: u32, y: u32) -> SingleId {
        SingleId {
            z: unsafe { ZoomLevel::new_unchecked(z) },
            f,
            x,
            y,
            i: Interval::WHOLE,
            #[cfg(feature = "temporal_id")]
            t: 0,
        }
    }
}
