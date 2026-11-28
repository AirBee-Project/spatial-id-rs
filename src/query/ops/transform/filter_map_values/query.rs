use crate::Error;
use crate::query::Query;
use crate::query::SafeValue;

impl<V: SafeValue + 'static> Query<V> {
    /// 各値を `f` に渡し、[`Some`] なら値を写し、[`None`] なら取り除く。値は別の型へ写せる。
    ///
    /// 動作例:
    /// ```
    /// # use spatial_id::{SingleId, collection::mem::SpatialIdMap, query::Source};
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
}
