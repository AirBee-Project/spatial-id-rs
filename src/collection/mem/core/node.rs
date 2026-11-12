use super::view::{Target, View};
use crate::{
    FlexId, Side,
    id::{
        dimension::{Dimension, DimensionSet},
        relative_flex_id::{RelativeFlexId, ShortRelativeFlexId},
    },
};
use core::{
    mem::{self, ManuallyDrop},
    ptr,
};
use triomphe::Arc;

/// Node は自分の領域を持たず、親から渡される領域 `this`との相対的な位置で意味を持つ。辿る関数が`this`を持つことで様々な操作を行う。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Node<V> {
    Value(V),
    Branch(Arc<Branch<V>>),
    /// 行き先を [ShortRelativeFlexId] で表せない Skip。
    LongSkip(Arc<Skip<V>>),
    /// 行き先を [ShortRelativeFlexId] で表せて、先が Value の Skip。確保せずに Node に埋め込む。
    ShortSkipValue(ShortRelativeFlexId, V),
    /// 行き先を [ShortRelativeFlexId] で表せて、先が Branch の Skip。
    ShortSkipBranch(ShortRelativeFlexId, Arc<Branch<V>>),
}

impl<V: Clone> Clone for Node<V> {
    fn clone(&self) -> Self {
        match self {
            Node::Value(value) => Node::Value(value.clone()),
            Node::Branch(branch) => Node::Branch(branch.clone()),
            Node::LongSkip(skip) => Node::LongSkip(skip.clone()),
            Node::ShortSkipValue(path, value) => Node::ShortSkipValue(*path, value.clone()),
            Node::ShortSkipBranch(path, branch) => Node::ShortSkipBranch(*path, branch.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Branch<V> {
    pub(super) dimension: Dimension,
    /// 自身と子孫が割っている次元の集合。自身の `dimension` とlowerとupperの集合を合わせたもの。
    pub(super) split_dimensions: DimensionSet,
    /// 自身と子孫で Branch が縦に何段続くかの最大。Node を辿るスタックの大きさに使う。
    pub(super) height: u8,
    /// 子孫に含まれる、値を持つ[FlexId]の数。
    pub(super) count: u32,
    pub(super) lower: Node<V>,
    pub(super) upper: Node<V>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skip<V> {
    pub(super) path: RelativeFlexId,
    /// 自身と子孫が割っている次元の集合。行き先が狭い次元（`path` の深くなっている次元）と `child` の集合を合わせたもの。
    pub(super) split_dimensions: DimensionSet,
    pub(super) child: Node<V>,
}

/// [`Node::filter`] で、Branch の領域から子孫をまとめて判断した結果。
pub(super) enum Decision {
    /// 子孫をすべて残す。
    KeepAll,
    /// 子孫をすべて捨てる。
    DropAll,
    /// 子へ降りて判断する。
    Descend,
}

impl<V> Node<V> {
    /// 子孫に含まれる、値を持つ[FlexId]の数。
    pub(super) fn count(&self) -> u32 {
        match self {
            Node::Value(_) | Node::ShortSkipValue(..) => 1,
            Node::Branch(branch) | Node::ShortSkipBranch(_, branch) => branch.count,
            Node::LongSkip(skip) => skip.child.count(),
        }
    }

    /// 自身と子孫で Branch が縦に何段続くかの最大。Value なら 0。
    pub(super) fn height(&self) -> u8 {
        match self {
            Node::Value(_) | Node::ShortSkipValue(..) => 0,
            Node::Branch(branch) | Node::ShortSkipBranch(_, branch) => branch.height,
            Node::LongSkip(skip) => skip.child.height(),
        }
    }

    /// 自身と子孫が割っている次元の集合。
    pub(super) fn split_dimensions(&self) -> DimensionSet {
        match self {
            Node::Value(_) => DimensionSet::EMPTY,
            Node::Branch(branch) => branch.split_dimensions,
            Node::LongSkip(skip) => skip.split_dimensions,
            Node::ShortSkipValue(path, _) => path.deeper_dimensions(),
            Node::ShortSkipBranch(path, branch) => {
                path.deeper_dimensions() | branch.split_dimensions
            }
        }
    }

    /// Skip なら、行き先が狭い次元の集合。Skip でなければ空。
    fn path_dimensions(&self) -> DimensionSet {
        match self {
            Node::LongSkip(skip) => skip.path.deeper_dimensions(),
            Node::ShortSkipValue(path, _) | Node::ShortSkipBranch(path, _) => {
                path.deeper_dimensions()
            }
            Node::Value(_) | Node::Branch(_) => DimensionSet::EMPTY,
        }
    }

    /// 領域 `this` のこの Node が Skip なら、行き先の領域とその先。
    // `merge` の各段で呼ぶので展開させる
    #[inline(always)]
    pub(super) fn skip_target(&self, this: &FlexId) -> Option<(FlexId, Target<'_, V>)> {
        match self {
            Node::LongSkip(skip) => Some((
                skip.path.to_absolute(this).unwrap(),
                Target::of(&skip.child),
            )),
            Node::ShortSkipValue(path, value) => {
                Some((path.to_absolute(this), Target::Value(value)))
            }
            Node::ShortSkipBranch(path, branch) => {
                Some((path.to_absolute(this), Target::Branch(branch)))
            }
            _ => None,
        }
    }

    /// この Node を最初に割る次元の候補。このうち一番粗い次元で最初に割る。Value なら空。
    pub(super) fn head_dimensions(&self) -> DimensionSet {
        match self {
            Node::Value(_) => DimensionSet::EMPTY,
            Node::Branch(branch) => branch.dimension.into(),
            node => node.split_dimensions(),
        }
    }

    /// 領域 `this` のこの Node の子とその領域を、下側から順に返す。Value・ShortSkipValue や空なら何も返さない。
    ///
    /// ShortSkipBranch は、行き先の Branch の子を返す。
    // 展開されないと、`get` の1段ごとに呼び出しが挟まり、`get` が4割ほど遅くなる
    #[inline(always)]
    pub(super) fn children(
        &self,
        this: FlexId,
    ) -> impl DoubleEndedIterator<Item = (&Self, FlexId)> {
        fn halves<V>(branch: &Branch<V>, this: FlexId) -> [Option<(&Node<V>, FlexId)>; 2] {
            let half = |side| this.split_on(branch.dimension, side).unwrap();
            [
                Some((&branch.lower, half(Side::Lower))),
                Some((&branch.upper, half(Side::Upper))),
            ]
        }
        let children = match self {
            Node::Value(_) | Node::ShortSkipValue(..) => [None, None],
            Node::Branch(branch) => halves(branch, this),
            Node::ShortSkipBranch(path, branch) => halves(branch, path.to_absolute(&this)),
            Node::LongSkip(skip) => [
                Some((&skip.child, skip.path.to_absolute(&this).unwrap())),
                None,
            ],
        };
        children.into_iter().flatten()
    }

    /// 同じ Branch・Skip の Arc を指しているなら true。値の型が違う Node とも比べられる。
    pub(super) fn ptr_eq<W>(&self, other: &Node<W>) -> bool {
        match (self, other) {
            (Node::Branch(a), Node::Branch(b)) => ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (Node::LongSkip(a), Node::LongSkip(b)) => ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (Node::ShortSkipBranch(p, a), Node::ShortSkipBranch(q, b)) => {
                p == q && ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
            }
            _ => false,
        }
    }
}

impl<V> Branch<V> {
    fn new(dimension: Dimension, lower: Node<V>, upper: Node<V>) -> Self {
        Branch {
            dimension,
            split_dimensions: DimensionSet::from(dimension)
                | lower.split_dimensions()
                | upper.split_dimensions(),
            height: 1 + lower.height().max(upper.height()),
            count: lower.count() + upper.count(),
            lower,
            upper,
        }
    }
}

impl<V> Skip<V> {
    fn new(path: RelativeFlexId, child: Node<V>) -> Self {
        Skip {
            split_dimensions: path.deeper_dimensions() | child.split_dimensions(),
            path,
            child,
        }
    }
}

impl<V: PartialEq + Clone> Node<V> {
    /// `join` が Branch を作るなら true。
    fn forms_branch(dimension: Dimension, lower: &Self, upper: &Self) -> bool {
        // Node は領域との相対位置で意味を持つので、子孫が `dimension` で割っている Node を
        // 広い領域へ持ち上げると意味が変わる。左右が同じでも、そのときは割ったままにする。
        let same = !lower.split_dimensions().contains(dimension)
            && (lower.ptr_eq(upper) || lower == upper);
        !same
    }

    /// 領域 `this` を `dimension` で割った左右 `lower`,`upper` を、1つの Node に合わせる。まとめられれば1つにし、片側が空なら Skip にする。
    pub(super) fn join(
        this: &FlexId,
        dimension: Dimension,
        lower: Option<Self>,
        upper: Option<Self>,
    ) -> Option<Self> {
        match (lower, upper) {
            (None, None) => None,
            (Some(lower), None) => {
                let lower_id = this.split_on(dimension, Side::Lower).unwrap();
                Some(Node::rebase(lower, &lower_id, this))
            }
            (None, Some(upper)) => {
                let upper_id = this.split_on(dimension, Side::Upper).unwrap();
                Some(Node::rebase(upper, &upper_id, this))
            }
            (Some(lower), Some(upper)) => {
                // Branch はここと `merge` でしか作らない。これでどの操作の結果も、同じ形状なら同じ FlexTreeCore になる。
                if Node::forms_branch(dimension, &lower, &upper) {
                    Some(Node::Branch(Arc::new(Branch::new(dimension, lower, upper))))
                } else {
                    Some(lower)
                }
            }
        }
    }

    /// 領域 `from` から見た `node` を、領域 `to` から見た Node にする。`node` の中身は `to` に収まっていること。
    ///
    /// 他と共有していない Skip は、確保し直さずに行き先だけを付け替える。
    pub(super) fn rebase(node: Self, from: &FlexId, to: &FlexId) -> Self {
        match node {
            _ if from == to => node,
            // Skip の先へさらに Skip しないよう、Skip の行き先から直接張り直す
            Node::LongSkip(mut skip) => {
                let region = skip.path.to_absolute(from).unwrap();
                if region == *to {
                    return Arc::unwrap_or_clone(skip).child;
                }
                let path = region.relative_to(to).unwrap();
                if let Some(short) = ShortRelativeFlexId::new(path) {
                    return Node::embed(short, Arc::unwrap_or_clone(skip).child);
                }
                let skip_mut = Arc::make_mut(&mut skip);
                skip_mut.path = path;
                skip_mut.split_dimensions =
                    path.deeper_dimensions() | skip_mut.child.split_dimensions();
                Node::LongSkip(skip)
            }
            Node::ShortSkipValue(path, value) => {
                Node::rebase(Node::Value(value), &path.to_absolute(from), to)
            }
            Node::ShortSkipBranch(path, branch) => {
                Node::rebase(Node::Branch(branch), &path.to_absolute(from), to)
            }
            node => Node::skip(from.relative_to(to).unwrap(), node),
        }
    }

    /// `path` の先だけに Value か Branch の `child` がある Skip。`path` が短ければ確保せずに埋め込む。
    fn skip(path: RelativeFlexId, child: Self) -> Self {
        match ShortRelativeFlexId::new(path) {
            Some(short) => Node::embed(short, child),
            None => Node::LongSkip(Arc::new(Skip::new(path, child))),
        }
    }

    /// `path` の先だけに Value か Branch の `child` がある Skip を、確保せずに埋め込む。
    fn embed(path: ShortRelativeFlexId, child: Self) -> Self {
        match child {
            Node::Value(value) => Node::ShortSkipValue(path, value),
            Node::Branch(branch) => Node::ShortSkipBranch(path, branch),
            _ => unreachable!("Skip の先は Value か Branch"),
        }
    }

    /// 領域 `this` のこの Node を `dimension` で割った `[下, 上]`（`join` の逆）。
    ///
    /// `dimension` で割っていなければ、両側にこの Node を返す。
    ///
    /// `halves` は `this` を `dimension` で割った `[下, 上]` の領域。
    fn split(self, this: &FlexId, dimension: Dimension, halves: &[FlexId; 2]) -> [Option<Self>; 2] {
        if self.path_dimensions().contains(dimension) {
            let (region, _) = self.skip_target(this).unwrap();
            let side = this.side_toward(dimension, &region);
            let rest = Node::rebase(self, this, &halves[side as usize]);
            return match side {
                Side::Lower => [Some(rest), None],
                Side::Upper => [None, Some(rest)],
            };
        }
        // その次元では中身が変わらないので、下も上も同じ中身になる
        [Some(self.clone()), Some(self)]
    }

    /// 領域 `this` の `a` に `b` を `rule` に従って重ね合わせる。
    ///
    /// `rule` は、`a` と `b` から結果がすぐ決まるなら `a` を書き換えて true を返す。決まらなければ両方を割って子へ降りる。
    /// 他と共有していない Branch・Skip は作り直さずに書き換え、共有しているものは複製してから書き換える。
    pub(super) fn merge<W>(
        this: &FlexId,
        a: &mut Option<Self>,
        b: View<'_, W>,
        rule: &impl Fn(&FlexId, &mut Option<Self>, View<'_, W>) -> bool,
    ) {
        if rule(this, a, b) {
            return;
        }
        // Skip どうしは、行き先が分かれるところまで1段ずつ割らずに一気に降りる
        if let Some(node_a) = a
            && let Some((region_a, split_a)) = node_a
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
            // 変わるのは行き先の中だけなので、先が Value か Branch のままなら行き先を求め直さない
            if at == region_a {
                match a.take().unwrap() {
                    Node::LongSkip(skip) => {
                        let merged = update_in_place(skip, |skip| {
                            let mut child = Some(skip.child);
                            Node::merge(&at, &mut child, b, rule);
                            // LongSkip の行き先は埋め込めない長さのまま変わらないので、先が Value か Branch ならそのまま張り直す
                            match child {
                                Some(child @ (Node::Value(_) | Node::Branch(_))) => {
                                    Ok(Skip::new(skip.path, child))
                                }
                                child => Err(child.map(|child| Node::rebase(child, &at, this))),
                            }
                        });
                        *a = merged.map_or_else(|node| node, |skip| Some(Node::LongSkip(skip)));
                    }
                    short @ (Node::ShortSkipValue(..) | Node::ShortSkipBranch(..)) => {
                        let (path, inner) = match short {
                            Node::ShortSkipValue(path, value) => (path, Node::Value(value)),
                            Node::ShortSkipBranch(path, branch) => (path, Node::Branch(branch)),
                            _ => unreachable!(),
                        };
                        let mut inner = Some(inner);
                        Node::merge(&at, &mut inner, b, rule);
                        *a = inner.map(|inner| match inner {
                            inner @ (Node::Value(_) | Node::Branch(_)) => Node::embed(path, inner),
                            inner => Node::rebase(inner, &at, this),
                        });
                    }
                    _ => unreachable!("行き先があるのは Skip だけ"),
                }
                return;
            }
            if at != *this {
                let mut inner = Some(Node::rebase(a.take().unwrap(), this, &at));
                Node::merge(&at, &mut inner, b, rule);
                *a = inner.map(|inner| Node::rebase(inner, &at, this));
                return;
            }
        }
        // 結果の FlexTreeCore も粗い次元から割るよう、両方の最初の次元のうち粗い方で割る
        let a_head = a
            .as_ref()
            .map_or(DimensionSet::EMPTY, |n| n.head_dimensions());
        let dimension = this
            .coarsest_dimension_in(a_head | b.head_dimensions(this))
            .expect("Value どうしは rule が答えを決める");
        let lower_id = this.split_on(dimension, Side::Lower).unwrap();
        let upper_id = this.split_on(dimension, Side::Upper).unwrap();
        let halves = [lower_id, upper_id];
        let [b_lower, b_upper] = b.split(this, dimension, &halves);

        if let Some(Node::Branch(branch)) = a
            && branch.dimension == dimension
        {
            let Some(Node::Branch(branch)) = a.take() else {
                unreachable!()
            };
            let merged = update_in_place(branch, |branch| {
                let mut lower = Some(branch.lower);
                let mut upper = Some(branch.upper);
                Node::merge(&lower_id, &mut lower, b_lower, rule);
                Node::merge(&upper_id, &mut upper, b_upper, rule);
                match (lower, upper) {
                    (Some(lower), Some(upper)) if Node::forms_branch(dimension, &lower, &upper) => {
                        Ok(Branch::new(dimension, lower, upper))
                    }
                    // `join` に渡すと左右をもう一度比べるので、まとめられると分かった左右は直接返す
                    (Some(lower), Some(_)) => Err(Some(lower)),
                    (lower, upper) => Err(Node::join(this, dimension, lower, upper)),
                }
            });
            *a = merged.map_or_else(|node| node, |branch| Some(Node::Branch(branch)));
            return;
        }
        let [mut lower, mut upper] = match a.take() {
            Some(node) => node.split(this, dimension, &halves),
            None => [None, None],
        };
        Node::merge(&lower_id, &mut lower, b_lower, rule);
        Node::merge(&upper_id, &mut upper, b_upper, rule);
        *a = Node::join(this, dimension, lower, upper);
    }

    /// 和。`a` に値がある場所は `a`、無い場所は `b`。
    pub(super) fn union_rule(this: &FlexId, a: &mut Option<Self>, b: View<'_, V>) -> bool {
        if a.is_none() {
            *a = b.to_node(this);
            return true;
        }
        matches!(a, Some(Node::Value(_)))
            || b.is_empty()
            || a.as_ref().is_some_and(|a| b.ptr_eq(this, a))
    }

    /// 積。`b` に値がある場所だけ `a` を残す。
    pub(super) fn intersection_rule<W>(
        this: &FlexId,
        a: &mut Option<Self>,
        b: View<'_, W>,
    ) -> bool {
        if b.is_empty() {
            *a = None;
            return true;
        }
        a.is_none() || b.value().is_some() || a.as_ref().is_some_and(|a| b.ptr_eq(this, a))
    }

    /// 差。`b` に値がある場所の `a` を消す。
    pub(super) fn difference_rule<W>(this: &FlexId, a: &mut Option<Self>, b: View<'_, W>) -> bool {
        if b.value().is_some() || a.as_ref().is_some_and(|a| b.ptr_eq(this, a)) {
            *a = None;
            return true;
        }
        a.is_none() || b.is_empty()
    }

    /// 領域 `this` のこの Node から、条件を満たす Value だけを残した Node を返す。何も捨てなければ [`None`]。
    ///
    /// Branch は領域を `classify` で判断し、子孫をまとめて残すか捨てられるならそれ以上降りない。
    /// Value は領域と値を `keep` で判断する。
    pub(super) fn filter(
        &self,
        this: &FlexId,
        classify: &impl Fn(&FlexId) -> Decision,
        keep: &impl Fn(&FlexId, &V) -> bool,
    ) -> Option<Option<Self>> {
        match self {
            Node::Value(value) => (!keep(this, value)).then_some(None),
            Node::ShortSkipValue(path, value) => {
                let region = path.to_absolute(this);
                (!keep(&region, value)).then_some(None)
            }
            Node::Branch(branch) => Node::filter_branch(branch, this, classify, keep),
            Node::ShortSkipBranch(path, branch) => {
                let region = path.to_absolute(this);
                match Node::filter_branch(branch, &region, classify, keep)? {
                    Some(child) => Some(Some(Node::rebase(child, &region, this))),
                    None => Some(None),
                }
            }
            Node::LongSkip(skip) => {
                let region = skip.path.to_absolute(this).unwrap();
                match skip.child.filter(&region, classify, keep)? {
                    Some(child) => Some(Some(Node::rebase(child, &region, this))),
                    None => Some(None),
                }
            }
        }
    }

    /// 領域 `this` の `branch` に [`filter`](Self::filter) をかける。
    fn filter_branch(
        branch: &Arc<Branch<V>>,
        this: &FlexId,
        classify: &impl Fn(&FlexId) -> Decision,
        keep: &impl Fn(&FlexId, &V) -> bool,
    ) -> Option<Option<Self>> {
        match classify(this) {
            Decision::KeepAll => None,
            Decision::DropAll => Some(None),
            Decision::Descend => {
                let dimension = branch.dimension;
                let lower_id = this.split_on(dimension, Side::Lower).unwrap();
                let upper_id = this.split_on(dimension, Side::Upper).unwrap();
                let lower_opt = branch.lower.filter(&lower_id, classify, keep);
                let upper_opt = branch.upper.filter(&upper_id, classify, keep);
                if lower_opt.is_none() && upper_opt.is_none() {
                    return None;
                }
                let lower = match lower_opt {
                    Some(l) => l,
                    None => Some(branch.lower.clone()),
                };
                let upper = match upper_opt {
                    Some(u) => u,
                    None => Some(branch.upper.clone()),
                };
                Some(Node::join(this, dimension, lower, upper))
            }
        }
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
#[inline]
pub(super) fn common_descent(
    this: &FlexId,
    (region_a, split_a): (&FlexId, DimensionSet),
    (region_b, split_b): (&FlexId, DimensionSet),
) -> FlexId {
    let splits = split_a | split_b;

    #[inline(always)]
    fn agree_u32(za: u8, ia: u32, zb: u8, ib: u32) -> u8 {
        let deepest = za.min(zb);
        let diff = (ia >> (za - deepest)) ^ (ib >> (zb - deepest));
        deepest - (u32::BITS - diff.leading_zeros()) as u8
    }

    let (fa, fb) = (region_a.f_zoomlevel(), region_b.f_zoomlevel());
    let f_agree = {
        let deepest = fa.min(fb);
        let diff = ((region_a.f_index() >> (fa - deepest)) ^ (region_b.f_index() >> (fb - deepest)))
            as u32;
        deepest - (u32::BITS - diff.leading_zeros()) as u8
    };
    let (xa, xb) = (region_a.x_zoomlevel(), region_b.x_zoomlevel());
    let x_agree = agree_u32(xa, region_a.x_index(), xb, region_b.x_index());
    let (ya, yb) = (region_a.y_zoomlevel(), region_b.y_zoomlevel());
    let y_agree = agree_u32(ya, region_a.y_index(), yb, region_b.y_index());
    let (ta, tb) = (region_a.t_zoomlevel(), region_b.t_zoomlevel());
    let t_agree = {
        let deepest = ta.min(tb);
        let diff = (region_a.t_index() >> (ta - deepest)) ^ (region_b.t_index() >> (tb - deepest));
        deepest - (u64::BITS - diff.leading_zeros()) as u8
    };

    let bound = [f_agree, x_agree, y_agree, t_agree];
    let blocks = [
        fa > f_agree || fb > f_agree || splits.contains(Dimension::F),
        xa > x_agree || xb > x_agree || splits.contains(Dimension::X),
        ya > y_agree || yb > y_agree || splits.contains(Dimension::Y),
        ta > t_agree || tb > t_agree || splits.contains(Dimension::T),
    ];

    let mut stop_bound = u8::MAX;
    let mut stop_dim = Dimension::F;
    for d in Dimension::ALL {
        let i = d as usize;
        if blocks[i] && bound[i] < stop_bound {
            stop_bound = bound[i];
            stop_dim = d;
        }
    }

    let zoom = Dimension::ALL.map(|d| {
        let reach = if stop_bound == u8::MAX {
            bound[d as usize]
        } else {
            // 止まる次元より前の次元は、同じ深さの番を1段ぶん先に済ませている
            bound[d as usize].min(stop_bound + u8::from(d < stop_dim))
        };
        this.zoomlevel_on(d).max(reach)
    });
    region_a.ancestor_at(zoom)
}

/// 他と共有していない状態にした `arc` の中身を `update` に渡す。
/// `update` が [`Ok`] を返せば同じ確保へ書き戻し、[`Err`] を返せば確保を解放してそれを返す。
///
/// 確保し直さずに Node を書き換えるためのもの。`update` が panic すると、取り出した中身と `arc` に残った中身を
/// 二重に解放してしまうので、そのときはプロセスを abort する。
fn update_in_place<T: Clone, E>(
    mut arc: Arc<T>,
    update: impl FnOnce(T) -> Result<T, E>,
) -> Result<Arc<T>, E> {
    struct AbortOnUnwind;
    impl Drop for AbortOnUnwind {
        fn drop(&mut self) {
            // unwind の途中で panic すると abort になる
            panic!("取り出した Node を二重に解放しないよう abort する");
        }
    }

    let inner: *mut T = Arc::make_mut(&mut arc);
    let guard = AbortOnUnwind;
    // SAFETY: `make_mut` で共有していないので、ほかに中身を読む者はいない。`update` が panic したら guard が abort する
    let result = update(unsafe { ptr::read(inner) });
    mem::forget(guard);
    match result {
        Ok(value) => {
            // SAFETY: 中身は取り出し済みなので、落とさずに上書きする
            unsafe { ptr::write(inner, value) };
            Ok(arc)
        }
        Err(other) => {
            // SAFETY: `ManuallyDrop<T>` は `T` と同じレイアウトなので、同じ確保として解放できる。取り出し済みの中身は落とさない
            drop(unsafe { Arc::from_raw(Arc::into_raw(arc).cast::<ManuallyDrop<T>>()) });
            Err(other)
        }
    }
}
