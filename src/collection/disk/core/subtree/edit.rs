//! ページ上の Node を書き換える。

use core::ops::Range;

use zerocopy::{FromBytes, IntoBytes, KnownLayout, Unaligned};

use super::SubTree;
use super::layout::{BranchPrefix, LongSkipPrefix, NodeHeader, ShortSkipPrefix};
use super::read::Node;
use crate::collection::disk::DiskValue;
use crate::id::dimension::DimensionSet;
use crate::id::relative_flex_id::ShortRelativeFlexId;
use crate::{DiskError, Error, FlexId, Side};

impl<V: DiskValue> SubTree<V> {
    /// `range` を、領域 `this` のうち `target` だけに `value` がある Node に置き換える。
    /// `value` はエンコードした Value の Node。
    pub(super) fn put(
        &mut self,
        range: Range<usize>,
        this: &FlexId,
        target: &FlexId,
        value: &[u8],
    ) -> Result<(), Error> {
        let pos = range.start;
        self.splice(range, value)?;
        self.rebase(pos..pos, target, this)
    }

    /// `header` の後ろにある、領域 `from` の Node を、`header` を含めて領域 `to` から見た Node に置き換える。
    /// `header` は張り替える古い Skip の、子より前の部分で、空でもよい。
    ///
    /// `from` が `to` に含まれるか、Node が Skip でその行き先が `to` に含まれること。
    pub(super) fn rebase(
        &mut self,
        header: Range<usize>,
        from: &FlexId,
        to: &FlexId,
    ) -> Result<(), Error> {
        if header.is_empty() && from == to {
            return Ok(());
        }
        let pos = header.start;
        // Skip の先へさらに Skip しないよう、Skip の行き先から直接張り直す
        let (region, header) = match self.node(header.end)? {
            Node::Skip { path, child } => (path.region(from)?, pos..child),
            _ => (*from, header),
        };
        if region == *to {
            return self.resize(header, 0);
        }
        let path = region.relative_to(to)?;
        match ShortRelativeFlexId::new(path) {
            Some(short) => self.splice(header, ShortSkipPrefix::new(short).as_bytes())?,
            None => self.splice(header, LongSkipPrefix::new(path).as_bytes())?,
        }
        self.refresh(pos)
    }

    /// `pos` の Node が持つ、割っている次元の集合を子から求め直す。
    pub(super) fn refresh(&mut self, pos: usize) -> Result<(), Error> {
        let split = match self.node(pos)? {
            Node::Branch {
                dimension,
                children,
            } => children
                .iter()
                .fold(DimensionSet::from(dimension), |split, &child| {
                    split | self.split_dimensions(child)
                }),
            Node::Skip { path, child } => path.deeper_dimensions() | self.split_dimensions(child),
            // Value は割らず、Ref は先が変わるたびに丸ごと置き換えるので古くならない
            Node::Value(_) | Node::Ref(_) => return Ok(()),
        };
        let header = self.prefix_mut::<NodeHeader>(pos);
        *header = header.with_split(split);
        Ok(())
    }

    /// `pos` の Branch の `side` 側の子を `update` で書き換え、下側の子の長さを合わせる。
    ///
    /// `update` は子の位置を受け取る。
    pub(super) fn update_child(
        &mut self,
        pos: usize,
        side: Side,
        update: impl FnOnce(&mut Self, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let Node::Branch { children, .. } = self.node(pos)? else {
            unreachable!("子を持つのは Branch");
        };
        let before = self.len;
        update(self, children[side as usize])?;
        // 書き換わるのは子の中だけなので、全体の長さの変化がそのまま子の長さの変化になる
        if side == Side::Lower {
            let len = self.len;
            let prefix = self.prefix_mut::<BranchPrefix>(pos);
            let lower_len = usize::from(prefix.lower_len.get()) + len - before;
            prefix.lower_len.set(lower_len as u16);
        }
        Ok(())
    }

    /// `range` を `bytes` に置き換え、後ろの Node をずらす。
    pub(super) fn splice(&mut self, range: Range<usize>, bytes: &[u8]) -> Result<(), Error> {
        let start = range.start;
        self.resize(range, bytes.len())?;
        self.inner[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// `range` の長さを `new_len` にし、後ろの Node をずらす。伸ばした部分の中身は不定。
    pub(super) fn resize(&mut self, range: Range<usize>, new_len: usize) -> Result<(), Error> {
        if range.len() == new_len {
            return Ok(());
        }
        let len = self.len + new_len - range.len();
        if len > self.inner.len() {
            return Err(Error::Disk(DiskError::PageFull));
        }
        self.inner
            .copy_within(range.end..self.len, range.start + new_len);
        // ページのチェックサムが中身だけで決まるよう、使わなくなった部分はゼロに戻す
        if len < self.len {
            self.inner[len..self.len].fill(0);
        }
        self.len = len;
        Ok(())
    }

    pub(super) fn prefix_mut<H: FromBytes + IntoBytes + KnownLayout + Unaligned>(
        &mut self,
        pos: usize,
    ) -> &mut H {
        H::mut_from_prefix(&mut self.inner[pos..])
            .expect("Node はページに収まっている")
            .0
    }
}
