use crate::DiskError;
use alloc::string::String;
use alloc::vec::Vec;

/// [`DiskValue::encode`] が書くバイト列の最大長。
///
/// 値は Node の中にそのまま置き、書き換えのたびにページを丸ごと書き直す。値が大きいほど関係のない値まで書き直し、
/// 1ページに入る Node も減るので、短い値に限る。
pub const MAX_VALUE_LEN: usize = 128;

/// [`FlexTreeCore`](super::FlexTreeCore) に格納できる値。値とバイト列を相互に変換する。
pub trait DiskValue: Sized {
    /// ファイルに記録し、別の型で開こうとしたときに見つけるための名前。型ごとに一意にし、後から変えないこと。
    fn type_name() -> String;

    /// 自身のバイト列を `out` の末尾に書く。
    ///
    /// バイト列は [`MAX_VALUE_LEN`] バイト以下にすること。超える値は書き込むときに [`DiskError::ValueTooLarge`] になる。
    ///
    /// 同じ値は必ず同じバイト列にすること。隣り合う領域を1つにまとめるかを、バイト列の一致で決めるため。
    /// 異なるバイト列になると、まとめられずに別々の領域として残る（読み出す値は正しいまま）。
    fn encode(&self, out: &mut Vec<u8>);

    /// [`encode`](Self::encode) の逆。値として不正なバイト列なら [`DiskError::Corrupt`] を返す。
    fn decode(bytes: &[u8]) -> Result<Self, DiskError>;
}

impl DiskValue for u8 {
    fn type_name() -> String {
        "u8".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(u8::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for u16 {
    fn type_name() -> String {
        "u16".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(u16::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for u32 {
    fn type_name() -> String {
        "u32".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(u32::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for u64 {
    fn type_name() -> String {
        "u64".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(u64::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for u128 {
    fn type_name() -> String {
        "u128".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(u128::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for i8 {
    fn type_name() -> String {
        "i8".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(i8::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for i16 {
    fn type_name() -> String {
        "i16".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(i16::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for i32 {
    fn type_name() -> String {
        "i32".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(i32::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for i64 {
    fn type_name() -> String {
        "i64".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(i64::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for i128 {
    fn type_name() -> String {
        "i128".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes
            .try_into()
            .map(i128::from_be_bytes)
            .map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for bool {
    fn type_name() -> String {
        "bool".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(DiskError::Corrupt),
        }
    }
}

impl<const N: usize> DiskValue for [u8; N] {
    fn type_name() -> String {
        format!("[u8; {N}]")
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        bytes.try_into().map_err(|_| DiskError::Corrupt)
    }
}

impl DiskValue for Vec<u8> {
    fn type_name() -> String {
        "Vec<u8>".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        Ok(bytes.to_vec())
    }
}

impl DiskValue for String {
    fn type_name() -> String {
        "String".into()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }

    fn decode(bytes: &[u8]) -> Result<Self, DiskError> {
        String::from_utf8(bytes.to_vec()).map_err(|_| DiskError::Corrupt)
    }
}
