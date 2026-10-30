use crate::query::CancellationToken;
use crate::query::SafeValue;
use crate::query::{QueryIter, Source};
use crate::{Error, RangeId, SpatialIdMap, SpatialIdSet};

impl Source for SpatialIdSet {
    type Value = ();

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        _token: &CancellationToken,
    ) -> Result<QueryIter<'a, ()>, Error> {
        Ok(QueryIter::from_ordered(
            self.inner
                .get_overlapping(targets.iter().cloned())
                .map(|(id, ())| Ok((id, ()))),
        ))
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
        Ok(QueryIter::from_ordered(
            self.inner
                .get_overlapping(targets.iter().cloned())
                .map(|(id, value)| Ok((id, value.clone()))),
        ))
    }
}
