//! メモリ上に置いたページ。

use alloc::boxed::Box;
use std::collections::HashMap;

use super::address::{NodeAddr, PageAddr};
use super::subtree::SubTree;
use crate::Error;
use crate::collection::disk::DiskValue;

/// ファイルから読んだページと、まだ書いていないページ。
///
/// ファイルにあるページは書き換えない。同じページを複数の Ref が指していることがあるので、
/// 書き換えるときは写して、まだ書いていない新しいページにする。
pub(super) struct Pages<V> {
    stored: HashMap<u64, Box<SubTree<V>>>,
    pending: HashMap<u64, Box<SubTree<V>>>,
    next_pending: u64,
}

impl<V: DiskValue> Pages<V> {
    pub(super) fn new() -> Self {
        Pages {
            stored: HashMap::new(),
            pending: HashMap::new(),
            next_pending: 0,
        }
    }

    pub(super) fn contains(&self, page: PageAddr) -> bool {
        match page {
            PageAddr::Stored(index) => self.stored.contains_key(&index),
            PageAddr::Pending(id) => self.pending.contains_key(&id),
        }
    }

    /// 読み込み済みか、まだ書いていないページ。
    pub(super) fn get(&self, page: PageAddr) -> &SubTree<V> {
        let tree = match page {
            PageAddr::Stored(index) => self.stored.get(&index),
            PageAddr::Pending(id) => self.pending.get(&id),
        };
        tree.expect("使うページは先に読み込んでおく")
    }

    /// ファイルの `index` にあるページとして置く。
    pub(super) fn insert_stored(&mut self, index: u64, tree: SubTree<V>) {
        self.stored.insert(index, Box::new(tree));
    }

    /// `tree` をまだ書いていないページとして加え、そのルートを指す。
    pub(super) fn add(&mut self, tree: SubTree<V>) -> NodeAddr {
        let id = self.next_pending;
        self.next_pending += 1;
        self.pending.insert(id, Box::new(tree));
        NodeAddr::new(PageAddr::Pending(id), SubTree::<V>::ROOT)
    }

    /// `addr` の Node をルートとして写した部分木。
    pub(super) fn copy_node(&self, addr: NodeAddr) -> Result<SubTree<V>, Error> {
        SubTree::copy_of(self.get(addr.page()), addr.offset())
    }

    /// まだ書いていないページを捨てる。書いたものは [`insert_stored`](Self::insert_stored) で置き直しておくこと。
    pub(super) fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// ファイルにあるページのうち、`keep` 以外を忘れる。GC で空けたセグメントには別のページを書くため。
    pub(super) fn retain_stored(&mut self, keep: impl Fn(u64) -> bool) {
        self.stored.retain(|&index, _| keep(index));
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.stored.len() + self.pending.len()
    }
}
