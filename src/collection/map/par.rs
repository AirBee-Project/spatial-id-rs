use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::{FlexId, SpatialId, SpatialIdMap};

fn expand<I: SpatialId, V: Clone>((id, value): (I, V)) -> impl Iterator<Item = (FlexId, V)> {
    id.into_iter().map(move |flex_id| (flex_id, value.clone()))
}

/// `(SpatialId, V)` 列から [`SpatialIdMap`] を並列に構築する（`feature = "rayon"`）。
///
/// 同じ空間へ異なる値が重なった場合にどちらが残るかは、チャンクの分かれ方で決まり、
/// 逐次 `insert` の後勝ちとは一致しない（値が衝突しない使い方なら結果は一意）。
impl<I, V> rayon::iter::FromParallelIterator<(I, V)> for SpatialIdMap<V>
where
    I: SpatialId + Send,
    V: PartialEq + Clone + Send + Sync,
{
    fn from_par_iter<T>(par_iter: T) -> Self
    where
        T: IntoParallelIterator<Item = (I, V)>,
    {
        Self {
            inner: par_iter.into_par_iter().flat_map_iter(expand).collect(),
        }
    }
}

/// 既存の [`SpatialIdMap`] へ `(SpatialId, V)` 列を並列にマージする（`feature = "rayon"`）。
///
/// 重なる場所は追加する側の値で上書きされる。
impl<I, V> rayon::iter::ParallelExtend<(I, V)> for SpatialIdMap<V>
where
    I: SpatialId + Send,
    V: PartialEq + Clone + Send + Sync,
{
    fn par_extend<T>(&mut self, par_iter: T)
    where
        T: IntoParallelIterator<Item = (I, V)>,
    {
        self.inner
            .par_extend(par_iter.into_par_iter().flat_map_iter(expand));
    }
}
