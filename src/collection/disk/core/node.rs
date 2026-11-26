// ビットの位置は、先頭バイトの最上位ビットを bit 0 として数える。
use crate::collection::disk::io::PageId;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::id::relative_flex_id::ShortRelativeFlexId;
use crate::{DiskError, Error};
use zerocopy::byteorder::{BigEndian, U16, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

/// 子の Node の種類。3bit。
//  残り5種類を増やす余裕がある。
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromBytes)]
#[repr(u8)]
pub(super) enum NodeKind {
    Value = 0,
    Branch = 1,
    ShortSkip = 2,
    RefPage = 3,
}

impl TryFrom<u8> for NodeKind {
    type Error = Error;
    fn try_from(code: u8) -> Result<Self, Self::Error> {
        zerocopy::try_transmute!(code).map_err(|_| DiskError::Corrupt.into())
    }
}

/// Branch の子より前の部分。この後ろに lower、upper が続く。
///
/// | bit   | 内容                                   |
/// |-------|----------------------------------------|
/// | 0-1   | 割る次元                               |
/// | 2-5   | 自身と子孫が割っている次元の集合       |
/// | 6-8   | lower の子の種類                       |
/// | 9-11  | upper の子の種類                       |
/// | 12-23 | lower の子のバイト長                   |
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(transparent)]
pub(super) struct Branch([u8; 3]);

impl Branch {
    /// Branchから分岐するLowerとUpperのうちLowerの長さを持つことで、Upperをダイレクトに見れるようにしている。
    /// そのLowerの長さを記録するための12Bit。
    const LOWER_LEN_BITS: u32 = 12;

    /// 各値を 3 バイトに詰めた Branch。
    pub(super) fn new(
        dimension: Dimension,
        split_dimensions: DimensionSet,
        lower_kind: NodeKind,
        upper_kind: NodeKind,
        lower_len: usize,
    ) -> Self {
        debug_assert!(lower_len < 1 << Self::LOWER_LEN_BITS);
        let bits = (dimension as u32) << 22
            | u32::from(u8::from(split_dimensions)) << 18
            | (lower_kind as u32) << 15
            | (upper_kind as u32) << 12
            | lower_len as u32;
        let [_, bytes @ ..] = bits.to_be_bytes();
        Branch(bytes)
    }

    /// 3 バイトをビッグエンディアンで読んだ 24bit の値。
    fn bits(&self) -> u32 {
        let [a, b, c] = self.0;
        u32::from_be_bytes([0, a, b, c])
    }

    /// この Branch が割る次元。
    pub(super) fn dimension(&self) -> Dimension {
        Dimension::ALL[(self.bits() >> 22) as usize]
    }

    /// 自身と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        DimensionSet::from_bits((self.bits() >> 18) as u8 & 0xF)
    }

    /// lower の子の種類。
    pub(super) fn lower_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from((self.bits() >> 15) as u8 & 0b111)
    }

    /// upper の子の種類。
    pub(super) fn upper_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from((self.bits() >> 12) as u8 & 0b111)
    }

    /// lower の子のバイト長。この後ろの lower と upper の境目。
    pub(super) fn lower_len(&self) -> usize {
        (self.bits() & ((1 << Self::LOWER_LEN_BITS) - 1)) as usize
    }
}

/// 行き先を [ShortRelativeFlexId] で表せる Skip の、子より前の部分。この後ろに子が続く。
///
/// | bit   | 内容                                   |
/// |-------|----------------------------------------|
/// | 0-15  | 行き先の [ShortRelativeFlexId]         |
/// | 16-20 | 0 （現状は空き）                       |
/// | 21-23 | 子の種類                               |
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct ShortSkip {
    path: U16<BigEndian>,
    child: u8,
}

impl ShortSkip {
    /// 行き先と子の種類を 3 バイトに詰めた ShortSkip。
    pub(super) fn new(path: ShortRelativeFlexId, child_kind: NodeKind) -> Self {
        ShortSkip {
            path: U16::new(path.to_bits()),
            child: child_kind as u8,
        }
    }

    /// 親の領域から見た行き先。
    pub(super) fn path(&self) -> Result<ShortRelativeFlexId, Error> {
        Ok(ShortRelativeFlexId::from_bits(self.path.get()).ok_or(DiskError::Corrupt)?)
    }

    /// 行き先にある子の種類。
    pub(super) fn child_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.child)
    }
}

