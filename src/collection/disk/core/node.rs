use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;

use bytemuck::{CheckedBitPattern, NoUninit, Pod, Zeroable, checked};
use triomphe::Arc;

use crate::collection::disk::io::{Page, PageFile};
use crate::error::DiskError;
use crate::id::dimension::DimensionSet;
use crate::id::relative_flex_id::RelativeFlexId;
use crate::{Dimension, Error, FlexId};

/// Node は自身の領域を持たず、親から渡される領域 `this` との相対的な位置で意味を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Value(u64),
    Branch(Arc<Branch>),
    LongSkip(Arc<Skip>),
    /// まだ読み込んでいないページの根の Branch か LongSkip。
    OnDisk {
        page: u64,
        kind: Kind,
        split_dimensions: DimensionSet,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(super) dimension: Dimension,
    /// 自身と子孫が割っている次元の集合。
    pub(super) split_dimensions: DimensionSet,
    pub(super) lower: Node,
    pub(super) upper: Node,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skip {
    pub(super) path: RelativeFlexId,
    /// 自身と子孫が割っている次元の集合。
    pub(super) split_dimensions: DimensionSet,
    pub(super) child: Node,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Branch(Dimension),
    LongSkip,
}

impl Node {
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        match self {
            Node::Value(_) => DimensionSet::EMPTY,
            Node::Branch(branch) => branch.split_dimensions,
            Node::LongSkip(skip) => skip.split_dimensions,
            Node::OnDisk {
                split_dimensions, ..
            } => *split_dimensions,
        }
    }

    /// Value なら [`None`]。
    pub(super) fn kind(&self) -> Option<Kind> {
        match self {
            Node::Value(_) => None,
            Node::Branch(branch) => Some(Kind::Branch(branch.dimension)),
            Node::LongSkip(_) => Some(Kind::LongSkip),
            Node::OnDisk { kind, .. } => Some(*kind),
        }
    }

    /// この Node を最初に割る次元。Value なら [`None`]。
    pub(super) fn head_dimension(&self, this: &FlexId) -> Option<Dimension> {
        match self.kind()? {
            Kind::Branch(dimension) => Some(dimension),
            Kind::LongSkip => this.coarsest_dimension_in(self.split_dimensions()),
        }
    }
}

impl Branch {
    pub(super) fn new(dimension: Dimension, lower: Node, upper: Node) -> Self {
        Branch {
            dimension,
            split_dimensions: DimensionSet::from(dimension)
                | lower.split_dimensions()
                | upper.split_dimensions(),
            lower,
            upper,
        }
    }
}

impl Skip {
    pub(super) fn new(path: RelativeFlexId, child: Node) -> Self {
        Skip {
            split_dimensions: path.deeper_dimensions() | child.split_dimensions(),
            path,
            child,
        }
    }
}

impl From<Branch> for Node {
    fn from(branch: Branch) -> Self {
        Node::Branch(Arc::new(branch))
    }
}

impl From<Skip> for Node {
    fn from(skip: Skip) -> Self {
        Node::LongSkip(Arc::new(skip))
    }
}

/// 行き先 `target`・その先が `split_dimensions` で割る Skip と、`region` への書き込みが、
/// 領域 `this` から途中で割らずに一緒に降りられる一番狭い領域。
pub(super) fn common_descent(
    this: &FlexId,
    target: &FlexId,
    split_dimensions: DimensionSet,
    region: &FlexId,
) -> FlexId {
    let head = |at: &FlexId, to: &FlexId, child_split: DimensionSet| {
        let finer = to.finer_dimensions_than(at);
        (finer != DimensionSet::EMPTY).then(|| at.coarsest_dimension_in(finer | child_split))?
    };
    let mut at = *this;
    while let (Some(da), Some(db)) = (
        head(&at, target, split_dimensions),
        head(&at, region, DimensionSet::EMPTY),
    ) && da == db
    {
        let next = at.split_toward(da, target).unwrap();
        if !next.contains(region) {
            break;
        }
        at = next;
    }
    at
}

