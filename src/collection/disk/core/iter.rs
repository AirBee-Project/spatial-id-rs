use super::FlexTreeCore;
use super::node::{EncodedValue, Node};
use super::node_page::NodePage;
use crate::collection::disk::DiskValue;
use crate::collection::disk::io::{PageId, PagedFile};
use crate::id::flex_id::ops::PreparedRange;
use crate::{CancellationToken, DiskError, Error, FlexId, QueryError, RangeId};
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::iter::FusedIterator;
use core::marker::PhantomData;

/// 木の葉（値を持つ領域）を Z-order 昇順に辿る途中の状態。ページの読み方（同期・非同期）は呼び出し側が決める。
pub(super) struct Leaves {
    /// pop で次に辿るものが出るよう積む。ページは辿り着くまで読まない。
    stack: Vec<Frame>,
}

enum Frame {
    Page(PageId, FlexId),
    Node(Node, FlexId),
}

impl Frame {
    /// 領域 `region` の `node`。RefPage なら、ほかのページとまとめて読めるよう行き先のページとして積む。
    fn of(node: Node, region: FlexId) -> Self {
        match node {
            Node::RefPage { page_id, .. } => Frame::Page(page_id, region),
            node => Frame::Node(node, region),
        }
    }
}

/// [`Leaves::step`] で進んだ結果。
pub(super) enum Step {
    /// 値を持つ領域と、その値。
    Leaf(FlexId, EncodedValue),
    /// 先へ進むのに要るページ。読んだ Node を同じ順に [`Leaves::supply`] で渡す。
    Read(Vec<PageId>),
    Done,
}

impl Leaves {
    /// ルート `roots` のうち `allow` を満たす領域から辿り始める。
    pub(super) fn new(roots: [Option<PageId>; 2], allow: impl Fn(&FlexId) -> bool) -> Self {
        let stack = roots
            .into_iter()
            .zip(FlexId::ROOTS)
            .rev()
            .filter_map(|(root, region)| allow(&region).then_some(Frame::Page(root?, region)))
            .collect();
        Leaves { stack }
    }

    /// `allow` を満たす領域だけを辿り、次の葉か、読む必要のあるページまで進む。
    pub(super) fn step(&mut self, allow: impl Fn(&FlexId) -> bool) -> Result<Step, DiskError> {
        while let Some(frame) = self.stack.pop() {
            let (mut node, mut this) = match frame {
                Frame::Node(node, region) => (node, region),
                Frame::Page(..) => {
                    self.stack.push(frame);
                    // 積んであるページをまとめて読ませ、読み込みを並行に進める
                    let pages = self
                        .stack
                        .iter()
                        .filter_map(|frame| match frame {
                            Frame::Page(page_id, _) => Some(*page_id),
                            Frame::Node(..) => None,
                        })
                        .collect();
                    return Ok(Step::Read(pages));
                }
            };
            loop {
                match node {
                    Node::Value(value) => return Ok(Step::Leaf(this, value)),
                    node @ Node::RefPage { .. } => {
                        self.stack.push(Frame::of(node, this));
                        break;
                    }
                    Node::Skip { path, child } => {
                        this = path.to_absolute(&this).map_err(|_| DiskError::Corrupt)?;
                        if !allow(&this) {
                            break;
                        }
                        node = *child;
                    }
                    Node::Branch {
                        dimension,
                        lower,
                        upper,
                        ..
                    } => {
                        let [lower_region, upper_region] =
                            this.halves(dimension).ok_or(DiskError::Corrupt)?;
                        if allow(&upper_region) {
                            self.stack.push(Frame::of(*upper, upper_region));
                        }
                        if !allow(&lower_region) {
                            break;
                        }
                        node = *lower;
                        this = lower_region;
                    }
                }
            }
        }
        Ok(Step::Done)
    }

    /// [`Step::Read`] で求めたページの Node を、求めた順に渡す。
    pub(super) fn supply(&mut self, nodes: Vec<Node>) {
        let pages = self
            .stack
            .iter_mut()
            .filter(|frame| matches!(frame, Frame::Page(..)));
        for (frame, node) in pages.zip(nodes) {
            let Frame::Page(_, region) = *frame else {
                unreachable!()
            };
            *frame = Frame::Node(node, region);
        }
    }

    /// 同期的にページを読みながら、次の葉まで進む。
    pub(super) fn next_sync(
        &mut self,
        file: &PagedFile,
    ) -> Option<Result<(FlexId, EncodedValue), DiskError>> {
        let result = loop {
            match self.step(|_| true) {
                Ok(Step::Leaf(id, value)) => return Some(Ok((id, value))),
                Ok(Step::Done) => return None,
                Ok(Step::Read(pages)) => match file
                    .read_many_sync(&pages)
                    .and_then(|pages| pages.iter().map(|page| NodePage::read(page)).collect())
                {
                    Ok(nodes) => self.supply(nodes),
                    Err(err) => break err,
                },
                Err(err) => break err,
            }
        };
        self.stack.clear();
        Some(Err(result))
    }

    /// `allow` を満たす領域だけを辿り、キャンセルを確認しながら同期的に次の葉まで進む。
    pub(super) fn next_sync_matching(
        &mut self,
        file: &PagedFile,
        allow: impl Fn(&FlexId) -> bool,
        token: &CancellationToken,
    ) -> Option<Result<(FlexId, EncodedValue), Error>> {
        let err = loop {
            match self.step(&allow) {
                Ok(Step::Leaf(id, value)) => return Some(Ok((id, value))),
                Ok(Step::Done) => return None,
                Ok(Step::Read(pages)) => {
                    if token.is_cancelled() {
                        break Error::from(QueryError::Cancelled);
                    }
                    match file
                        .read_many_sync(&pages)
                        .and_then(|pages| pages.iter().map(|page| NodePage::read(page)).collect())
                    {
                        Ok(nodes) => self.supply(nodes),
                        Err(err) => break Error::from(err),
                    }
                }
                Err(err) => break Error::from(err),
            }
        };
        self.stack.clear();
        Some(Err(err))
    }
}

