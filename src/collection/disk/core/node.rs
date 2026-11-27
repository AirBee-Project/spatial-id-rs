// ビットの位置は、バイト列をビッグエンディアンの整数として読んだときの最下位ビットを bit 0 として数える。
use crate::collection::disk::io::PageId;
use crate::collection::disk::{DiskValue, MAX_VALUE_LEN};
use crate::id::dimension::{Dimension, DimensionSet};
use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};
use crate::{DiskError, FlexId};
use alloc::boxed::Box;
use alloc::vec::Vec;
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

/// 子の Node の種類。3bit で、あと 3 種類まで増やせる。
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
    type Error = DiskError;
    fn try_from(code: u8) -> Result<Self, Self::Error> {
        zerocopy::try_transmute!(code).map_err(|_| DiskError::Corrupt)
    }
}

/// Node が持つ、エンコードした [`MAX_VALUE_LEN`] バイト以下の値のバイト列。一致の判定はバイト列で行う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EncodedValue(Box<[u8]>);

impl EncodedValue {
    /// `value` をエンコードしたもの。[`MAX_VALUE_LEN`] バイトを超えれば [`DiskError::ValueTooLarge`] を返す。
    pub(super) fn encode<V: DiskValue>(value: &V) -> Result<Self, DiskError> {
        let mut bytes = Vec::new();
        value.encode(&mut bytes);
        if bytes.len() > MAX_VALUE_LEN {
            return Err(DiskError::ValueTooLarge);
        }
        Ok(EncodedValue(bytes.into_boxed_slice()))
    }

    /// `bytes` をそのまま持つ値。`bytes` は [`MAX_VALUE_LEN`] バイト以下であること。
    pub(super) fn from_bytes(bytes: &[u8]) -> Self {
        debug_assert!(bytes.len() <= MAX_VALUE_LEN);
        EncodedValue(bytes.into())
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// ページから読み出した Node。親から渡される領域との相対位置で意味を持つ。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Node {
    Value(EncodedValue),
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
        split_dimensions: DimensionSet,
    },
}

impl Node {
    /// 種類 `kind` の Node のバイト列 `bytes` を読み出す。別のページへは辿らない。
    ///
    /// 領域が最大ズームを超えないかは、辿るときに [`FlexId::halves`] と [`RelativeFlexId::to_absolute`] で確かめる。
    pub(super) fn decode(kind: NodeKind, bytes: &[u8]) -> Result<Self, DiskError> {
        Ok(match kind {
            // 値の長さは親（Branch の lower_len、NodePage の長さ、Skip の残り）が決める
            NodeKind::Value if bytes.len() <= MAX_VALUE_LEN => {
                Node::Value(EncodedValue::from_bytes(bytes))
            }
            NodeKind::Value => return Err(DiskError::Corrupt),
            NodeKind::Branch => {
                let (header, rest) =
                    raw::Branch::ref_from_prefix(bytes).map_err(|_| DiskError::Corrupt)?;
                let (lower, upper) = rest
                    .split_at_checked(header.lower_len())
                    .ok_or(DiskError::Corrupt)?;
                Node::Branch {
                    dimension: header.dimension(),
                    split_dimensions: header.split_dimensions(),
                    lower: Box::new(Node::decode(header.lower_kind()?, lower)?),
                    upper: Box::new(Node::decode(header.upper_kind()?, upper)?),
                }
            }
            NodeKind::ShortSkip => {
                let (header, child) =
                    raw::ShortSkip::ref_from_prefix(bytes).map_err(|_| DiskError::Corrupt)?;
                Node::Skip {
                    path: header.path()?.into(),
                    child: Box::new(Node::decode(header.child_kind()?, child)?),
                }
            }
            NodeKind::LongSkip => {
                let (header, child) =
                    raw::LongSkip::ref_from_prefix(bytes).map_err(|_| DiskError::Corrupt)?;
                Node::Skip {
                    path: header.path()?,
                    child: Box::new(Node::decode(header.child_kind()?, child)?),
                }
            }
            NodeKind::RefPage => {
                let raw = raw::RefPage::ref_from_bytes(bytes).map_err(|_| DiskError::Corrupt)?;
                Node::RefPage {
                    page_id: raw.page_id(),
                    split_dimensions: raw.split_dimensions(),
                }
            }
        })
    }

    /// 領域 `this` の自身の子と、子が受け持つ領域。Value と RefPage は子を持たない。
    pub(super) fn children(
        &self,
        this: &FlexId,
    ) -> Result<impl Iterator<Item = (&Node, FlexId)>, DiskError> {
        let children = match self {
            Node::Branch {
                dimension,
                lower,
                upper,
                ..
            } => {
                let [lower_region, upper_region] =
                    this.halves(*dimension).ok_or(DiskError::Corrupt)?;
                [
                    Some((&**lower, lower_region)),
                    Some((&**upper, upper_region)),
                ]
            }
            Node::Skip { path, child } => {
                let region = path.to_absolute(this).map_err(|_| DiskError::Corrupt)?;
                [Some((&**child, region)), None]
            }
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
        self.header_len()
            + match self {
                Node::Branch { lower, upper, .. } => lower.len() + upper.len(),
                Node::Skip { child, .. } => child.len(),
                Node::Value(_) | Node::RefPage { .. } => 0,
            }
    }

    /// [`write`](Self::write) したときの、子を除いたバイト長。
    pub(super) fn header_len(&self) -> usize {
        match self {
            Node::Value(value) => value.as_bytes().len(),
            Node::Branch { .. } => size_of::<raw::Branch>(),
            Node::Skip { path, .. } => match ShortRelativeFlexId::new(*path) {
                Some(_) => size_of::<raw::ShortSkip>(),
                None => size_of::<raw::LongSkip>(),
            },
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
            Node::Value(value) => put(dst, value.as_bytes()),
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
                split_dimensions,
            } => put(
                dst,
                raw::RefPage::new(*page_id, *split_dimensions).as_bytes(),
            ),
        }
    }
}

