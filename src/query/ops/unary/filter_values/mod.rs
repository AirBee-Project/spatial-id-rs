pub(crate) mod query;

use crate::{
    Error, QueryError,
    query::{SafeValue, query_iter::QueryIter, traits::UnaryOperator},
};

use core::ops::Bound;

/// 値に対する宣言的な絞り込み条件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValuePredicate<V> {
    /// この値を持つ FlexId だけを残す。
    Equals(V),
    /// 範囲に入る値の FlexId だけを残す。
    InRange(Bound<V>, Bound<V>),
    /// 範囲に入る値の FlexId を取り除く（範囲外だけを残す）。
    NotInRange(Bound<V>, Bound<V>),
}

impl<V: Ord> ValuePredicate<V> {
    /// 範囲に入るか。
    fn in_range(value: &V, start: &Bound<V>, end: &Bound<V>) -> bool {
        let after_start = match start {
            Bound::Included(s) => value >= s,
            Bound::Excluded(s) => value > s,
            Bound::Unbounded => true,
        };
        let before_end = match end {
            Bound::Included(e) => value <= e,
            Bound::Excluded(e) => value < e,
            Bound::Unbounded => true,
        };
        after_start && before_end
    }

    /// この値の FlexId を残すか。
    fn matches(&self, value: &V) -> bool {
        match self {
            ValuePredicate::Equals(target) => value == target,
            ValuePredicate::InRange(start, end) => Self::in_range(value, start, end),
            ValuePredicate::NotInRange(start, end) => !Self::in_range(value, start, end),
        }
    }
}

/// 値の条件に一致する FlexId だけを残す単項演算子。
///
/// 空間的な形は変えず、条件から外れた空間IDを取り除くだけ。
pub(crate) struct FilterValues<V> {
    predicate: ValuePredicate<V>,
}

impl<V> FilterValues<V> {
    pub(crate) fn new(predicate: ValuePredicate<V>) -> Self {
        Self { predicate }
    }
}

impl<V> UnaryOperator<V> for FilterValues<V>
where
    V: SafeValue + Ord + 'static,
{
    fn validate(&self) -> Result<(), Error> {
        let (start, end) = match &self.predicate {
            ValuePredicate::Equals(_) => return Ok(()),
            ValuePredicate::InRange(s, e) | ValuePredicate::NotInRange(s, e) => (s, e),
        };
        let s_val = match start {
            Bound::Included(v) | Bound::Excluded(v) => Some(v),
            Bound::Unbounded => None,
        };
        let e_val = match end {
            Bound::Included(v) | Bound::Excluded(v) => Some(v),
            Bound::Unbounded => None,
        };
        if let (Some(s), Some(e)) = (s_val, e_val)
            && s > e
        {
            return Err(QueryError::InvalidParameter(
                "value range lower bound is greater than upper bound",
            )
            .into());
        }
        Ok(())
    }

    fn run<'a>(&'a self, input: QueryIter<'a, V>) -> Result<QueryIter<'a, V>, Error> {
        let common_ancestor = input.common_ancestor();
        Ok(QueryIter::from_ordered(input.filter(
            move |item| !matches!(item, Ok((_, value)) if !self.predicate.matches(value)),
        ))
        .with_common_ancestor(common_ancestor))
    }

    fn reverse_target(&self, bounds: crate::RangeId, out: &mut alloc::vec::Vec<crate::RangeId>) {
        out.push(bounds);
    }
}