/// `get_overlapping` で使う交差判定ターゲット集合。
pub(super) enum OverlapTargets {
    Empty,
    One(PreparedRange),
    Many(Vec<PreparedRange>),
}

impl OverlapTargets {
    pub(super) fn new(target: impl IntoIterator<Item = impl Into<RangeId>>) -> Self {
        let mut iter = target.into_iter().map(|t| PreparedRange::new(t.into()));
        match iter.next() {
            None => Self::Empty,
            Some(first) => match iter.next() {
                None => Self::One(first),
                Some(second) => {
                    let mut rest = Vec::with_capacity(iter.size_hint().0 + 2);
                    rest.push(first);
                    rest.push(second);
                    rest.extend(iter);
                    Self::Many(rest)
                }
            },
        }
    }

    #[inline(always)]
    pub(super) fn matches(&self, region: &FlexId) -> bool {
        match self {
            Self::Empty => false,
            Self::One(t) => t.intersects(region),
            Self::Many(ts) => ts.iter().any(|t| t.intersects(region)),
        }
    }
}

/// 値を持つ領域と値を、ページ単位で同期的に読み出しながら返すイテレーター。`F` は借りたファイルか所有するファイル。
struct Entries<F, V> {
    file: F,
    leaves: Leaves,
    value: PhantomData<fn() -> V>,
}

impl<F: Borrow<PagedFile>, V: DiskValue> Iterator for Entries<F, V> {
    type Item = Result<(FlexId, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self
            .leaves
            .next_sync(self.file.borrow())?
            .and_then(|(id, value)| V::decode(value.as_bytes()).map(|value| (id, value)));
        if entry.is_err() {
            self.leaves.stack.clear();
        }
        Some(entry.map_err(Error::from))
    }
}

/// 値を持つ領域と値の組 `(FlexId, V)` を、ページ単位で同期的に読み出しながら返すイテレーター。
///
/// [`SpatialIdMap`](crate::collection::disk::SpatialIdMap) や [`SpatialIdSet`](crate::collection::disk::SpatialIdSet) の参照イテレーターで使われる。
pub struct Iter<'a, V>(Entries<&'a PagedFile, V>);

impl<'a, V: DiskValue> Iter<'a, V> {
    pub(super) fn new(tree: &'a FlexTreeCore<V>) -> Self {
        Self(Entries {
            file: &tree.file,
            leaves: Leaves::new(tree.roots, |_| true),
            value: PhantomData,
        })
    }
}

impl<V: DiskValue> Iterator for Iter<'_, V> {
    type Item = Result<(FlexId, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<V: DiskValue> FusedIterator for Iter<'_, V> {}

/// 所有権を消費し、値を持つ領域と値の組 `(FlexId, V)` をページ単位で同期的に読み出しながら返すイテレーター。
///
/// [`SpatialIdMap::into_iter_flex_ids`](crate::collection::disk::SpatialIdMap::into_iter_flex_ids) や `into_iter` が返す。
pub struct IntoIter<V>(Entries<PagedFile, V>);

impl<V: DiskValue> Iterator for IntoIter<V> {
    type Item = Result<(FlexId, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<V: DiskValue> FusedIterator for IntoIter<V> {}

/// `targets` と交差する領域だけを、ページ読み込み前に枝刈りしつつ同期的に返すイテレーター。
pub(crate) struct OverlappingIter<'a, V> {
    file: &'a PagedFile,
    leaves: Leaves,
    targets: OverlapTargets,
    token: CancellationToken,
    value: PhantomData<fn() -> V>,
}

impl<'a, V: DiskValue> OverlappingIter<'a, V> {
    pub(super) fn new(
        tree: &'a FlexTreeCore<V>,
        targets: &[RangeId],
        token: &CancellationToken,
    ) -> Self {
        let targets = OverlapTargets::new(targets.iter().copied());
        let leaves = Leaves::new(tree.roots, |region| targets.matches(region));
        Self {
            file: &tree.file,
            leaves,
            targets,
            token: token.clone(),
            value: PhantomData,
        }
    }
}

impl<V: DiskValue> Iterator for OverlappingIter<'_, V> {
    type Item = Result<(FlexId, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self
            .leaves
            .next_sync_matching(self.file, |r| self.targets.matches(r), &self.token)?
            .and_then(|(id, encoded)| {
                V::decode(encoded.as_bytes())
                    .map(|value| (id, value))
                    .map_err(Error::from)
            });
        if entry.is_err() {
            self.leaves.stack.clear();
        }
        Some(entry)
    }
}

impl<V: DiskValue> FusedIterator for OverlappingIter<'_, V> {}

impl<'a, V: DiskValue> IntoIterator for &'a FlexTreeCore<V> {
    type Item = Result<(FlexId, V), Error>;
    type IntoIter = Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<V: DiskValue> IntoIterator for FlexTreeCore<V> {
    type Item = Result<(FlexId, V), Error>;
    type IntoIter = IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(Entries {
            file: self.file,
            leaves: Leaves::new(self.roots, |_| true),
            value: PhantomData,
        })
    }
}
