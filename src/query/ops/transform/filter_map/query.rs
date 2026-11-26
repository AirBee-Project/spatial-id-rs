use super::{FilterMap, TryFilterMap};
use crate::query::SafeValue;
use crate::query::{Node, Query};
use crate::{Error, FlexId};
use alloc::boxed::Box;

impl<V: SafeValue + 'static> Query<V> {
    /// 各要素の [`FlexId`] と値を `f` に渡し、[`Some`] なら値を写し、[`None`] なら取り除く。
    /// [`FlexId`] は変えられない。値は別の型へ写せる。
    ///
    /// `f` に渡る [`FlexId`] は、同じ値を持つ領域の断片である。切れ方は木の状態や
    /// [`get`](Self::get) に渡す範囲によって変わるので、`f` の結果が切れ方に左右されないようにする。
    /// 値だけを見るなら [`filter_map_values`](Self::filter_map_values) を使う。
    pub fn filter_map<U, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        F: Fn(FlexId, V) -> Option<U> + Send + Sync + 'static,
    {
        Query(Node::Source(Box::new(FilterMap::new(self, f))))
    }

    /// 各要素の [`FlexId`] と値を `f` に渡し、`Ok(Some(_))` なら値を写し、`Ok(None)` なら取り除く。
    /// [`Err`] を返した時点で変換を打ち切り、そのエラーを評価結果として返す。
    /// [`FlexId`] の扱いは [`filter_map`](Self::filter_map) と同じ。
    pub fn try_filter_map<U, E, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        E: Into<Error>,
        F: Fn(FlexId, V) -> Result<Option<U>, E> + Send + Sync + 'static,
    {
        Query(Node::Source(Box::new(TryFilterMap::new(
            self,
            move |id, value| f(id, value).map_err(Into::into),
        ))))
    }

    /// 各要素の [`FlexId`] と値を `f` で写す。[`FlexId`] の扱いは [`filter_map`](Self::filter_map) と同じ。
    pub fn map<U, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        F: Fn(FlexId, V) -> U + Send + Sync + 'static,
    {
        self.filter_map(move |id, value| Some(f(id, value)))
    }
}
