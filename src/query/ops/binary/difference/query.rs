use super::Difference;
use crate::query::SafeValue;
use crate::query::{Node, Query};
use alloc::boxed::Box;

impl<V: SafeValue + 'static> Query<V> {
    /// `self` から、`other` と重なる部分を取り除く。
    ///
    /// 青の A（左）から、橙の B と重なる部分を取り除いた例（右）。
    ///
    /// ![difference の入力と出力][query_difference]
    #[doc = doc_image!("query_difference", "docs/images/query_difference.jpg")]
    pub fn difference(self, other: impl Into<Self>) -> Self {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        let other = other.into();
        if matches!(other.0, Node::Error(_)) {
            return other;
        }
        let op = Difference::<V>::new();
        Query(Node::Binary(Box::new(op), Box::new(self), Box::new(other)))
    }
}
