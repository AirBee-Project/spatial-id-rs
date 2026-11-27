use super::node::{Node, NodeKind};
use crate::collection::disk::io::{Page, PageId};
use crate::{DiskError, Error, FlexId};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::mem;
use zerocopy::byteorder::{BigEndian, U16};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout, Unaligned};

/// 1 ページに収まる Node のバイト列。ページの中身そのもので、ファイルにはこのまま書く。
///
/// `head` に続けて、ルートの Node のバイト列を `body` の先頭から置く。
///
/// | bit   | 内容                                   |
/// |-------|----------------------------------------|
/// | 0     | 0 （現状は空き）                       |
/// | 1-3   | ルートの Node の種類                   |
/// | 4-15  | ルートの Node のバイト長               |
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct NodePage {
    head: U16<BigEndian>,
    body: [u8; NodePage::BODY_LEN],
}

impl NodePage {
    /// ルートの Node に使えるバイト長。
    pub(super) const BODY_LEN: usize = Page::CAPACITY - 2;

    /// 領域 `this` のこの NodePage から Node を読み出す。
    pub(super) fn decode(&self, this: &FlexId) -> Result<Node, Error> {
        let head = self.head.get();
        let len = usize::from(head & 0xFFF);
        let kind = NodeKind::try_from((head >> 12) as u8 & 0b111)?;
        // split はルートが RefPage のページを作らない。許すと自身を指すページで読み続けてしまう
        if len == 0 || kind == NodeKind::RefPage {
            return Err(DiskError::Corrupt.into());
        }
        let bytes = self.body.get(..len).ok_or(DiskError::Corrupt)?;
        Node::decode(kind, bytes, this)
    }

    /// `root` を、`alloc` で得たページに収まるよう分けた `(ルートのページ番号, 書き込むページ列)`。
    ///
    /// 収まらない間は、収まる中で一番大きい子孫の Node を別のページへ出して [`Node::RefPage`] に置き換える。
    pub(super) fn split(
        mut root: Node,
        mut alloc: impl FnMut() -> PageId,
    ) -> (PageId, Vec<(PageId, Box<Self>)>) {
        let mut node_pages = Vec::new();
        while root.len() > Self::BODY_LEN {
            let mut node = &mut root;
            while node.len() > Self::BODY_LEN {
                node = match node {
                    Node::Branch { lower, upper, .. } if lower.len() >= upper.len() => lower,
                    Node::Branch { upper, .. } => upper,
                    Node::Skip { child, .. } => child,
                    Node::Value(_) | Node::RefPage { .. } => {
                        unreachable!("Value と RefPage は1ページに収まる")
                    }
                };
            }
            let moved = mem::replace(node, Node::Value(0));
            let page_id = alloc();
            *node = Node::RefPage {
                page_id,
                kind: moved.kind(),
                split_dimensions: moved.split_dimensions(),
            };
            node_pages.push((page_id, Self::encode(&moved)));
        }
        let root_id = alloc();
        node_pages.push((root_id, Self::encode(&root)));
        (root_id, node_pages)
    }

    /// 1 ページに収まる `node` を NodePage へ書き出す。
    fn encode(node: &Node) -> Box<Self> {
        let len = node.len();
        assert!(len <= Self::BODY_LEN);
        let mut node_page = Self::new_box_zeroed().unwrap_or_else(|_| {
            alloc::alloc::handle_alloc_error(core::alloc::Layout::new::<Self>())
        });
        node_page
            .head
            .set(u16::from(node.kind() as u8) << 12 | len as u16);
        let rest = node.write(&mut node_page.body[..len]);
        debug_assert!(rest.is_empty());
        node_page
    }
}
