use crate::{SpatialId, SpatialIdSet};

impl<S: SpatialId> FromIterator<S> for SpatialIdSet {
    fn from_iter<T: IntoIterator<Item = S>>(iter: T) -> Self {
        let mut set = SpatialIdSet::new();
        set.extend(iter);
        set
    }
}

impl<S: SpatialId> Extend<S> for SpatialIdSet {
    fn extend<T: IntoIterator<Item = S>>(&mut self, iter: T) {
        for item in iter {
            self.insert(item);
        }
    }
}

/// 空間 ID 列から [`SpatialIdSet`] を並列に構築する（`feature = "rayon"`）。
///
/// [`SingleId`](crate::SingleId) / [`RangeId`](crate::RangeId) / [`FlexId`](crate::FlexId) のいずれの
/// [`SpatialId`] 型でも受け取れる。集合なので結果は挿入順・チャンク境界に依らず
/// 一意に定まる。
///
/// ```
/// use spatial_id::collection::SpatialIdSet;
/// use spatial_id::SingleId;
/// use rayon::prelude::*;
///
/// // 0..1024 は境界整列した連続 X 区間なので 1 異方Segment（x_zoom を 10 段浅く）へ畳まれる。
/// let ids: Vec<SingleId> = (0..1024)
///     .map(|x| SingleId::new(20, 0, x, 0).unwrap())
///     .collect();
/// let set: SpatialIdSet = ids.into_par_iter().collect();
/// assert_eq!(set.count(), 1);
/// ```
#[cfg(feature = "rayon")]
impl<S> rayon::iter::FromParallelIterator<S> for SpatialIdSet
where
    S: crate::SpatialId + Send + Sync,
{
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: rayon::iter::IntoParallelIterator<Item = S>,
    {
        use rayon::prelude::*;
        Self {
            inner: par_iter
                .into_par_iter()
                .flat_map_iter(|id| id.flex_ids().map(|flex_id| (flex_id, ())))
                .collect(),
        }
    }
}

/// 既存の [`SpatialIdSet`] へ空間 ID 列を並列にマージする（`feature = "rayon"`）。
#[cfg(feature = "rayon")]
impl<S> rayon::iter::ParallelExtend<S> for SpatialIdSet
where
    S: crate::SpatialId + Send + Sync,
{
    fn par_extend<I>(&mut self, par_iter: I)
    where
        I: rayon::iter::IntoParallelIterator<Item = S>,
    {
        use rayon::prelude::*;
        self.inner.par_extend(
            par_iter
                .into_par_iter()
                .flat_map_iter(|id| id.flex_ids().map(|flex_id| (flex_id, ()))),
        );
    }
}
