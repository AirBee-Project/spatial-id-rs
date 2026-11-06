use alloc::string::ToString;

use crate::{Coordinate, Ecef, Error, SingleId, SpatialId, SpatialIdError, id::helpers};
use core::fmt;
use core::str::FromStr;

impl fmt::Display for SingleId {
    /// [`SingleId`] を文字列形式で表示する。
    ///
    /// 形式は `"{z}/{f}/{x}/{y}"`。
    ///
    /// ```no_run
    /// # use spatial_id::SingleId;
    /// # use core::fmt::Write;
    /// let id = SingleId::new(4, 6, 9, 10).unwrap();
    /// let s = format!("{}", id);
    /// assert_eq!(s, "4/6/9/10");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}/{}", self.z.get(), self.f, self.x, self.y)?;
        //時間の情報があれば書き込み
        if !self.time_is_whole() {
            write!(f, "_{}/{}", self.i, self.t())?;
        }
        Ok(())
    }
}

impl SpatialId for SingleId {
    /// 指定したインデックス差 `by` に基づき、この [`SingleId`] を垂直上下方向に動かす。
    ///
    /// # パラメータ
    /// * `by` — インデックス差
    ///
    /// # バリデーション
    /// - Fインデックスが範囲外になる場合は[`SpatialIdError::FOutOfRange`]を返す。
    ///
    /// 移動
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialId;
    /// let mut id = SingleId::new(4, 6, 9, 10).unwrap();
    /// assert_eq!(id.f(), 6);
    ///
    /// let _ = id.move_f(-10).unwrap();
    /// assert_eq!(id.f(), -4);
    /// ```
    ///
    /// 範囲外の検知によるエラー
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SpatialIdError;
    /// let mut id = SingleId::new(4, 6, 9, 10).unwrap();
    /// assert_eq!(id.f(), 6);
    /// assert_eq!(id.move_f(50), Err(SpatialIdError::FOutOfRange { z: 4, f: 56 }.into()));
    /// ```
    fn move_f(&mut self, by: i32) -> Result<(), Error> {
        let new = self.f.checked_add(by).ok_or_else(|| {
            Error::from(SpatialIdError::FOutOfRange {
                f: if by >= 0 { i32::MAX } else { i32::MIN },
                z: self.z.get(),
            })
        })?;

        self.z.check_f(new)?;

        self.f = new;

        Ok(())
    }

    /// 指定したインデックス差 `by` に基づき、この [`SingleId`] を東西方向に動かす。Webメルカトル図法において、東西方向は循環しているためどのような値を指定してもエラーは発生しない。
    ///
    /// # パラメータ
    /// * `by` — インデックス差
    ///
    /// 移動
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialId;
    /// let mut id = SingleId::new(4, 6, 9, 10).unwrap();
    /// assert_eq!(id.x(), 9);
    ///
    /// let _ = id.move_x(-3);
    /// assert_eq!(id.x(), 6);
    /// ```
    ///
    /// 循環による移動
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialId;
    /// let mut id = SingleId::new(4, 6, 9, 10).unwrap();
    /// assert_eq!(id.x(), 9);
    ///
    /// let _ = id.move_x(100);
    /// assert_eq!(id.x(), 13);
    /// ```
    fn move_x(&mut self, by: i32) {
        let max_len = self.z.xy_max() as i64 + 1;
        let new = (self.x as i64 + by as i64).rem_euclid(max_len);
        self.x = new as u32;
    }

