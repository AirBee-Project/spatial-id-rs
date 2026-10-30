use super::node::{Branch, Node};
use crate::{FlexId, Side, id::dimension::Dimension};
use core::ptr;
use triomphe::Arc;

/// 領域 `this` の上の Node を借りて見たもの。
pub(super) enum View<'a, V> {
    Empty,
    Leaf(&'a V),
    Branch(&'a Arc<Branch<V>>),
    /// 領域のうち `region` だけに `child` があり、外側は空。
    Skip {
        region: FlexId,
        child: Target<'a, V>,
    },
}

/// Skip の先にある、空でも Skip でもない Node を借りたもの。
pub(super) enum Target<'a, V> {
    Leaf(&'a V),
    Branch(&'a Arc<Branch<V>>),
}

impl<V> Clone for View<'_, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> Copy for View<'_, V> {}

impl<V> Clone for Target<'_, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> Copy for Target<'_, V> {}

impl<'a, V> Target<'a, V> {
    /// Leaf か Branch の `node` を借りる。
    pub(super) fn of(node: &'a Node<V>) -> Self {
        match node {
            Node::Leaf(value) => Target::Leaf(value),
            Node::Branch(branch) => Target::Branch(branch),
            _ => unreachable!("Skip の先は Leaf か Branch"),
        }
    }

    pub(super) fn split_dimensions(self) -> u8 {
        match self {
            Target::Leaf(_) => 0,
            Target::Branch(branch) => branch.split_dimensions,
        }
    }
}

impl<'a, V> View<'a, V> {
    /// 領域 `this` の `node` を見る。
    pub(super) fn of(node: &'a Node<V>, this: &FlexId) -> Self {
        match node {
            Node::Empty => View::Empty,
            Node::Leaf(value) => View::Leaf(value),
            Node::Branch(branch) => View::Branch(branch),
            node => {
                let (region, child) = node.skip_target(this).unwrap();
                View::Skip { region, child }
            }
        }
    }

    /// 領域 `this` のうち `region` だけに、領域 `region` の `node` がある View。
    pub(super) fn skip(this: &FlexId, region: FlexId, node: &'a Node<V>) -> Self {
        match View::of(node, &region) {
            View::Empty => View::Empty,
            View::Leaf(value) => View::at(this, region, Target::Leaf(value)),
            View::Branch(branch) => View::at(this, region, Target::Branch(branch)),
            View::Skip { region, child } => View::at(this, region, child),
        }
    }

    /// 領域 `this` のうち `region` だけに `child` がある View。
    pub(super) fn at(this: &FlexId, region: FlexId, child: Target<'a, V>) -> Self {
        match child {
            _ if region != *this => View::Skip { region, child },
            Target::Leaf(value) => View::Leaf(value),
            Target::Branch(branch) => View::Branch(branch),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        matches!(self, View::Empty)
    }

    /// Leaf ならその値。
    pub(super) fn leaf(&self) -> Option<&'a V> {
        match self {
            View::Leaf(value) => Some(value),
            _ => None,
        }
    }

    /// 領域 `this` の `node` と同じ領域に同じ Branch の Arc を見ているなら true。値の型が違う Node とも比べられる。
    // `merge` の各段で呼ぶので展開させる
    #[inline(always)]
    pub(super) fn ptr_eq<W>(&self, this: &FlexId, node: &Node<W>) -> bool {
        let same =
            |a: &Arc<Branch<V>>, b: &Arc<Branch<W>>| ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b));
        match (self, node) {
            (View::Branch(a), Node::Branch(b)) => same(a, b),
            // 行き先の領域を求めるのは、先の Branch が同じときだけ
            (
                View::Skip {
                    region,
                    child: Target::Branch(a),
                },
                Node::BranchAt(_, b),
            ) => same(a, b) && node.skip_target(this).is_some_and(|(r, _)| r == *region),
            (
                View::Skip {
                    region,
                    child: Target::Branch(a),
                },
                Node::Skip(skip),
            ) => {
                matches!(&skip.child, Node::Branch(b) if same(a, b))
                    && node.skip_target(this).is_some_and(|(r, _)| r == *region)
            }
            _ => false,
        }
    }

    /// Skip なら、行き先の領域とその先。
    pub(super) fn as_skip(&self) -> Option<(FlexId, Target<'a, V>)> {
        match *self {
            View::Skip { region, child } => Some((region, child)),
            _ => None,
        }
    }

    /// 最初に割る次元の候補（[`Dimension::bit`] の OR）。Leaf や空なら 0。
    pub(super) fn head_dimensions(&self, this: &FlexId) -> u8 {
        match self {
            View::Empty | View::Leaf(_) => 0,
            View::Branch(branch) => branch.dimension.bit(),
            View::Skip { region, child } => {
                region.finer_dimensions_than(this) | child.split_dimensions()
            }
        }
    }

    /// `dimension` で割った `[下, 上]`。`dimension` で割っていなければ、両側に自身を返す。
    ///
    /// `halves` は `this` を `dimension` で割った `[下, 上]` の領域。
    pub(super) fn split(
        &self,
        this: &FlexId,
        dimension: Dimension,
        halves: &[FlexId; 2],
    ) -> [Self; 2] {
        let (region, child) = match *self {
            View::Branch(branch) if branch.dimension == dimension => {
                return [
                    View::of(&branch.lower, &halves[0]),
                    View::of(&branch.upper, &halves[1]),
                ];
            }
            View::Skip { region, child } => (region, child),
            _ => return [*self, *self],
        };
        if region.finer_dimensions_than(this) & dimension.bit() != 0 {
            let side = this.side_toward(dimension, &region);
            let rest = View::at(&halves[side as usize], region, child);
            return match side {
                Side::Lower => [rest, View::Empty],
                Side::Upper => [View::Empty, rest],
            };
        }
        // その次元では中身が変わらないので、下も上も同じ中身になる。
        // 行き先は絶対位置なので、それぞれの半分に切り詰める
        halves.map(|half| View::at(&half, region.intersection(&half).unwrap(), child))
    }
}

impl<V: PartialEq + Clone> View<'_, V> {
    /// 領域 `this` の Node として取り出す。
    pub(super) fn to_node(self, this: &FlexId) -> Node<V> {
        match self {
            View::Empty => Node::Empty,
            View::Leaf(value) => Node::Leaf(value.clone()),
            View::Branch(branch) => Node::Branch(branch.clone()),
            View::Skip { region, child } => {
                let child = match child {
                    Target::Leaf(value) => Node::Leaf(value.clone()),
                    Target::Branch(branch) => Node::Branch(branch.clone()),
                };
                Node::rebase(child, &region, this)
            }
        }
    }
}
