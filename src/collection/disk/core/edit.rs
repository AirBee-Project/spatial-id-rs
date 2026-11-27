//! [`Node`] を書き換える編集ロジック。
//!
//! 触る領域にある [`Node::RefPage`] は、中身を読む必要があるときだけ呼び出し側の `read_page` で読み出す。

use super::node::{Node, half};
use crate::collection::disk::io::PageId;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::{Error, FlexId, Side};
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

/// 領域 `this` の `node` の `targets` に `op` を施す。空になれば [`None`]。
///
/// `targets` はどれも `this` に含まれること。
pub(super) async fn edit(
    mut node: Option<Node>,
    this: &FlexId,
    targets: &[FlexId],
    op: Op,
    read_page: &impl AsyncFn(PageId, FlexId) -> Result<Node, Error>,
) -> Result<Option<Node>, Error> {
    for target in targets {
        node = edit_node(node, this, target, op, read_page).await?;
    }
    Ok(node)
}

/// 領域 `this` の `node` の `target` に `op` を施す。`target` は `this` に含まれること。
async fn edit_node(
    node: Option<Node>,
    this: &FlexId,
    target: &FlexId,
    op: Op,
    read_page: &impl AsyncFn(PageId, FlexId) -> Result<Node, Error>,
) -> Result<Option<Node>, Error> {
    let Some(node) = node else {
        return match op {
            Op::Remove => Ok(None),
            Op::Insert(value) | Op::Upsert(value) => {
                Ok(Some(rebase(Node::Value(value), target, this)?))
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
            let loaded = read_page(page_id, *this).await?;
            return Box::pin(edit_node(Some(loaded), this, target, op, read_page)).await;
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
            let region = path.to_absolute(this)?;
            if region.contains(target) {
                let child =
                    Box::pin(edit_node(Some(*child), &region, target, op, read_page)).await?;
                return child.map(|child| rebase(child, &region, this)).transpose();
            }
            if matches!(op, Op::Remove) && !region.intersects(target) {
                return Ok(Some(Node::Skip { path, child }));
            }
            // 行き先の外にも施すので、行き先が深い次元で割り、行き先のない側を空として扱う
            let dimension = this
                .coarsest_dimension_in(path.deeper_dimensions())
                .expect("Skip の行き先はどれかの次元で深い");
            let side = this.side_toward(dimension, &region);
            let rest = Some(rebase(*child, &region, &half(this, dimension, side)?)?);
            let halves = match side {
                Side::Lower => [rest, None],
                Side::Upper => [None, rest],
            };
            (dimension, halves)
        }
    };

    let regions = [
        half(this, dimension, Side::Lower)?,
        half(this, dimension, Side::Upper)?,
    ];
    let [mut lower, mut upper] = halves;
    if let Some(sub) = target.intersection(&regions[0]) {
        lower = Box::pin(edit_node(lower, &regions[0], &sub, op, read_page)).await?;
    }
    if let Some(sub) = target.intersection(&regions[1]) {
        upper = Box::pin(edit_node(upper, &regions[1], &sub, op, read_page)).await?;
    }
    join(this, dimension, [lower, upper], &regions)
}

/// 領域 `this` を `dimension` で割った `[下, 上]` の Node を1つの Node に合わせる。
///
/// 左右が同じで `dimension` で割っていなければ1つにまとめる。
/// 割っている Node を広い領域へ持ち上げると意味が変わるので、そのときは割ったままにする。
fn join(
    this: &FlexId,
    dimension: Dimension,
    [lower, upper]: [Option<Node>; 2],
    [lower_region, upper_region]: &[FlexId; 2],
) -> Result<Option<Node>, Error> {
    Ok(match (lower, upper) {
        (None, None) => None,
        (Some(lower), None) => Some(rebase(lower, lower_region, this)?),
        (None, Some(upper)) => Some(rebase(upper, upper_region, this)?),
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

/// 領域 `from` の `node` を、領域 `to` から見た Node に張り直す。Skip の先へさらに Skip しないよう統合する。
fn rebase(node: Node, from: &FlexId, to: &FlexId) -> Result<Node, Error> {
    if from == to {
        return Ok(node);
    }
    Ok(match node {
        Node::Skip { path, child } => Node::Skip {
            path: path.to_absolute(from)?.relative_to(to)?,
            child,
        },
        node => Node::Skip {
            path: from.relative_to(to)?,
            child: Box::new(node),
        },
    })
}

#[cfg(test)]
mod tests;
