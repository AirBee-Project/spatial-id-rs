use alloc::string::{FromUtf8Error, String};
use alloc::vec::Vec;
use core::array::TryFromSliceError;
use core::convert::Infallible;

use crate::DiskError;

/// ディスクに書ける値。
///
/// 木は値が等しいかをエンコードしたバイト列で判定するので、等しい値は必ず同じバイト列にエンコードすること。
pub trait DiskValue: Sized {
    /// [`decode`](Self::decode) が失敗したときのエラー。
    type Error: core::error::Error + Send + Sync + 'static;

    /// 固定長ならその長さ。[`None`] なら、長さもページに書く。
    const FIXED_SIZE: Option<usize> = None;

    /// エンコードしたバイト列の長さ。[`FIXED_SIZE`](Self::FIXED_SIZE) が [`Some`] なら呼ばれず、その長さを使う。
    fn encoded_len(&self) -> usize;

    /// エンコードしたバイト列の長さ（[`FIXED_SIZE`](Self::FIXED_SIZE) か [`encoded_len`](Self::encoded_len)）の `out` にエンコードする。
    fn encode(&self, out: &mut [u8]);

    /// [`encode`](Self::encode) の逆。
    fn decode(bytes: &[u8]) -> Result<Self, Self::Error>;
}

macro_rules! impl_fixed {
    ($($t:ty),*) => {$(
        impl DiskValue for $t {
            type Error = TryFromSliceError;

            const FIXED_SIZE: Option<usize> = Some(size_of::<$t>());

            fn encoded_len(&self) -> usize {
                size_of::<$t>()
            }

            fn encode(&self, out: &mut [u8]) {
                out.copy_from_slice(&self.to_be_bytes());
            }

            fn decode(bytes: &[u8]) -> Result<Self, Self::Error> {
                Ok(<$t>::from_be_bytes(bytes.try_into()?))
            }
        }
    )*};
}

impl_fixed!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64);

impl DiskValue for bool {
    type Error = DiskError;

    const FIXED_SIZE: Option<usize> = Some(1);

    fn encoded_len(&self) -> usize {
        1
    }

    fn encode(&self, out: &mut [u8]) {
        out[0] = u8::from(*self);
    }

    fn decode(bytes: &[u8]) -> Result<Self, Self::Error> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(DiskError::Corrupt),
        }
    }
}

impl DiskValue for Vec<u8> {
    type Error = Infallible;

    fn encoded_len(&self) -> usize {
        self.len()
    }

    fn encode(&self, out: &mut [u8]) {
        out.copy_from_slice(self);
    }

    fn decode(bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(bytes.to_vec())
    }
}

impl DiskValue for String {
    type Error = FromUtf8Error;

    fn encoded_len(&self) -> usize {
        self.len()
    }

    fn encode(&self, out: &mut [u8]) {
        out.copy_from_slice(self.as_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, Self::Error> {
        String::from_utf8(bytes.to_vec())
    }
}
