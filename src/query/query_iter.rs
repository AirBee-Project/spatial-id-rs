use crate::query::SafeValue;
use crate::{Error, FlexId, SpatialIdMap};
use alloc::boxed::Box;
#[cfg(feature = "rayon")]
use alloc::vec::Vec;

/// 演算子の間を流れる `(FlexId, V)` の列。FlexId の Z-order 列である。
pub struct QueryIter<'a, V>(Box<dyn Iterator<Item = Result<(FlexId, V), Error>> + Send + 'a>);

impl<'a, V> QueryIter<'a, V> {
    /// 制約を満たす `iter` を包む。制約を守るのは呼び出し側の責任。
    pub fn from_ordered(
        iter: impl Iterator<Item = Result<(FlexId, V), Error>> + Send + 'a,
    ) -> Self {
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
    #[cfg(not(feature = "rayon"))]
    pub(crate) fn filter_map_ordered<U: Send + 'a>(
        self,
        f: impl Fn(FlexId, V) -> Option<U> + Send + Sync + 'a,
    ) -> QueryIter<'a, U>
    where
        V: Send + 'a,
    {
        QueryIter::from_ordered(self.filter_map(move |item| match item {
            Ok((id, value)) => f(id, value).map(|value| Ok((id, value))),
            Err(e) => Some(Err(e)),
        }))
    }

    /// 要素ごとに独立な変換 `f` を、順序を保ったまま適用する。[`None`] を返した要素は取り除く。
    ///
    /// 先頭の数十件で変換 1 件あたりの所要時間を測り、以降は倍々に大きくする塊ごとに、
    /// 変換に見込む時間がスレッドを起こす費用を十分に上回る塊だけを並列で処理する。
    /// 逐次の塊は 1 件ずつ遅延で流し、バッファを持たない。
    #[cfg(feature = "rayon")]
    pub(crate) fn filter_map_ordered<U: Send + 'a>(
        self,
        f: impl Fn(FlexId, V) -> Option<U> + Send + Sync + 'a,
    ) -> QueryIter<'a, U>
    where
        V: Send + 'a,
    {
        use core::time::Duration;
        use rayon::prelude::*;
        use std::time::Instant;

        // 先頭を小さく読み、消費側が途中で打ち切っても読み過ぎないようにする
        const FIRST_CHUNK: usize = 64;
        // 眠っているワーカーを起こして塊を配るだけで 1 ms 近くかかることがあるので、塊の変換がその倍に満たなければ逐次で処理する
        const PAR_MIN_WORK: Duration = Duration::from_millis(2);
        // 起こす費用を薄めるため塊は大きく取るが、一度に持つ件数はこの数までに抑える
        const MAX_CHUNK: usize = 1 << 16;

        /// エラーの後ろは消費側が読まないことが多いので、エラーで塊を閉じて先読みしない。
        fn read_chunk<T>(
            input: &mut impl Iterator<Item = Result<T, Error>>,
            n: usize,
        ) -> Vec<Result<T, Error>> {
            let mut chunk = Vec::with_capacity(n);
            for item in input.take(n) {
                let is_err = item.is_err();
                chunk.push(item);
                if is_err {
                    break;
                }
            }
            chunk
        }

        let apply = move |item: Result<(FlexId, V), Error>| match item {
            Ok((id, value)) => f(id, value).map(|value| Ok((id, value))),
            Err(e) => Some(Err(e)),
        };

        let mut input = self.fuse();
        let mut ready = Vec::new().into_iter();
        let mut per_item: Option<Duration> = None;
        let mut limit = FIRST_CHUNK;
        let mut seq_left = 0;

        QueryIter::from_ordered(core::iter::from_fn(move || {
            loop {
                if let Some(item) = ready.next() {
                    return Some(item);
                }
                if seq_left > 0 {
                    seq_left -= 1;
                    match apply(input.next()?) {
                        Some(item) => return Some(item),
                        None => continue,
                    }
                }

                let Some(cost) = per_item else {
                    let chunk = read_chunk(&mut input, FIRST_CHUNK);
                    let len = chunk.len() as u32;
                    if len == 0 {
                        return None;
                    }
                    let started = Instant::now();
                    ready = chunk
                        .into_iter()
                        .filter_map(&apply)
                        .collect::<Vec<_>>()
                        .into_iter();
                    per_item = Some(started.elapsed() / len);
                    continue;
                };
                if cost * (MAX_CHUNK as u32) < PAR_MIN_WORK {
                    return input.by_ref().find_map(&apply);
                }

                limit = (limit * 2).min(MAX_CHUNK);
                if cost * limit as u32 >= PAR_MIN_WORK {
                    let chunk = read_chunk(&mut input, limit);
                    if chunk.is_empty() {
                        return None;
                    }
                    ready = chunk
                        .into_par_iter()
                        .filter_map(&apply)
                        .collect::<Vec<_>>()
                        .into_iter();
                } else {
                    seq_left = limit;
                }
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
        Self::from_ordered(table.into_iter().map(Ok))
    }
}
