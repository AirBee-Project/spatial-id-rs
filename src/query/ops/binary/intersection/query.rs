use super::Intersection;
use crate::query::SafeValue;
use crate::query::{Node, Query};
use alloc::boxed::Box;

impl<V: SafeValue + 'static> Query<V> {
    /// `self` と `other` の両方にある部分だけを残す。
    ///
    /// 青の A と橙の B（左）から、重なる部分だけを残した例（右）。
    ///
    /// ![intersection の入力と出力][query_intersection]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image(
            "query_intersection",
            "docs/images/query_intersection.jpg"
        )
    )]
    pub fn intersection(self, other: impl Into<Self>) -> Self {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        let other = other.into();
        if matches!(other.0, Node::Error(_)) {
            return other;
        }
        let op = Intersection::<V>::new();
        Query(Node::Binary(Box::new(op), Box::new(self), Box::new(other)))
    }
}
