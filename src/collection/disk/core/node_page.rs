use super::node::{Node, NodeKind};
use crate::DiskError;
use crate::collection::disk::io::{Page, PageId};
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
/// | 15    | 0 （現状は空き）                       |
/// | 14-12 | ルートの Node の種類                   |
/// | 11-0  | ルートの Node のバイト長               |
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct NodePage {
    head: U16<BigEndian>,
    body: [u8; NodePage::BODY_LEN],
}

impl NodePage {
    /// ルートの Node に使えるバイト長。
    pub(super) const BODY_LEN: usize = Page::CAPACITY - 2;

    /// この NodePage から Node を読み出す。
    pub(super) fn decode(&self) -> Result<Node, DiskError> {
        let head = self.head.get();
        let len = usize::from(head & 0xFFF);
        let kind = NodeKind::try_from((head >> 12) as u8 & 0b111)?;
        // split はルートが RefPage のページを作らない。許すと自身を指すページで読み続けてしまう
        if len == 0 || kind == NodeKind::RefPage {
            return Err(DiskError::Corrupt);
        }
        let bytes = self.body.get(..len).ok_or(DiskError::Corrupt)?;
        Node::decode(kind, bytes)
    }

    /// `root` を、`alloc` で得たページに収まるよう分けた `(ルートのページ番号, 書き込むページ列)`。
    pub(super) fn split(
        mut root: Node,
        mut alloc: impl FnMut() -> PageId,
    ) -> (PageId, Vec<(PageId, Box<Self>)>) {
        let mut node_pages = Vec::new();
        Self::fit(&mut root, &mut alloc, &mut node_pages);
        let root_id = alloc();
        node_pages.push((root_id, Self::encode(&root)));
        (root_id, node_pages)
    }

    /// `node` が1ページに収まるよう子孫を別のページへ出し、出したあとのバイト長を返す。
    ///
    /// 子から先に収め、それでも収まらなければ大きいほうの子から出す。
    fn fit(
        node: &mut Node,
        alloc: &mut impl FnMut() -> PageId,
        node_pages: &mut Vec<(PageId, Box<Self>)>,
    ) -> usize {
        let header = node.header_len();
        match node {
            Node::Value(_) | Node::RefPage { .. } => header,
            Node::Skip { child, .. } => match Self::fit(child, alloc, node_pages) {
                len if header + len <= Self::BODY_LEN => header + len,
                _ => header + Self::spill(child, alloc, node_pages),
            },
            Node::Branch { lower, upper, .. } => {
                let mut lens = [
                    Self::fit(lower, alloc, node_pages),
                    Self::fit(upper, alloc, node_pages),
                ];
                let children = [lower, upper];
                while header + lens[0] + lens[1] > Self::BODY_LEN {
                    let i = usize::from(lens[1] > lens[0]);
                    lens[i] = Self::spill(children[i], alloc, node_pages);
                }
                header + lens[0] + lens[1]
            }
        }
    }

    /// 1ページに収まる `node` を別のページへ出して [`Node::RefPage`] に置き換え、置き換えたあとのバイト長を返す。
    fn spill(
        node: &mut Node,
        alloc: &mut impl FnMut() -> PageId,
        node_pages: &mut Vec<(PageId, Box<Self>)>,
    ) -> usize {
        let page_id = alloc();
        let split_dimensions = node.split_dimensions();
        let moved = mem::replace(
            node,
            Node::RefPage {
                page_id,
                split_dimensions,
            },
        );
        node_pages.push((page_id, Self::encode(&moved)));
        node.header_len()
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