/// ディスク上のバイナリ表現。
mod raw {
    use super::NodeKind;
    use crate::DiskError;
    use crate::collection::disk::io::PageId;
    use crate::id::dimension::{Dimension, DimensionSet};
    use crate::id::relative_flex_id::{RelativeFlexId, ShortRelativeFlexId};
    use zerocopy::byteorder::{BigEndian, U16};
    use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

    /// 下位 40bit に詰めた PageId。
    #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
    #[repr(transparent)]
    pub(super) struct PageId40([u8; 5]);

    impl From<PageId> for PageId40 {
        fn from(page_id: PageId) -> Self {
            // 2^40 ページは 4PiB で、実際のファイルでは超えない
            assert!(page_id.0 >> 40 == 0);
            let [_, _, _, bytes @ ..] = page_id.0.to_be_bytes();
            PageId40(bytes)
        }
    }

    impl From<&PageId40> for PageId {
        fn from(PageId40([a, b, c, d, e]): &PageId40) -> Self {
            PageId(u64::from_be_bytes([0, 0, 0, *a, *b, *c, *d, *e]))
        }
    }

    /// Branch の子より前の部分。この後ろに lower、upper が続く。
    ///
    /// | bit   | 内容                                   |
    /// |-------|----------------------------------------|
    /// | 23-22 | 割る次元                               |
    /// | 21-18 | 自身と子孫が割っている次元の集合       |
    /// | 17-15 | lower の子の種類                       |
    /// | 14-12 | upper の子の種類                       |
    /// | 11-0  | lower の子のバイト長                   |
    #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
    #[repr(transparent)]
    pub(super) struct Branch([u8; 3]);

    impl Branch {
        /// lower のバイト長に使うビット数。lower を読まずに upper の位置を求めるために持つ。
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
        pub(super) fn lower_kind(&self) -> Result<NodeKind, DiskError> {
            NodeKind::try_from((self.bits() >> 15) as u8 & 0b111)
        }

        /// upper の子の種類。
        pub(super) fn upper_kind(&self) -> Result<NodeKind, DiskError> {
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
    /// | 23-8  | 行き先の [ShortRelativeFlexId]         |
    /// | 7-3   | 0 （現状は空き）                       |
    /// | 2-0   | 子の種類                               |
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
                path: U16::new(u16::from(path)),
                child: child_kind as u8,
            }
        }

        /// 親の領域から見た行き先。
        pub(super) fn path(&self) -> Result<ShortRelativeFlexId, DiskError> {
            ShortRelativeFlexId::from_bits(self.path.get()).ok_or(DiskError::Corrupt)
        }

        /// 行き先にある子の種類。
        pub(super) fn child_kind(&self) -> Result<NodeKind, DiskError> {
            NodeKind::try_from(self.child)
        }
    }

    /// 行き先を [ShortRelativeFlexId] で表せない Skip の、子より前の部分。この後ろに子が続く。
    ///
    /// | bit     | 内容                                                     |
    /// |---------|----------------------------------------------------------|
    /// | 167-8   | 行き先の [RelativeFlexId]（[`RelativeFlexId::encode`]）  |
    /// | 7-3     | 0 （現状は空き）                                         |
    /// | 2-0     | 子の種類                                                 |
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
        pub(super) fn path(&self) -> Result<RelativeFlexId, DiskError> {
            RelativeFlexId::decode(&self.path).map_err(|_| DiskError::Corrupt)
        }

        /// 行き先にある子の種類。
        pub(super) fn child_kind(&self) -> Result<NodeKind, DiskError> {
            NodeKind::try_from(self.child)
        }
    }

    /// 別のページのルートを指す Node。
    ///
    /// 行き先の分割次元の集合を写しておき、行き先のページを読まずに済ませる。
    ///
    /// | bit   | 内容                                       |
    /// |-------|--------------------------------------------|
    /// | 47-44 | 0 （現状は空き）                           |
    /// | 43-40 | 行き先のルートと子孫が割っている次元の集合 |
    /// | 39-0  | 行き先の PageId                            |
    #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
    #[repr(C)]
    pub(super) struct RefPage {
        head: u8,
        page_id: PageId40,
    }

    impl RefPage {
        /// `page_id` のルートを指す RefPage。`split_dimensions` は行き先のルートのもの。
        pub(super) fn new(page_id: PageId, split_dimensions: DimensionSet) -> Self {
            RefPage {
                head: u8::from(split_dimensions),
                page_id: page_id.into(),
            }
        }

        /// 行き先のページ番号。
        pub(super) fn page_id(&self) -> PageId {
            (&self.page_id).into()
        }

        /// 行き先のルートと子孫が割っている次元の集合。
        pub(super) fn split_dimensions(&self) -> DimensionSet {
            DimensionSet::from_bits(self.head & 0xF)
        }
    }
}
