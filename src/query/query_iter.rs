use crate::query::SafeValue;
use crate::{Error, FlexId, SpatialIdMap};
use alloc::boxed::Box;

/// 演算子の間を流れる `(FlexId, V)` の列。FlexId の Z-order 列である。
pub struct QueryIter<'a, V>(Box<dyn Iterator<Item = Result<(FlexId, V), Error>> + Send + 'a>);

impl<'a, V> QueryIter<'a, V> {
    /// 制約を満たす `iter` を包む。制約を守るのは呼び出し側の責任。
    #[cfg(feature = "unstable")]
    pub fn from_ordered(
        iter: impl Iterator<Item = Result<(FlexId, V), Error>> + Send + 'a,
    ) -> Self {
        Self::new(iter)
    }

    /// 制約を満たす `iter` を包む。制約を守るのは呼び出し側の責任。
    pub(crate) fn new(iter: impl Iterator<Item = Result<(FlexId, V), Error>> + Send + 'a) -> Self {
        #[cfg(debug_assertions)]
        let iter = {
            let mut prev: Option<FlexId> = None;
            iter.inspect(move |item| {
                if let Ok((id, _)) = item
                    && let Some(prev) = prev.replace(*id)
                {
                    assert!(
                        prev < *id,
                        "QueryIter の制約違反: {prev:?} の次に {id:?} が来た"
                    );
                }
            })
        };
        Self(Box::new(iter))
    }

    /// 要素ごとに独立な変換 `f` を、順序を保ったまま適用する。[`None`] を返した要素は取り除く。
    pub(crate) fn filter_map_ordered<U: Send + 'a>(
        self,
        f: impl Fn(FlexId, V) -> Option<U> + Send + 'a,
    ) -> QueryIter<'a, U>
    where
        V: Send + 'a,
    {
        QueryIter::new(self.filter_map(move |item| match item {
            Ok((id, value)) => f(id, value).map(|u| Ok((id, u))),
            Err(e) => Some(Err(e)),
        }))
    }

    /// 要素ごとに独立な変換 `f` を、順序を保ったまま適用する。`Ok(None)` を返した要素は取り除き、[`Err`] で打ち切る。
    pub(crate) fn try_filter_map_ordered<U: Send + 'a>(
        self,
        f: impl Fn(FlexId, V) -> Result<Option<U>, Error> + Send + 'a,
    ) -> QueryIter<'a, U>
    where
        V: Send + 'a,
    {
        let mut iter = self.fuse();
        let mut done = false;
        QueryIter::new(core::iter::from_fn(move || {
            if done {
                return None;
            }
            for item in iter.by_ref() {
                let mapped = item
                    .and_then(|(id, value)| Ok(f(id, value)?.map(|u| (id, u))))
                    .transpose();
                if let Some(mapped) = mapped {
                    done = mapped.is_err();
                    return Some(mapped);
                }
            }
            done = true;
            None
        }))
    }
}

impl<V> Iterator for QueryIter<'_, V> {
    type Item = Result<(FlexId, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a, V: SafeValue + 'static> From<SpatialIdMap<V>> for QueryIter<'a, V> {
    fn from(table: SpatialIdMap<V>) -> Self {
        Self::new(table.into_iter_flex_ids().map(Ok))
    }
}
