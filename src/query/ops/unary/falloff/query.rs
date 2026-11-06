use super::{Falloff, FalloffAxis, FalloffPattern};
use crate::query::Node;
use crate::query::SafeValue;
use crate::query::{Query, merge_policy::MergePolicy};
use core::convert::TryFrom;
use core::ops::{Add, Div, Mul, RangeInclusive, Rem, Sub};

impl<V: SafeValue + 'static> Query<V> {
    fn falloff<Z: Into<u8>, P>(
        self,
        axis: FalloffAxis,
        z: Z,
        range: RangeInclusive<i32>,
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
        match Falloff::<P>::new(axis, z, range, pattern) {
            Ok(op) => self.wrap_unary(op),
            Err(e) => Query(Node::Error(e)),
        }
    }

    /// F方向へ値を減らしながら広げる。
    ///
    /// `range` はズーム `z` での広げる距離の範囲で、0を含む必要がある。
    /// 負の側は `-range.start()`、正の側は `range.end()` の距離で値が0になる。
    /// 例えば `-10..=10` は両方向へ、`0..=10` は正の方向だけへ広げる。
    ///
    /// 値 100 の板（左）から、F 方向へ値を減らしながら広げた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![falloff_f の入力と出力][query_falloff_f]
    #[doc = doc_image!("query_falloff_f", "docs/images/query_falloff_f.jpg")]
    pub fn falloff_f<Z: Into<u8>, P>(
        self,
        z: Z,
        range: RangeInclusive<i32>,
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
        self.falloff::<Z, P>(FalloffAxis::F, z, range, pattern)
    }

    /// X方向へ値を減らしながら広げる。
    ///
    /// `range` はズーム `z` での広げる距離の範囲で、0を含む必要がある。
    /// 負の側は `-range.start()`、正の側は `range.end()` の距離で値が0になる。
    /// 例えば `-10..=10` は両方向へ、`0..=10` は正の方向だけへ広げる。
    ///
    /// 値 100 の板（左）から、X 方向へ値を減らしながら広げた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![falloff_x の入力と出力][query_falloff_x]
    #[doc = doc_image!("query_falloff_x", "docs/images/query_falloff_x.jpg")]
    pub fn falloff_x<Z: Into<u8>, P>(
        self,
        z: Z,
        range: RangeInclusive<i32>,
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
        self.falloff::<Z, P>(FalloffAxis::X, z, range, pattern)
    }

    /// Y方向へ値を減らしながら広げる。
    ///
    /// `range` はズーム `z` での広げる距離の範囲で、0を含む必要がある。
    /// 負の側は `-range.start()`、正の側は `range.end()` の距離で値が0になる。
    /// 例えば `-10..=10` は両方向へ、`0..=10` は正の方向だけへ広げる。
    ///
    /// 値 100 の板（左）から、Y 方向へ値を減らしながら広げた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![falloff_y の入力と出力][query_falloff_y]
    #[doc = doc_image!("query_falloff_y", "docs/images/query_falloff_y.jpg")]
    pub fn falloff_y<Z: Into<u8>, P>(
        self,
        z: Z,
        range: RangeInclusive<i32>,
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
        self.falloff::<Z, P>(FalloffAxis::Y, z, range, pattern)
    }
}