/// ディスク上の [Node] の種類。Branch はどの次元で割るかまで含める。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, CheckedBitPattern, NoUninit)]
enum Tag {
    Value,
    /// 同じページの中で、親のレコードの後ろに続く子。
    Local,
    /// 空のルート。
    Empty,
    LongSkip,
    BranchF,
    BranchX,
    BranchY,
    BranchT,
}

impl From<Kind> for Tag {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::LongSkip => Tag::LongSkip,
            Kind::Branch(Dimension::F) => Tag::BranchF,
            Kind::Branch(Dimension::X) => Tag::BranchX,
            Kind::Branch(Dimension::Y) => Tag::BranchY,
            Kind::Branch(Dimension::T) => Tag::BranchT,
        }
    }
}

impl Tag {
    /// Branch・LongSkip でなければ [`None`]。
    fn kind(self) -> Option<Kind> {
        match self {
            Tag::Value | Tag::Local | Tag::Empty => None,
            Tag::LongSkip => Some(Kind::LongSkip),
            Tag::BranchF => Some(Kind::Branch(Dimension::F)),
            Tag::BranchX => Some(Kind::Branch(Dimension::X)),
            Tag::BranchY => Some(Kind::Branch(Dimension::Y)),
            Tag::BranchT => Some(Kind::Branch(Dimension::T)),
        }
    }
}

/// 子を指すディスク上の [Node]。Value は値をそのまま埋め込む。
#[repr(C)]
#[derive(Debug, Clone, Copy, CheckedBitPattern, NoUninit)]
pub(crate) struct RawNode {
    tag: Tag,
    /// 別のページの Branch・LongSkip の `split_dimensions`。
    split_dimensions: DimensionSet,
    _pad: [u8; 6],
    /// Value なら値、別のページの Branch・LongSkip ならページ番号。
    payload: u64,
}

impl RawNode {
    const LOCAL: RawNode = RawNode::new(Tag::Local, DimensionSet::EMPTY, 0);
    pub(super) const EMPTY: RawNode = RawNode::new(Tag::Empty, DimensionSet::EMPTY, 0);

    const fn new(tag: Tag, split_dimensions: DimensionSet, payload: u64) -> Self {
        RawNode {
            tag,
            split_dimensions,
            _pad: [0; 6],
            payload,
        }
    }
}

impl TryFrom<RawNode> for Option<Node> {
    type Error = Error;

    fn try_from(raw: RawNode) -> Result<Self, Error> {
        Ok(match raw.tag {
            Tag::Local => return Err(DiskError::Corrupt.into()),
            Tag::Empty => None,
            Tag::Value => Some(Node::Value(raw.payload)),
            tag => Some(Node::OnDisk {
                page: raw.payload,
                kind: tag.kind().ok_or(DiskError::Corrupt)?,
                split_dimensions: raw.split_dimensions,
            }),
        })
    }
}

/// ページに詰める Branch・LongSkip。
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct RawRecord([u8; 40]);

/// [RawBranch]・[RawSkip] に共通の先頭部分。
#[repr(C)]
#[derive(Clone, Copy, CheckedBitPattern, NoUninit)]
struct RawRecordHead {
    tag: Tag,
    split_dimensions: DimensionSet,
}

#[repr(C)]
#[derive(Clone, Copy, CheckedBitPattern, NoUninit)]
struct RawBranch {
    head: RawRecordHead,
    /// 左の子（lower）の部分木が同じページ内で占めるレコード数。
    lower_len: u8,
    _pad: [u8; 5],
    lower: RawNode,
    upper: RawNode,
}

#[repr(C)]
#[derive(Clone, Copy, CheckedBitPattern, NoUninit)]
struct RawSkip {
    head: RawRecordHead,
    path: [u8; RelativeFlexId::ENCODED_LEN],
    _pad: [u8; 2],
    child: RawNode,
}

const _: () = assert!(size_of::<RawBranch>() == size_of::<RawRecord>());
const _: () = assert!(size_of::<RawSkip>() == size_of::<RawRecord>());