/// 別のページのルートを指す Node。
///
/// 行き先のルートの種類と分割次元の集合を写しておき、行き先のページを読まずに済ませる。
///
/// | bit   | 内容                                   |
/// |-------|----------------------------------------|
/// | 0     | 0 （現状は空き）                       |
/// | 1-3   | 行き先のルートの種類                   |
/// | 4-7   | 行き先のルートと子孫が割っている次元の集合 |
/// | 8-47  | 行き先の PageId                        |
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct RefPage {
    head: u8,
    page: [u8; RefPage::PAGE_LEN],
}

impl RefPage {
    const PAGE_LEN: usize = 5;

    /// `page` のルートを指す RefPage。`kind`・`split_dimensions` は行き先のルートのもの。
    pub(super) fn new(page: PageId, kind: NodeKind, split_dimensions: DimensionSet) -> Self {
        // 2^40 ページは 4PiB で、実際のファイルでは超えない
        assert!(page.0 >> (8 * Self::PAGE_LEN) == 0);
        RefPage {
            head: (kind as u8) << 4 | u8::from(split_dimensions),
            page: page.0.to_be_bytes()[8 - Self::PAGE_LEN..]
                .try_into()
                .unwrap(),
        }
    }

    /// 行き先のページ。
    pub(super) fn page(&self) -> PageId {
        let mut bytes = [0; 8];
        bytes[8 - Self::PAGE_LEN..].copy_from_slice(&self.page);
        PageId(u64::from_be_bytes(bytes))
    }

    /// 行き先のルートの種類。
    pub(super) fn kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.head >> 4)
    }

    /// 行き先のルートと子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        DimensionSet::from_bits(self.head & 0xF)
    }
}

/// 種類と範囲が決まった、1 つの Node のバイト列。
#[derive(Debug, Clone, Copy)]
pub(super) struct NodeBytes<'a> {
    pub(super) kind: NodeKind,
    pub(super) bytes: &'a [u8],
}

/// [NodeBytes] を読んだもの。子は読まずに [NodeBytes] のまま持つ。
pub(super) enum Node<'a> {
    Value(u64),
    Branch {
        header: &'a Branch,
        lower: NodeBytes<'a>,
        upper: NodeBytes<'a>,
    },
    ShortSkip {
        header: &'a ShortSkip,
        child: NodeBytes<'a>,
    },
    RefPage(&'a RefPage),
}

impl<'a> TryFrom<NodeBytes<'a>> for Node<'a> {
    type Error = Error;

    fn try_from(node: NodeBytes<'a>) -> Result<Self, Self::Error> {
        Ok(match node.kind {
            NodeKind::Value => Node::Value(
                U64::<BigEndian>::read_from_bytes(node.bytes)
                    .map_err(|_| DiskError::Corrupt)?
                    .get(),
            ),
            NodeKind::Branch => {
                let (header, rest) =
                    Branch::ref_from_prefix(node.bytes).map_err(|_| DiskError::Corrupt)?;
                let (lower, upper) = rest
                    .split_at_checked(header.lower_len())
                    .ok_or(DiskError::Corrupt)?;
                Node::Branch {
                    header,
                    lower: NodeBytes {
                        kind: header.lower_kind()?,
                        bytes: lower,
                    },
                    upper: NodeBytes {
                        kind: header.upper_kind()?,
                        bytes: upper,
                    },
                }
            }
            NodeKind::ShortSkip => {
                let (header, child) =
                    ShortSkip::ref_from_prefix(node.bytes).map_err(|_| DiskError::Corrupt)?;
                Node::ShortSkip {
                    header,
                    child: NodeBytes {
                        kind: header.child_kind()?,
                        bytes: child,
                    },
                }
            }
            NodeKind::RefPage => {
                Node::RefPage(RefPage::ref_from_bytes(node.bytes).map_err(|_| DiskError::Corrupt)?)
            }
        })
    }
}

impl NodeBytes<'_> {
    /// 自身と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(self) -> Result<DimensionSet, Error> {
        Ok(match Node::try_from(self)? {
            Node::Value(_) => DimensionSet::EMPTY,
            Node::Branch { header, .. } => header.split_dimensions(),
            Node::ShortSkip { header, child } => {
                header.path()?.deeper_dimensions() | child.split_dimensions()?
            }
            Node::RefPage(page) => page.split_dimensions(),
        })
    }
}
