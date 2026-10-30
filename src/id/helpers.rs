use alloc::string::String;

use crate::{ALT_ABS_MAX, LON_ABS_MAX};
use core::{f64::consts::PI, fmt};
pub fn scale_range_i32(start: i32, end: i32, scale: i32) -> [i32; 2] {
    [
        start.saturating_mul(scale),
        end.saturating_mul(scale).saturating_add(scale - 1),
    ]
}

pub fn scale_range_u32(start: u32, end: u32, scale: u32) -> [u32; 2] {
    [
        start.saturating_mul(scale),
        end.saturating_mul(scale).saturating_add(scale - 1),
    ]
}

/// 経度 (longitude) を返す（実数 x 対応）
///
/// x: 水平方向のタイル/Segment座標（連続値）  
/// z: ズームレベル  
///
/// Segment番号 x の左端なら x、中心なら x+0.5 を渡せる。
pub fn longitude(x: f64, z: u8) -> f64 {
    let n = libm::pow(2_f64, (z as i32) as f64);
    2.0 * LON_ABS_MAX * (x / n) - LON_ABS_MAX
}

/// 緯度 (latitude) を返す（Web Mercator の逆変換, 実数 y 対応）
///
/// y: 垂直方向のタイル/Segment座標（連続値）  
/// z: ズームレベル  
///
/// 公式: lat = atan( sinh( π * (1 - 2*y/n) ) )
pub fn latitude(y: f64, z: u8) -> f64 {
    let n = libm::pow(2_f64, (z as i32) as f64);
    let t = PI * (1.0 - 2.0 * (y / n));
    let lat_rad = libm::atan(libm::sinh(t));
    lat_rad.to_degrees()
}

/// 高度 (altitude) を返す（実数 f 対応）
///
/// f: 高度方向 index（連続値）  
/// z: ズームレベル  
///
pub fn altitude(f: f64, z: u8) -> f64 {
    let n = libm::pow(2_f64, (z as i32) as f64);
    ALT_ABS_MAX * (f / n)
}

/// 座標軸のどちら側かを表す。
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum Side {
    /// 座標が小さい方 (Left, Down, Low-F)
    Lower = 0,
    /// 座標が大きい方 (Right, Up, High-F)
    Upper = 1,
}

///次元の区間表記の文字列を圧縮するための関数
pub fn format_dimension<T: PartialEq + fmt::Display>(dimension: [T; 2]) -> String {
    if dimension[0] == dimension[1] {
        format!("{}", dimension[0])
    } else {
        format!("{}:{}", dimension[0], dimension[1])
    }
}

/// `[min, max]`の範囲、またはその両端が等しい単一の値のどちらでも受け取れるようにするための変換Trait。
pub trait IntoRange<T> {
    fn into_range(self) -> [T; 2];
}

impl<T: Copy> IntoRange<T> for T {
    fn into_range(self) -> [T; 2] {
        [self, self]
    }
}

impl<T> IntoRange<T> for [T; 2] {
    fn into_range(self) -> [T; 2] {
        self
    }
}
