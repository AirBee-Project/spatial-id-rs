//! コミットと GC。
//!
//! どちらも「ルートからたどれるページを、子から順に新しい位置へ書く」同じ処理で、違いはたどる範囲だけ。
//! コミットはまだ書いていないページだけを書き、GC はファイルにあるページも含めてすべて写す。

use alloc::vec::Vec;
use std::collections::{HashMap, HashSet};

use super::FlexTreeCore;
use super::address::PageAddr;
use super::meta::{Meta, WriteCounts};
use crate::Error;
use crate::collection::disk::DiskValue;

impl<V: DiskValue> FlexTreeCore<V> {
    /// ここまでの変更をファイルに残す。書いた量がたまっていれば、あわせて GC する。
    ///
    /// 失敗したときは、ファイルは直前にコミットした状態のまま残る。続けて使わずに開き直すこと。
    pub async fn commit(&mut self) -> Result<(), Error> {
        let gc = self.counts.gc_due(self.space.size());
        if gc {
            self.space.restart();
        }
        let written = self.write_reachable(gc).await?;
        // メタページより先に、メタページが指すページを永続化しておく
        self.file.sync().await?;

        self.counts = if gc {
            WriteCounts {
                live: written,
                since_gc: 0,
            }
        } else {
            WriteCounts {
                since_gc: self.counts.since_gc + written,
                ..self.counts
            }
        };
        self.generation += 1;
        self.write_meta().await?;
        if gc {
            self.space.finish();
        }
        Ok(())
    }

    /// ルートからたどれるページのうち、`all` ならすべてを、そうでなければまだ書いていないものを、
    /// 子から順に新しい位置へ書き、書いたページ数を返す。
    ///
    /// 子を書いて位置が決まってから、親の Ref をその位置に直して親を書く。
    async fn write_reachable(&mut self, all: bool) -> Result<u64, Error> {
        let order = self.children_first(all).await?;
        let mut moved: HashMap<PageAddr, u64> = HashMap::with_capacity(order.len());
        for page in order {
            let mut tree = self.pages.get(page).clone();
            tree.patch_refs(|addr| match moved.get(&addr.page()) {
                Some(&index) => addr.moved_to(index),
                None => addr,
            })?;
            let index = self.space.allocate();
            self.file.write(index, (&tree).into()).await?;
            self.pages.insert_stored(index, tree);
            moved.insert(page, index);
        }
        for root in self.roots.iter_mut() {
            if let Some(&index) = moved.get(&root.page()) {
                *root = root.moved_to(index);
            }
        }

        self.pages.clear_pending();
        if all {
            let written: HashSet<u64> = moved.values().copied().collect();
            self.pages.retain_stored(|index| written.contains(&index));
        }
        Ok(moved.len() as u64)
    }

    /// 書くページを、子が親より先に来る順で返す。`all` でなければ、ファイルにあるページはたどらない。
    ///
    /// ファイルにあるページが指すのはファイルにあるページだけなので、まだ書いていないページの中だけをたどれば足りる。
    async fn children_first(&mut self, all: bool) -> Result<Vec<PageAddr>, Error> {
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        let mut stack: Vec<(PageAddr, bool)> =
            self.roots.iter().map(|root| (root.page(), false)).collect();
        while let Some((page, children_done)) = stack.pop() {
            if children_done {
                order.push(page);
                continue;
            }
            let needs_write = all || matches!(page, PageAddr::Pending(_));
            if !needs_write || !seen.insert(page) {
                continue;
            }
            stack.push((page, true));
            let refs = self.load(page).await?.refs()?;
            stack.extend(refs.into_iter().map(|addr| (addr.page(), false)));
        }
        Ok(order)
    }

    pub(super) async fn write_meta(&mut self) -> Result<(), Error> {
        let meta = Meta::new(self.generation, &self.space, self.counts, &self.roots);
        self.file.write(meta.slot(), (&meta).into()).await?;
        Ok(self.file.sync().await?)
    }
}
