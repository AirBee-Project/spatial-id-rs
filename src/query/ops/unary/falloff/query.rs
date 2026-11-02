use super::{Falloff, FalloffAxis, FalloffPattern};
use crate::id::helpers::Side;
use crate::query::Node;
use crate::query::SafeValue;
use crate::query::{Query, merge_policy::MergePolicy};
use core::convert::TryFrom;
use core::ops::{Add, Div, Mul, Rem, Sub};

impl<V: SafeValue + 'static> Query<V> {
    fn falloff<Z: Into<u8>, P>(
        self,
        axis: FalloffAxis,
        z: Z,
        radius: u32,
        direction: Option<Side>,
        pattern: FalloffPattern,
    ) -> Self
    where
        P: MergePolicy<V> + Send + Sync,
        V: Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>
            + Clone
            + Send
            + Sync,
    {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        match Falloff::<P>::new(axis, z, radius, direction, pattern) {
            Ok(op) => self.wrap_unary(op),
            Err(e) => Query(Node::Error(e)),
        }
    }

    /// F方向へ値を減少させる。
    /// 指定した距離で0になる。
    ///
    /// 値 100 の板（左）から、F 方向へ値を減らしながら広げた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![falloff_f の入力と出力][query_falloff_f]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_falloff_f", "docs/images/query_falloff_f.jpg")
    )]
    pub fn falloff_f<Z: Into<u8>, P>(
        self,
        z: Z,
        radius: u32,
        direction: Option<Side>,
        pattern: FalloffPattern,
        _policy: P,
    ) -> Self
    where
        P: MergePolicy<V> + Send + Sync,
        V: Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>
            + Clone
            + Send
            + Sync,
    {
        self.falloff::<Z, P>(FalloffAxis::F, z, radius, direction, pattern)
    }

    /// X方向へ値を減少させる。
    /// 指定した距離で0になる。
    ///
    /// 値 100 の板（左）から、X 方向へ値を減らしながら広げた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![falloff_x の入力と出力][query_falloff_x]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_falloff_x", "docs/images/query_falloff_x.jpg")
    )]
    pub fn falloff_x<Z: Into<u8>, P>(
        self,
        z: Z,
        radius: u32,
        direction: Option<Side>,
        pattern: FalloffPattern,
        _policy: P,
    ) -> Self
    where
        P: MergePolicy<V> + Send + Sync,
        V: Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>
            + Clone
            + Send
            + Sync,
    {
        self.falloff::<Z, P>(FalloffAxis::X, z, radius, direction, pattern)
    }

    /// Y方向へ値を減少させる。
    /// 指定した距離で0になる。
    ///
    /// 値 100 の板（左）から、Y 方向へ値を減らしながら広げた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![falloff_y の入力と出力][query_falloff_y]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_falloff_y", "docs/images/query_falloff_y.jpg")
    )]
    pub fn falloff_y<Z: Into<u8>, P>(
        self,
        z: Z,
        radius: u32,
        direction: Option<Side>,
        pattern: FalloffPattern,
        _policy: P,
    ) -> Self
    where
        P: MergePolicy<V> + Send + Sync,
        V: Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>
            + Clone
            + Send
            + Sync,
    {
        self.falloff::<Z, P>(FalloffAxis::Y, z, radius, direction, pattern)
    }
}