impl RawRecord {
    fn head(&self) -> Result<RawRecordHead, Error> {
        let head: &RawRecordHead = checked::try_from_bytes(&self.0[..size_of::<RawRecordHead>()])?;
        Ok(*head)
    }
}

/// ページの種類。ヘッダーとデータのページを見分ける。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, CheckedBitPattern, NoUninit)]
enum PageKind {
    Records = 1,
    Header = 2,
}

/// 1 ページに詰められるレコードの数。
const RECORDS_PER_PAGE: usize = (Page::CAPACITY - 8) / size_of::<RawRecord>();

const _: () = assert!(RECORDS_PER_PAGE <= u8::MAX as usize);

/// Branch・LongSkip を詰めたページ。先頭のレコードがそのページの部分木の根になる。
#[repr(C)]
#[derive(Clone, Copy, CheckedBitPattern, NoUninit)]
struct RawPage {
    kind: PageKind,
    _pad: [u8; 7],
    records: [RawRecord; RECORDS_PER_PAGE],
}

impl RawPage {
    /// `slot` 番目のレコードから下を展開する。別のページの子は [`Node::OnDisk`] のまま残す。
    fn decode(&self, slot: usize) -> Result<Node, Error> {
        let record = self.records.get(slot).ok_or(DiskError::Corrupt)?;
        Ok(match record.head()?.tag.kind().ok_or(DiskError::Corrupt)? {
            Kind::Branch(dimension) => {
                let raw: &RawBranch = checked::try_from_bytes(&record.0)?;
                let lower = self.child(raw.lower, slot + 1)?;
                let upper = self.child(raw.upper, slot + 1 + usize::from(raw.lower_len))?;
                Branch::new(dimension, lower, upper).into()
            }
            Kind::LongSkip => {
                let raw: &RawSkip = checked::try_from_bytes(&record.0)?;
                let child = self.child(raw.child, slot + 1)?;
                Skip::new(RelativeFlexId::decode(&raw.path)?, child).into()
            }
        })
    }

    /// 子 `raw` を展開する。同じページの中にあるなら、それは `local_slot` 番目にある。
    fn child(&self, raw: RawNode, local_slot: usize) -> Result<Node, Error> {
        match raw.tag {
            Tag::Local => self.decode(local_slot),
            _ => Option::<Node>::try_from(raw)?.ok_or_else(|| DiskError::Corrupt.into()),
        }
    }
}

/// コミットごとに末尾へ追記するヘッダーのページ。その時点の各ルートを持つ。
#[repr(C)]
#[derive(Clone, Copy, CheckedBitPattern, NoUninit)]
pub(crate) struct RawHeader {
    kind: PageKind,
    _pad: [u8; 7],
    /// [`FlexId::ROOTS`] の各領域のルート。空なら [`RawNode::EMPTY`]。
    upper: RawNode,
    lower: RawNode,
}

impl RawHeader {
    pub(super) fn new([upper, lower]: [RawNode; 2]) -> Self {
        RawHeader {
            kind: PageKind::Header,
            _pad: [0; 7],
            upper,
            lower,
        }
    }

    /// `page` がヘッダーなら、その中身。
    pub(super) fn parse(page: &Page) -> Option<Self> {
        page.try_head_as::<RawHeader>()
            .ok()
            .filter(|header| header.kind == PageKind::Header)
            .copied()
    }

    pub(super) fn roots(&self) -> Result<[Option<Node>; 2], Error> {
        Ok([self.upper.try_into()?, self.lower.try_into()?])
    }
}

/// `page` 番目のページの部分木を読み込む。
pub(super) async fn load_page(file: &PageFile, page: u64) -> Result<Node, Error> {
    let page = file.read(page).await?;
    let raw = page.try_head_as::<RawPage>()?;
    if raw.kind != PageKind::Records {
        return Err(DiskError::Corrupt.into());
    }
    raw.decode(0)
}

