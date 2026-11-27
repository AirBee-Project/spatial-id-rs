use super::{
    FlexTreeCore,
    node::{Branch, Node, Skip},
    value::CompactValue,
};
use crate::id::{dimension::Dimension, relative_flex_id::RelativeFlexId};
use core::{
    hash::{BuildHasher, Hash, Hasher},
    mem::discriminant,
};
use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};
use triomphe::Arc;

impl<V: Eq + Hash + Clone> FlexTreeCore<V> {
    /// 同じ形状と値を持つ [`Branch`]・[`LongSkip`](Node::LongSkip)・退避済みの値を探索し、同一の `Arc` を共有するよう圧縮する。
    ///
    /// 保持している [`FlexId`](crate::FlexId) と値の対応は変わらない。他の [`FlexTreeCore`] と共有されている `Node` は書き換えず、
    /// 新しい `Branch` や `LongSkip` のみを組み立てて共有する。
    ///
    /// ユニークな `Node` 数を n として O(n) 時間で動作し、実行中のみ O(n) の作業メモリを使用する。
    pub fn compact(&mut self) {
        let mut compactor = Compactor::new();
        for root in [&mut self.upper_root, &mut self.lower_root]
            .into_iter()
            .flatten()
        {
            compactor.compact_node(root);
        }
    }
}

/// 既に正規化済みの子孫について、同じ `Arc` ポインタまたは同じインライン値を同一とみなすビュー。
struct ShallowNode<'a, V>(&'a Node<V>);

impl<V: PartialEq> PartialEq for ShallowNode<'_, V> {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Node::Value(a), Node::Value(b)) => {
                if CompactValue::<V>::IS_INLINE {
                    a == b
                } else {
                    a.ptr_eq(b)
                }
            }
            (Node::Branch(a), Node::Branch(b)) => Arc::ptr_eq(a, b),
            (Node::LongSkip(a), Node::LongSkip(b)) => Arc::ptr_eq(a, b),
            (Node::ShortSkipValue(pa, va), Node::ShortSkipValue(pb, vb)) => {
                pa == pb
                    && if CompactValue::<V>::IS_INLINE {
                        va == vb
                    } else {
                        va.ptr_eq(vb)
                    }
            }
            (Node::ShortSkipBranch(pa, ba), Node::ShortSkipBranch(pb, bb)) => {
                pa == pb && Arc::ptr_eq(ba, bb)
            }
            _ => false,
        }
    }
}

impl<V: Eq> Eq for ShallowNode<'_, V> {}

impl<V: Hash> Hash for ShallowNode<'_, V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self.0).hash(state);
        match self.0 {
            Node::Value(v) => match v.as_arc() {
                Some(arc) => Arc::as_ptr(arc).hash(state),
                None => v.hash(state),
            },
            Node::Branch(arc) => Arc::as_ptr(arc).hash(state),
            Node::LongSkip(arc) => Arc::as_ptr(arc).hash(state),
            Node::ShortSkipValue(path, v) => {
                path.hash(state);
                match v.as_arc() {
                    Some(arc) => Arc::as_ptr(arc).hash(state),
                    None => v.hash(state),
                }
            }
            Node::ShortSkipBranch(path, arc) => {
                path.hash(state);
                Arc::as_ptr(arc).hash(state);
            }
        }
    }
}

trait Internable<V>: Sized {
    type Key<'a>: Hash
    where
        Self: 'a,
        V: 'a;

    fn shallow_key(&self) -> Self::Key<'_>;
    fn matches_key(&self, key: &Self::Key<'_>) -> bool;
}

impl<V: Eq + Hash> Internable<V> for Branch<V> {
    type Key<'a>
        = (Dimension, ShallowNode<'a, V>, ShallowNode<'a, V>)
    where
        V: 'a;

    fn shallow_key(&self) -> Self::Key<'_> {
        (
            self.dimension,
            ShallowNode(&self.lower),
            ShallowNode(&self.upper),
        )
    }

    fn matches_key(&self, key: &Self::Key<'_>) -> bool {
        self.dimension == key.0
            && ShallowNode(&self.lower) == key.1
            && ShallowNode(&self.upper) == key.2
    }
}

impl<V: Eq + Hash> Internable<V> for Skip<V> {
    type Key<'a>
        = (RelativeFlexId, ShallowNode<'a, V>)
    where
        V: 'a;

    fn shallow_key(&self) -> Self::Key<'_> {
        (self.path, ShallowNode(&self.child))
    }

