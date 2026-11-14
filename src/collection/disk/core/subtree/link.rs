//! ページをまたぐ操作。部分木を別のページへ移し、Ref で指す。

use core::ops::Range;

use alloc::vec::Vec;
use zerocopy::IntoBytes;

use super::SubTree;
use super::layout::RefNode;
use super::read::Node;
use crate::collection::disk::DiskValue;
use crate::collection::disk::core::address::NodeAddr;
use crate::collection::disk::core::pages::Pages;
use crate::{Error, FlexId, Side};

impl<V: DiskValue> SubTree<V> {
    /// `pos` の Ref の先 `addr` へ、領域 `this` のうち `target` だけに `value` がある Node を重ねる。
    ///
    /// 先のページは書き換えず、写したものに重ねる。同じページを別の場所の Ref も指していることがあるため。
    pub(super) fn insert_through(
        &mut self,
        pos: usize,
        addr: NodeAddr,
        this: &FlexId,
        target: &FlexId,
        value: &[u8],
        pages: &mut Pages<V>,
    ) -> Result<(), Error> {
        let original = pages.copy_node(addr)?;
        let mut child = original.clone();
        child.insert(this, target, value, pages)?;
        // 中身が変わらなければ同じページを指し続ける。新しいページにすると、同じ中身の左右を `join` でまとめられなくなる
        if child.node_bytes() == original.node_bytes() {
            return Ok(());
        }
        self.attach(pos..pos + RefNode::LEN, child, pages)
    }

    /// ページを空けるため、`pos` の Node の中から、大きさが `half` 以下でなるべく大きい部分木を
    /// 別のページへ移す。移せたら true。
    ///
    /// 大きいほうの子へ降りていき、`half` 以下になった子を移す。移せるのは Branch の子だけで、
    /// 移した子は Ref に置き換わる。
    pub(super) fn extract(
        &mut self,
        pos: usize,
        half: usize,
        pages: &mut Pages<V>,
    ) -> Result<bool, Error> {
        match self.node(pos)? {
            Node::Value(_) | Node::Ref(_) => Ok(false),
            Node::Skip { child, .. } => self.extract(child, half, pages),
            Node::Branch { children, .. } => {
                let sizes = [
                    self.end(children[0])? - children[0],
                    self.end(children[1])? - children[1],
                ];
                let side = if sizes[0] >= sizes[1] {
                    Side::Lower
                } else {
                    Side::Upper
                };
                let size = sizes[side as usize];
                let mut moved = false;
                self.update_child(pos, side, |tree, child| {
                    if size > half && tree.extract(child, half, pages)? {
                        moved = true;
                        return Ok(());
                    }
                    // Ref より小さくならないものは移さない。中に移せるものがなければ、`half` を超えていても子ごと移す
                    if size <= RefNode::LEN {
                        return Ok(());
                    }
                    moved = true;
                    let subtree = Self::copy_of(tree, child)?;
                    tree.move_out(child..child + size, subtree, pages)
                })?;
                Ok(moved)
            }
        }
    }

    /// ページの中にある Ref の先。
    pub(in crate::collection::disk::core) fn refs(&self) -> Result<Vec<NodeAddr>, Error> {
        self.ref_positions()?
            .into_iter()
            .map(|pos| Ok(self.prefix::<RefNode>(pos).addr()))
            .collect()
    }

    /// ページの中にある Ref の先を、`map` で付け替える。
    pub(in crate::collection::disk::core) fn patch_refs(
        &mut self,
        map: impl Fn(NodeAddr) -> NodeAddr,
    ) -> Result<(), Error> {
        for pos in self.ref_positions()? {
            let node = self.prefix_mut::<RefNode>(pos);
            node.set_addr(map(node.addr()));
        }
        Ok(())
    }

    /// 領域 `this` の `pos` にある Node のうち、`target` と重なる領域にある Ref の先を、
    /// その領域と一緒に `out` に積む。
    pub(in crate::collection::disk::core) fn refs_toward(
        &self,
        pos: usize,
        this: &FlexId,
        target: &FlexId,
        out: &mut Vec<(NodeAddr, FlexId)>,
    ) -> Result<(), Error> {
        if !this.intersects(target) {
            return Ok(());
        }
        match self.node(pos)? {
            Node::Value(_) => {}
            Node::Ref(addr) => out.push((addr, *this)),
            Node::Branch {
                dimension,
                children,
            } => {
                for (side, child) in [Side::Lower, Side::Upper].into_iter().zip(children) {
                    let half = this.split_on(dimension, side).unwrap();
                    self.refs_toward(child, &half, target, out)?;
                }
            }
            Node::Skip { path, child } => {
                self.refs_toward(child, &path.region(this)?, target, out)?;
            }
        }
        Ok(())
    }

    /// 領域 `this` の `pos` にある Node が表す、値のある [FlexId] と値。Ref の先もたどる。
    #[cfg(test)]
    pub(in crate::collection::disk::core) fn collect(
        &self,
        pos: usize,
        this: &FlexId,
        pages: &Pages<V>,
        out: &mut Vec<(FlexId, V)>,
    ) -> Result<(), Error>
    where
        V::Error: core::fmt::Debug,
    {
        match self.node(pos)? {
            Node::Value(value) => out.push((*this, V::decode(&self.inner[value]).unwrap())),
            Node::Ref(addr) => {
                pages
                    .get(addr.page())
                    .collect(addr.offset(), this, pages, out)?;
            }
            Node::Branch {
                dimension,
                children,
            } => {
                for (side, child) in [Side::Lower, Side::Upper].into_iter().zip(children) {
                    let half = this.split_on(dimension, side).unwrap();
                    self.collect(child, &half, pages, out)?;
                }
            }
            Node::Skip { path, child } => {
                self.collect(child, &path.region(this)?, pages, out)?;
            }
        }
        Ok(())
    }

    /// `range` を部分木 `child` に置き換える。
    ///
    /// 小さければ中身をそのまま取り込み、大きければ別のページに置いて Ref で指す。
    fn attach(
        &mut self,
        range: Range<usize>,
        child: Self,
        pages: &mut Pages<V>,
    ) -> Result<(), Error> {
        if child.node_bytes().len() > Self::MIN_PAGE_NODE {
            return self.move_out(range, child, pages);
        }
        self.splice(range, child.node_bytes())
    }

    /// `range` を、部分木 `child` を置いた別のページを指す Ref に置き換える。
    fn move_out(
        &mut self,
        range: Range<usize>,
        child: Self,
        pages: &mut Pages<V>,
    ) -> Result<(), Error> {
        let split = child.split_dimensions(Self::ROOT);
        let addr = pages.add(child);
        self.splice(range, RefNode::new(addr, split).as_bytes())
    }

    /// ルートの Node のバイト列。
    fn node_bytes(&self) -> &[u8] {
        &self.inner[Self::ROOT..self.len]
    }

    /// ページの中にある Ref の位置。
    fn ref_positions(&self) -> Result<Vec<usize>, Error> {
        let mut out = Vec::new();
        let mut pending = alloc::vec![Self::ROOT];
        while let Some(pos) = pending.pop() {
            match self.node(pos)? {
                Node::Value(_) => {}
                Node::Ref(_) => out.push(pos),
                Node::Branch { children, .. } => pending.extend(children),
                Node::Skip { child, .. } => pending.push(child),
            }
        }
        Ok(out)
    }
}
