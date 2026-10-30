use super::view::{Target, View};
use crate::collection::summary::{BitMask, Summary, ValueSet};
use crate::{
    FlexId, Side,
    id::{
        dimension::Dimension,
        flex_id::ZOrderRel,
        relative_flex_id::{RelativeFlexId, ShortPath},
    },
};
use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::{mem, ptr};
use triomphe::Arc;

/// Node は自分の領域を持たず、親から渡される領域 `this`との相対的な位置で意味を持つ。辿る関数が`this`を持つことで様々な操作を行う。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum Node<V, S> {
    #[default]
    Empty,
    Leaf(V),
    Branch(Arc<Branch<V, S>>),
    /// 行き先を [ShortPath] で表せない Skip。
    Skip(Arc<Skip<V, S>>),
    /// 行き先を [ShortPath] で表せて、先が Leaf の Skip。確保せずに Node に埋め込む。
    LeafAt(ShortPath, V),
    /// 行き先を [ShortPath] で表せて、先が Branch の Skip。
    BranchAt(ShortPath, Arc<Branch<V, S>>),
}

// derive では `S: Clone` まで要求されるが、Arc の複製に `S` の複製は要らない
impl<V: Clone, S> Clone for Node<V, S> {
    fn clone(&self) -> Self {
        match self {
            Node::Empty => Node::Empty,
            Node::Leaf(value) => Node::Leaf(value.clone()),
            Node::Branch(branch) => Node::Branch(branch.clone()),
            Node::Skip(skip) => Node::Skip(skip.clone()),
            Node::LeafAt(path, value) => Node::LeafAt(*path, value.clone()),
            Node::BranchAt(path, branch) => Node::BranchAt(*path, branch.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Branch<V, S> {
    pub(super) dimension: Dimension,
    /// 自身と子孫が割っている次元のビットマスク。自身の `dimension` とlowerとupperの集合を合わせたもの。
    pub(super) split_dimensions: u8,
    /// 自身と子孫で Branch が縦に何段続くかの最大。Node を辿るスタックの大きさに使う。
    pub(super) height: u8,
    /// 子孫に含まれる、値を持つ[FlexId]の数。
    pub(super) count: u32,
    pub(super) summary: S,
    pub(super) lower: Node<V, S>,
    pub(super) upper: Node<V, S>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skip<V, S> {
    pub(super) path: RelativeFlexId,
    /// 自身と子孫が割っている次元のビットマスク。行き先が狭い次元（`path` の深くなっている次元）と `child` の集合を合わせたもの。
    pub(super) split_dimensions: u8,
    pub(super) child: Node<V, S>,
}

/// [`Node::filter`] で、Branch の領域と Summary から子孫をまとめて判断した結果。
pub(super) enum Decision {
    /// 子孫をすべて残す。
    KeepAll,
    /// 子孫をすべて捨てる。
    DropAll,
    /// 子へ降りて判断する。
    Descend,
}

impl<V, S> Node<V, S> {
    pub(super) fn is_empty(&self) -> bool {
        matches!(self, Node::Empty)
    }

    /// 子孫に含まれる、値を持つ[FlexId]の数。
    pub(super) fn count(&self) -> u32 {
        match self {
            Node::Empty => 0,
            Node::Leaf(_) | Node::LeafAt(..) => 1,
            Node::Branch(branch) | Node::BranchAt(_, branch) => branch.count,
            Node::Skip(skip) => skip.child.count(),
        }
    }

    /// 自身と子孫で Branch が縦に何段続くかの最大。Leaf なら 0。
    pub(super) fn height(&self) -> u8 {
        match self {
            Node::Empty | Node::Leaf(_) | Node::LeafAt(..) => 0,
            Node::Branch(branch) | Node::BranchAt(_, branch) => branch.height,
            Node::Skip(skip) => skip.child.height(),
        }
    }

    /// 自身と子孫が割っている次元の集合のビットマスク。ビットマスクは[`Dimension::bit`]のOR。
    pub(super) fn split_dimensions(&self) -> u8 {
        match self {
            Node::Empty | Node::Leaf(_) => 0,
            Node::Branch(branch) => branch.split_dimensions,
            Node::Skip(skip) => skip.split_dimensions,
            Node::LeafAt(path, _) => path.deeper_dimensions(),
            Node::BranchAt(path, branch) => path.deeper_dimensions() | branch.split_dimensions,
        }
    }

    /// Skip なら、行き先が狭い次元の集合（[`Dimension::bit`] の OR）。Skip でなければ 0。
    fn path_dimensions(&self) -> u8 {
        match self {
            Node::Skip(skip) => skip.path.deeper_dimensions(),
            Node::LeafAt(path, _) | Node::BranchAt(path, _) => path.deeper_dimensions(),
            _ => 0,
        }
    }

    /// 領域 `this` のこの Node が Skip なら、行き先の領域とその先。
    // `merge` の各段で呼ぶので展開させる
    #[inline(always)]
    pub(super) fn skip_target(&self, this: &FlexId) -> Option<(FlexId, Target<'_, V, S>)> {
        match self {
            Node::Skip(skip) => Some((
                skip.path.to_absolute(this).unwrap(),
                Target::of(&skip.child),
            )),
            Node::LeafAt(path, value) => Some((path.to_absolute(this), Target::Leaf(value))),
            Node::BranchAt(path, branch) => Some((path.to_absolute(this), Target::Branch(branch))),
            _ => None,
        }
    }

    /// この Node を最初に割る次元の候補（[`Dimension::bit`] の OR）。このうち一番粗い次元で最初に割る。
    /// Leaf や空なら 0。
    pub(super) fn head_dimensions(&self) -> u8 {
        match self {
            Node::Empty | Node::Leaf(_) => 0,
            Node::Branch(branch) => branch.dimension.bit(),
            node => node.split_dimensions(),
        }
    }

    /// 領域 `this` のこの Node の子とその領域を、下側から順に返す。Leaf・LeafAt や空なら何も返さない。
    ///
    /// BranchAt は、行き先の Branch の子を返す。
    // 展開されないと、`get` の1段ごとに呼び出しが挟まり、`get` が4割ほど遅くなる
    #[inline(always)]
    pub(super) fn children(
        &self,
        this: FlexId,
    ) -> impl DoubleEndedIterator<Item = (&Self, FlexId)> {
        fn halves<V, S>(branch: &Branch<V, S>, this: FlexId) -> [Option<(&Node<V, S>, FlexId)>; 2] {
            let half = |side| this.split_on(branch.dimension, side).unwrap();
            [
                Some((&branch.lower, half(Side::Lower))),
                Some((&branch.upper, half(Side::Upper))),
            ]
        }
        let children = match self {
            Node::Empty | Node::Leaf(_) | Node::LeafAt(..) => [None, None],
            Node::Branch(branch) => halves(branch, this),
            Node::BranchAt(path, branch) => halves(branch, path.to_absolute(&this)),
            Node::Skip(skip) => [
                Some((&skip.child, skip.path.to_absolute(&this).unwrap())),
                None,
            ],
        };
        children.into_iter().flatten()
    }

    /// 同じ Branch・Skip の Arc を指しているなら true。値の型が違う Node とも比べられる。
    pub(super) fn ptr_eq<W, T>(&self, other: &Node<W, T>) -> bool {
        match (self, other) {
            (Node::Branch(a), Node::Branch(b)) => ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (Node::Skip(a), Node::Skip(b)) => ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (Node::BranchAt(p, a), Node::BranchAt(q, b)) => {
                p == q && ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
            }
            _ => false,
        }
    }


}

impl<V: Ord, S: Summary<V>> Node<V, S> {
    /// この Node と子孫の値の範囲 `(最小, 最大)`。空なら [`None`]。
    ///
    /// Summary が範囲を持っていればそれを使い、持っていなければ子孫を辿って求める。
    pub(super) fn value_range(&self) -> Option<(&V, &V)> {
        match self {
            Node::Empty => None,
            Node::Leaf(value) | Node::LeafAt(_, value) => Some((value, value)),
            Node::Branch(branch) | Node::BranchAt(_, branch) => branch
                .summary
                .value_range()
                .or_else(|| wider_range(branch.lower.value_range(), branch.upper.value_range())),
            Node::Skip(skip) => skip.child.value_range(),
        }
    }
}

impl<V: BitMask, S: Summary<V>> Node<V, S> {
    /// この Node と子孫に現れる値の集合。
    ///
    /// Summary が集合を持っていればそれを使い、持っていなければ子孫を辿って求める。
    pub(super) fn value_set(&self) -> ValueSet<V> {
        match self {
            Node::Empty => ValueSet::EMPTY,
            Node::Leaf(value) | Node::LeafAt(_, value) => ValueSet::single(*value),
            Node::Branch(branch) | Node::BranchAt(_, branch) => branch
                .summary
                .value_set()
                .unwrap_or_else(|| branch.lower.value_set().union(&branch.upper.value_set())),
            Node::Skip(skip) => skip.child.value_set(),
        }
    }
}

impl<V: Clone, S> Node<V, S> {
    /// 形はそのままに、Branch の Summary を `T` で作り直した Node。
    pub(super) fn with_summary<T: Summary<V>>(&self) -> Node<V, T> {
        match self {
            Node::Empty => Node::Empty,
            Node::Leaf(value) => Node::Leaf(value.clone()),
            Node::Branch(branch) => Node::Branch(branch.with_summary()),
            Node::Skip(skip) => {
                Node::Skip(Arc::new(Skip::new(skip.path, skip.child.with_summary())))
            }
            Node::LeafAt(path, value) => Node::LeafAt(*path, value.clone()),
            Node::BranchAt(path, branch) => Node::BranchAt(*path, branch.with_summary()),
        }
    }
}

impl<V: Clone, S> Branch<V, S> {
    fn with_summary<T: Summary<V>>(&self) -> Arc<Branch<V, T>> {
        Arc::new(Branch::new(
            self.dimension,
            self.lower.with_summary(),
            self.upper.with_summary(),
        ))
    }
}

/// 2つの値の範囲を合わせた範囲。
pub(super) fn wider_range<'a, V: Ord>(
    a: Option<(&'a V, &'a V)>,
    b: Option<(&'a V, &'a V)>,
) -> Option<(&'a V, &'a V)> {
    match (a, b) {
        (Some((a_min, a_max)), Some((b_min, b_max))) => Some((a_min.min(b_min), a_max.max(b_max))),
        (range, None) | (None, range) => range,
    }
}

impl<V, S: Summary<V>> Branch<V, S> {
    fn new(dimension: Dimension, lower: Node<V, S>, upper: Node<V, S>) -> Self {
        let (Some(lower_summary), Some(upper_summary)) = (lower.summary(), upper.summary()) else {
            unreachable!("Branch の子は空にならない")
        };
        Branch {
            dimension,
            split_dimensions: dimension.bit() | lower.split_dimensions() | upper.split_dimensions(),
            height: 1 + lower.height().max(upper.height()),
            count: lower.count() + upper.count(),
            summary: lower_summary.merge(&upper_summary),
            lower,
            upper,
        }
    }
}

impl<V, S> Skip<V, S> {
    fn new(path: RelativeFlexId, child: Node<V, S>) -> Self {
        Skip {
            split_dimensions: path.deeper_dimensions() | child.split_dimensions(),
            path,
            child,
        }
    }
}

impl<V, S: Summary<V>> Node<V, S> {
    /// この Node と子孫の値の Summary。空なら [`None`]。
    pub(super) fn summary(&self) -> Option<Cow<'_, S>> {
        match self {
            Node::Empty => None,
            Node::Leaf(value) | Node::LeafAt(_, value) => Some(Cow::Owned(S::new(value))),
            Node::Branch(branch) | Node::BranchAt(_, branch) => {
                Some(Cow::Borrowed(&branch.summary))
            }
            Node::Skip(skip) => skip.child.summary(),
        }
    }
}

impl<V: PartialEq + Clone, S: Summary<V>> Node<V, S> {
    /// `join` が Branch を作るなら true。
    fn forms_branch(dimension: Dimension, lower: &Self, upper: &Self) -> bool {
        // Node は領域との相対位置で意味を持つので、子孫が `dimension` で割っている Node を
        // 広い領域へ持ち上げると意味が変わる。左右が同じでも、そのときは割ったままにする。
        let same = lower.split_dimensions() & dimension.bit() == 0
            && (lower.ptr_eq(upper) || lower == upper);
        !lower.is_empty() && !upper.is_empty() && !same
    }

    /// 領域 `this` を `dimension` で割った左右 `lower`,`upper` を、1つの[Node]に合わせる。まとめられれば1つにし、片側が空なら Skip にする。
    pub(super) fn join(this: &FlexId, dimension: Dimension, lower: Self, upper: Self) -> Self {
        // Branch はここと `merge` でしか作らない。これでどの操作の結果も、同じ形状なら同じ FlexTreeCore になる。
        if Node::forms_branch(dimension, &lower, &upper) {
            return Node::Branch(Arc::new(Branch::new(dimension, lower, upper)));
        }
        // FlexTreeCore では Branch の子は空にならない
        if lower.is_empty() {
            let upper_id = this.split_on(dimension, Side::Upper).unwrap();
            return Node::rebase(upper, &upper_id, this);
        }
        if upper.is_empty() {
            let lower_id = this.split_on(dimension, Side::Lower).unwrap();
            return Node::rebase(lower, &lower_id, this);
        }
        lower
    }

    /// 領域 `from` から見た `node` を、領域 `to` から見た Node にする。`node` の中身は `to` に収まっていること。
    ///
    /// 他と共有していない Skip は、確保し直さずに行き先だけを付け替える。
    pub(super) fn rebase(node: Self, from: &FlexId, to: &FlexId) -> Self {
        match node {
            _ if from == to => node,
            Node::Empty => Node::Empty,
            // Skip の先へさらに Skip しないよう、Skip の行き先から直接張り直す
            Node::Skip(mut skip) => {
                let region = skip.path.to_absolute(from).unwrap();
                if region == *to {
                    return Arc::unwrap_or_clone(skip).child;
                }
                let path = region.relative_to(to).unwrap();
                if ShortPath::new(path).is_some() {
                    return Node::skip(path, Arc::unwrap_or_clone(skip).child);
                }
                let skip_mut = Arc::make_mut(&mut skip);
                let child = mem::take(&mut skip_mut.child);
                *skip_mut = Skip::new(path, child);
                Node::Skip(skip)
            }
            Node::LeafAt(path, value) => {
                Node::rebase(Node::Leaf(value), &path.to_absolute(from), to)
            }
            Node::BranchAt(path, branch) => {
                Node::rebase(Node::Branch(branch), &path.to_absolute(from), to)
            }
            node => Node::skip(from.relative_to(to).unwrap(), node),
        }
    }

    /// `path` の先だけに Leaf か Branch の `child` がある Skip。`path` が短ければ確保せずに埋め込む。
    fn skip(path: RelativeFlexId, child: Self) -> Self {
        match (ShortPath::new(path), child) {
            (Some(short), Node::Leaf(value)) => Node::LeafAt(short, value),
            (Some(short), Node::Branch(branch)) => Node::BranchAt(short, branch),
            (_, child) => Node::Skip(Arc::new(Skip::new(path, child))),
        }
    }

    /// 領域 `this` のこの Node を `dimension` で割った `[下, 上]`（`join` の逆）。
    ///
    /// `dimension` で割っていなければ、両側にこの Node を返す。
    ///
    /// `halves` は `this` を `dimension` で割った `[下, 上]` の領域。
    fn split(self, this: &FlexId, dimension: Dimension, halves: &[FlexId; 2]) -> [Self; 2] {
        if self.path_dimensions() & dimension.bit() != 0 {
            let (region, _) = self.skip_target(this).unwrap();
            let side = this.side_toward(dimension, &region);
            let rest = Node::rebase(self, this, &halves[side as usize]);
            return match side {
                Side::Lower => [rest, Node::Empty],
                Side::Upper => [Node::Empty, rest],
            };
        }
        // その次元では中身が変わらないので、下も上も同じ中身になる
        [self.clone(), self]
    }

    /// 領域 `this` の `a` に `b` を `rule` に従って重ね合わせる。
    ///
    /// `rule` は、`a` と `b` から結果がすぐ決まるなら `a` を書き換えて true を返す。決まらなければ両方を割って子へ降りる。
    /// 他と共有していない Branch・Skip は作り直さずに書き換え、共有しているものは複製してから書き換える。
    pub(super) fn merge<W, T>(
        this: &FlexId,
        a: &mut Self,
        b: View<'_, W, T>,
        rule: &impl Fn(&FlexId, &mut Self, View<'_, W, T>) -> bool,
    ) {
        if rule(this, a, b) {
            return;
        }
        // Skip どうしは、行き先が分かれるところまで1段ずつ割らずに一気に降りる
        if let Some((region_a, split_a)) = a
            .skip_target(this)
            .map(|(region, child)| (region, child.split_dimensions()))
            && let Some((region_b, child_b)) = b.as_skip()
        {
            let at = common_descent(
                this,
                (&region_a, split_a),
                (&region_b, child_b.split_dimensions()),
            );
            let b = View::at(&at, region_b, child_b);
            // 変わるのは行き先の中だけなので、先が Leaf か Branch のままなら行き先を求め直さない
            if at == region_a {
                match a {
                    Node::Skip(skip) => {
                        let skip = Arc::make_mut(skip);
                        Node::merge(&at, &mut skip.child, b, rule);
                        match mem::take(&mut skip.child) {
                            child @ (Node::Leaf(_) | Node::Branch(_)) => {
                                *skip = Skip::new(skip.path, child)
                            }
                            child => *a = Node::rebase(child, &at, this),
                        }
                    }
                    Node::LeafAt(path, _) | Node::BranchAt(path, _) => {
                        let path = *path;
                        let mut inner = match mem::take(a) {
                            Node::LeafAt(_, value) => Node::Leaf(value),
                            Node::BranchAt(_, branch) => Node::Branch(branch),
                            _ => unreachable!(),
                        };
                        Node::merge(&at, &mut inner, b, rule);
                        *a = match inner {
                            Node::Leaf(value) => Node::LeafAt(path, value),
                            Node::Branch(branch) => Node::BranchAt(path, branch),
                            inner => Node::rebase(inner, &at, this),
                        };
                    }
                    _ => unreachable!("行き先があるのは Skip だけ"),
                }
                return;
            }
            if at != *this {
                let mut inner = Node::rebase(mem::take(a), this, &at);
                Node::merge(&at, &mut inner, b, rule);
                *a = Node::rebase(inner, &at, this);
                return;
            }
        }
        // 結果の FlexTreeCore も粗い次元から割るよう、両方の最初の次元のうち粗い方で割る
        let dimension = this
            .coarsest_dimension_in(a.head_dimensions() | b.head_dimensions(this))
            .expect("Leaf どうしは rule が答えを決める");
        let lower_id = this.split_on(dimension, Side::Lower).unwrap();
        let upper_id = this.split_on(dimension, Side::Upper).unwrap();
        let halves = [lower_id, upper_id];
        let [b_lower, b_upper] = b.split(this, dimension, &halves);

        if let Node::Branch(branch) = a
            && branch.dimension == dimension
        {
            let branch = Arc::make_mut(branch);
            Node::merge(&lower_id, &mut branch.lower, b_lower, rule);
            Node::merge(&upper_id, &mut branch.upper, b_upper, rule);
            let lower = mem::take(&mut branch.lower);
            let upper = mem::take(&mut branch.upper);
            if Node::forms_branch(dimension, &lower, &upper) {
                *branch = Branch::new(dimension, lower, upper);
            } else {
                *a = Node::join(this, dimension, lower, upper);
            }
            return;
        }
        let [mut lower, mut upper] = mem::take(a).split(this, dimension, &halves);
        Node::merge(&lower_id, &mut lower, b_lower, rule);
        Node::merge(&upper_id, &mut upper, b_upper, rule);
        *a = Node::join(this, dimension, lower, upper);
    }

    /// 和。`a` に値がある場所は `a`、無い場所は `b`。
    pub(super) fn union_rule(this: &FlexId, a: &mut Self, b: View<'_, V, S>) -> bool {
        if a.is_empty() {
            *a = b.to_node(this);
            return true;
        }
        matches!(a, Node::Leaf(_)) || b.is_empty() || b.ptr_eq(this, a)
    }

    /// 積。`b` に値がある場所だけ `a` を残す。
    pub(super) fn intersection_rule<W, T>(this: &FlexId, a: &mut Self, b: View<'_, W, T>) -> bool {
        if b.is_empty() {
            *a = Node::Empty;
            return true;
        }
        a.is_empty() || b.leaf().is_some() || b.ptr_eq(this, a)
    }

    /// 差。`b` に値がある場所の `a` を消す。
    pub(super) fn difference_rule<W, T>(this: &FlexId, a: &mut Self, b: View<'_, W, T>) -> bool {
        if b.leaf().is_some() || b.ptr_eq(this, a) {
            *a = Node::Empty;
            return true;
        }
        a.is_empty() || b.is_empty()
    }

    /// 領域 `this` のこの Node から、条件を満たす Leaf だけを残した Node を返す。何も捨てなければ [`None`]。
    ///
    /// Branch は領域と Summary を `classify` で判断し、子孫をまとめて残すか捨てられるならそれ以上降りない。
    /// Leaf は領域と値を `keep` で判断する。
    pub(super) fn filter(
        &self,
        this: &FlexId,
        classify: &impl Fn(&FlexId, &S) -> Decision,
        keep: &impl Fn(&FlexId, &V) -> bool,
    ) -> Option<Self> {
        match self {
            Node::Empty => None,
            Node::Leaf(value) => (!keep(this, value)).then_some(Node::Empty),
            Node::LeafAt(path, value) => {
                (!keep(&path.to_absolute(this), value)).then_some(Node::Empty)
            }
            Node::Branch(branch) => Node::filter_branch(branch, this, classify, keep),
            Node::BranchAt(path, branch) => {
                let region = path.to_absolute(this);
                let child = Node::filter_branch(branch, &region, classify, keep)?;
                Some(Node::rebase(child, &region, this))
            }
            Node::Skip(skip) => {
                let region = skip.path.to_absolute(this).unwrap();
                let child = skip.child.filter(&region, classify, keep)?;
                Some(Node::rebase(child, &region, this))
            }
        }
    }

    /// 領域 `this` の `branch` に [`filter`](Self::filter) をかける。
    fn filter_branch(
        branch: &Arc<Branch<V, S>>,
        this: &FlexId,
        classify: &impl Fn(&FlexId, &S) -> Decision,
        keep: &impl Fn(&FlexId, &V) -> bool,
    ) -> Option<Self> {
        match classify(this, &branch.summary) {
            Decision::KeepAll => None,
            Decision::DropAll => Some(Node::Empty),
            Decision::Descend => {
                let dimension = branch.dimension;
                let lower_id = this.split_on(dimension, Side::Lower).unwrap();
                let upper_id = this.split_on(dimension, Side::Upper).unwrap();
                let lower = branch.lower.filter(&lower_id, classify, keep);
                let upper = branch.upper.filter(&upper_id, classify, keep);
                if lower.is_none() && upper.is_none() {
                    return None;
                }
                let lower = lower.unwrap_or_else(|| branch.lower.clone());
                let upper = upper.unwrap_or_else(|| branch.upper.clone());
                Some(Node::join(this, dimension, lower, upper))
            }
        }
    }

    /// ルート領域 `root_id` のこの Node から、`horizon` より前に確定した要素を Z-order 順に抜き出して `out` へ追加する。
    pub(super) fn drain_confirmed(
        &mut self,
        root_id: FlexId,
        horizon: &FlexId,
        out: &mut Vec<(FlexId, V)>,
    ) {
        if self.is_empty() {
            return;
        }
        let top_region = match self.skip_target(&root_id) {
            Some((region, _)) => region,
            None => root_id,
        };
        if top_region.zorder_rel(horizon) == ZOrderRel::Greater {
            return;
        }
        if top_region.zorder_rel(horizon) == ZOrderRel::Less {
            Self::append_all(mem::take(self), root_id, out);
            return;
        }
        self.drain_inner(root_id, horizon, out);
    }

    /// `this` 自身は空にせず、`horizon` より前に確定した部分木を抜き出して `out` へ追加する。
    fn drain_inner(
        &mut self,
        this: FlexId,
        horizon: &FlexId,
        out: &mut Vec<(FlexId, V)>,
    ) -> bool {
        match self {
            Node::Empty | Node::Leaf(_) | Node::LeafAt(..) => false,
            Node::Skip(skip) => {
                let region = skip.path.to_absolute(&this).unwrap();
                if region.zorder_rel(horizon) == ZOrderRel::Greater {
                    return false;
                }
                let skip_mut = Arc::make_mut(skip);
                if !skip_mut.child.drain_inner(region, horizon, out) {
                    return false;
                }
                let child = mem::take(&mut skip_mut.child);
                *self = Node::rebase(child, &region, &this);
                true
            }
            Node::BranchAt(path, _) => {
                let region = path.to_absolute(&this);
                if region.zorder_rel(horizon) == ZOrderRel::Greater {
                    return false;
                }
                let Node::BranchAt(path, branch) = mem::take(self) else {
                    unreachable!()
                };
                let mut inner = Node::Branch(branch);
                let changed = inner.drain_branch(region, horizon, out);
                *self = match inner {
                    Node::Branch(branch) if !changed => Node::BranchAt(path, branch),
                    inner => Node::rebase(inner, &region, &this),
                };
                changed
            }
            Node::Branch(_) => self.drain_branch(this, horizon, out),
        }
    }

    fn drain_branch(
        &mut self,
        this: FlexId,
        horizon: &FlexId,
        out: &mut Vec<(FlexId, V)>,
    ) -> bool {
        let Node::Branch(branch) = self else {
            unreachable!()
        };
        let dim = branch.dimension;
        let lower_id = this.split_on(dim, Side::Lower).unwrap();
        if lower_id.zorder_rel(horizon) == ZOrderRel::Greater {
            return false;
        }
        let upper_id = this.split_on(dim, Side::Upper).unwrap();

        if lower_id.zorder_rel(horizon) == ZOrderRel::Less {
            let b = Arc::make_mut(branch);
            let lower = mem::take(&mut b.lower);
            let upper = mem::take(&mut b.upper);
            Self::append_all(lower, lower_id, out);
            *self = Node::rebase(upper, &upper_id, &this);
            self.drain_inner(this, horizon, out);
            return true;
        }

        let b = Arc::make_mut(branch);
        if !b.lower.drain_inner(lower_id, horizon, out) {
            return false;
        }
        let (Some(ls), Some(us)) = (b.lower.summary(), b.upper.summary()) else {
            unreachable!()
        };
        b.split_dimensions = dim.bit() | b.lower.split_dimensions() | b.upper.split_dimensions();
        b.height = 1 + b.lower.height().max(b.upper.height());
        b.count = b.lower.count() + b.upper.count();
        b.summary = ls.merge(&us);
        true
    }

    /// 領域 `this` の `node` に含まれる全要素を Z-order 順で `out` に追加する。
    pub(super) fn append_all(node: Self, this: FlexId, out: &mut Vec<(FlexId, V)>) {
        match node {
            Node::Empty => {}
            Node::Leaf(value) => out.push((this, value)),
            Node::LeafAt(path, value) => out.push((path.to_absolute(&this), value)),
            Node::Branch(branch) => Self::append_branch(this, branch, out),
            Node::BranchAt(path, branch) => Self::append_branch(path.to_absolute(&this), branch, out),
            Node::Skip(skip) => {
                let region = skip.path.to_absolute(&this).unwrap();
                let child = Arc::try_unwrap(skip).map_or_else(|s| s.child.clone(), |s| s.child);
                Self::append_all(child, region, out);
            }
        }
    }

    fn append_branch(region: FlexId, branch: Arc<Branch<V, S>>, out: &mut Vec<(FlexId, V)>) {
        let (dim, lower, upper) = match Arc::try_unwrap(branch) {
            Ok(b) => (b.dimension, b.lower, b.upper),
            Err(b) => (b.dimension, b.lower.clone(), b.upper.clone()),
        };
        Self::append_all(lower, region.split_on(dim, Side::Lower).unwrap(), out);
        Self::append_all(upper, region.split_on(dim, Side::Upper).unwrap(), out);
    }


}

/// 行き先 `region`・その先の Node がそれぞれ `a`・`b` の 2 つの Skip が、領域 `this` から
/// 途中で割らずに一緒に降りられる一番狭い領域。
///
/// 1段ずつ降りると、各次元をズームの浅い順（同じなら F→X→Y→T）に1段ずつ深くしていく。
/// 次元ごとに「両方の行き先が同じ側にいられる一番深いズーム」で止まり、そこで降りられなくなる
/// 次元のうち一番先に順番が来るもので全体が止まる。止まる位置は次元ごとに直接求まるので、
/// Skip の長さぶん1段ずつ辿らない。
///
/// `split_a`・`split_b` はそれぞれの先の Node が割っている次元の集合。
pub(super) fn common_descent(
    this: &FlexId,
    (region_a, split_a): (&FlexId, u8),
    (region_b, split_b): (&FlexId, u8),
) -> FlexId {
    let splits = split_a | split_b;
    let mut bound = [0u8; 4];
    let mut blocks = [false; 4];
    for d in Dimension::ALL {
        let (za, zb) = (region_a.zoomlevel_on(d), region_b.zoomlevel_on(d));
        let deepest = za.min(zb);
        let diff =
            (region_a.index_on(d) >> (za - deepest)) ^ (region_b.index_on(d) >> (zb - deepest));
        // 一番上の食い違うビットより上だけが一致している
        let agree = deepest - (u64::BITS - (diff as u64).leading_zeros()) as u8;
        bound[d as usize] = agree;
        // ここまで降りても、どちらかがまだこの次元で深いか、先の Node がこの次元で割っているなら止まる
        blocks[d as usize] = za > agree || zb > agree || splits & d.bit() != 0;
    }
    let stop = Dimension::ALL
        .into_iter()
        .filter(|&d| blocks[d as usize])
        .min_by_key(|&d| (bound[d as usize], d));
    let zoom = Dimension::ALL.map(|d| {
        let reach = match stop {
            None => bound[d as usize],
            // 止まる次元より前の次元は、同じ深さの番を1段ぶん先に済ませている
            Some(s) => bound[d as usize].min(bound[s as usize] + u8::from(d < s)),
        };
        this.zoomlevel_on(d).max(reach)
    });
    region_a.ancestor_at(zoom)
}