    fn matches_key(&self, key: &Self::Key<'_>) -> bool {
        self.path == key.0 && ShallowNode(&self.child) == key.1
    }
}

struct Pool<T> {
    hasher: DefaultHashBuilder,
    // 表の拡張時に全エントリのキーを再ハッシュしないよう、ハッシュ値を併せて持つ
    interned: HashTable<(u64, Arc<T>)>,
    // 元の `Arc` を保持して解放を防ぎ、探索中のポインタアドレス再利用（ABA問題）を避ける
    visited: HashTable<(Arc<T>, Arc<T>)>,
}

impl<T> Pool<T> {
    fn new() -> Self {
        Self {
            hasher: DefaultHashBuilder::default(),
            interned: HashTable::new(),
            visited: HashTable::new(),
        }
    }

    /// 挿入時に要るハッシュ値を `Entry` と併せて返す。
    fn entry<'a, V>(&'a mut self, key: &T::Key<'_>) -> (u64, Entry<'a, (u64, Arc<T>)>)
    where
        T: Internable<V>,
    {
        let h = self.hasher.hash_one(key);
        let entry = self
            .interned
            .entry(h, |(_, existing)| existing.matches_key(key), |&(h, _)| h);
        (h, entry)
    }

    fn intern_existing<V>(&mut self, arc: &Arc<T>) -> Option<Arc<T>>
    where
        T: Internable<V>,
    {
        let key = arc.shallow_key();
        match self.entry(&key) {
            (_, Entry::Occupied(entry)) => {
                let (_, found) = entry.get();
                (!Arc::ptr_eq(found, arc)).then(|| Arc::clone(found))
            }
            (h, Entry::Vacant(entry)) => {
                entry.insert((h, Arc::clone(arc)));
                None
            }
        }
    }

    fn find_visited(&self, orig: &Arc<T>) -> Option<&Arc<T>> {
        let h = self.hasher.hash_one(Arc::as_ptr(orig));
        self.visited
            .find(h, |(k, _)| Arc::ptr_eq(k, orig))
            .map(|(_, v)| v)
    }

    fn insert_visited(&mut self, orig: Arc<T>, canonical: Arc<T>) {
        let h = self.hasher.hash_one(Arc::as_ptr(&orig));
        let hasher = &self.hasher;
        self.visited.insert_unique(h, (orig, canonical), |(k, _)| {
            hasher.hash_one(Arc::as_ptr(k))
        });
    }
}

struct Compactor<V> {
    branches: Pool<Branch<V>>,
    skips: Pool<Skip<V>>,
    values_hasher: DefaultHashBuilder,
    values: HashTable<(u64, Arc<V>)>,
}

impl<V: Eq + Hash + Clone> Compactor<V> {
    fn new() -> Self {
        Self {
            branches: Pool::new(),
            skips: Pool::new(),
            values_hasher: DefaultHashBuilder::default(),
            values: HashTable::new(),
        }
    }

    fn compact_node(&mut self, node: &mut Node<V>) {
        match node {
            Node::Value(value) | Node::ShortSkipValue(_, value) => self.compact_value_mut(value),
            Node::Branch(arc) | Node::ShortSkipBranch(_, arc) => self.compact_branch_mut(arc),
            Node::LongSkip(arc) => self.compact_skip_mut(arc),
        }
    }

    fn compact_node_shared(&mut self, node: &Node<V>) -> Option<Node<V>> {
        match node {
            Node::Value(value) => self.compact_value_ref(value).map(Node::Value),
            Node::ShortSkipValue(path, value) => self
                .compact_value_ref(value)
                .map(|v| Node::ShortSkipValue(*path, v)),
            Node::Branch(arc) => self.compact_branch_ref(arc).map(Node::Branch),
            Node::ShortSkipBranch(path, arc) => self
                .compact_branch_ref(arc)
                .map(|b| Node::ShortSkipBranch(*path, b)),
            Node::LongSkip(arc) => self.compact_skip_ref(arc).map(Node::LongSkip),
        }
    }

    fn compact_value_mut(&mut self, value: &mut CompactValue<V>) {
        if let Some(canonical) = self.compact_value_ref(value) {
            *value = canonical;
        }
    }

