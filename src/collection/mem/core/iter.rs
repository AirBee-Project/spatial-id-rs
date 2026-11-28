use alloc::vec::Vec;
use core::iter::FusedIterator;
use triomphe::Arc;

use super::FlexTreeCore;
use super::node::{Branch, Node};
use crate::{FlexId, Side, id::dimension::Dimension};

/// [FlexTreeCore]の値を持つ領域と値への参照を、Node を辿りながら1つずつ返すイテレーター。
#[derive(Debug)]
pub(crate) struct Iter<'a, V> {
    stack: Vec<(&'a Node<V>, FlexId)>,
    /// まだ返していない要素の数。
    remaining: usize,
}

impl<'a, V> Iter<'a, V> {
    pub(super) fn new(tree: &'a FlexTreeCore<V>) -> Self {
        Iter {
            stack: tree.new_stack(),
            remaining: tree.count(),
        }
    }
}

impl<V> Clone for Iter<'_, V> {
    fn clone(&self) -> Self {
        Iter {
            stack: self.stack.clone(),
            remaining: self.remaining,
        }
    }
}

impl<'a, V> Iterator for Iter<'a, V> {
    type Item = (FlexId, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let (mut node, mut this) = self.stack.pop()?;
        loop {
            match node {
                Node::Value(value) => {
                    self.remaining -= 1;
                    return Some((this, value));
                }
                Node::ShortSkipValue(path, value) => {
                    self.remaining -= 1;
                    return Some((path.to_absolute(&this), value));
                }
                Node::Branch(branch) => {
                    (node, this) = push_upper_ref(&mut self.stack, this, branch);
                }
                Node::ShortSkipBranch(path, branch) => {
                    let region = path.to_absolute(&this);
                    (node, this) = push_upper_ref(&mut self.stack, region, branch);
                }
                Node::LongSkip(skip) => {
                    this = skip.path.to_absolute(&this).unwrap();
                    node = &skip.child;
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }

    fn fold<B, F>(mut self, mut init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        while let Some((node, this)) = self.stack.pop() {
            init = fold_node_ref(node, this, init, &mut f);
        }
        init
    }
}

/// 領域 `this` の `branch` の上側を `stack` へ積み、そのまま降りる下側を返す。
#[inline(always)]
fn push_upper_ref<'a, V>(
    stack: &mut Vec<(&'a Node<V>, FlexId)>,
    this: FlexId,
    branch: &'a Branch<V>,
) -> (&'a Node<V>, FlexId) {
    // 走査はポインタを辿るキャッシュミスが律速になる。upper は lower 側の部分木を返し終えるまで使わないので、その間に読み込ませておく
    branch.upper.prefetch();
    let dimension = branch.dimension;
    stack.push((
        &branch.upper,
        this.split_on(dimension, Side::Upper).unwrap(),
    ));
    (
        &branch.lower,
        this.split_on(dimension, Side::Lower).unwrap(),
    )
}

fn fold_node_ref<'a, V, B, F>(mut node: &'a Node<V>, mut this: FlexId, mut acc: B, f: &mut F) -> B
where
    F: FnMut(B, (FlexId, &'a V)) -> B,
{
    loop {
        match node {
            Node::Value(value) => return f(acc, (this, value)),
            Node::ShortSkipValue(path, value) => return f(acc, (path.to_absolute(&this), value)),
            Node::Branch(branch) => {
                branch.upper.prefetch();
                let dimension = branch.dimension;
                acc = fold_node_ref(
                    &branch.lower,
                    this.split_on(dimension, Side::Lower).unwrap(),
                    acc,
                    f,
                );
                node = &branch.upper;
                this = this.split_on(dimension, Side::Upper).unwrap();
            }
            Node::ShortSkipBranch(path, branch) => {
                let region = path.to_absolute(&this);
                branch.upper.prefetch();
                let dimension = branch.dimension;
                acc = fold_node_ref(
                    &branch.lower,
                    region.split_on(dimension, Side::Lower).unwrap(),
                    acc,
                    f,
                );
                node = &branch.upper;
                this = region.split_on(dimension, Side::Upper).unwrap();
            }
            Node::LongSkip(skip) => {
                this = skip.path.to_absolute(&this).unwrap();
                node = &skip.child;
            }
        }
    }
}

impl<V> ExactSizeIterator for Iter<'_, V> {}

impl<V> FusedIterator for Iter<'_, V> {}

/// 値を持つ領域と値の組 `(FlexId, V)` を、所有権ごと1つずつ返すイテレーター。
///
/// [`SpatialIdMap::remove`](crate::SpatialIdMap::remove) や [`SpatialIdMap`](crate::SpatialIdMap) の `into_iter` が返す
/// [`MapIter`](crate::collection::mem::MapIter) に包まれ、[`MapIter::flex_ids`](crate::collection::mem::MapIter::flex_ids) で取り出せる。
#[derive(Debug)]
pub struct IntoIter<V> {
    stack: Vec<(Node<V>, FlexId)>,
    /// まだ返していない要素の数。
    remaining: usize,
}

impl<V: Clone> Iterator for IntoIter<V> {
    type Item = (FlexId, V);

    fn next(&mut self) -> Option<Self::Item> {
        let (mut node, mut this) = self.stack.pop()?;
        loop {
            match node {
                Node::Value(value) => {
                    self.remaining -= 1;
                    return Some((this, value));
                }
                Node::ShortSkipValue(path, value) => {
                    self.remaining -= 1;
                    return Some((path.to_absolute(&this), value));
                }
                Node::Branch(branch) => {
                    (node, this) = push_upper_owned(&mut self.stack, this, branch);
                }
                Node::ShortSkipBranch(path, branch) => {
                    let region = path.to_absolute(&this);
                    (node, this) = push_upper_owned(&mut self.stack, region, branch);
                }
                Node::LongSkip(skip) => {
                    this = skip.path.to_absolute(&this).unwrap();
                    node = Arc::try_unwrap(skip)
                        .map_or_else(|skip| skip.child.clone(), |skip| skip.child);
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }

    fn fold<B, F>(mut self, mut init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        while let Some((node, this)) = self.stack.pop() {
            init = fold_node_owned(node, this, init, &mut f);
        }
        init
    }
}

#[inline(always)]
fn unwrap_branch<V: Clone>(branch: Arc<Branch<V>>) -> (Dimension, Node<V>, Node<V>) {
    match Arc::try_unwrap(branch) {
        Ok(branch) => (branch.dimension, branch.lower, branch.upper),
        Err(branch) => (branch.dimension, branch.lower.clone(), branch.upper.clone()),
    }
}

/// 領域 `this` の `branch` の上側を `stack` へ積み、そのまま降りる下側を返す。
#[inline(always)]
fn push_upper_owned<V: Clone>(
    stack: &mut Vec<(Node<V>, FlexId)>,
    this: FlexId,
    branch: Arc<Branch<V>>,
) -> (Node<V>, FlexId) {
    let (dimension, lower, upper) = unwrap_branch(branch);
    upper.prefetch();
    stack.push((upper, this.split_on(dimension, Side::Upper).unwrap()));
    (lower, this.split_on(dimension, Side::Lower).unwrap())
}

fn fold_node_owned<V: Clone, B, F>(mut node: Node<V>, mut this: FlexId, mut acc: B, f: &mut F) -> B
where
    F: FnMut(B, (FlexId, V)) -> B,
{
    loop {
        match node {
            Node::Value(value) => return f(acc, (this, value)),
            Node::ShortSkipValue(path, value) => return f(acc, (path.to_absolute(&this), value)),
            Node::Branch(branch) => {
                let (dimension, lower, upper) = unwrap_branch(branch);
                upper.prefetch();
                acc = fold_node_owned(
                    lower,
                    this.split_on(dimension, Side::Lower).unwrap(),
                    acc,
                    f,
                );
                node = upper;
                this = this.split_on(dimension, Side::Upper).unwrap();
            }
            Node::ShortSkipBranch(path, branch) => {
                let region = path.to_absolute(&this);
                let (dimension, lower, upper) = unwrap_branch(branch);
                upper.prefetch();
                acc = fold_node_owned(
                    lower,
                    region.split_on(dimension, Side::Lower).unwrap(),
                    acc,
                    f,
                );
                node = upper;
                this = region.split_on(dimension, Side::Upper).unwrap();
            }
            Node::LongSkip(skip) => {
                this = skip.path.to_absolute(&this).unwrap();
                node =
                    Arc::try_unwrap(skip).map_or_else(|skip| skip.child.clone(), |skip| skip.child);
            }
        }
    }
}

impl<V: Clone> ExactSizeIterator for IntoIter<V> {}

impl<V: Clone> FusedIterator for IntoIter<V> {}

impl<'a, V> IntoIterator for &'a FlexTreeCore<V> {
    type Item = (FlexId, &'a V);
    type IntoIter = Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<V: Clone> IntoIterator for FlexTreeCore<V> {
    type Item = (FlexId, V);
    type IntoIter = IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        let remaining = self.count();
        let height = self.max_height();
        let mut stack = Vec::with_capacity(usize::from(height) + 1);
        // 後に積んだものから取り出すので、上側のルートを後に積む
        if let Some(lower) = self.lower_root {
            stack.push((lower, FlexId::LOWER_MAX));
        }
        if let Some(upper) = self.upper_root {
            stack.push((upper, FlexId::UPPER_MAX));
        }
        IntoIter { stack, remaining }
    }
}

impl<V: PartialEq + Clone> Extend<(FlexId, V)> for FlexTreeCore<V> {
    /// 順に [`insert`](FlexTreeCore::insert) する。重なる場所は後の値で上書きされる。
    fn extend<I: IntoIterator<Item = (FlexId, V)>>(&mut self, iter: I) {
        for (id, value) in iter {
            self.insert(id, value);
        }
    }
}

/// FlexId の Z-order 列（`prev < next`）から、1つのルート配下の Node を下から O(n) で組む。
struct OrderedRootBuilder<V> {
    root_id: FlexId,
    prev_id: FlexId,
    cur_region: FlexId,
    cur_node: Option<Node<V>>,
    stack: Vec<(FlexId, Dimension, u8, Node<V>)>,
}

impl<V: PartialEq + Clone> OrderedRootBuilder<V> {
    fn new(root_id: FlexId, id: FlexId, value: V) -> Self {
        Self {
            root_id,
            prev_id: id,
            cur_region: id,
            cur_node: Some(Node::Value(value)),
            stack: Vec::new(),
        }
    }

    fn try_push(&mut self, id: FlexId, value: V) -> Result<(), (FlexId, V)> {
        let Some((fork, split_dim, agree)) = self.prev_id.zorder_fork(&id) else {
            return Err((id, value));
        };
        while let Some(&(top_fork, top_dim, top_agree, _)) = self.stack.last() {
            if (top_agree, top_dim) <= (agree, split_dim) {
                break;
            }
            let (_, _, _, lower) = self.stack.pop().unwrap();
            let upper_id = top_fork.split_on(top_dim, Side::Upper).unwrap();
            let cur = self.cur_node.take().unwrap();
            let upper = Node::rebase(cur, &self.cur_region, &upper_id);
            self.cur_node = Some(Node::join(&top_fork, top_dim, Some(lower), Some(upper)).unwrap());
            self.cur_region = top_fork;
        }
        let lower_id = fork.split_on(split_dim, Side::Lower).unwrap();
        let cur = self.cur_node.take().unwrap();
        let lower = Node::rebase(cur, &self.cur_region, &lower_id);
        self.stack.push((fork, split_dim, agree, lower));
        self.prev_id = id;
        self.cur_region = id;
        self.cur_node = Some(Node::Value(value));
        Ok(())
    }

    fn finish(mut self) -> Node<V> {
        let mut cur = self.cur_node.take().unwrap();
        while let Some((top_fork, top_dim, _, lower)) = self.stack.pop() {
            let upper_id = top_fork.split_on(top_dim, Side::Upper).unwrap();
            let upper = Node::rebase(cur, &self.cur_region, &upper_id);
            cur = Node::join(&top_fork, top_dim, Some(lower), Some(upper)).unwrap();
            self.cur_region = top_fork;
        }
        Node::rebase(cur, &self.cur_region, &self.root_id)
    }
}

fn finish_builders<V: PartialEq + Clone>(
    upper: Option<OrderedRootBuilder<V>>,
    lower: Option<OrderedRootBuilder<V>>,
) -> FlexTreeCore<V> {
    FlexTreeCore {
        upper_root: upper.map(OrderedRootBuilder::finish),
        lower_root: lower.map(OrderedRootBuilder::finish),
    }
}

impl<V: PartialEq + Clone> FromIterator<(FlexId, V)> for FlexTreeCore<V> {
    /// FlexId の Z-order 列なら下から O(n) で組み立て、途中で順序が崩れていれば [`insert`](FlexTreeCore::insert) に切り替える。
    fn from_iter<I: IntoIterator<Item = (FlexId, V)>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let mut upper: Option<OrderedRootBuilder<V>> = None;
        let mut lower: Option<OrderedRootBuilder<V>> = None;

        for (id, value) in iter.by_ref() {
            let root_idx = id.root_index();
            let slot = if root_idx == 0 {
                if lower.is_some() {
                    let mut tree = finish_builders(upper, lower);
                    tree.insert(id, value);
                    tree.extend(iter);
                    return tree;
                }
                &mut upper
            } else {
                &mut lower
            };

            match slot {
                None => *slot = Some(OrderedRootBuilder::new(FlexId::ROOTS[root_idx], id, value)),
                Some(builder) => {
                    if let Err((id, value)) = builder.try_push(id, value) {
                        let mut tree = finish_builders(upper, lower);
                        tree.insert(id, value);
                        tree.extend(iter);
                        return tree;
                    }
                }
            }
        }

        finish_builders(upper, lower)
    }
}

/// スレッドごとに部分木を組み、和集合で畳む（`feature = "rayon"`）。
///
/// 同じ場所へ異なる値が重なった場合にどちらが残るかは、チャンクの分かれ方で決まり、
/// 逐次の [`FromIterator`] の後勝ちとは一致しない。
#[cfg(feature = "rayon")]
impl<V> rayon::iter::FromParallelIterator<(FlexId, V)> for FlexTreeCore<V>
where
    V: PartialEq + Clone + Send + Sync,
{
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: rayon::iter::IntoParallelIterator<Item = (FlexId, V)>,
    {
        use rayon::prelude::*;
        par_iter
            .into_par_iter()
            .fold(FlexTreeCore::default, |mut tree, (id, value)| {
                tree.insert(id, value);
                tree
            })
            .reduce(FlexTreeCore::default, |a, b| a.union(&b))
    }
}

/// 並列に組んだ木を和集合で重ねる（`feature = "rayon"`）。重なる場所は追加する側の値になる。
#[cfg(feature = "rayon")]
impl<V> rayon::iter::ParallelExtend<(FlexId, V)> for FlexTreeCore<V>
where
    V: PartialEq + Clone + Send + Sync,
{
    fn par_extend<I>(&mut self, par_iter: I)
    where
        I: rayon::iter::IntoParallelIterator<Item = (FlexId, V)>,
    {
        use rayon::iter::FromParallelIterator;
        *self = Self::from_par_iter(par_iter).union(self);
    }
}
