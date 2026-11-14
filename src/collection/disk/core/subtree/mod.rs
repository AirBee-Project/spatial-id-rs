//! 1 ページに収める FlexTree の部分木。

mod edit;
mod insert;
mod layout;
mod link;
mod read;

use core::marker::PhantomData;

use alloc::boxed::Box;
use zerocopy::IntoBytes;

use self::layout::{NodeHeader, NodeKind, ValueLen};
use super::pages::Pages;
use crate::collection::disk::DiskValue;
use crate::collection::disk::io::Page;
use crate::id::dimension::DimensionSet;
use crate::{DiskError, Error, FlexId};

/// 1 ページに収まる部分木。ページの中身そのもので、ファイルにはこのまま書く。
pub(super) struct SubTree<V> {
    inner: [u8; Page::CAPACITY],
    /// `inner` のうち使っている長さ。
    len: usize,
    _value: PhantomData<fn() -> V>,
}

impl<V> Clone for SubTree<V> {
    fn clone(&self) -> Self {
        SubTree {
            inner: self.inner,
            len: self.len,
            _value: PhantomData,
        }
    }
}

impl<V: DiskValue> SubTree<V> {
    /// ルートの Node が始まる位置。
    pub(super) const ROOT: usize = 0;

    /// これより小さい部分木は別のページに分けず、親のページに取り込む。
    const MIN_PAGE_NODE: usize = Page::CAPACITY / 16;

    /// 領域 `this` のうち `target` だけに `value` がある部分木。`value` は [`encode`](Self::encode) したもの。
    pub(super) fn single(this: &FlexId, target: &FlexId, value: &[u8]) -> Result<Self, Error> {
        let mut tree = Self::empty();
        tree.put(Self::ROOT..Self::ROOT, this, target, value)?;
        Ok(tree)
    }

    /// `source` の `pos` から始まる Node をルートとして写した部分木。
    pub(super) fn copy_of(source: &Self, pos: usize) -> Result<Self, Error> {
        let mut tree = Self::empty();
        tree.splice(Self::ROOT..Self::ROOT, &source.inner[pos..source.end(pos)?])?;
        Ok(tree)
    }

    /// 領域 `this` の部分木へ、`target` だけに `value` がある Node を上書きで重ねる。
    ///
    /// ページに収まらなくなったら、中の部分木を別のページへ移してからやり直す。
    pub(super) fn insert(
        &mut self,
        this: &FlexId,
        target: &FlexId,
        value: &[u8],
        pages: &mut Pages<V>,
    ) -> Result<(), Error> {
        loop {
            let before = self.clone();
            match self.insert_at(Self::ROOT, this, target, value, pages) {
                Ok(()) => return Ok(()),
                Err(Error::Disk(DiskError::PageFull)) => {
                    *self = before;
                    if !self.make_room(pages)? {
                        return Err(Error::Disk(DiskError::PageFull));
                    }
                }
                Err(err) => {
                    *self = before;
                    return Err(err);
                }
            }
        }
    }

    /// `value` を Value の Node のバイト列にして `buf` に書き、その部分を返す。
    pub(super) fn encode<'a>(
        value: &V,
        buf: &'a mut [u8; Page::CAPACITY],
    ) -> Result<&'a [u8], Error> {
        let (start, len) = match V::FIXED_SIZE {
            Some(len) => (NodeHeader::LEN, len),
            None => (NodeHeader::LEN + ValueLen::LEN, value.encoded_len()),
        };
        let node = buf
            .get_mut(..start + len)
            .ok_or(Error::Disk(DiskError::PageFull))?;
        node[0] = NodeHeader::new(NodeKind::Value, DimensionSet::EMPTY).as_bytes()[0];
        if V::FIXED_SIZE.is_none() {
            node[NodeHeader::LEN..start].copy_from_slice(ValueLen::new(len).as_bytes());
        }
        value.encode(&mut node[start..]);
        Ok(node)
    }

    /// ページの半分以下でなるべく大きい部分木を、別のページへ移す。移せるものがなければ false。
    fn make_room(&mut self, pages: &mut Pages<V>) -> Result<bool, Error> {
        let half = (self.len - Self::ROOT) / 2;
        self.extract(Self::ROOT, half, pages)
    }

    fn empty() -> Self {
        SubTree {
            inner: [0; Page::CAPACITY],
            len: Self::ROOT,
            _value: PhantomData,
        }
    }
}

impl<V> From<&SubTree<V>> for Box<Page> {
    fn from(tree: &SubTree<V>) -> Self {
        Page::of(&tree.inner)
    }
}

impl<V: DiskValue> TryFrom<&Page> for SubTree<V> {
    type Error = Error;

    fn try_from(page: &Page) -> Result<Self, Self::Error> {
        let mut tree = Self::empty();
        tree.inner = *page.try_head_as::<[u8; Page::CAPACITY]>()?;
        tree.len = tree.end(Self::ROOT)?;
        Ok(tree)
    }
}
