//! ページ上の Node を読む。

use core::ops::Range;

use zerocopy::{FromBytes, Immutable, KnownLayout, Unaligned};

use super::SubTree;
use super::layout::{
    BranchPrefix, LongSkipPrefix, NodeHeader, NodeKind, RefNode, ShortSkipPrefix, ValueLen,
};
use crate::collection::disk::DiskValue;
use crate::collection::disk::core::address::NodeAddr;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};
use crate::{DiskError, Error, FlexId};

/// ページ上の Node 1 つを読んだもの。
pub(super) enum Node {
    /// エンコードした値のバイト列の範囲。
    Value(Range<usize>),
    /// `children` は `[下側, 上側]` の子の位置。
    Branch {
        dimension: Dimension,
        children: [usize; 2],
    },
    Skip {
        path: SkipPath,
        child: usize,
    },
    Ref(NodeAddr),
}

/// Skip の行き先。親の領域からの相対位置。
#[derive(Clone, Copy)]
pub(super) enum SkipPath {
    Long(RelativeFlexId),
    Short(ShortRelativeFlexId),
}

impl SkipPath {
    /// 領域 `this` から見た行き先の領域。
    pub(super) fn region(self, this: &FlexId) -> Result<FlexId, Error> {
        match self {
            SkipPath::Long(path) => path.to_absolute(this),
            SkipPath::Short(path) => Ok(path.to_absolute(this)),
        }
    }

    /// 行き先が親より狭い次元の集合。
    pub(super) fn deeper_dimensions(self) -> DimensionSet {
        match self {
            SkipPath::Long(path) => path.deeper_dimensions(),
            SkipPath::Short(path) => path.deeper_dimensions(),
        }
    }
}

impl<V: DiskValue> SubTree<V> {
    /// `pos` から始まる Node。
    pub(super) fn node(&self, pos: usize) -> Result<Node, Error> {
        Ok(match self.header(pos).kind() {
            NodeKind::Value => {
                let start = pos + NodeHeader::LEN;
                match V::FIXED_SIZE {
                    Some(len) => Node::Value(start..start + len),
                    None => {
                        let start = start + ValueLen::LEN;
                        Node::Value(
                            start..start + self.prefix::<ValueLen>(start - ValueLen::LEN).get(),
                        )
                    }
                }
            }
            NodeKind::Branch(dimension) => {
                let lower = pos + BranchPrefix::LEN;
                let lower_len = usize::from(self.prefix::<BranchPrefix>(pos).lower_len.get());
                Node::Branch {
                    dimension,
                    children: [lower, lower + lower_len],
                }
            }
            NodeKind::LongSkip => Node::Skip {
                path: SkipPath::Long(RelativeFlexId::decode(
                    &self.prefix::<LongSkipPrefix>(pos).path,
                )?),
                child: pos + LongSkipPrefix::LEN,
            },
            NodeKind::ShortSkip => {
                let bits = self.prefix::<ShortSkipPrefix>(pos).path.get();
                Node::Skip {
                    path: SkipPath::Short(
                        ShortRelativeFlexId::from_bits(bits).ok_or(DiskError::Corrupt)?,
                    ),
                    child: pos + ShortSkipPrefix::LEN,
                }
            }
            NodeKind::Ref => Node::Ref(self.prefix::<RefNode>(pos).addr()),
        })
    }

    /// `pos` から始まる Node の終わりの位置。
    pub(super) fn end(&self, pos: usize) -> Result<usize, Error> {
        match self.node(pos)? {
            Node::Value(value) => Ok(value.end),
            Node::Branch { children, .. } => self.end(children[1]),
            Node::Skip { child, .. } => self.end(child),
            Node::Ref(_) => Ok(pos + RefNode::LEN),
        }
    }

    /// `pos` の Node と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self, pos: usize) -> DimensionSet {
        self.header(pos).split()
    }

    pub(super) fn header(&self, pos: usize) -> NodeHeader {
        *self.prefix::<NodeHeader>(pos)
    }

    /// `pos` から始まる Node の、子より前の部分。
    pub(super) fn prefix<H: FromBytes + Immutable + KnownLayout + Unaligned>(
        &self,
        pos: usize,
    ) -> &H {
        H::ref_from_prefix(&self.inner[pos..])
            .expect("Node はページに収まっている")
            .0
    }
}
