// ビットの位置は、先頭バイトの最上位ビットを bit 0 として数える。
use crate::collection::disk::io::PageId;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};
use crate::{DiskError, Error, FlexId, Side};
use alloc::boxed::Box;
use zerocopy::byteorder::{BigEndian, U64};
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

/// 子の Node の種類。3bit。
//  残り5種類を増やす余裕がある。
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromBytes)]
#[repr(u8)]
pub(super) enum NodeKind {
    Value = 0,
    Branch = 1,
    ShortSkip = 2,
    RefPage = 3,
    LongSkip = 4,
}

impl TryFrom<u8> for NodeKind {
    type Error = Error;
    fn try_from(code: u8) -> Result<Self, Self::Error> {
        zerocopy::try_transmute!(code).map_err(|_| DiskError::Corrupt.into())
    }
}

/// ページから読み出した Node。親から渡される領域との相対位置で意味を持つ。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Node {
    Value(u64),
    Branch {
        dimension: Dimension,
        /// 自身と子孫が割っている次元の集合。
        split_dimensions: DimensionSet,
        lower: Box<Node>,
        upper: Box<Node>,
    },
    /// 行き先 `path` の先だけに `child` がある。`child` は Skip ではない。
    Skip {
        path: RelativeFlexId,
        child: Box<Node>,
    },
    /// 別のページの Node。辿り着くまで読まない。
    RefPage {
        page_id: PageId,
        /// 行き先のルートの種類。
        kind: NodeKind,
        split_dimensions: DimensionSet,
    },
}

impl Node {
    /// 領域 `this` の、種類 `kind` の Node のバイト列 `bytes` を読み出す。別のページへは辿らない。
    pub(super) fn decode(kind: NodeKind, bytes: &[u8], this: &FlexId) -> Result<Self, Error> {
        fn corrupt<E>(_: E) -> DiskError {
            DiskError::Corrupt
        }
        Ok(match kind {
            NodeKind::Value => Node::Value(
                U64::<BigEndian>::read_from_bytes(bytes)
                    .map_err(corrupt)?
                    .get(),
            ),
            NodeKind::Branch => {
                let (header, rest) = raw::Branch::ref_from_prefix(bytes).map_err(corrupt)?;
                let (lower, upper) = rest
                    .split_at_checked(header.lower_len())
                    .ok_or(DiskError::Corrupt)?;
                let dimension = header.dimension();
                Node::Branch {
                    dimension,
                    split_dimensions: header.split_dimensions(),
                    lower: Box::new(Node::decode(
                        header.lower_kind()?,
                        lower,
                        &half(this, dimension, Side::Lower)?,
                    )?),
                    upper: Box::new(Node::decode(
                        header.upper_kind()?,
                        upper,
                        &half(this, dimension, Side::Upper)?,
                    )?),
                }
            }
            NodeKind::ShortSkip => {
                let (header, child) = raw::ShortSkip::ref_from_prefix(bytes).map_err(corrupt)?;
                let path = header.path()?;
                Node::Skip {
                    path: path.into(),
                    child: Box::new(Node::decode(
                        header.child_kind()?,
                        child,
                        &path.to_absolute(this),
                    )?),
                }
            }
            NodeKind::LongSkip => {
                let (header, child) = raw::LongSkip::ref_from_prefix(bytes).map_err(corrupt)?;
                let path = header.path()?;
                Node::Skip {
                    path,
                    child: Box::new(Node::decode(
                        header.child_kind()?,
                        child,
                        &path.to_absolute(this)?,
                    )?),
                }
            }
            NodeKind::RefPage => {
                let raw = raw::RefPage::ref_from_bytes(bytes).map_err(corrupt)?;
                Node::RefPage {
                    page_id: raw.page_id(),
                    kind: raw.kind()?,
                    split_dimensions: raw.split_dimensions(),
                }
            }
        })
    }

    /// 領域 `this` の自身の子と、子が受け持つ領域。Value と RefPage は子を持たない。
    pub(super) fn children(
        &self,
        this: &FlexId,
    ) -> Result<impl Iterator<Item = (&Node, FlexId)>, Error> {
        let children = match self {
            Node::Branch {
                dimension,
                lower,
                upper,
                ..
            } => [
                Some((&**lower, half(this, *dimension, Side::Lower)?)),
                Some((&**upper, half(this, *dimension, Side::Upper)?)),
            ],
            Node::Skip { path, child } => [Some((&**child, path.to_absolute(this)?)), None],
            Node::Value(_) | Node::RefPage { .. } => [None, None],
        };
        Ok(children.into_iter().flatten())
    }

    /// [`write`](Self::write) したときの種類。
    pub(super) fn kind(&self) -> NodeKind {
        match self {
            Node::Value(_) => NodeKind::Value,
            Node::Branch { .. } => NodeKind::Branch,
            Node::Skip { path, .. } => match ShortRelativeFlexId::new(*path) {
                Some(_) => NodeKind::ShortSkip,
                None => NodeKind::LongSkip,
            },
            Node::RefPage { .. } => NodeKind::RefPage,
        }
    }

