use crate::Error;
use crate::query::Query;
use crate::query::SafeValue;

impl<V: SafeValue + 'static> Query<V> {
    /// 各値を `f` に渡し、[`Some`] なら値を写し、[`None`] なら取り除く。値は別の型へ写せる。
    ///
    /// 動作例:
    /// ```
    /// # use spatial_id::{SingleId, collection::SpatialIdMap, query::Source};
    /// let mut map = SpatialIdMap::new();
    /// map.insert(SingleId::new(20, 0, 10, 200).unwrap(), 1);
    /// map.insert(SingleId::new(20, 0, 11, 200).unwrap(), 15);
    ///
    /// let out = map
    ///     .query()
    ///     .filter_map_values(|v: i32| (v > 10).then(|| v.to_string()))
    ///     .get(..)
    ///     .into_map()
    ///     .unwrap();
    /// assert_eq!(out.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>(), ["15"]);
    /// ```
    pub fn filter_map_values<U, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        F: Fn(V) -> Option<U> + Send + Sync + 'static,
    {
        self.filter_map(move |_, value| f(value))
    }

    /// 各値を `f` に渡し、`Ok(Some(_))` なら値を写し、`Ok(None)` なら取り除く。
    /// [`Err`] を返した時点で変換を打ち切り、そのエラーを評価結果として返す。
    pub fn try_filter_map_values<U, E, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        E: Into<Error>,
        F: Fn(V) -> Result<Option<U>, E> + Send + Sync + 'static,
    {
        self.try_filter_map(move |_, value| f(value))
    }

    /// 各値を `f` で写す。値は別の型へ写せる。
    pub fn map_values<U, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        F: Fn(V) -> U + Send + Sync + 'static,
    {
        self.filter_map_values(move |value| Some(f(value)))
    }

    /// [`filter_map_values`](Self::filter_map_values) の並列版。
    ///
    /// `rayon` feature が有効なときはチャンク単位で並列に評価し、無効なときは [`filter_map_values`](Self::filter_map_values) と同等に逐次評価する。
    pub fn par_filter_map_values<U, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        F: Fn(V) -> Option<U> + Send + Sync + 'static,
    {
        self.par_filter_map(move |_, value| f(value))
    }

    /// [`try_filter_map_values`](Self::try_filter_map_values) の並列版。
    ///
    /// `rayon` feature が有効なときはチャンク単位で並列に評価し、無効なときは [`try_filter_map_values`](Self::try_filter_map_values) と同等に逐次評価する。
    /// チャンク内で複数の [`Err`] が返された場合は、入力順で最初の [`Err`] を返す。
    pub fn par_try_filter_map_values<U, E, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        E: Into<Error>,
        F: Fn(V) -> Result<Option<U>, E> + Send + Sync + 'static,
    {
        self.par_try_filter_map(move |_, value| f(value))
    }

    /// [`map_values`](Self::map_values) の並列版。
    ///
    /// `rayon` feature が有効なときはチャンク単位で並列に評価し、無効なときは [`map_values`](Self::map_values) と同等に逐次評価する。
    pub fn par_map_values<U, F>(self, f: F) -> Query<U>
    where
        U: SafeValue + 'static,
        F: Fn(V) -> U + Send + Sync + 'static,
    {
        self.par_filter_map_values(move |value| Some(f(value)))
    }
}
