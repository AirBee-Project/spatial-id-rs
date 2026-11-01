use crate::collection::time_reconstruct;
use crate::{AllowedIntervals, FlexId, RangeId, Side, SingleId, SpatialId};
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::iter::from_fn;
use core::mem;
use core::ops::RangeBounds;
pub use iter::IntoIter;
pub(crate) use iter::Iter;
use node::{Decision, Node};
use view::View;

mod iter;
mod node;
#[cfg(test)]
mod tests;
mod view;

/// [FlexId]に対して割り当てられている値`V`を管理するためのインデックス構造。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FlexTreeCore<V> {
    upper_root: Node<V>,
    lower_root: Node<V>,
}

impl<V: Clone> Clone for FlexTreeCore<V> {
    fn clone(&self) -> Self {
        FlexTreeCore {
            upper_root: self.upper_root.clone(),
            lower_root: self.lower_root.clone(),
        }
    }
}

impl<V> Default for FlexTreeCore<V> {
    fn default() -> Self {
        FlexTreeCore {
            upper_root: Node::Empty,
            lower_root: Node::Empty,
        }
    }
}

impl<V> FlexTreeCore<V> {
    /// 値を持つ[FlexId]の数。
    pub fn count(&self) -> usize {
        self.upper_root.count() as usize + self.lower_root.count() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.upper_root.is_empty() && self.lower_root.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, V> {
        Iter::new(self)
    }

    /// 値を持つ[FlexId]の F/X/Y のズームレベルのうち、最も高いもの。空なら [`None`]。
    ///
    /// キャッシュを持たず、値を持つ全ての[FlexId]を走査する（O(n)）。
    pub fn max_zoomlevel(&self) -> Option<u8> {
        self.iter()
            .map(|(id, _)| id.f_zoomlevel().max(id.x_zoomlevel()).max(id.y_zoomlevel()))
            .max()
    }

    /// 全ての値を消す。
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `target` と重なる領域と値を、`target` との共通部分に切り取って返す。
    ///
    /// `target` の各[FlexId]が互いに重ならなければ、返す領域も互いに重ならない。
    pub fn get<'a>(
        &'a self,
        target: impl IntoIterator<Item = FlexId, IntoIter: 'a>,
    ) -> impl Iterator<Item = (FlexId, &'a V)> + 'a {
        target.into_iter().flat_map(move |t| {
            // 積むのは `t` と重なる Node だけ
            let mut stack = self.new_stack();
            stack.retain(|(_, root)| root.intersects(&t));
            from_fn(move || {
                while let Some((node, this)) = stack.pop() {
                    match node {
                        Node::Empty => {}
                        Node::Leaf(value) => return Some((this.intersection(&t)?, value)),
                        // 積んだ領域が `t` と重なっていても、埋め込んだ行き先は重ならないことがある
                        Node::LeafAt(path, value) => {
                            if let Some(region) = path.to_absolute(&this).intersection(&t) {
                                return Some((region, value));
                            }
                        }
                        Node::Branch(branch) => push_overlapping(&mut stack, branch, this, &t),
                        Node::BranchAt(path, branch) => {
                            let region = path.to_absolute(&this);
                            if region.intersects(&t) {
                                push_overlapping(&mut stack, branch, region, &t);
                            }
                        }
                        node => stack.extend(
                            node.children(this)
                                .filter(|(_, region)| region.intersects(&t))
                                .rev(),
                        ),
                    }
                }
                None
            })
        })
    }

    /// `target` と重なる領域と値を、切り取らずにそのまま返す。各領域は1回だけ返す。
    pub fn get_overlapping<'a>(
        &'a self,
        target: impl IntoIterator<Item = impl Into<RangeId>>,
    ) -> impl Iterator<Item = (FlexId, &'a V)> + 'a {
        let targets: Vec<RangeId> = target.into_iter().map(Into::into).collect();
        self.overlapping(move |region| targets.iter().any(|t| region.intersects_range(t)))
    }

    /// `target` と面で接している領域と値を返す。`target` 自身と重なる領域は除く。
    pub fn spatial_face_neighbors<'a, T: SpatialId>(
        &'a self,
        target: T,
    ) -> impl Iterator<Item = (FlexId, &'a V)> + 'a {
        // 各方向に1つずらした領域が、面で接しうる候補
        let mut shifted: Vec<RangeId> = Vec::with_capacity(6);
        for delta in [-1, 1] {
            let mut f = target;
            if f.move_f(delta).is_ok() {
                shifted.push(f.into());
            }
            let mut y = target;
            if y.move_y(delta).is_ok() {
                shifted.push(y.into());
            }
            let mut x = target;
            x.move_x(delta);
            shifted.push(x.into());
        }

        let own: Vec<FlexId> = target.into_iter().collect();
        self.get_overlapping(shifted).filter(move |(leaf, _)| {
            own.iter().all(|o| !leaf.intersects(o))
                && own.iter().any(|o| o.is_spatial_face_neighbor(leaf))
        })
    }

    /// 値を持つ Leaf のうち、領域が `overlaps` を満たすものを FlexId と値で返す。`overlaps` を満たさない Node の子孫は辿らない。
    fn overlapping<'a>(
        &'a self,
        overlaps: impl Fn(&FlexId) -> bool + 'a,
    ) -> impl Iterator<Item = (FlexId, &'a V)> + 'a {
        let mut stack = self.new_stack();
        from_fn(move || {
            while let Some((node, this)) = stack.pop() {
                if !overlaps(&this) {
                    continue;
                }
                match node {
                    Node::Leaf(value) => return Some((this, value)),
                    Node::LeafAt(path, value) => {
                        let region = path.to_absolute(&this);
                        if overlaps(&region) {
                            return Some((region, value));
                        }
                    }
                    node => stack.extend(node.children(this).rev()),
                }
            }
            None
        })
    }

    /// 上下のルートを積んだ、[Node] を辿るためのスタックを返す。
    fn new_stack(&self) -> Vec<(&Node<V>, FlexId)> {
        // 辿っている経路の各 Branch につき未処理の子が1つ積まれ、もう一方のルートと今の子の分が加わる。
        // スタックは高さ + 2 を超えないので、最後まで再確保しない。
        let height = self.upper_root.height().max(self.lower_root.height());
        let mut stack = Vec::with_capacity(usize::from(height) + 2);
        // 後に積んだものから取り出すので、上側のルートを後に積む
        stack.push((&self.lower_root, FlexId::LOWER_MAX));
        stack.push((&self.upper_root, FlexId::UPPER_MAX));
        stack
    }

    /// `target` を含む方のルート（北半球か南半球）と、その領域を返す。
    fn root_for(&mut self, target: &FlexId) -> (&mut Node<V>, FlexId) {
        if target.f_index().is_negative() {
            (&mut self.lower_root, FlexId::LOWER_MAX)
        } else {
            (&mut self.upper_root, FlexId::UPPER_MAX)
        }
    }
}

impl<V: PartialEq> FlexTreeCore<V> {
    /// 時間方向に隣接する同値のSegmentを結合した [`RangeId`] と値を返す。**空間解像度は変えない**。
    ///
    /// `allowed_intervals` が [`None`] なら、各区間はそれを表せる最も粗い単位（`gcd(開始秒, 幅)`）の1Segmentになる。
    /// [`AllowedIntervals`] を渡すと、その候補のうち区間を割り切る最も粗い単位で表す。
    ///
    /// [`iter`](Self::iter) が返す [FlexId] は時間を2の冪秒のSegmentとして持つので、
    /// `1800` 秒のような単位で入れた ID も、こちらなら元の表記で取り出せる。
    pub fn reconstructed_time_ranges<'a>(
        &'a self,
        allowed_intervals: Option<&'a AllowedIntervals>,
    ) -> impl Iterator<Item = (RangeId, &'a V)> + 'a {
        let has_temporal_split = (self.upper_root.split_dimensions()
            | self.lower_root.split_dimensions())
            & crate::Dimension::T.bit()
            != 0;
        time_reconstruct::reconstruct(self.iter(), has_temporal_split, allowed_intervals)
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を、全体の最大ズームレベルに揃えた [`SingleId`] へ展開する。
    pub fn flat_single_ids<'a>(
        &'a self,
        allowed_intervals: Option<&'a AllowedIntervals>,
    ) -> impl Iterator<Item = (SingleId, &'a V)> + 'a {
        let max_z = self.max_zoomlevel().unwrap_or(0);
        self.reconstructed_time_ranges(allowed_intervals)
            .flat_map(move |(range, value)| {
                let range = if range.z() == max_z {
                    range
                } else {
                    range
                        .spatial_children_at_zoom(max_z)
                        .expect("全体の最大ズームレベルは各Segmentのズーム以上")
                };
                range.single_ids().map(move |id| (id, value))
            })
    }
}

impl<V: PartialEq + Clone> FlexTreeCore<V> {
    /// `target` の領域に `value` を書き込む。既に値がある場所は上書きされる。
    ///
    /// `target` には [FlexId] のほか、[SingleId] や [RangeId] をそのまま渡せる。
    pub fn insert(&mut self, target: impl IntoIterator<Item = FlexId>, value: V) {
        self.insert_by_rule(target, value, &|this, existing, written| {
            if written.leaf().is_some() || existing.is_empty() {
                *existing = written.to_node(this);
                return true;
            }
            written.is_empty()
        });
    }

    /// `target` の領域に `value` を書き込む。既に値がある場所は `resolve(既存の値, value)` にする。
    pub fn insert_with(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: V,
        resolve: impl Fn(&V, &V) -> V,
    ) {
        self.insert_by_rule(target, value, &Self::resolve_rule(resolve));
    }

    /// `other` を重ねる。両方に値がある場所は `resolve(self の値, other の値)` にする。
    pub fn merge_with(&mut self, other: &Self, resolve: impl Fn(&V, &V) -> V) {
        self.merge(other, &Self::resolve_rule(resolve));
    }

    fn resolve_rule(
        resolve: impl Fn(&V, &V) -> V,
    ) -> impl Fn(&FlexId, &mut Node<V>, View<'_, V>) -> bool {
        move |this, existing, written| {
            match (&*existing, written.leaf()) {
                _ if written.is_empty() => {}
                (Node::Empty, _) => *existing = written.to_node(this),
                (Node::Leaf(old), Some(new)) => *existing = Node::Leaf(resolve(old, new)),
                _ => return false,
            }
            true
        }
    }

    /// `target` の領域を空にし、取り除いた部分を FlexTreeCore として返す。
    ///
    /// 1回の降下で、`target` に覆われた Node をその領域ごと抜き取り、あとで取り除いた部分に組み直す。
    pub fn remove(&mut self, target: impl IntoIterator<Item = FlexId>) -> Self {
        let cut = RefCell::new(Vec::new());
        let whole = Node::<()>::Leaf(());
        let rule = |this: &FlexId, a: &mut Node<V>, b: View<'_, ()>| {
            if b.leaf().is_some() {
                if !a.is_empty() {
                    cut.borrow_mut().push((*this, mem::take(a)));
                }
                return true;
            }
            a.is_empty() || b.is_empty()
        };
        for id in target {
            let (root, root_id) = self.root_for(&id);
            Node::merge(&root_id, root, View::skip(&root_id, id, &whole), &rule);
        }

        // 抜き取った部分は互いに重ならないので、和で順に植え直すだけでよい
        let mut removed = Self::default();
        for (region, node) in cut.into_inner() {
            let (root, root_id) = removed.root_for(&region);
            Node::merge(
                &root_id,
                root,
                View::skip(&root_id, region, &node),
                &Node::union_rule,
            );
        }
        removed
    }

    /// `target` と重なる領域を、切り取らずに丸ごと取り除き、取り除いた部分を FlexTreeCore として返す。
    pub fn remove_overlapping(&mut self, target: impl IntoIterator<Item = FlexId>) -> Self {
        let targets: Vec<FlexId> = target.into_iter().collect();
        let overlaps = |region: &FlexId| targets.iter().any(|t| region.intersects(t));
        let removed_part = |region: &FlexId| {
            if !overlaps(region) {
                Decision::DropAll
            } else if targets.iter().any(|t| t.contains(region)) {
                Decision::KeepAll
            } else {
                Decision::Descend
            }
        };
        let removed = self.filter(&removed_part, &|leaf, _| overlaps(leaf));
        *self = self.filter(
            &|region| match removed_part(region) {
                Decision::KeepAll => Decision::DropAll,
                Decision::DropAll => Decision::KeepAll,
                Decision::Descend => Decision::Descend,
            },
            &|leaf, _| !overlaps(leaf),
        );
        removed
    }

    /// 和集合。両方に値がある場所は `self` の値を使う。
    pub fn union(&self, other: &Self) -> Self {
        let mut result = self.clone();
        result.merge(other, &Node::union_rule);
        result
    }

    /// 積集合。`other` にも値がある場所だけ、`self` の値を残す。
    pub fn intersection<W: PartialEq>(&self, other: &FlexTreeCore<W>) -> Self {
        let mut result = self.clone();
        result.merge(other, &Node::intersection_rule);
        result
    }

    /// 差集合。`other` に値がある場所を `self` から取り除く。
    pub fn difference<W: PartialEq>(&self, other: &FlexTreeCore<W>) -> Self {
        let mut result = self.clone();
        result.merge(other, &Node::difference_rule);
        result
    }

    /// `target` の各領域だけに `value` がある Node を、既存の Node へ `rule` で重ね合わせる。
    fn insert_by_rule(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: V,
        rule: &impl Fn(&FlexId, &mut Node<V>, View<'_, V>) -> bool,
    ) {
        let leaf = Node::Leaf(value);
        for id in target {
            let (root, root_id) = self.root_for(&id);
            Node::merge(&root_id, root, View::skip(&root_id, id, &leaf), rule);
        }
    }

    /// 上下のルートへ、`other` のルートを `rule` で重ね合わせる。
    fn merge<W>(
        &mut self,
        other: &FlexTreeCore<W>,
        rule: &impl Fn(&FlexId, &mut Node<V>, View<'_, W>) -> bool,
    ) {
        let upper = View::of(&other.upper_root, &FlexId::UPPER_MAX);
        let lower = View::of(&other.lower_root, &FlexId::LOWER_MAX);
        Node::merge(&FlexId::UPPER_MAX, &mut self.upper_root, upper, rule);
        Node::merge(&FlexId::LOWER_MAX, &mut self.lower_root, lower, rule);
    }

    /// 条件を満たす[FlexId]だけを残す。
    fn filter(
        &self,
        classify: &impl Fn(&FlexId) -> Decision,
        keep: &impl Fn(&FlexId, &V) -> bool,
    ) -> Self {
        let filter = |root: &Node<V>, this: &FlexId| {
            root.filter(this, classify, keep)
                .unwrap_or_else(|| root.clone())
        };
        FlexTreeCore {
            upper_root: filter(&self.upper_root, &FlexId::UPPER_MAX),
            lower_root: filter(&self.lower_root, &FlexId::LOWER_MAX),
        }
    }
}

impl<V: Ord + Clone> FlexTreeCore<V> {
    /// 全ての値の範囲 `(最小, 最大)`。空なら [`None`]。
    pub fn value_range(&self) -> Option<(&V, &V)> {
        self.iter()
            .map(|(_, value)| value)
            .fold(None, |range, value| {
                Some(match range {
                    None => (value, value),
                    Some((min, max)) => (min.min(value), max.max(value)),
                })
            })
    }

    /// 値が `range` に含まれる [FlexId] だけを残す。
    pub fn filter_range<R: RangeBounds<V>>(&self, range: R) -> Self {
        self.filter(&|_| Decision::Descend, &|_, value| range.contains(value))
    }

    /// 全ての値の集合。
    pub fn value_set(&self) -> BTreeSet<V> {
        self.iter().map(|(_, value)| value.clone()).collect()
    }

    /// 値が `values` に含まれる [FlexId] だけを残す。
    pub fn filter_values(&self, values: &BTreeSet<V>) -> Self {
        self.filter(&|_| Decision::Descend, &|_, value| values.contains(value))
    }
}

/// 領域 `this` の `branch` の子のうち `t` と重なるものを、下側から取り出されるように積む。
///
/// `t` が割った次元で `this` より細かければ、`t` がどちらの半分にあるかが割った次元だけで決まるので、
/// 両方の子の領域を作って確かめずに済む。
// 展開されないと、`get` の1段ごとに呼び出しが挟まり、`get` が4割ほど遅くなる
#[inline(always)]
fn push_overlapping<'a, V>(
    stack: &mut Vec<(&'a Node<V>, FlexId)>,
    branch: &'a node::Branch<V>,
    this: FlexId,
    t: &FlexId,
) {
    let dimension = branch.dimension;
    if t.zoomlevel_on(dimension) > this.zoomlevel_on(dimension) {
        let side = this.side_toward(dimension, t);
        let child = match side {
            Side::Lower => &branch.lower,
            Side::Upper => &branch.upper,
        };
        stack.push((child, this.split_on(dimension, side).unwrap()));
        return;
    }
    for (child, side) in [(&branch.upper, Side::Upper), (&branch.lower, Side::Lower)] {
        let region = this.split_on(dimension, side).unwrap();
        if region.intersects(t) {
            stack.push((child, region));
        }
    }
}