/// `node` のうちメモリ上にある部分をページへ詰めて `file` に追記し、`node` を指す RawNode を返す。
pub(super) async fn store(file: &mut PageFile, node: Node) -> Result<RawNode, Error> {
    match layout(file, node).await? {
        Part::Done(raw) => Ok(raw),
        Part::Open(records) => write_page(file, records).await,
    }
}

/// 子の側から割り付けている途中の部分木。
enum Part {
    /// ページに書き終えたか、書く必要がないもの。
    Done(RawNode),
    /// まだ書いていない、根から並べたレコードの列。
    Open(Vec<RawRecord>),
}

impl Part {
    fn len(&self) -> usize {
        match self {
            Part::Done(_) => 0,
            Part::Open(records) => records.len(),
        }
    }
}

/// `node` を子の側からページに割り付ける。根の側のまだ書いていない部分を返す。
async fn layout(file: &mut PageFile, node: Node) -> Result<Part, Error> {
    Ok(match node {
        Node::Value(value) => Part::Done(RawNode::new(Tag::Value, DimensionSet::EMPTY, value)),
        Node::OnDisk {
            page,
            kind,
            split_dimensions,
        } => Part::Done(RawNode::new(kind.into(), split_dimensions, page)),
        Node::Branch(branch) => {
            let Branch {
                dimension,
                split_dimensions,
                lower,
                upper,
            } = Arc::unwrap_or_clone(branch);
            let lower = Box::pin(layout(file, lower)).await?;
            let upper = Box::pin(layout(file, upper)).await?;
            let [(lower, lower_records), (upper, upper_records)] =
                pack(file, [lower, upper]).await?;
            let root = RawBranch {
                head: RawRecordHead {
                    tag: Kind::Branch(dimension).into(),
                    split_dimensions,
                },
                lower_len: lower_records.len() as u8,
                _pad: [0; 5],
                lower,
                upper,
            };
            Part::Open([vec![bytemuck::cast(root)], lower_records, upper_records].concat())
        }
        Node::LongSkip(skip) => {
            let Skip {
                path,
                split_dimensions,
                child,
            } = Arc::unwrap_or_clone(skip);
            let child = Box::pin(layout(file, child)).await?;
            let [(child, child_records)] = pack(file, [child]).await?;
            let root = RawSkip {
                head: RawRecordHead {
                    tag: Tag::LongSkip,
                    split_dimensions,
                },
                path: path.encode(),
                _pad: [0; 2],
                child,
            };
            Part::Open([vec![bytemuck::cast(root)], child_records].concat())
        }
    })
}

/// 根と `children` のまだ書いていない部分が 1 ページに収まるまで、大きい子から順にページとして書いて閉じる。
/// 各子を指す RawNode と、根の後ろに並べる子のレコードの列を返す。
async fn pack<const N: usize>(
    file: &mut PageFile,
    mut children: [Part; N],
) -> Result<[(RawNode, Vec<RawRecord>); N], Error> {
    while 1 + children.iter().map(Part::len).sum::<usize>() > RECORDS_PER_PAGE {
        // 同じ大きさなら前（lower）の子を閉じる
        let largest = children
            .iter_mut()
            .rev()
            .max_by_key(|part| part.len())
            .unwrap();
        if let Part::Open(records) = largest {
            *largest = Part::Done(write_page(file, mem::take(records)).await?);
        }
    }
    Ok(children.map(|part| match part {
        Part::Done(raw) => (raw, Vec::new()),
        Part::Open(records) => (RawNode::LOCAL, records),
    }))
}

/// 先頭を根とするレコードの列を新しいページに書き、そのページを指す RawNode を返す。
async fn write_page(file: &mut PageFile, records: Vec<RawRecord>) -> Result<RawNode, Error> {
    let head = records[0].head()?;
    let mut page = RawPage {
        kind: PageKind::Records,
        _pad: [0; 7],
        records: [RawRecord::zeroed(); RECORDS_PER_PAGE],
    };
    page.records[..records.len()].copy_from_slice(&records);
    let index = file.append(Page::of(&page)).await?;
    Ok(RawNode::new(head.tag, head.split_dimensions, index))
}
