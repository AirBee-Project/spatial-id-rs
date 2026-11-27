//! [`Node`] を書き換える編集ロジック。
//!
//! 触る領域にある [`Node::RefPage`] は、中身を読む必要があるときだけ読み出す。

use super::FlexTreeCore;
use super::node::{EncodedValue, Node};
use super::node_page::NodePage;
use crate::collection::disk::io::PageId;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::{DiskError, FlexId};
use alloc::boxed::Box;
use std::collections::HashMap;

/// 領域への操作。意味は [`SpatialIdMap`](crate::SpatialIdMap) の同名の操作と同じ。
#[derive(Debug, Clone)]
pub(super) enum Op {
    /// 値を書き込む。既に値がある場所は上書きする。
    Insert(EncodedValue),
    /// まだ値のない場所にだけ値を書き込む。
    Upsert(EncodedValue),
    /// 空にする。
    Remove,
}

impl<V> FlexTreeCore<V> {
    /// 領域 `this` の `node` の `target` に `op` を施し、`(結果の Node, 変更があったか)` を返す。
    /// 空になれば [`None`]。`target` は `this` に含まれること。
    ///
    /// `pages` は 1 回の編集の中で読んだページ。同じページを何度も読まないよう使い回す。
    pub(super) async fn edit_node(
        &mut self,
        node: Option<Node>,
        this: &FlexId,
        target: &FlexId,
        op: &Op,
        pages: &mut HashMap<PageId, Node>,
    ) -> Result<(Option<Node>, bool), DiskError> {
        let Some(node) = node else {
            return match op {
                Op::Remove => Ok((None, false)),
                Op::Insert(value) | Op::Upsert(value) => {
                    Ok((Some(Node::Value(value.clone()).rebase(target, this)?), true))
                }
            };
        };

        if target == this {
            match op {
                Op::Insert(value) => {
                    // 中身が Value だけのページを指す RefPage もあるので、Value かどうかではなく中身で比べる。
                    // 中身が同じなのに変更ありとすると、片側だけ編集した両側を比べずに割ったままにしてしまう
                    let value = Node::Value(value.clone());
                    if self.same_content(&node, &value, pages).await? {
                        return Ok((Some(node), false));
                    }
                    return Ok((Some(value), true));
                }
                Op::Remove => return Ok((None, true)),
                Op::Upsert(_) => {}
            }
        }

        let node = match node {
            Node::Value(old) if matches!(op, Op::Upsert(_)) => {
                return Ok((Some(Node::Value(old)), false));
            }
            Node::Value(old) if matches!(op, Op::Insert(value) if *value == old) => {
                return Ok((Some(Node::Value(old)), false));
            }
            Node::Skip { path, child } => {
                let region = path.to_absolute(this).map_err(|_| DiskError::Corrupt)?;
                if matches!(op, Op::Remove) && !region.intersects(target) {
                    return Ok((Some(Node::Skip { path, child }), false));
                }
                let at = this.common_descent(
                    (&region, child.split_dimensions()),
                    (target, DimensionSet::EMPTY),
                );
                if at != *this {
                    let child = (*child).rebase(&region, &at)?;
                    let (edited, changed) =
                        Box::pin(self.edit_node(Some(child), &at, target, op, pages)).await?;
                    return Ok((edited.map(|n| n.rebase(&at, this)).transpose()?, changed));
                }
                Node::Skip { path, child }
            }
            node => node,
        };

        let splits = node.split_dimensions();
        let dimension = this
            .coarsest_dimension_in(splits | target.finer_dimensions_than(this))
            .ok_or(DiskError::Corrupt)?;
        if !splits.contains(dimension) {
            return self
                .edit_one_side(node, this, dimension, target, op, pages)
                .await;
        }

        // `dimension` を割っている Node は、`dimension` より粗い次元を子孫が割る非正規形の木でも
        // 子の座標がずれないよう、自身が割る次元で降りる。
        let (dimension, halves) = match node {
            Node::Branch {
                dimension,
                lower,
                upper,
                ..
            } => (dimension, [Some(*lower), Some(*upper)]),
            Node::Skip { path, child } => {
                let region = path.to_absolute(this).map_err(|_| DiskError::Corrupt)?;
                let dimension = this
                    .coarsest_dimension_in(path.deeper_dimensions())
                    .ok_or(DiskError::Corrupt)?;
                let side = this.side_toward(dimension, &region) as usize;
                let half = this.halves(dimension).ok_or(DiskError::Corrupt)?[side];
                let mut halves = [None, None];
                halves[side] = Some((*child).rebase(&region, &half)?);
                (dimension, halves)
            }
            Node::RefPage { page_id, .. } => {
                let loaded = self.load(page_id, pages).await?;
                let (edited, changed) =
                    Box::pin(self.edit_node(Some(loaded), this, target, op, pages)).await?;
                return Ok(if changed {
                    (edited, true)
                } else {
                    (Some(node), false)
                });
            }
            Node::Value(_) => unreachable!("Value は何も割らないので片側だけの編集になる"),
        };

        let regions = this.halves(dimension).ok_or(DiskError::Corrupt)?;
        let [mut lower, mut upper] = halves;
        let mut changed = false;
        for (half, region) in [&mut lower, &mut upper].into_iter().zip(&regions) {
            if let Some(sub) = target.intersection(region) {
                let (edited, did_change) =
                    Box::pin(self.edit_node(half.take(), region, &sub, op, pages)).await?;
                *half = edited;
                changed |= did_change;
            }
        }
        if changed
            && let (Some(l), Some(u)) = (&lower, &upper)
            && !l.split_dimensions().contains(dimension)
            && self.same_content(l, u, pages).await?
        {
            // ページへ書き出すバイトが少ないほう（RefPage として既にファイルにあるほう）を残す
            return Ok((if u.len() < l.len() { upper } else { lower }, true));
        }
        Ok((
            Node::join(this, dimension, [lower, upper], &regions)?,
            changed,
        ))
    }

