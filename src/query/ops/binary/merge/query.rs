use super::Merge;
use crate::query::SafeValue;
use crate::query::merge_policy::MergePolicy;
use crate::query::{Node, Query};
use alloc::boxed::Box;

impl<V: SafeValue + 'static> Query<V> {
    /// 重なる部分を `P` で解決し、片側にしかない部分はそのまま残す。
    pub fn merge<P: MergePolicy<V>>(self, other: impl Into<Self>, _policy: P) -> Self {
        self.binary_merge(other.into(), Merge::<V, P>::new(None))
    }

    #[cfg(feature = "unstable")]
    /// [`merge`](Self::merge) と同じだが、片側にしかない部分も、もう片側を `default` とみなして `P` で解決する。
    pub fn merge_with_default<P: MergePolicy<V>>(
        self,
        other: impl Into<Self>,
        default: V,
        _policy: P,
    ) -> Self {
        self.binary_merge(other.into(), Merge::<V, P>::new(Some(default)))
    }

    fn binary_merge<P: MergePolicy<V>>(self, other: Self, op: Merge<V, P>) -> Self {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        if matches!(other.0, Node::Error(_)) {
            return other;
        }
        Query(Node::Binary(Box::new(op), Box::new(self), Box::new(other)))
    }
}
