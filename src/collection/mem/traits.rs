use crate::collection::core::FlexTreeCore;
use crate::query::CancellationToken;
use crate::query::SafeValue;
use crate::query::{QueryIter, Source};
use crate::{Error, FlexId, RangeId, SpatialIdMap, SpatialIdSet};

impl Source for SpatialIdSet {
    type Value = ();

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        _token: &CancellationToken,
    ) -> Result<QueryIter<'a, ()>, Error> {
        Ok(read(&self.inner, targets))
    }
}

impl<V> Source for SpatialIdMap<V>
where
    V: SafeValue + 'static,
{
    type Value = V;

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        _token: &CancellationToken,
    ) -> Result<QueryIter<'a, V>, Error> {
        Ok(read(&self.inner, targets))
    }
}

fn read<'a, V: SafeValue + 'static>(
    tree: &'a FlexTreeCore<V>,
    targets: &[RangeId],
) -> QueryIter<'a, V> {
    let item = |(id, value): (FlexId, &V)| Ok((id, value.clone()));
    // 全体が対象なら、Node ごとの交差判定を省いて全件を辿る
    if targets.contains(&RangeId::WHOLE) {
        QueryIter::new(tree.iter().map(item))
    } else {
        QueryIter::new(tree.get_overlapping(targets.iter().copied()).map(item))
    }
}