    /// 指定したインデックス差 `by` に基づき、この [`SingleId`] を南北方向に動かす。
    ///
    /// # パラメータ
    /// * `by` — インデックス差
    ///
    /// # バリデーション
    /// - Yインデックスが範囲外になる場合は[`SpatialIdError::YOutOfRange`]を返す。
    ///
    /// 移動
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialId;
    /// let mut id = SingleId::new(4, 6, 9, 10).unwrap();
    /// assert_eq!(id.y(), 10);
    ///
    /// let _ = id.move_y(-3).unwrap();
    /// assert_eq!(id.y(), 7);
    /// ```
    ///
    /// 範囲外の検知によるエラー
    /// ```
    /// # use spatial_id::SingleId;
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SpatialIdError;
    /// let mut id = SingleId::new(4, 6, 9, 10).unwrap();
    /// assert_eq!(id.y(), 10);
    /// assert_eq!(id.move_y(-20), Err(SpatialIdError::YOutOfRange { z: 4, y: 0 }.into()));
    /// ```
    fn move_y(&mut self, by: i32) -> Result<(), Error> {
        let new = if by >= 0 {
            self.y.checked_add(by as u32).ok_or_else(|| {
                Error::from(SpatialIdError::YOutOfRange {
                    y: u32::MAX,
                    z: self.z.get(),
                })
            })?
        } else {
            self.y
                .checked_sub(by.unsigned_abs())
                .ok_or(SpatialIdError::YOutOfRange {
                    y: 0,
                    z: self.z.get(),
                })?
        };

        self.z.check_y(new)?;

        self.y = new;

        Ok(())
    }

    /// [`SingleId`] の空間中心座標を [`Coordinate`] 型で返す。
    ///
    /// 中心座標は空間 ID の最も外側の頂点の8点の平均座標である。現実空間における空間 ID は完全な直方体ではなく、緯度や高度によって歪みが発生していることに注意する必要がある。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// # use spatial_id::geometry::Coordinate;
    /// let id = SingleId::new(4, 6, 9, 14).unwrap();
    /// let center: Coordinate = id.spatial_center();
    /// println!("{:?}", center);
    /// ```
    fn spatial_center(&self) -> Coordinate {
        Coordinate::new(
            helpers::latitude(self.y as f64 + 0.5, self.z.get()),
            helpers::longitude(self.x as f64 + 0.5, self.z.get()),
            helpers::altitude(self.f as f64 + 0.5, self.z.get()),
        )
        .unwrap()
    }

    /// [`SingleId`] の最も外側の頂点の8点の座標を [`Coordinate`] 型の配列として返す。
    ///
    /// 現実空間における空間 ID は完全な直方体ではなく、緯度や高度によって歪みが発生していることに注意する必要がある。
    ///
    /// ```
    /// # use spatial_id::SpatialId;
    /// # use spatial_id::SingleId;
    /// # use spatial_id::geometry::Coordinate;
    /// let id = SingleId::new(4, 6, 9, 14).unwrap();
    /// let vertices: [Coordinate; 8] = id.spatial_vertices();
    /// println!("{:?}", vertices);
    /// ```
    fn spatial_vertices(&self) -> [Coordinate; 8] {
        let xs = [self.x as f64, self.x as f64 + 1.0];
        let ys = [self.y as f64, self.y as f64 + 1.0];
        let fs = [self.f as f64, self.f as f64 + 1.0];

        // 各端点の値を前計算しておく
        let lon2 = [
            helpers::longitude(xs[0], self.z.get()),
            helpers::longitude(xs[1], self.z.get()),
        ];
        let lat2 = [
            helpers::latitude(ys[0], self.z.get()),
            helpers::latitude(ys[1], self.z.get()),
        ];
        let alt2 = [
            helpers::altitude(fs[0], self.z.get()),
            helpers::altitude(fs[1], self.z.get()),
        ];

        // 結果配列
        let mut out = [Coordinate::default(); 8];

        let mut i = 0;
        for &altitude in &alt2 {
            for &latitude in &lat2 {
                for &longitude in &lon2 {
                    out[i]
                        .set_longitude(longitude)
                        .expect("longitude must be within valid range");
                    out[i]
                        .set_latitude(latitude)
                        .expect("latitude must be within valid range");
                    out[i]
                        .set_altitude(altitude)
                        .expect("altitude must be within valid range");
                    i += 1;
                }
            }
        }

        out
    }

    ///その空間IDのＦ方向の長さをメートル単位で計算する関数
    fn length_f_meters(&self) -> f64 {
        //Z=25のとき、ちょうど高さが1mとなる
        libm::pow(2_f64, (25 - self.z() as i32) as f64)
    }

