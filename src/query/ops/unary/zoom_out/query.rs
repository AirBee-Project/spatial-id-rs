use super::ZoomOut;
use crate::ZoomLevel;
use crate::query::SafeValue;
use crate::query::merge_policy::MergePolicy;
use crate::query::{Node, Query};

impl<V: SafeValue + 'static> Query<V> {
    /// 指定されたズームレベルまで情報を落とし、複数の子 FlexId を [`MergePolicy::resolve_many`] で一括マージする単項演算子。
    ///
    /// ズームレベル 21 の値（左）を、ズームレベル 19 の平均（[`Average`](crate::query::merge_policy::Average)）にまとめた例（右）。色は値の大きさを表し、赤ほど大きく、青ほど小さい。
    ///
    /// ![zoom_out の入力と出力][query_zoom_out]
    #[doc = doc_image!("query_zoom_out", "docs/images/query_zoom_out.jpg")]
    pub fn zoom_out<P: MergePolicy<V>, Z: Into<u8>>(self, target_z: Z, _policy: P) -> Self {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        match ZoomLevel::new(target_z.into()) {
            Ok(v) => {
                let op = ZoomOut::<V, P>::new(v);
                self.wrap_unary(op)
            }
            Err(e) => Query(Node::Error(e)),
        }
    }
}
