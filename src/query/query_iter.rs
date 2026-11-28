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

    /// 要素ごとに独立な変換 `f` をチャンク単位で並列に適用し、順序を保ったまま返す。[`None`] を返した要素は取り除く。
    #[cfg(feature = "rayon")]
    pub(crate) fn par_filter_map_ordered<U: Send + 'a>(
        self,
        f: impl Fn(FlexId, V) -> Option<U> + Send + Sync + 'a,
    ) -> QueryIter<'a, U>
    where
        V: Send + 'a,
    {
        self.par_try_filter_map_ordered(move |id, value| Ok(f(id, value)))
    }

    /// 要素ごとに独立な変換 `f` をチャンク単位で並列に適用し、順序を保ったまま返す。
    /// `Ok(None)` を返した要素は取り除き、最初の [`Err`] で打ち切る。
    #[cfg(feature = "rayon")]
    pub(crate) fn par_try_filter_map_ordered<U: Send + 'a>(
        self,
        f: impl Fn(FlexId, V) -> Result<Option<U>, Error> + Send + Sync + 'a,
    ) -> QueryIter<'a, U>
    where
        V: Send + 'a,
    {
        use alloc::vec::Vec;
        use rayon::prelude::*;

        // 遅延評価と早期打ち切りを保ちつつ並列性を確保するためのチャンクサイズ
        const CHUNK_SIZE: usize = 1024;

        let mut iter = self.fuse();
        let mut buf: alloc::vec::IntoIter<Result<(FlexId, U), Error>> = Vec::new().into_iter();
        let mut done = false;

        QueryIter::new(core::iter::from_fn(move || {
            loop {
                if let Some(next) = buf.next() {
                    return Some(next);
                }
                if done {
                    return None;
                }

                let cap = iter.size_hint().1.unwrap_or(CHUNK_SIZE).min(CHUNK_SIZE);
                let mut chunk = Vec::with_capacity(cap);
                let mut input_err = None;
                for item in iter.by_ref().take(CHUNK_SIZE) {
                    match item {
                        Ok(pair) => chunk.push(pair),
                        Err(e) => {
                            input_err = Some(e);
                            break;
                        }
                    }
                }

                if chunk.is_empty() {
                    done = true;
                    return input_err.map(Err);
                }

                let mut results: Vec<Result<(FlexId, U), Error>> = chunk
                    .into_par_iter()
                    .filter_map(|(id, value)| {
                        f(id, value).map(|opt| opt.map(|u| (id, u))).transpose()
                    })
                    .collect();

                if let Some(err_pos) = results.iter().position(Result::is_err) {
                    results.truncate(err_pos + 1);
                    done = true;
                } else if let Some(e) = input_err {
                    results.push(Err(e));
                    done = true;
                }

                buf = results.into_iter();
            }
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
        Self::new(table.into_iter().flex_ids().map(Ok))
    }
}
