//! 挿入。メモリ版の `Node::merge` に insert のルールを当てはめたものを、ページ上の位置をたどる再帰で行う。

use zerocopy::IntoBytes;

use super::SubTree;
use super::layout::BranchPrefix;
use super::read::Node;
use crate::collection::disk::DiskValue;
use crate::collection::disk::core::pages::Pages;
use crate::id::dimension::{Dimension, DimensionSet};
use crate::{Error, FlexId, Side};

impl<V: DiskValue> SubTree<V> {
    /// 領域 `this` の `pos` にある Node へ、`target` だけに `value` がある Node を上書きで重ねる。
    /// `value` はエンコードした Value の Node。
    ///
    /// Ref に行き当たったら、その先のページへ続ける。
    pub(super) fn insert_at(
        &mut self,
        pos: usize,
        this: &FlexId,
        target: &FlexId,
        value: &[u8],
        pages: &mut Pages<V>,
    ) -> Result<(), Error> {
        if target == this {
            let end = self.end(pos)?;
            return self.put(pos..end, this, target, value);
        }
        let node = self.node(pos)?;
        match node {
            Node::Ref(addr) => return self.insert_through(pos, addr, this, target, value, pages),
            Node::Value(_) if self.inner[pos..self.end(pos)?] == *value => return Ok(()),
            // Skip どうしは、行き先が分かれるところまで1段ずつ割らずに一気に降りる
            Node::Skip { path, child } => {
                let region = path.region(this)?;
                let at = Self::common_descent(
                    this,
                    (&region, self.split_dimensions(child)),
                    (target, DimensionSet::EMPTY),
                );
                if at == region {
                    self.insert_at(child, &region, target, value, pages)?;
                    return self.rebase(pos..child, &region, this);
                }
                if at != *this {
                    self.rebase(pos..pos, this, &at)?;
                    self.insert_at(pos, &at, target, value, pages)?;
                    return self.rebase(pos..pos, &at, this);
                }
            }
            _ => {}
        }

        // 結果も粗い次元から割るよう、両方の最初の次元のうち粗い方で割る
        let branch_dimension = match node {
            Node::Branch { dimension, .. } => Some(dimension),
            _ => None,
        };
        let head = match branch_dimension {
            Some(dimension) => dimension.into(),
            None => self.split_dimensions(pos),
        };
        let dimension = this
            .coarsest_dimension_in(head | target.finer_dimensions_than(this))
            .expect("target は this より狭い");
        let halves = [Side::Lower, Side::Upper].map(|side| this.split_on(dimension, side).unwrap());
        let filled = if branch_dimension == Some(dimension) {
            None
        } else {
            self.split(pos, node, this, dimension, &halves, target, value)?
        };
        for (side, half) in [Side::Lower, Side::Upper].into_iter().zip(&halves) {
            if filled == Some(side) {
                continue;
            }
            if let Some(target) = target.intersection(half) {
                self.update_child(pos, side, |tree, child| {
                    tree.insert_at(child, half, &target, value, pages)
                })?;
            }
        }
        self.join(pos)
    }

    /// 領域 `this` の `pos` にある `node` を、`dimension` で割った Branch にする（`join` の逆）。
    ///
    /// Skip を片側へ寄せたときは、空いた側に `target` の Node を書き、その側を返す。
    #[allow(clippy::too_many_arguments)]
    fn split(
        &mut self,
        pos: usize,
        node: Node,
        this: &FlexId,
        dimension: Dimension,
        halves: &[FlexId; 2],
        target: &FlexId,
        value: &[u8],
    ) -> Result<Option<Side>, Error> {
        let moved = match node {
            Node::Skip { path, .. } if path.deeper_dimensions().contains(dimension) => {
                Some(this.side_toward(dimension, &path.region(this)?))
            }
            _ => None,
        };
        let (lower_len, filled) = match moved {
            Some(side) => {
                self.rebase(pos..pos, this, &halves[side as usize])?;
                let end = self.end(pos)?;
                let (empty, at) = match side {
                    Side::Lower => (Side::Upper, end),
                    Side::Upper => (Side::Lower, pos),
                };
                let half = &halves[empty as usize];
                // `common_descent` が `this` で止まったので、`target` は行き先の反対側にも掛かる
                let target = target
                    .intersection(half)
                    .expect("target は Skip の行き先の反対側に掛かる");
                let before = self.len;
                self.put(at..at, half, &target, value)?;
                let lower_len = match side {
                    Side::Lower => end - pos,
                    Side::Upper => self.len - before,
                };
                (lower_len, Some(empty))
            }
            // その次元では中身が変わらないので、下も上も同じ中身になる
            None => {
                let end = self.end(pos)?;
                self.resize(end..end, end - pos)?;
                self.inner.copy_within(pos..end, end);
                (end - pos, None)
            }
        };
        let prefix = BranchPrefix::new(dimension, lower_len);
        self.splice(pos..pos, prefix.as_bytes())?;
        self.refresh(pos)?;
        Ok(filled)
    }

    /// `pos` の Branch の左右がまとめられれば 1 つにする。
    fn join(&mut self, pos: usize) -> Result<(), Error> {
        let Node::Branch {
            dimension,
            children: [lower, upper],
        } = self.node(pos)?
        else {
            unreachable!("join するのは Branch");
        };
        let end = self.end(upper)?;
        // Node は領域との相対位置で意味を持つので、子孫が割る次元で割っている Node を
        // 広い領域へ持ち上げると意味が変わる。左右が同じでも、そのときは割ったままにする。
        let same = !self.split_dimensions(lower).contains(dimension)
            && self.inner[lower..upper] == self.inner[upper..end];
        if same {
            self.resize(upper..end, 0)?;
            return self.resize(pos..lower, 0);
        }
        self.refresh(pos)
    }

    /// 行き先 `region`・その先の Node がそれぞれ `a`・`b` の 2 つの Skip が、領域 `this` から
    /// 途中で割らずに一緒に降りられる一番狭い領域。メモリ版の `common_descent` と同じもの。
    ///
    /// 1段ずつ降りると、各次元をズームの浅い順（同じなら F→X→Y→T）に1段ずつ深くしていく。
    /// 次元ごとに「両方の行き先が同じ側にいられる一番深いズーム」で止まり、そこで降りられなくなる
    /// 次元のうち一番先に順番が来るもので全体が止まる。止まる位置は次元ごとに直接求まるので、
    /// Skip の長さぶん1段ずつ辿らない。
    ///
    /// `split_a`・`split_b` はそれぞれの先の Node が割っている次元の集合。
    fn common_descent(
        this: &FlexId,
        (region_a, split_a): (&FlexId, DimensionSet),
        (region_b, split_b): (&FlexId, DimensionSet),
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
            blocks[d as usize] = za > agree || zb > agree || splits.contains(d);
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
}
