use alloc::string::ToString;

use core::fmt;

use crate::{Coordinate, Ecef, Error, FlexId, SpatialId, SpatialIdError, id::helpers};
use core::str::FromStr;

/// [`FlexId`] を文字列形式で表示する。
///
/// 形式は `"{fz}/{fi}|{xz}/{xi}|{yz}/{yi}"`。時間を持つ場合は同じ `|` 区切りで
/// 4軸目 `"|{tz}/{ti}"` が続く。
///
/// **`_` は使わない。** [`SingleId`](crate::SingleId)/[`RangeId`](crate::RangeId)の
/// `_` は仕様の `{z}/{f}/{x}/{y}_{i}/{t}` の区切りで、右側は「秒数/インデックス」だが、
/// [`FlexId`]の時間は「ズーム/インデックス」であり意味が違う。同じ記号を使うと、
/// [`FlexId`] の時間部分を `{i}/{t}` として読み違えても（[`Interval`](crate::Interval) は任意秒数を許すため）
/// エラーにならず気付けない。軸ごとに `|` で並べる本来の形へ揃えることで取り違えを防ぐ。
///
/// ```
/// # use spatial_id::FlexId;
/// let id: FlexId = "5/3|2/3|10/1".parse().unwrap();
/// assert_eq!(id.to_string(), "5/3|2/3|10/1");
/// ```
impl fmt::Display for FlexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}|{}/{}|{}/{}",
            self.f_zoomlevel.get(),
            self.f_index,
            self.x_zoomlevel.get(),
            self.x_index,
            self.y_zoomlevel.get(),
            self.y_index
        )?;

        // 時間軸も他の軸と同じ「ズーム/インデックス」の形で、同じ区切りで続ける。
        if !self.time_is_whole() {
            write!(f, "|{}/{}", self.t_zoomlevel(), self.t_index())?;
        }
        Ok(())
    }
}

impl SpatialId for FlexId {
    fn move_f(&mut self, by: i32) -> Result<(), crate::Error> {
        let new = self.f_index.checked_add(by).ok_or_else(|| {
            Error::from(SpatialIdError::FOutOfRange {
                f: if by >= 0 { i32::MAX } else { i32::MIN },
                z: self.f_zoomlevel.get(),
            })
        })?;

        self.f_zoomlevel.check_f(new)?;

        self.f_index = new;
        Ok(())
    }

    fn move_x(&mut self, by: i32) {
        let max_len = self.x_zoomlevel.xy_max() as i64 + 1;
        let new = (self.x_index as i64 + by as i64).rem_euclid(max_len);
        self.x_index = new as u32;
    }

    fn move_y(&mut self, by: i32) -> Result<(), crate::Error> {
        let new = if by >= 0 {
            self.y_index.checked_add(by as u32).ok_or_else(|| {
                Error::from(SpatialIdError::YOutOfRange {
                    y: u32::MAX,
                    z: self.y_zoomlevel.get(),
                })
            })?
        } else {
            self.y_index
                .checked_sub(by.unsigned_abs())
                .ok_or(SpatialIdError::YOutOfRange {
                    y: 0,
                    z: self.y_zoomlevel.get(),
                })?
        };

        self.y_zoomlevel.check_y(new)?;

        self.y_index = new;

        Ok(())
    }

    fn length_f_meters(&self) -> f64 {
        libm::pow(2_f64, (25 - self.f_zoomlevel() as i32) as f64)
    }

    fn length_x_meters(&self) -> f64 {
        let ecef: Ecef = self.spatial_center().into();
        let r = libm::sqrt(ecef.x() * ecef.x() + ecef.y() * ecef.y());
        r * 2.0 * core::f64::consts::PI / ((1_u64 << self.x_zoomlevel()) as f64)
    }

    fn length_y_meters(&self) -> f64 {
        let ecef: Ecef = self.spatial_center().into();
        let r = libm::sqrt(ecef.x() * ecef.x() + ecef.y() * ecef.y());
        r * 2.0 * core::f64::consts::PI / ((1_u64 << self.y_zoomlevel()) as f64)
    }

    fn spatial_center(&self) -> crate::Coordinate {
        Coordinate::new(
            helpers::latitude(self.y_index as f64 + 0.5, self.y_zoomlevel.get()),
            helpers::longitude(self.x_index as f64 + 0.5, self.x_zoomlevel.get()),
            helpers::altitude(self.f_index as f64 + 0.5, self.f_zoomlevel.get()),
        )
        .unwrap()
    }

