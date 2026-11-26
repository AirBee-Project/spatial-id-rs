use crate::collection::disk::io::PageId;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};
use crate::{DiskError, Error};
use zerocopy::byteorder::{BigEndian, U16, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

/// 親が 4bit で持つ、子の Node の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum NodeKind {
    Value = 0,
    Branch = 1,
    ShortSkip = 2,
    LongSkip = 3,
    RefPage = 4,
}

impl TryFrom<u8> for NodeKind {
    type Error = Error;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        Ok(match code {
            0 => NodeKind::Value,
            1 => NodeKind::Branch,
            2 => NodeKind::ShortSkip,
            3 => NodeKind::LongSkip,
            4 => NodeKind::RefPage,
            _ => return Err(DiskError::Corrupt.into()),
        })
    }
}

/// Value のバイト列の長さ。
pub(super) const VALUE_LEN: usize = size_of::<u64>();

/// Branch の子より前の部分。この後ろに lower、upper が続く。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct Branch {
    /// 上位 2bit が割る次元、続く 4bit が自身と子孫が割っている次元の集合。
    split: u8,
    /// 上位 4bit が lower、下位 4bit が upper の種類。
    kinds: u8,
    lower_len: U16<BigEndian>,
}

impl Branch {
    pub(super) fn new(
        dimension: Dimension,
        split: DimensionSet,
        lower: NodeKind,
        upper: NodeKind,
        lower_len: usize,
    ) -> Self {
        debug_assert!(lower_len <= u16::MAX as usize);
        Branch {
            split: (dimension as u8) << 6 | set_bits(split) << 2,
            kinds: (lower as u8) << 4 | upper as u8,
            lower_len: U16::new(lower_len as u16),
        }
    }

    pub(super) fn dimension(&self) -> Dimension {
        Dimension::ALL[usize::from(self.split >> 6)]
    }

    /// 自身と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        bits_set(self.split >> 2)
    }

    pub(super) fn lower_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.kinds >> 4)
    }

    pub(super) fn upper_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.kinds & 0xF)
    }

    pub(super) fn lower_len(&self) -> usize {
        usize::from(self.lower_len.get())
    }
}

/// 行き先を [ShortRelativeFlexId] で表せる Skip の、子より前の部分。この後ろに子が続く。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct ShortSkip {
    path: U16<BigEndian>,
    /// 上位 4bit が子の種類。
    child: u8,
}

impl ShortSkip {
    pub(super) fn new(path: ShortRelativeFlexId, child: NodeKind) -> Self {
        ShortSkip {
            path: U16::new(path.to_bits()),
            child: (child as u8) << 4,
        }
    }

    pub(super) fn path(&self) -> Result<ShortRelativeFlexId, Error> {
        Ok(ShortRelativeFlexId::from_bits(self.path.get()).ok_or(DiskError::Corrupt)?)
    }

    pub(super) fn child_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.child >> 4)
    }
}

/// 行き先を [ShortRelativeFlexId] で表せない Skip の、子より前の部分。この後ろに子が続く。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct LongSkip {
    path: [u8; RelativeFlexId::ENCODED_LEN],
    /// 上位 4bit が子の種類。
    child: u8,
}

impl LongSkip {
    pub(super) fn new(path: &RelativeFlexId, child: NodeKind) -> Self {
        LongSkip {
            path: path.encode(),
            child: (child as u8) << 4,
        }
    }

    pub(super) fn path(&self) -> Result<RelativeFlexId, Error> {
        RelativeFlexId::decode(&self.path)
    }

    pub(super) fn child_kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.child >> 4)
    }
}

/// 別のページのルートを指す Node。
///
/// 行き先のルートの種類と分割次元の集合を写しておき、行き先のページを読まずに済ませる。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(transparent)]
pub(super) struct RefPage(U64<BigEndian>);

impl RefPage {
    /// PageId に使う下位のビット数。上位 8bit に種類と分割次元の集合を置く。
    const PAGE_BITS: u32 = 56;

    pub(super) fn new(page: PageId, kind: NodeKind, split: DimensionSet) -> Self {
        // 2^56 ページは 256EiB で、実際のファイルでは超えない
        assert!(page.0 >> Self::PAGE_BITS == 0);
        let head = u64::from((kind as u8) << 4 | set_bits(split));
        RefPage(U64::new(head << Self::PAGE_BITS | page.0))
    }

