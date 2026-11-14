//! ディスク上の空間インデックス。
//!
//! 木は 1 ページに収まる部分木（[SubTree]）に分けて置き、部分木どうしは Ref でつなぐ。
//! ファイルに書いたページは書き換えず、変更は新しいページとして書く（コピーオンライト）。
//! 使われなくなったページは、コミットのときに自動で GC する。

mod address;
mod commit;
mod meta;
mod pages;
mod space;
mod subtree;
#[cfg(test)]
mod tests;

use core::fmt;
use std::path::Path;

use self::address::{NodeAddr, PageAddr};
use self::meta::{Hemisphere, Meta, Roots, WriteCounts};
use self::pages::Pages;
use self::space::{SegmentSize, Space};
use self::subtree::SubTree;
use crate::collection::disk::DiskValue;
use crate::collection::disk::io::{Page, PagedFile};
use crate::{Error, FlexId};

/// ディスク上の空間インデックス。呼び出し側の非同期ランタイムは問わない。
///
/// 変更は [`commit`](Self::commit) するまでファイルに残らない。
pub struct FlexTreeCore<V> {
    file: PagedFile,
    space: Space,
    pages: Pages<V>,
    roots: Roots,
    /// 最後にコミットした世代。
    generation: u64,
    counts: WriteCounts,
}

impl<V: DiskValue> FlexTreeCore<V> {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with(path, SegmentSize::DEFAULT).await
    }

    /// ファイルを開く。新しく作るときは、セグメントを `size` にする。
    async fn open_with(path: impl AsRef<Path>, size: SegmentSize) -> Result<Self, Error> {
        let file = PagedFile::open(path).await?;
        if file.len() == 0 {
            let mut tree = FlexTreeCore {
                space: Space::new(size, file.len(), []),
                file,
                pages: Pages::new(),
                roots: Roots::default(),
                generation: 0,
                counts: WriteCounts::default(),
            };
            tree.write_meta().await?;
            return Ok(tree);
        }
        let meta = Meta::read_latest(&file).await?;
        Ok(FlexTreeCore {
            space: Space::new(meta.segment_size()?, file.len(), meta.free_runs()),
            file,
            pages: Pages::new(),
            roots: meta.roots(),
            generation: meta.generation(),
            counts: meta.counts(),
        })
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// `target` の領域に `value` を書き込む。既に値がある場所は上書きされる。
    pub async fn insert(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: V,
    ) -> Result<(), Error> {
        let mut buf = [0; Page::CAPACITY];
        let value = SubTree::encode(&value, &mut buf)?;
        for target in target {
            let hemisphere = Hemisphere::of(&target);
            let region = hemisphere.region();
            let tree = match self.roots[hemisphere] {
                None => SubTree::single(&region, &target, value)?,
                Some(root) => {
                    // 書き換えはページを読みながらだと進められないので、触れるページを先に読んでおく
                    self.load_toward(root, region, &target).await?;
                    let mut tree = self.pages.copy_node(root)?;
                    tree.insert(&region, &target, value, &mut self.pages)?;
                    tree
                }
            };
            self.roots[hemisphere] = Some(self.pages.add(tree));
        }
        Ok(())
    }

    /// ファイルを閉じる。コミットしていない変更は捨てる。閉じる際のエラーが要らなければ drop するだけでよい。
    pub async fn close(self) -> Result<(), Error> {
        Ok(self.file.close().await?)
    }

    /// `page` を読み込む。読み込み済みなら何もしない。
    async fn load(&mut self, page: PageAddr) -> Result<&SubTree<V>, Error> {
        if let PageAddr::Stored(index) = page
            && !self.pages.contains(page)
        {
            let tree = SubTree::try_from(&*self.file.read(index).await?)?;
            self.pages.insert_stored(index, tree);
        }
        Ok(self.pages.get(page))
    }

    /// 領域 `region` の `root` から、`target` と重なる領域にあるページをすべて読み込む。
    async fn load_toward(
        &mut self,
        root: NodeAddr,
        region: FlexId,
        target: &FlexId,
    ) -> Result<(), Error> {
        let mut pending = alloc::vec![(root, region)];
        while let Some((addr, region)) = pending.pop() {
            self.load(addr.page()).await?.refs_toward(
                addr.offset(),
                &region,
                target,
                &mut pending,
            )?;
        }
        Ok(())
    }
}

impl<V> fmt::Debug for FlexTreeCore<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlexTreeCore")
            .field("path", &self.file.path())
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}