    fn spatial_vertices(&self) -> [crate::Coordinate; 8] {
        let xs = [self.x_index as f64, self.x_index as f64 + 1.0];
        let ys = [self.y_index as f64, self.y_index as f64 + 1.0];
        let fs = [self.f_index as f64, self.f_index as f64 + 1.0];

        // 各端点の値を前計算しておく
        let lon2 = [
            helpers::longitude(xs[0], self.x_zoomlevel.get()),
            helpers::longitude(xs[1], self.x_zoomlevel.get()),
        ];
        let lat2 = [
            helpers::latitude(ys[0], self.y_zoomlevel.get()),
            helpers::latitude(ys[1], self.y_zoomlevel.get()),
        ];
        let alt2 = [
            helpers::altitude(fs[0], self.f_zoomlevel.get()),
            helpers::altitude(fs[1], self.f_zoomlevel.get()),
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

    fn as_flex_id(&self) -> Option<FlexId> {
        Some(*self)
    }

    fn time_range(&self) -> (u64, u64) {
        let zoom = crate::id::zoom_level::TZoomLevel::new(self.t_zoomlevel()).unwrap();
        let time_span = crate::time::segment::TimeSegment::new(zoom, self.t_index()).time_span();
        (time_span.start(), time_span.end())
    }

    fn time_clear(&mut self) {
        *self = unsafe { self.time_with_segment_unchecked(0, 0) };
    }

    fn spatial_parent_at_zoom(&self, target_z: u8) -> Result<Self, Error> {
        self.spatial_parent_at_zoom(target_z)
    }

    fn single_ids(self) -> impl Iterator<Item = crate::SingleId> {
        self.single_ids()
    }
}

/// 文字列表現から [`FlexId`] を復元する。
///
/// 形式は [`Display`](core::fmt::Display) が出力する
/// `"{f_zoom}/{f}|{x_zoom}/{x}|{y_zoom}/{y}"`。時間軸があれば4つ目の
/// `"|{t_zoom}/{t}"` も受け付ける。
///
/// ```
/// # use spatial_id::FlexId;
/// let id: FlexId = "5/3|2/3|10/1".parse().unwrap();
/// assert_eq!(id.f_zoomlevel(), 5);
/// assert_eq!(id.f_index(), 3);
/// assert_eq!(id.x_zoomlevel(), 2);
/// assert_eq!(id.x_index(), 3);
/// assert_eq!(id.y_zoomlevel(), 10);
/// assert_eq!(id.y_index(), 1);
/// ```
impl FromStr for FlexId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('|');
        let f_part = parts.next().ok_or_else(|| parse_error(s))?;
        let x_part = parts.next().ok_or_else(|| parse_error(s))?;
        let y_part = parts.next().ok_or_else(|| parse_error(s))?;
        let temporal_text = parts.next();
        if parts.next().is_some() {
            return Err(parse_error(s));
        }

        let (f_zoom_text, f_index_text) = f_part.split_once('/').ok_or_else(|| parse_error(s))?;
        let (x_zoom_text, x_index_text) = x_part.split_once('/').ok_or_else(|| parse_error(s))?;
        let (y_zoom_text, y_index_text) = y_part.split_once('/').ok_or_else(|| parse_error(s))?;

        let f_zoomlevel = f_zoom_text.parse::<u8>().map_err(|_| parse_error(s))?;
        let f_index = f_index_text.parse::<i32>().map_err(|_| parse_error(s))?;
        let x_zoomlevel = x_zoom_text.parse::<u8>().map_err(|_| parse_error(s))?;
        let x_index = x_index_text.parse::<u32>().map_err(|_| parse_error(s))?;
        let y_zoomlevel = y_zoom_text.parse::<u8>().map_err(|_| parse_error(s))?;
        let y_index = y_index_text.parse::<u32>().map_err(|_| parse_error(s))?;

        match temporal_text {
            None => FlexId::new(
                f_zoomlevel,
                f_index,
                x_zoomlevel,
                x_index,
                y_zoomlevel,
                y_index,
            ),
            Some(part) => {
                let (t_zoom_text, t_index_text) =
                    part.split_once('/').ok_or_else(|| parse_error(s))?;
                let t_zoomlevel = t_zoom_text.parse::<u8>().map_err(|_| parse_error(s))?;
                let t_index = t_index_text.parse::<u64>().map_err(|_| parse_error(s))?;
                // 文字列は `{i}/{t}` ではなく「ズーム/インデックス」なので、Segmentの入口を使う。
                FlexId::new(
                    f_zoomlevel,
                    f_index,
                    x_zoomlevel,
                    x_index,
                    y_zoomlevel,
                    y_index,
                )?
                .time_with_segment(t_zoomlevel, t_index)
            }
        }
    }
}

/// [`FlexId`] の文字列表現として解釈できない入力を表すエラーを生成する。
fn parse_error(input: &str) -> Error {
    SpatialIdError::ParseSpatialIdFormat {
        kind: "FlexId",
        input: input.to_string(),
    }
    .into()
}
