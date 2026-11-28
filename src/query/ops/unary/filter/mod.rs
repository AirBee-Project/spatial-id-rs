pub(crate) mod query;

use crate::query::{SafeValue, query_iter::QueryIter, traits::UnaryOperator};
use crate::{Error, FlexId, RangeId};
use alloc::vec::Vec;

/// 条件に一致する要素だけを残す単項演算子。
pub(crate) struct Filter<F> {
    f: F,
    #[cfg(feature = "rayon")]
    parallel: bool,
}

impl<F> Filter<F> {
    pub(crate) fn new(f: F) -> Self {
        Self {
            f,
            #[cfg(feature = "rayon")]
            parallel: false,
        }
    }

    #[cfg(feature = "rayon")]
    pub(crate) fn new_parallel(f: F) -> Self {
        Self { f, parallel: true }
    }
}

impl<V, F> UnaryOperator<V> for Filter<F>
where
    V: SafeValue + 'static,
    F: Fn(FlexId, &V) -> bool + Send + Sync + 'static,
{
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }

    fn run<'a>(&'a self, input: QueryIter<'a, V>) -> Result<QueryIter<'a, V>, Error> {
        #[cfg(feature = "rayon")]
        if self.parallel {
            return Ok(input
                .par_filter_map_ordered(move |id, value| (self.f)(id, &value).then_some(value)));
        }

        Ok(QueryIter::new(input.filter(move |item| match item {
            Ok((id, value)) => (self.f)(*id, value),
            Err(_) => true,
        })))
    }

    fn reverse_target(&self, bounds: RangeId, out: &mut Vec<RangeId>) {
        out.push(bounds);
    }
}