    /// `dimension` を割っていない `node` を `dimension` で割り、`target` のある側だけを編集する。
    ///
    /// 反対側には `node` をそのまま置く。変更があったときの両側は必ず異なるので、中身は比べない。
    async fn edit_one_side(
        &mut self,
        node: Node,
        this: &FlexId,
        dimension: Dimension,
        target: &FlexId,
        op: &Op,
        pages: &mut HashMap<PageId, Node>,
    ) -> Result<(Option<Node>, bool), DiskError> {
        let regions = this.halves(dimension).ok_or(DiskError::Corrupt)?;
        let side = this.side_toward(dimension, target) as usize;
        let (edited, changed) =
            Box::pin(self.edit_node(Some(node.clone()), &regions[side], target, op, pages)).await?;
        if !changed {
            return Ok((Some(node), false));
        }
        let mut halves = [None, None];
        halves[side] = edited;
        halves[1 - side] = Some(self.share(node).await?);
        Ok((Node::join(this, dimension, halves, &regions)?, true))
    }

    /// 編集の前後で両側に置く `node`。1 ページに 2 つ収まらない大きさなら、両側で同じ中身を
    /// 2 度書き出さないよう、先に 1 度だけページへ書いて [`Node::RefPage`] にする。
    ///
    /// Skip は行き先だけをページへ書く。RefPage の先が Skip だと、外側の Skip と1つにまとめられず、
    /// 同じ中身が別の形になって [`same_content`](Self::same_content) で比べられなくなる。
    async fn share(&mut self, node: Node) -> Result<Node, DiskError> {
        if 2 * node.len() <= NodePage::BODY_LEN {
            return Ok(node);
        }
        let (path, node) = match node {
            Node::Skip { path, child } => (Some(path), *child),
            node => (None, node),
        };
        let split_dimensions = node.split_dimensions();
        let ref_page = Node::RefPage {
            page_id: self.store(node).await?,
            split_dimensions,
        };
        Ok(match path {
            Some(path) => Node::Skip {
                path,
                child: Box::new(ref_page),
            },
            None => ref_page,
        })
    }

    /// `a` と `b` が、[`Node::RefPage`] の先まで含めて同じ中身か。ページは違いが見つかるまでの分だけ読む。
    async fn same_content(
        &self,
        a: &Node,
        b: &Node,
        pages: &mut HashMap<PageId, Node>,
    ) -> Result<bool, DiskError> {
        if a.split_dimensions() != b.split_dimensions() {
            return Ok(false);
        }
        Ok(match (a, b) {
            (Node::RefPage { page_id: p, .. }, Node::RefPage { page_id: q, .. }) if p == q => true,
            (Node::RefPage { page_id, .. }, other) | (other, Node::RefPage { page_id, .. }) => {
                let loaded = self.load(*page_id, pages).await?;
                Box::pin(self.same_content(&loaded, other, pages)).await?
            }
            (Node::Value(a), Node::Value(b)) => a == b,
            (
                Node::Branch {
                    dimension: da,
                    lower: la,
                    upper: ua,
                    ..
                },
                Node::Branch {
                    dimension: db,
                    lower: lb,
                    upper: ub,
                    ..
                },
            ) => {
                da == db
                    && Box::pin(self.same_content(la, lb, pages)).await?
                    && Box::pin(self.same_content(ua, ub, pages)).await?
            }
            (
                Node::Skip {
                    path: pa,
                    child: ca,
                },
                Node::Skip {
                    path: pb,
                    child: cb,
                },
            ) => pa == pb && Box::pin(self.same_content(ca, cb, pages)).await?,
            _ => false,
        })
    }

    /// ページ `page_id` の Node。`pages` に読んであればファイルを読まない。
    async fn load(
        &self,
        page_id: PageId,
        pages: &mut HashMap<PageId, Node>,
    ) -> Result<Node, DiskError> {
        if let Some(node) = pages.get(&page_id) {
            return Ok(node.clone());
        }
        let node = self.read_page(page_id).await?;
        pages.insert(page_id, node.clone());
        Ok(node)
    }
}

impl Node {
    /// 領域 `this` を `dimension` で割った `[下, 上]` の Node を1つの Node に合わせる。片側だけなら Skip にする。
    pub(super) fn join(
        this: &FlexId,
        dimension: Dimension,
        [lower, upper]: [Option<Node>; 2],
        [lower_region, upper_region]: &[FlexId; 2],
    ) -> Result<Option<Node>, DiskError> {
        Ok(match (lower, upper) {
            (None, None) => None,
            (Some(lower), None) => Some(lower.rebase(lower_region, this)?),
            (None, Some(upper)) => Some(upper.rebase(upper_region, this)?),
            (Some(lower), Some(upper)) => Some(Node::Branch {
                dimension,
                split_dimensions: DimensionSet::from(dimension)
                    | lower.split_dimensions()
                    | upper.split_dimensions(),
                lower: Box::new(lower),
                upper: Box::new(upper),
            }),
        })
    }

    /// 領域 `from` の自身を、`from` を含む領域 `to` から見た Node に張り直す。Skip の先へさらに Skip しないよう統合する。
    pub(super) fn rebase(self, from: &FlexId, to: &FlexId) -> Result<Node, DiskError> {
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
