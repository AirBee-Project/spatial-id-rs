use super::{FilterValues, ValuePredicate};
use crate::query::SafeValue;
use crate::query::{Node, Query};

impl<V: SafeValue + Ord + 'static> Query<V> {
    /// 指定した値を持つ空間IDだけを残す。
    ///
    /// 中心ほど値が大きい山（左）から、値が 70 の空間IDだけを残した例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![filter_eq の入力と出力][query_filter_eq]
    #[doc = doc_image!("query_filter_eq", "docs/images/query_filter_eq.jpg")]
    pub fn filter_eq(self, value: V) -> Self {
        self.filter_by(ValuePredicate::Equals(value))
    }

    /// 範囲に入る空間IDだけを残す。
    ///
    /// 中心ほど値が大きい山（左）から、値が 40 以上 80 未満の空間IDだけを残した例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![filter_in の入力と出力][query_filter_in]
    #[doc = doc_image!("query_filter_in", "docs/images/query_filter_in.jpg")]
    pub fn filter_in<R>(self, range: R) -> Self
    where
        R: core::ops::RangeBounds<V>,
        V: Clone,
    {
        self.filter_by(ValuePredicate::InRange(
            range.start_bound().cloned(),
            range.end_bound().cloned(),
        ))
    }

    /// 範囲に入る空間IDを取り除く（範囲外だけを残す）。
    ///
    /// 中心ほど値が大きい山（左）から、値が 40 以上 80 未満の空間IDを取り除いた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![filter_not_in の入力と出力][query_filter_not_in]
    #[doc = doc_image!("query_filter_not_in", "docs/images/query_filter_not_in.jpg")]
    pub fn filter_not_in<R>(self, range: R) -> Self
    where
        R: core::ops::RangeBounds<V>,
        V: Clone,
    {
        self.filter_by(ValuePredicate::NotInRange(
            range.start_bound().cloned(),
            range.end_bound().cloned(),
        ))
    }

    fn filter_by(self, predicate: ValuePredicate<V>) -> Self {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        self.wrap_unary(FilterValues::new(predicate))
    }
}
