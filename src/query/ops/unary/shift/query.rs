use super::Shift;
use crate::query::SafeValue;
use crate::query::{Node, Query};

impl<V: SafeValue + 'static> Query<V> {
    /// ズームレベル `z` のインデックス値で、F・X・Y 方向へそれぞれ `f`・`x`・`y` 個分だけ平行移動する。
    ///
    /// L 字形の立体（左）を F・X・Y 方向へ動かした例（右。灰色は元の位置）。
    ///
    /// ![shift_fxy の入力と出力][query_shift_fxy]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_shift_fxy", "docs/images/query_shift_fxy.jpg")
    )]
    pub fn shift_fxy<Z: Into<u8>>(self, z: Z, f: i32, x: i32, y: i32) -> Self {
        if matches!(self.0, Node::Error(_)) {
            return self;
        }
        match Shift::new(z, f, x, y) {
            Ok(op) => self.wrap_unary(op),
            Err(e) => Query(Node::Error(e)),
        }
    }

    /// F方向のShift演算を適用する
    ///
    /// L 字形の立体（左）を F 方向へ動かした例（右。灰色は元の位置）。
    ///
    /// ![shift_f の入力と出力][query_shift_f]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_shift_f", "docs/images/query_shift_f.jpg")
    )]
    pub fn shift_f<Z: Into<u8>>(self, z: Z, index: i32) -> Self {
        self.shift_fxy(z, index, 0, 0)
    }

    /// X方向のShift演算を適用する
    ///
    /// L 字形の立体（左）を X 方向へ動かした例（右。灰色は元の位置）。
    ///
    /// ![shift_x の入力と出力][query_shift_x]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_shift_x", "docs/images/query_shift_x.jpg")
    )]
    pub fn shift_x<Z: Into<u8>>(self, z: Z, index: i32) -> Self {
        self.shift_fxy(z, 0, index, 0)
    }

    /// Y方向のShift演算を適用する
    ///
    /// L 字形の立体（左）を Y 方向へ動かした例（右。灰色は元の位置）。
    ///
    /// ![shift_y の入力と出力][query_shift_y]
    #[cfg_attr(
        doc,
        embed_doc_image::embed_doc_image("query_shift_y", "docs/images/query_shift_y.jpg")
    )]
    pub fn shift_y<Z: Into<u8>>(self, z: Z, index: i32) -> Self {
        self.shift_fxy(z, 0, 0, index)
    }
}