    ///その空間IDのX方向の長さをメートル単位で計算する関数
    fn length_x_meters(&self) -> f64 {
        let ecef: Ecef = self.spatial_center().into();
        let r = libm::sqrt(ecef.x() * ecef.x() + ecef.y() * ecef.y());
        r * 2.0 * core::f64::consts::PI / ((1_u64 << self.z()) as f64)
    }

    ///その空間IDのY方向の長さをメートル単位で計算する関数
    fn length_y_meters(&self) -> f64 {
        let ecef: Ecef = self.spatial_center().into();
        let r = libm::sqrt(ecef.x() * ecef.x() + ecef.y() * ecef.y());
        r * 2.0 * core::f64::consts::PI / ((1_u64 << self.z()) as f64)
    }

    fn as_single_id(&self) -> Option<SingleId> {
        Some(*self)
    }

    fn seconds_range(&self) -> (u64, u64) {
        let unit = self.i.seconds();
        let t = self.t();
        (t * unit, (t + 1) * unit)
    }

    fn time_cleared(self) -> Self {
        self.time_cleared()
    }

    fn spatial_parent_at_zoom(&self, target_z: u8) -> Result<Self, Error> {
        self.spatial_parent_at_zoom(target_z)
    }

    fn flex_ids(self) -> impl Iterator<Item = crate::FlexId> {
        self.flex_ids()
    }

    fn single_ids(self) -> impl Iterator<Item = SingleId> {
        self.single_ids()
    }
}

/// 文字列表現から [`SingleId`] を復元する。
///
/// 形式は [`Display`](core::fmt::Display) が出力する `"{z}/{f}/{x}/{y}"`
/// で、時間を持つ場合のみ末尾に `_{i}/{t}` を付ける（`{t}` は単数）。
///
/// ```
/// # use spatial_id::SingleId;
/// let id: SingleId = "5/3/2/10".parse().unwrap();
/// assert_eq!(id.z(), 5);
/// assert_eq!(id.f(), 3);
/// assert_eq!(id.x(), 2);
/// assert_eq!(id.y(), 10);
/// ```
impl FromStr for SingleId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (body, temporal_text) = match s.split_once('_') {
            Some((body, temporal_text)) => (body, Some(temporal_text)),
            None => (s, None),
        };

        let mut parts = body.split('/');
        let z_text = parts.next().ok_or_else(|| parse_error(s))?;
        let f_text = parts.next().ok_or_else(|| parse_error(s))?;
        let x_text = parts.next().ok_or_else(|| parse_error(s))?;
        let y_text = parts.next().ok_or_else(|| parse_error(s))?;
        if parts.next().is_some() {
            return Err(parse_error(s));
        }

        let z = z_text.parse::<u8>().map_err(|_| parse_error(s))?;
        let f = f_text.parse::<i32>().map_err(|_| parse_error(s))?;
        let x = x_text.parse::<u32>().map_err(|_| parse_error(s))?;
        let y = y_text.parse::<u32>().map_err(|_| parse_error(s))?;

        let id = SingleId::new(z, f, x, y)?;

        // 時間部分は仕様の `{i}/{t}`。`{t}` は単数なので `:` を含む範囲表記は受け付けない。
        match temporal_text {
            None => Ok(id),
            Some(text) => {
                let (i_text, t_text) = text.split_once('/').ok_or_else(|| parse_error(s))?;
                if t_text.contains(':') {
                    return Err(parse_error(s));
                }
                let i = i_text.parse::<i64>().map_err(|_| parse_error(s))?;
                let t = t_text.parse::<u64>().map_err(|_| parse_error(s))?;
                id.time_with(i, t)
            }
        }
    }
}

/// [`SingleId`] の文字列表現として解釈できないことを表すエラーを生成する。
fn parse_error(input: &str) -> Error {
    SpatialIdError::ParseSpatialIdFormat {
        kind: "SingleId",
        input: input.to_string(),
    }
    .into()
}
