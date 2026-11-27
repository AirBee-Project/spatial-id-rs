mod edit;
mod node;
mod node_page;
use self::edit::Op;
use self::node::Node;
use self::node_page::NodePage;
use crate::collection::disk::io::{Page, PageId, PagedFile};
use crate::{DiskError, Error, FlexId, SpatialId};
use alloc::boxed::Box;
use alloc::vec::Vec;
use std::path::Path;
use zerocopy::byteorder::{BigEndian, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// 空間IDごとに `u64` の値を持つ、ファイルに残るマップ。
///
/// 書き換えた Node は、ファイル上のページを上書きせず、ファイルの末尾へすぐ書き足す（追記のみ）。
/// [`commit`](Self::commit) でメタページを書き換えるまでは直前のコミットの状態がファイルに残るので、
/// 途中で落ちても直前のコミットに戻る。参照されなくなったページはそのまま残り、コンパクションで取り除く。
#[derive(Debug)]
pub struct FlexTreeCore {
    file: PagedFile,
    /// 最後にコミットした世代。
    generation: u64,
    /// [`FlexId::ROOTS`] の各領域を受け持つルートのページ。値がなければ [`None`]。
    roots: [Option<PageId>; 2],
    /// 次に書き足すページ。ファイルの末尾。
    next: PageId,
}

impl FlexTreeCore {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        // PagedFile型として開く
        let file = PagedFile::open(path).await?;

        // 空のファイルにはまだメタページがない
        let meta_pages = match file.len() {
            0 => Vec::new(),
            _ => file.read_many(&MetaPage::SLOTS).await?,
        };

        // 世代が大きいほうのMetaPageを持ってくる
        let meta = meta_pages
            .iter()
            .filter_map(|page| page.try_head_as::<MetaPage>().ok())
            .max_by_key(|meta| meta.generation.get());

        // メタページがないのは、まだ一度もコミットしていない空のファイルだけ
        if meta.is_none() && file.len() != 0 {
            return Err(DiskError::Corrupt.into());
        }

        Ok(FlexTreeCore {
            generation: meta.map_or(0, |m| m.generation.get()),
            roots: [
                meta.and_then(|m| PageId::decode(m.upper_root.get())),
                meta.and_then(|m| PageId::decode(m.lower_root.get())),
            ],
            // 空のファイルでも、先頭の 2 ページはメタページの場所として空けておく
            next: PageId(file.len().max(MetaPage::SLOTS.len() as u64)),
            file,
        })
    }

    /// `target` の領域に `value` を書き込む。既に値がある場所は上書きする。
    pub async fn insert(&mut self, target: impl SpatialId, value: u64) -> Result<(), Error> {
        self.edit(target, Op::Insert(value)).await
    }

    /// `target` の領域のうち、まだ値のない場所にだけ `value` を書き込む。
    pub async fn upsert(&mut self, target: impl SpatialId, value: u64) -> Result<(), Error> {
        self.edit(target, Op::Upsert(value)).await
    }

    /// `target` の領域を空にする。
    pub async fn remove(&mut self, target: impl SpatialId) -> Result<(), Error> {
        self.edit(target, Op::Remove).await
    }

    /// `target` と重なる領域と値を、`target` との共通部分に切り取って返す。
    pub async fn get(&self, target: impl SpatialId) -> Result<Vec<(FlexId, u64)>, Error> {
        // どの FlexId も F の符号でどちらか一方のルートに丸ごと収まるので、振り分けた先で切り取らなくてよい
        let (upper, lower): (Vec<FlexId>, Vec<FlexId>) =
            target.flex_ids().partition(|id| id.root_index() == 0);
        let mut found = Vec::new();
        for ((root_id, region), targets) in self
            .roots
            .into_iter()
            .zip(FlexId::ROOTS)
            .zip([upper, lower])
        {
            let Some(root_id) = root_id.filter(|_| !targets.is_empty()) else {
                continue;
            };
            let root = self.read_page(root_id, &region).await?;
            for target in &targets {
                self.collect(&root, region, target, &mut found).await?;
            }
        }
        Ok(found)
    }

    /// ここまでの変更をファイルに残す。
    pub async fn commit(&mut self) -> Result<(), Error> {
        self.file.sync().await?;
        let generation = self.generation + 1;
        let meta = MetaPage {
            generation: U64::new(generation),
            upper_root: U64::new(PageId::encode(self.roots[0])),
            lower_root: U64::new(PageId::encode(self.roots[1])),
        };
        let slot = MetaPage::SLOTS[(generation % 2) as usize];
        self.file.write(slot, Page::of(&meta)).await?;
        self.file.sync().await?;
        self.generation = generation;
        Ok(())
    }

    /// ファイルを閉じる。コミットしていない変更は捨てる。
    pub async fn close(self) -> Result<(), Error> {
        Ok(self.file.close().await?)
    }

    /// `target` の領域に `op` を施す。
    async fn edit(&mut self, target: impl SpatialId, op: Op) -> Result<(), Error> {
        // どの FlexId も F の符号でどちらか一方のルートに丸ごと収まるので、振り分けた先で切り取らなくてよい
        let (upper, lower): (Vec<FlexId>, Vec<FlexId>) =
            target.flex_ids().partition(|id| id.root_index() == 0);
        for (i, (region, targets)) in FlexId::ROOTS.iter().zip([upper, lower]).enumerate() {
            if targets.is_empty() {
                continue;
            }
            let node = match self.roots[i] {
                Some(page_id) => Some(self.read_page(page_id, region).await?),
                None => None,
            };
            let edited = edit::edit(node, region, &targets, op, &async |page_id, this| {
                self.read_page(page_id, &this).await
            })
            .await?;
            self.roots[i] = match edited {
                Some(node) => Some(self.store(node).await?),
                None => None,
            };
        }
        Ok(())
    }

    /// `node` をページに収まるよう分けて書き、ルートのページ番号を返す。
    async fn store(&mut self, node: Node) -> Result<PageId, Error> {
        let (root_id, node_pages) = NodePage::split(node, || self.alloc_page());
        for (page_id, node_page) in node_pages {
            self.file.write(page_id, Page::of(&*node_page)).await?;
        }
        Ok(root_id)
    }

    /// 領域 `this` のページ `page_id` の Node を読み出す。
    async fn read_page(&self, page_id: PageId, this: &FlexId) -> Result<Node, Error> {
        self.file
            .read(page_id)
            .await?
            .try_head_as::<NodePage>()?
            .decode(this)
    }

    /// 領域 `this` の `node` のうち `target` と重なる領域と値を、`target` との共通部分に切り取って `found` に足す。
    async fn collect(
        &self,
        node: &Node,
        this: FlexId,
        target: &FlexId,
        found: &mut Vec<(FlexId, u64)>,
    ) -> Result<(), Error> {
        let Some(overlap) = this.intersection(target) else {
            return Ok(());
        };
        match node {
            Node::Value(value) => found.push((overlap, *value)),
            Node::RefPage { page_id, .. } => {
                let child = self.read_page(*page_id, &this).await?;
                Box::pin(self.collect(&child, this, target, found)).await?;
            }
            Node::Branch { .. } | Node::Skip { .. } => {
                for (child, region) in node.children(&this)? {
                    Box::pin(self.collect(child, region, target, found)).await?;
                }
            }
        }
        Ok(())
    }

    /// ファイルの末尾に、書き足すページを割り当てる。
    fn alloc_page(&mut self) -> PageId {
        let page_id = self.next;
        self.next.0 += 1;
        page_id
    }
}

/// コミットのたびに書く、ルートの位置を残すページ。
///
/// ファイル先頭の 2 ページに世代の偶奇で交互に書く。書いている途中で落ちてもチェックサムが合わなくなるだけで、
/// もう一方に直前のコミットが残る。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct MetaPage {
    generation: U64<BigEndian>,
    /// 0 ならルートがない。0 はメタページの位置なので、ルートには使われない。
    upper_root: U64<BigEndian>,
    lower_root: U64<BigEndian>,
}

impl MetaPage {
    /// メタページを置く 2 ページ。
    const SLOTS: [PageId; 2] = [PageId(0), PageId(1)];
}

#[cfg(test)]
mod tests;
