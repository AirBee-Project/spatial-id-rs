pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::Error;
use crate::query::SafeValue;
use crate::query::ops::binary::run_binary;
use crate::query::{merge_policy::MergePolicy, query_iter::QueryIter, traits::BinaryOperator};

/// [`MergePolicy`] で重ね合わせる二項演算子。
///
/// `default` があれば、片側にしかない値も `resolve(a, default)` / `resolve(default, b)` で解決する。
pub struct Merge<V, P> {
    default: Option<V>,
    _marker: core::marker::PhantomData<fn() -> P>,
}

impl<V, P> Merge<V, P> {
    pub fn new(default: Option<V>) -> Self {
        Self {
            default,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<V: SafeValue + 'static, P> BinaryOperator<V> for Merge<V, P>
where
    P: MergePolicy<V>,
{
    fn run<'a>(
        &'a self,
        lhs: QueryIter<'a, V>,
        rhs: QueryIter<'a, V>,
    ) -> Result<QueryIter<'a, V>, Error> {
        let default = &self.default;
        run_binary(
            lhs,
            rhs,
            Some(move |a| match default {
                Some(d) => P::resolve(a, d.clone()),
                None => a,
            }),
            Some(move |b| match default {
                Some(d) => P::resolve(d.clone(), b),
                None => b,
            }),
            Some(P::resolve),
        )
    }
}