    pub(super) fn page(&self) -> PageId {
        PageId(self.0.get() & ((1 << Self::PAGE_BITS) - 1))
    }

    /// 行き先のルートの種類。
    pub(super) fn kind(&self) -> Result<NodeKind, Error> {
        NodeKind::try_from(self.head() >> 4)
    }

    /// 行き先のルートと子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        bits_set(self.head())
    }

    fn head(&self) -> u8 {
        (self.0.get() >> Self::PAGE_BITS) as u8
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
    LongSkip {
        header: &'a LongSkip,
        child: NodeBytes<'a>,
    },
    RefPage(&'a RefPage),
}

impl<'a> NodeBytes<'a> {
    pub(super) fn read(self) -> Result<Node<'a>, Error> {
        Ok(match self.kind {
            NodeKind::Value => Node::Value(
                U64::<BigEndian>::read_from_bytes(self.bytes)
                    .map_err(|_| DiskError::Corrupt)?
                    .get(),
            ),
            NodeKind::Branch => {
                let (header, rest) = prefix::<Branch>(self.bytes)?;
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
                let (header, child) = prefix::<ShortSkip>(self.bytes)?;
                Node::ShortSkip {
                    header,
                    child: NodeBytes {
                        kind: header.child_kind()?,
                        bytes: child,
                    },
                }
            }
            NodeKind::LongSkip => {
                let (header, child) = prefix::<LongSkip>(self.bytes)?;
                Node::LongSkip {
                    header,
                    child: NodeBytes {
                        kind: header.child_kind()?,
                        bytes: child,
                    },
                }
            }
            NodeKind::RefPage => {
                Node::RefPage(RefPage::ref_from_bytes(self.bytes).map_err(|_| DiskError::Corrupt)?)
            }
        })
    }

    /// 自身と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(self) -> Result<DimensionSet, Error> {
        Ok(match self.read()? {
            Node::Value(_) => DimensionSet::EMPTY,
            Node::Branch { header, .. } => header.split_dimensions(),
            Node::ShortSkip { header, child } => {
                header.path()?.deeper_dimensions() | child.split_dimensions()?
            }
            Node::LongSkip { header, child } => {
                header.path()?.deeper_dimensions() | child.split_dimensions()?
            }
            Node::RefPage(page) => page.split_dimensions(),
        })
    }
}

fn prefix<T: FromBytes + KnownLayout + Immutable>(bytes: &[u8]) -> Result<(&T, &[u8]), Error> {
    Ok(T::ref_from_prefix(bytes).map_err(|_| DiskError::Corrupt)?)
}

/// 4bit に収まる、次元の集合のビット列。
fn set_bits(set: DimensionSet) -> u8 {
    zerocopy::transmute!(set)
}

fn bits_set(bits: u8) -> DimensionSet {
    zerocopy::transmute!(bits & 0xF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn branch_round_trips() {
        let split = DimensionSet::from(Dimension::X) | Dimension::T.into();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            Branch::new(
                Dimension::X,
                split,
                NodeKind::Value,
                NodeKind::RefPage,
                VALUE_LEN,
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(&7u64.to_be_bytes());
        bytes.extend_from_slice(
            RefPage::new(PageId(42), NodeKind::Branch, Dimension::Y.into()).as_bytes(),
        );

        let node = NodeBytes {
            kind: NodeKind::Branch,
            bytes: &bytes,
        };
        let Node::Branch {
            header,
            lower,
            upper,
        } = node.read().unwrap()
        else {
            panic!("Branch として読めない");
        };
        assert_eq!(header.dimension(), Dimension::X);
        assert_eq!(header.split_dimensions(), split);
        assert!(matches!(lower.read().unwrap(), Node::Value(7)));
        let Node::RefPage(page) = upper.read().unwrap() else {
            panic!("RefPage として読めない");
        };
        assert_eq!(page.page(), PageId(42));
        assert_eq!(page.kind().unwrap(), NodeKind::Branch);
        assert_eq!(page.split_dimensions(), Dimension::Y.into());
    }

    #[test]
    fn wrong_length_is_corrupt() {
        let node = NodeBytes {
            kind: NodeKind::Value,
            bytes: &[0; VALUE_LEN + 1],
        };
        assert!(node.read().is_err());
    }

    #[test]
    fn unknown_kind_is_corrupt() {
        assert!(NodeKind::try_from(5).is_err());
    }
}
