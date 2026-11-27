//! 使われていないページを詰め、生きているページを木の行きがけ順に並べ直す。

use super::node::Node;
use super::node_page::NodePage;
use super::{FlexTreeCore, MetaPage};
use crate::collection::disk::io::{Page, PageId};
use crate::{DiskError, Error};
use alloc::vec::Vec;
use std::collections::{HashMap, HashSet};

impl<V> FlexTreeCore<V> {
    /// ここまでの変更をコミットし、ファイルを生きているページだけに詰める。
    ///
    /// ページはルートから行きがけ順（lower 側を先）に並ぶ。途中で落ちても、直前のコミットか、
    /// compact を呼んだ時点の内容で開ける。生きているページをすべてメモリに読み、2回書き直すので、
    /// 書き込みを重ねた後にまとめて呼ぶ。
    pub async fn compact(&mut self) -> Result<(), Error> {
        let roots = self.roots;
        let pages = self.live_pages_in_preorder().await?;
        // 先頭側は生きているページと重なるので、いったん末尾へ写し、両方のメタページを写しに向けてから書き戻す。
        // 両方を向けておかないと、書き戻しが1つ前の世代のページを壊し、落ちたときの戻り先がなくなる
        self.roots = self.relocate(roots, &pages, self.file.len()).await?;
        self.persist_to_both_slots().await?;
        let slot_count = MetaPage::SLOTS.len() as u64;
        self.roots = self.relocate(roots, &pages, slot_count).await?;
        self.persist_to_both_slots().await?;
        self.file.truncate(slot_count + pages.len() as u64).await?;
        Ok(())
    }

    /// ルートから辿れるページと、その中の Node を、行きがけ順（lower 側を先）に並べたもの。
    ///
    /// 同じ深さのページはまとめて読む。複数箇所から共有されるページは最初の出現位置に 1 度だけ並べ、
    /// 循環参照は [`DiskError::Corrupt`] で弾く。
    pub(super) async fn live_pages_in_preorder(&self) -> Result<Vec<(PageId, Node)>, DiskError> {
        let mut nodes = HashMap::new();
        let mut queued = HashSet::new();
        let mut frontier: Vec<PageId> = Vec::new();
        for page_id in self.roots.iter().flatten().copied() {
            if queued.insert(page_id.0) {
                frontier.push(page_id);
            }
        }
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for (page_id, page) in frontier.iter().zip(self.file.read_many(&frontier).await?) {
                let mut node = page.try_head_as::<NodePage>()?.decode()?;
                node.for_each_ref_page(&mut |child| {
                    if queued.insert(child.0) {
                        next.push(*child);
                    }
                });
                nodes.insert(page_id.0, node);
            }
            frontier = next;
        }

        enum Visit {
            Active,
            Done,
        }
        enum Frame {
            Enter(PageId),
            Exit(PageId),
        }

        let mut pages = Vec::with_capacity(nodes.len());
        let mut state: HashMap<u64, Visit> = HashMap::with_capacity(nodes.len());
        let mut stack: Vec<Frame> = self
            .roots
            .iter()
            .rev()
            .flatten()
            .copied()
            .map(Frame::Enter)
            .collect();
        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Exit(page_id) => {
                    state.insert(page_id.0, Visit::Done);
                }
                Frame::Enter(page_id) => match state.get(&page_id.0) {
                    Some(Visit::Active) => return Err(DiskError::Corrupt),
                    Some(Visit::Done) => {}
                    None => {
                        state.insert(page_id.0, Visit::Active);
                        stack.push(Frame::Exit(page_id));
                        let mut node = nodes
                            .remove(&page_id.0)
                            .expect("ルートから辿れるページはすべて読んである");
                        let children_start = stack.len();
                        node.for_each_ref_page(&mut |child| stack.push(Frame::Enter(*child)));
                        stack[children_start..].reverse();
                        pages.push((page_id, node));
                    }
                },
            }
        }
        Ok(pages)
    }

    /// ルートが `roots` の `pages` を、ページ番号 `base` から順に書き直し、書き直した木のルートを返す。
    /// メタページは書き換えない。
    ///
    /// `roots` と `pages` のページ番号は、[`live_pages_in_preorder`](Self::live_pages_in_preorder) で読んだときのもの。
    pub(super) async fn relocate(
        &mut self,
        roots: [Option<PageId>; 2],
        pages: &[(PageId, Node)],
        base: u64,
    ) -> Result<[Option<PageId>; 2], DiskError> {
        let new_ids: HashMap<u64, u64> = pages
            .iter()
            .zip(base..)
            .map(|((old, _), new)| (old.0, new))
            .collect();
        let relocated = |page_id: PageId| PageId(new_ids[&page_id.0]);
        self.file.reserve(base + pages.len() as u64);
        for ((_, node), new_id) in pages.iter().zip(base..) {
            let mut node = node.clone();
            node.for_each_ref_page(&mut |child| *child = relocated(*child));
            let node_page = NodePage::encode(&node);
            self.file
                .write(PageId(new_id), Page::of(&*node_page))
                .await?;
        }
        Ok(roots.map(|root| root.map(relocated)))
    }

    /// 書いたページを永続化し、今のルートを両方のメタページに書く。
    pub(super) async fn persist_to_both_slots(&mut self) -> Result<(), DiskError> {
        self.file.sync().await?;
        for _ in MetaPage::SLOTS {
            self.write_meta(self.generation + 1).await?;
        }
        Ok(())
    }
}

impl Node {
    /// 自身と子孫の RefPage が指すページ番号を、行きがけ順（lower 側を先）に `f` へ渡す。
    fn for_each_ref_page(&mut self, f: &mut impl FnMut(&mut PageId)) {
        match self {
            Node::RefPage { page_id, .. } => f(page_id),
            Node::Branch { lower, upper, .. } => {
                lower.for_each_ref_page(f);
                upper.for_each_ref_page(f);
            }
            Node::Skip { child, .. } => child.for_each_ref_page(f),
            Node::Value(_) => {}
        }
    }
}