    /// [`write`](Self::write) したときのバイト長。
    pub(super) fn len(&self) -> usize {
        match self {
            Node::Value(_) => size_of::<u64>(),
            Node::Branch { lower, upper, .. } => {
                size_of::<raw::Branch>() + lower.len() + upper.len()
            }
            Node::Skip { path, child } => {
                let header = match ShortRelativeFlexId::new(*path) {
                    Some(_) => size_of::<raw::ShortSkip>(),
                    None => size_of::<raw::LongSkip>(),
                };
                header + child.len()
            }
            Node::RefPage { .. } => size_of::<raw::RefPage>(),
        }
    }

    /// 自身と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        match self {
            Node::Value(_) => DimensionSet::EMPTY,
            Node::Branch {
                split_dimensions, ..
            }
            | Node::RefPage {
                split_dimensions, ..
            } => *split_dimensions,
            Node::Skip { path, child } => path.deeper_dimensions() | child.split_dimensions(),
        }
    }

    /// 自身のバイト列を `dst` の先頭から書き、残りのスライスを返す。
    pub(super) fn write<'a>(&self, dst: &'a mut [u8]) -> &'a mut [u8] {
        fn put<'a>(dst: &'a mut [u8], src: &[u8]) -> &'a mut [u8] {
            let (head, rest) = dst.split_at_mut(src.len());
            head.copy_from_slice(src);
            rest
        }
        match self {
            Node::Value(value) => put(dst, &value.to_be_bytes()),
            Node::Branch {
                dimension,
                split_dimensions,
                lower,
                upper,
            } => {
                let header = raw::Branch::new(
                    *dimension,
                    *split_dimensions,
                    lower.kind(),
                    upper.kind(),
                    lower.len(),
                );
                let dst = put(dst, header.as_bytes());
                let dst = lower.write(dst);
                upper.write(dst)
            }
            Node::Skip { path, child } => {
                let dst = match ShortRelativeFlexId::new(*path) {
                    Some(short) => put(dst, raw::ShortSkip::new(short, child.kind()).as_bytes()),
                    None => put(dst, raw::LongSkip::new(path, child.kind()).as_bytes()),
                };
                child.write(dst)
            }
            Node::RefPage {
                page_id,
                kind,
                split_dimensions,
            } => put(
                dst,
                raw::RefPage::new(*page_id, *kind, *split_dimensions).as_bytes(),
            ),
        }
    }
}

/// 領域 `this` を `dimension` で割った `side` 側の領域。割れなければ壊れたファイルとして扱う。
pub(super) fn half(this: &FlexId, dimension: Dimension, side: Side) -> Result<FlexId, Error> {
    this.split_on(dimension, side)
        .ok_or_else(|| DiskError::Corrupt.into())
}

/// ディスク上のバイナリ表現。
mod raw {
    use super::NodeKind;
    use crate::collection::disk::io::PageId;
    use crate::id::dimension::{Dimension, DimensionSet};
    use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};
    use crate::{DiskError, Error};
    use zerocopy::byteorder::{BigEndian, U16};
    use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

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
            assert!(lower_len < 1 << Self::LOWER_LEN_BITS);
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

    /// 行き先を [ShortRelativeFlexId] で表せない Skip の、子より前の部分。この後ろに子が続く。
    ///
    /// | bit     | 内容                                                     |
    /// |---------|----------------------------------------------------------|
    /// | 0-159   | 行き先の [RelativeFlexId]（[`RelativeFlexId::encode`]）  |
    /// | 160-164 | 0 （現状は空き）                                         |
    /// | 165-167 | 子の種類                                                 |
    #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
    #[repr(C)]
    pub(super) struct LongSkip {
        path: [u8; RelativeFlexId::ENCODED_LEN],
        child: u8,
    }

    impl LongSkip {
        /// 行き先と子の種類を詰めた LongSkip。
        pub(super) fn new(path: &RelativeFlexId, child_kind: NodeKind) -> Self {
            LongSkip {
                path: path.encode(),
                child: child_kind as u8,
            }
        }

        /// 親の領域から見た行き先。
        pub(super) fn path(&self) -> Result<RelativeFlexId, Error> {
            RelativeFlexId::decode(&self.path)
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
        page_id: [u8; RefPage::PAGE_ID_LEN],
    }

    impl RefPage {
        const PAGE_ID_LEN: usize = 5;

        /// `page_id` のルートを指す RefPage。`kind`・`split_dimensions` は行き先のルートのもの。
        pub(super) fn new(page_id: PageId, kind: NodeKind, split_dimensions: DimensionSet) -> Self {
            // 2^40 ページは 4PiB で、実際のファイルでは超えない
            assert!(page_id.0 >> (8 * Self::PAGE_ID_LEN) == 0);
            RefPage {
                head: (kind as u8) << 4 | u8::from(split_dimensions),
                page_id: page_id.0.to_be_bytes()[8 - Self::PAGE_ID_LEN..]
                    .try_into()
                    .unwrap(),
            }
        }

        /// 行き先のページ番号。
        pub(super) fn page_id(&self) -> PageId {
            let mut bytes = [0; 8];
            bytes[8 - Self::PAGE_ID_LEN..].copy_from_slice(&self.page_id);
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
}
