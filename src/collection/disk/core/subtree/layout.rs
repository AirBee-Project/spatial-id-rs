//! ページ上の形式。Node は行きがけ順に並べ、ページの先頭がルートの Node になる。
//!
//! 各 Node は先頭 1 バイトの [NodeHeader] に、自分の種類と、自身と子孫が割っている次元の集合を持つ。
//! ページはファイル上の位置も、どの領域の部分木かも持たない。Node は親から渡される領域との相対位置で意味を持ち、
//! 同じページを別の領域から指すこともあるため。

use zerocopy::byteorder::{BigEndian, U16, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::collection::disk::core::address::NodeAddr;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};

/// Node の種類。Value・Branch・Skip の分け方はメモリ版の `Node` と同じ。
///
/// - Value: `[NodeHeader][値]`。[`DiskValue::FIXED_SIZE`](crate::collection::disk::DiskValue::FIXED_SIZE)
///   が [`None`] なら、値の前に [ValueLen] を置く。
/// - Branch: `[BranchPrefix][下側の子][上側の子]`
/// - LongSkip: `[LongSkipPrefix][子]`。行き先を [ShortRelativeFlexId] で表せない Skip。
/// - ShortSkip: `[ShortSkipPrefix][子]`
/// - Ref: `[RefNode]`。ページをまたいで Node を指す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeKind {
    Value,
    Branch(Dimension),
    LongSkip,
    ShortSkip,
    Ref,
}

impl NodeKind {
    /// [NodeHeader] に置く 3bit。Branch は割る次元ごとに分ける。
    fn code(self) -> u8 {
        match self {
            NodeKind::Value => 0,
            NodeKind::LongSkip => 1,
            NodeKind::ShortSkip => 2,
            NodeKind::Ref => 3,
            NodeKind::Branch(dimension) => 4 + dimension as u8,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            0 => NodeKind::Value,
            1 => NodeKind::LongSkip,
            2 => NodeKind::ShortSkip,
            3 => NodeKind::Ref,
            _ => NodeKind::Branch(Dimension::ALL[usize::from(code - 4)]),
        }
    }
}

/// Node の先頭 1 バイト。種類（3bit）と、自身と子孫が割っている次元の集合（下位 4bit）。
#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(transparent)]
pub(super) struct NodeHeader(u8);

impl NodeHeader {
    pub(super) const LEN: usize = size_of::<Self>();
    const SPLIT_MASK: u8 = 0b1111;
    const KIND_SHIFT: u32 = 4;

    pub(super) fn new(kind: NodeKind, split: DimensionSet) -> Self {
        NodeHeader(kind.code() << Self::KIND_SHIFT).with_split(split)
    }

    pub(super) fn kind(self) -> NodeKind {
        NodeKind::from_code(self.0 >> Self::KIND_SHIFT & 0b111)
    }

    pub(super) fn split(self) -> DimensionSet {
        zerocopy::transmute!(self.0 & Self::SPLIT_MASK)
    }

    pub(super) fn with_split(self, split: DimensionSet) -> Self {
        let split: u8 = zerocopy::transmute!(split);
        NodeHeader(self.0 & !Self::SPLIT_MASK | split)
    }
}

/// Branch の子より前の部分。この後ろに下側の子、上側の子が続く。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct BranchPrefix {
    header: NodeHeader,
    pub(super) lower_len: U16<BigEndian>,
}

impl BranchPrefix {
    pub(super) const LEN: usize = size_of::<Self>();

    /// 割っている次元の集合は空で作るので、子を置いた後に求め直すこと。
    pub(super) fn new(dimension: Dimension, lower_len: usize) -> Self {
        BranchPrefix {
            header: NodeHeader::new(NodeKind::Branch(dimension), DimensionSet::EMPTY),
            // ページに収まる長さなので u16 に収まる
            lower_len: U16::new(lower_len as u16),
        }
    }
}

/// LongSkip の子より前の部分。この後ろに子が続く。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct LongSkipPrefix {
    header: NodeHeader,
    pub(super) path: [u8; RelativeFlexId::ENCODED_LEN],
}

impl LongSkipPrefix {
    pub(super) const LEN: usize = size_of::<Self>();

    /// 割っている次元の集合は空で作るので、子を置いた後に求め直すこと。
    pub(super) fn new(path: RelativeFlexId) -> Self {
        LongSkipPrefix {
            header: NodeHeader::new(NodeKind::LongSkip, DimensionSet::EMPTY),
            path: path.encode(),
        }
    }
}

/// ShortSkip の子より前の部分。この後ろに子が続く。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct ShortSkipPrefix {
    header: NodeHeader,
    pub(super) path: U16<BigEndian>,
}

impl ShortSkipPrefix {
    pub(super) const LEN: usize = size_of::<Self>();

    /// 割っている次元の集合は空で作るので、子を置いた後に求め直すこと。
    pub(super) fn new(path: ShortRelativeFlexId) -> Self {
        ShortSkipPrefix {
            header: NodeHeader::new(NodeKind::ShortSkip, DimensionSet::EMPTY),
            path: U16::new(path.to_bits()),
        }
    }
}

/// 別の場所（ふつうは別のページ）にある Node を指す Node。
///
/// 割っている次元の集合には、先の Node のものを写しておく。先を読まずに済むように。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct RefNode {
    header: NodeHeader,
    addr: U64<BigEndian>,
}

impl RefNode {
    pub(super) const LEN: usize = size_of::<Self>();

    pub(super) fn new(addr: NodeAddr, split: DimensionSet) -> Self {
        RefNode {
            header: NodeHeader::new(NodeKind::Ref, split),
            addr: U64::new(addr.to_bits()),
        }
    }

    pub(super) fn addr(&self) -> NodeAddr {
        NodeAddr::from_bits(self.addr.get())
    }

    pub(super) fn set_addr(&mut self, addr: NodeAddr) {
        self.addr = U64::new(addr.to_bits());
    }
}

/// 長さが決まっていない値の前に置く、値の長さ。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(transparent)]
pub(super) struct ValueLen(U16<BigEndian>);

impl ValueLen {
    pub(super) const LEN: usize = size_of::<Self>();

    pub(super) fn new(len: usize) -> Self {
        // ページに収まる長さなので u16 に収まる
        ValueLen(U16::new(len as u16))
    }

    pub(super) fn get(&self) -> usize {
        usize::from(self.0.get())
    }
}
