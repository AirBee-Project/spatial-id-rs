//! [`Node`] を書き換える編集ロジック。
//!
//! 触る領域にある [`Node::RefPage`] は、中身を読む必要があるときだけ読み出す。

use super::FlexTreeCore;
use super::node::Node;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::{DiskError, FlexId, Side};
use alloc::boxed::Box;

/// 領域への操作。意味は [`SpatialIdMap`](crate::SpatialIdMap) の同名の操作と同じ。
#[derive(Debug, Clone, Copy)]
pub(super) enum Op {
    /// 値を書き込む。既に値がある場所は上書きする。
    Insert(u64),
    /// まだ値のない場所にだけ値を書き込む。
    Upsert(u64),
    /// 空にする。
    Remove,
}

impl FlexTreeCore {
    /// 領域 `this` の `node` の `target` に `op` を施す。空になれば [`None`]。`target` は `this` に含まれること。
    pub(super) async fn edit_node(
        &self,
        node: Option<Node>,
        this: &FlexId,
        target: &FlexId,
        op: Op,
    ) -> Result<Option<Node>, DiskError> {
        let Some(node) = node else {
            return match op {
                Op::Remove => Ok(None),
                Op::Insert(value) | Op::Upsert(value) => {
                    Ok(Some(Node::Value(value).rebase(target, this)?))
                }
            };
        };

        if target == this {
            match op {
                Op::Insert(value) => return Ok(Some(Node::Value(value))),
                Op::Remove => return Ok(None),
                Op::Upsert(_) => {}
            }
        }

        // Branch と Skip は割った両側に同じ Node を複製しないので、RefPage が2か所から指されることはない
        let (dimension, halves) = match node {
            Node::RefPage { page_id, .. } => {
                let loaded = self.read_page(page_id).await?;
                return Box::pin(self.edit_node(Some(loaded), this, target, op)).await;
            }
            Node::Value(old) => match op {
                Op::Upsert(_) => return Ok(Some(Node::Value(old))),
                Op::Insert(value) if value == old => return Ok(Some(Node::Value(old))),
                Op::Insert(_) | Op::Remove => {
                    let dimension = this
                        .coarsest_dimension_in(target.finer_dimensions_than(this))
                        .expect("target == this の Insert / Remove は上で処理済み");
                    (dimension, [Some(Node::Value(old)), Some(Node::Value(old))])
                }
            },
            Node::Branch {
                dimension,
                lower,
                upper,
                ..
            } => (dimension, [Some(*lower), Some(*upper)]),
            Node::Skip { path, child } => {
                let region = path.to_absolute(this).map_err(|_| DiskError::Corrupt)?;
                if region.contains(target) {
                    let child = Box::pin(self.edit_node(Some(*child), &region, target, op)).await?;
                    return child.map(|child| child.rebase(&region, this)).transpose();
                }
                if matches!(op, Op::Remove) && !region.intersects(target) {
                    return Ok(Some(Node::Skip { path, child }));
                }
                // 行き先の外にも施すので、行き先が深い次元で割り、行き先のない側を空として扱う
                let dimension = this
                    .coarsest_dimension_in(path.deeper_dimensions())
                    .expect("Skip の行き先はどれかの次元で深い");
                let side = this.side_toward(dimension, &region);
                let half = this.halves(dimension).ok_or(DiskError::Corrupt)?[side as usize];
                let rest = Some((*child).rebase(&region, &half)?);
                let halves = match side {
                    Side::Lower => [rest, None],
                    Side::Upper => [None, rest],
                };
                (dimension, halves)
            }
        };

        let regions = this.halves(dimension).ok_or(DiskError::Corrupt)?;
        let [mut lower, mut upper] = halves;
        if let Some(sub) = target.intersection(&regions[0]) {
            lower = Box::pin(self.edit_node(lower, &regions[0], &sub, op)).await?;
        }
        if let Some(sub) = target.intersection(&regions[1]) {
            upper = Box::pin(self.edit_node(upper, &regions[1], &sub, op)).await?;
        }
        Node::join(this, dimension, [lower, upper], &regions)
    }
}

impl Node {
    /// 領域 `this` を `dimension` で割った `[下, 上]` の Node を1つの Node に合わせる。
    ///
    /// 左右が同じで `dimension` で割っていなければ1つにまとめる。
    /// 割っている Node を広い領域へ持ち上げると意味が変わるので、そのときは割ったままにする。
    fn join(
        this: &FlexId,
        dimension: Dimension,
        [lower, upper]: [Option<Node>; 2],
        [lower_region, upper_region]: &[FlexId; 2],
    ) -> Result<Option<Node>, DiskError> {
        Ok(match (lower, upper) {
            (None, None) => None,
            (Some(lower), None) => Some(lower.rebase(lower_region, this)?),
            (None, Some(upper)) => Some(upper.rebase(upper_region, this)?),
            (Some(lower), Some(upper)) => {
                if lower == upper && !lower.split_dimensions().contains(dimension) {
                    Some(lower)
                } else {
                    Some(Node::Branch {
                        dimension,
                        split_dimensions: DimensionSet::from(dimension)
                            | lower.split_dimensions()
                            | upper.split_dimensions(),
                        lower: Box::new(lower),
                        upper: Box::new(upper),
                    })
                }
            }
        })
    }

    /// 領域 `from` の自身を、`from` を含む領域 `to` から見た Node に張り直す。Skip の先へさらに Skip しないよう統合する。
    fn rebase(self, from: &FlexId, to: &FlexId) -> Result<Node, DiskError> {
        if from == to {
            return Ok(self);
        }
        let (from, child) = match self {
            Node::Skip { path, child } => (
                path.to_absolute(from).map_err(|_| DiskError::Corrupt)?,
                child,
            ),
            node => (*from, Box::new(node)),
        };
        let path = from.relative_to(to).expect("from は to に含まれる");
        Ok(Node::Skip { path, child })
    }
}
