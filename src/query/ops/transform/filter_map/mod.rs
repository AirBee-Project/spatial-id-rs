pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::query::SafeValue;
use crate::query::cancellation::CancellationToken;
use crate::query::query_iter::QueryIter;
use crate::query::{Query, traits::Source};
use crate::{Error, FlexId, RangeId};

fn stream_inner<'a, V: SafeValue + 'static>(
    inner: &'a Query<V>,
    targets: &[RangeId],
    token: &CancellationToken,
) -> Result<QueryIter<'a, V>, Error> {
    // `Query::validate` は入力源の中まで辿らないので、内側のクエリはここで検証する
    inner.validate()?;
    inner.stream_within(targets.to_vec(), token)
}

/// `inner` の各 `(FlexId, V)` を `f` で写し、`None` を返した要素を取り除く。
pub(crate) struct FilterMap<V: SafeValue + 'static, U, F> {
    inner: Query<V>,
    f: F,
    #[cfg(feature = "rayon")]
    parallel: bool,
    _marker: core::marker::PhantomData<fn() -> U>,
}

impl<V: SafeValue + 'static, U, F> FilterMap<V, U, F> {
    pub(crate) fn new(inner: Query<V>, f: F) -> Self {
        Self {
            inner,
            f,
            #[cfg(feature = "rayon")]
            parallel: false,
            _marker: core::marker::PhantomData,
        }
    }

    #[cfg(feature = "rayon")]
    pub(crate) fn new_parallel(inner: Query<V>, f: F) -> Self {
        Self {
            inner,
            f,
            parallel: true,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<V, U, F> Source for FilterMap<V, U, F>
where
    V: SafeValue + 'static,
    U: SafeValue + 'static,
    F: Fn(FlexId, V) -> Option<U> + Send + Sync + 'static,
{
    type Value = U;

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        token: &CancellationToken,
    ) -> Result<QueryIter<'a, U>, Error> {
        let stream = stream_inner(&self.inner, targets, token)?;
        #[cfg(feature = "rayon")]
        if self.parallel {
            return Ok(stream.par_filter_map_ordered(&self.f));
        }
        Ok(stream.filter_map_ordered(&self.f))
    }
}

/// `inner` の各 `(FlexId, V)` を `f` で写し、`Ok(None)` を返した要素を取り除き、[`Err`] で打ち切る。
pub(crate) struct TryFilterMap<V: SafeValue + 'static, U, F> {
    inner: Query<V>,
    f: F,
    #[cfg(feature = "rayon")]
    parallel: bool,
    _marker: core::marker::PhantomData<fn() -> U>,
}

impl<V: SafeValue + 'static, U, F> TryFilterMap<V, U, F> {
    pub(crate) fn new(inner: Query<V>, f: F) -> Self {
        Self {
            inner,
            f,
            #[cfg(feature = "rayon")]
            parallel: false,
            _marker: core::marker::PhantomData,
        }
    }

    #[cfg(feature = "rayon")]
    pub(crate) fn new_parallel(inner: Query<V>, f: F) -> Self {
        Self {
            inner,
            f,
            parallel: true,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<V, U, F> Source for TryFilterMap<V, U, F>
where
    V: SafeValue + 'static,
    U: SafeValue + 'static,
    F: Fn(FlexId, V) -> Result<Option<U>, Error> + Send + Sync + 'static,
{
    type Value = U;

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        token: &CancellationToken,
    ) -> Result<QueryIter<'a, U>, Error> {
        let stream = stream_inner(&self.inner, targets, token)?;
        #[cfg(feature = "rayon")]
        if self.parallel {
            return Ok(stream.par_try_filter_map_ordered(&self.f));
        }
        Ok(stream.try_filter_map_ordered(&self.f))
    }
}