    fn compact_value_ref(&mut self, value: &CompactValue<V>) -> Option<CompactValue<V>> {
        let arc = value.as_arc()?;
        let h = self.values_hasher.hash_one(&**arc);
        match self
            .values
            .entry(h, |(_, existing)| **existing == **arc, |&(h, _)| h)
        {
            Entry::Occupied(entry) => {
                let (_, found) = entry.get();
                (!Arc::ptr_eq(found, arc)).then(|| CompactValue::from_arc(Arc::clone(found)))
            }
            Entry::Vacant(entry) => {
                entry.insert((h, Arc::clone(arc)));
                None
            }
        }
    }

    fn memoize_shared<T>(
        &mut self,
        pool: fn(&mut Self) -> &mut Pool<T>,
        arc: &Arc<T>,
        compact: impl FnOnce(&mut Self) -> Option<Arc<T>>,
    ) -> Option<Arc<T>> {
        if Arc::is_unique(arc) {
            return compact(self);
        }
        if let Some(cached) = pool(self).find_visited(arc) {
            return (!Arc::ptr_eq(arc, cached)).then(|| Arc::clone(cached));
        }
        let replacement = compact(self);
        let canonical = Arc::clone(replacement.as_ref().unwrap_or(arc));
        pool(self).insert_visited(Arc::clone(arc), canonical);
        replacement
    }

    fn compact_branch_mut(&mut self, arc: &mut Arc<Branch<V>>) {
        let replacement = if let Some(branch) = Arc::get_mut(arc) {
            self.compact_node(&mut branch.lower);
            self.compact_node(&mut branch.upper);
            self.branches.intern_existing(arc)
        } else {
            self.compact_branch_ref(arc)
        };
        if let Some(canonical) = replacement {
            *arc = canonical;
        }
    }

    fn compact_skip_mut(&mut self, arc: &mut Arc<Skip<V>>) {
        let replacement = if let Some(skip) = Arc::get_mut(arc) {
            self.compact_node(&mut skip.child);
            self.skips.intern_existing(arc)
        } else {
            self.compact_skip_ref(arc)
        };
        if let Some(canonical) = replacement {
            *arc = canonical;
        }
    }

    fn compact_branch_ref(&mut self, arc: &Arc<Branch<V>>) -> Option<Arc<Branch<V>>> {
        self.memoize_shared(
            |s| &mut s.branches,
            arc,
            |this| match (
                this.compact_node_shared(&arc.lower),
                this.compact_node_shared(&arc.upper),
            ) {
                (None, None) => this.branches.intern_existing(arc),
                (lower, upper) => Some(this.intern_rebuilt_branch(arc, lower, upper)),
            },
        )
    }

    fn compact_skip_ref(&mut self, arc: &Arc<Skip<V>>) -> Option<Arc<Skip<V>>> {
        self.memoize_shared(
            |s| &mut s.skips,
            arc,
            |this| match this.compact_node_shared(&arc.child) {
                None => this.skips.intern_existing(arc),
                Some(child) => Some(this.intern_rebuilt_skip(arc, child)),
            },
        )
    }

    fn intern_rebuilt_branch(
        &mut self,
        orig: &Branch<V>,
        lower_node: Option<Node<V>>,
        upper_node: Option<Node<V>>,
    ) -> Arc<Branch<V>> {
        let key = (
            orig.dimension,
            ShallowNode(lower_node.as_ref().unwrap_or(&orig.lower)),
            ShallowNode(upper_node.as_ref().unwrap_or(&orig.upper)),
        );
        match self.branches.entry(&key) {
            (_, Entry::Occupied(entry)) => Arc::clone(&entry.get().1),
            (h, Entry::Vacant(entry)) => {
                let new_arc = Arc::new(Branch {
                    dimension: orig.dimension,
                    split_dimensions: orig.split_dimensions,
                    height: orig.height,
                    count: orig.count,
                    lower: lower_node.unwrap_or_else(|| orig.lower.clone()),
                    upper: upper_node.unwrap_or_else(|| orig.upper.clone()),
                });
                entry.insert((h, Arc::clone(&new_arc)));
                new_arc
            }
        }
    }

    fn intern_rebuilt_skip(&mut self, orig: &Skip<V>, child_node: Node<V>) -> Arc<Skip<V>> {
        let key = (orig.path, ShallowNode(&child_node));
        match self.skips.entry(&key) {
            (_, Entry::Occupied(entry)) => Arc::clone(&entry.get().1),
            (h, Entry::Vacant(entry)) => {
                let new_arc = Arc::new(Skip {
                    path: orig.path,
                    split_dimensions: orig.split_dimensions,
                    child: child_node,
                });
                entry.insert((h, Arc::clone(&new_arc)));
                new_arc
            }
        }
    }
}
