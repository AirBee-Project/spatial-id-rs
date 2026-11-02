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
        while let Some((node, this)) = self.stack.pop() {
            match node {
                Node::Leaf(value) => {
                    self.remaining -= 1;
                    return Some((this, value));
                }
                Node::LeafAt(path, value) => {
                    self.remaining -= 1;
                    return Some((path.to_absolute(&this), value));
                }
                Node::Branch(branch) => self.push_branch(this, branch),
                Node::BranchAt(path, branch) => self.push_branch(path.to_absolute(&this), branch),
                Node::Skip(skip) => self
                    .stack
                    .push((&skip.child, skip.path.to_absolute(&this).unwrap())),
                Node::Empty => {}
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, V> Iter<'a, V> {
    /// 領域 `this` の `branch` の子を、下側から取り出されるように積む。
    fn push_branch(&mut self, this: FlexId, branch: &'a Branch<V>) {
        let dimension = branch.dimension;
        self.stack.push((
            &branch.upper,
            this.split_on(dimension, Side::Upper).unwrap(),
        ));
        self.stack.push((
            &branch.lower,
            this.split_on(dimension, Side::Lower).unwrap(),
        ));
    }
}

impl<V> ExactSizeIterator for Iter<'_, V> {}

impl<V> FusedIterator for Iter<'_, V> {}

/// 値を持つ領域と値の組 `(FlexId, V)` を、所有権ごと1つずつ返すイテレーター。
///
/// [`SpatialIdMap::remove`](crate::SpatialIdMap::remove) や、[`SpatialIdMap`](crate::SpatialIdMap) の `into_iter` が返す。
#[derive(Debug)]
pub struct IntoIter<V> {
    stack: Vec<(Node<V>, FlexId)>,
    /// まだ返していない要素の数。
    remaining: usize,
}

impl<V: Clone> Iterator for IntoIter<V> {
    type Item = (FlexId, V);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((node, this)) = self.stack.pop() {
            match node {
                Node::Empty => {}
                Node::Leaf(value) => {
                    self.remaining -= 1;
                    return Some((this, value));
                }
                Node::LeafAt(path, value) => {
                    self.remaining -= 1;
                    return Some((path.to_absolute(&this), value));
                }
                Node::Branch(branch) => self.push_branch(this, branch),
                Node::BranchAt(path, branch) => self.push_branch(path.to_absolute(&this), branch),
                Node::Skip(skip) => {
                    let region = skip.path.to_absolute(&this).unwrap();
                    let child = Arc::try_unwrap(skip)
                        .map_or_else(|skip| skip.child.clone(), |skip| skip.child);
                    self.stack.push((child, region));
                }
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<V> IntoIter<V> {
    /// 領域 `this` の `branch` の子を、[Iter]と同じく下側から取り出されるように積む。
    fn push_branch(&mut self, this: FlexId, branch: Arc<Branch<V>>)
    where
        V: Clone,
    {
        let (dimension, lower, upper) = match Arc::try_unwrap(branch) {
            Ok(branch) => (branch.dimension, branch.lower, branch.upper),
            Err(branch) => (branch.dimension, branch.lower.clone(), branch.upper.clone()),
        };
        self.stack
            .push((upper, this.split_on(dimension, Side::Upper).unwrap()));
        self.stack
            .push((lower, this.split_on(dimension, Side::Lower).unwrap()));
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
        let height = self.upper_root.height().max(self.lower_root.height());
        let mut stack = Vec::with_capacity(usize::from(height) + 2);
        // 後に積んだものから取り出すので、上側のルートを後に積む
        stack.push((self.lower_root, FlexId::LOWER_MAX));
        stack.push((self.upper_root, FlexId::UPPER_MAX));
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
    cur_node: Node<V>,
    stack: Vec<(FlexId, Dimension, u8, Node<V>)>,
}

impl<V: PartialEq + Clone> OrderedRootBuilder<V> {
    fn new(root_id: FlexId, id: FlexId, value: V) -> Self {
        Self {
            root_id,
            prev_id: id,
            cur_region: id,
            cur_node: Node::Leaf(value),
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
            let upper = Node::rebase(
                core::mem::take(&mut self.cur_node),
                &self.cur_region,
                &upper_id,
            );
            self.cur_node = Node::join(&top_fork, top_dim, lower, upper);
            self.cur_region = top_fork;
        }
        let lower_id = fork.split_on(split_dim, Side::Lower).unwrap();
        let lower = Node::rebase(
            core::mem::take(&mut self.cur_node),
            &self.cur_region,
            &lower_id,
        );
        self.stack.push((fork, split_dim, agree, lower));
        self.prev_id = id;
        self.cur_region = id;
        self.cur_node = Node::Leaf(value);
        Ok(())
    }

    fn finish(mut self) -> Node<V> {
        while let Some((top_fork, top_dim, _, lower)) = self.stack.pop() {
            let upper_id = top_fork.split_on(top_dim, Side::Upper).unwrap();
            let upper = Node::rebase(self.cur_node, &self.cur_region, &upper_id);
            self.cur_node = Node::join(&top_fork, top_dim, lower, upper);
            self.cur_region = top_fork;
        }
        Node::rebase(self.cur_node, &self.cur_region, &self.root_id)
    }
}

fn finish_builders<V: PartialEq + Clone>(
    upper: Option<OrderedRootBuilder<V>>,
    lower: Option<OrderedRootBuilder<V>>,
) -> FlexTreeCore<V> {
    FlexTreeCore {
        upper_root: upper.map_or(Node::Empty, OrderedRootBuilder::finish),
        lower_root: lower.map_or(Node::Empty, OrderedRootBuilder::finish),
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
