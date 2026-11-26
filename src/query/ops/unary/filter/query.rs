use super::Filter;
use crate::FlexId;
use crate::query::{Query, SafeValue};

impl<V: SafeValue + 'static> Query<V> {
    /// 各要素の [`FlexId`] と値を `f` に渡し、`true` を返した要素だけを残す。
    ///
    /// `f` に渡る [`FlexId`] は、同じ値を持つ領域の断片である。切れ方は木の状態や
    /// [`get`](Self::get) に渡す範囲によって変わるので、`f` の結果が切れ方に左右されないようにする。
    /// 値だけを見るなら [`filter_values`](Self::filter_values) を使う。
    pub fn filter<F>(self, f: F) -> Self
    where
        F: Fn(FlexId, &V) -> bool + Send + Sync + 'static,
    {
        self.wrap_unary(Filter::new(f))
    }

    /// 各値を `f` に渡し、`true` を返した要素だけを残す。
    pub fn filter_values<F>(self, f: F) -> Self
    where
        F: Fn(&V) -> bool + Send + Sync + 'static,
    {
        self.filter(move |_, value| f(value))
    }
}
