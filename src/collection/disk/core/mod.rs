mod edit;
mod node;
mod node_page;
use self::edit::Op;
use self::node::{EncodedValue, Node};
use self::node_page::NodePage;
use crate::collection::disk::DiskValue;
use crate::collection::disk::io::{Page, PageId, PagedFile};
use crate::{DiskError, Error, FlexId, SpatialId};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;
use std::path::Path;
use zerocopy::byteorder::{BigEndian, U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};
#[cfg(test)]
mod tests;

/// 空間IDごとに値 `V` を持つ、ファイルに残るマップ。
///
/// 書き換えた Node は、ファイル上のページを上書きせず、ファイルの末尾へすぐ書き足す（追記のみ）。
/// [`commit`](Self::commit) でメタページを書き換えるまでは直前のコミットの状態がファイルに残るので、
/// 途中で落ちても直前のコミットに戻る。参照されなくなったページはファイルにそのまま残る。
#[derive(Debug)]
pub struct FlexTreeCore<V> {
    file: PagedFile,
    /// 最後にコミットした世代。
    generation: u64,
    /// [`FlexId::ROOTS`] の各領域を受け持つルートのページ。値がなければ [`None`]。
    roots: [Option<PageId>; 2],
    /// [`DiskValue::type_name`] のチェックサム。メタページに記録し、開くときに照らし合わせる。
    value_type: u32,
    value: PhantomData<fn() -> V>,
}

impl<V: DiskValue> FlexTreeCore<V> {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    ///
    /// 別の値の型で作ったファイルなら [`DiskError::Incompatible`] を返す。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut file = PagedFile::open(path).await?;
        let slot_count = MetaPage::SLOTS.len() as u64;
        let existing_slots = &MetaPage::SLOTS[..file.len().min(slot_count) as usize];
        let meta = file
            .read_many(existing_slots)
            .await?
            .iter()
            .filter_map(|page| page.try_head_as::<MetaPage>().ok())
            .max_by_key(|meta| meta.generation.get())
            .cloned();
        // Node はメタページより後ろにしか書かないので、そこまで伸びていなければ
        // 新規作成か、初期のメタページを書く途中で落ちたファイル
        let fresh = file.len() <= slot_count;
        file.reserve(slot_count);

        let mut tree = FlexTreeCore {
            file,
            generation: 0,
            roots: [None, None],
            value_type: crc32c::crc32c(V::type_name().as_bytes()),
            value: PhantomData,
        };
        match meta {
            Some(meta) => {
                if meta.value_type.get() != tree.value_type {
                    return Err(DiskError::Incompatible.into());
                }
                tree.generation = meta.generation.get();
                tree.roots = meta.roots();
            }
            // 有効なメタページを先に置かないと、初回コミット前に落ちたファイルを開けなくなる
            None if fresh => tree.write_meta(0).await?,
            None => return Err(DiskError::Corrupt.into()),
        }
        Ok(tree)
    }

    /// `target` の領域に `value` を書き込む。既に値がある場所は上書きする。
    ///
    /// `value` のバイト列が [`MAX_VALUE_LEN`](crate::collection::disk::MAX_VALUE_LEN) を超えれば
    /// [`DiskError::ValueTooLarge`] を返す。
    pub async fn insert(&mut self, target: impl SpatialId, value: V) -> Result<(), Error> {
        let value = EncodedValue::encode(&value)?;
        Ok(self.edit(target, Op::Insert(value)).await?)
    }

    /// `target` の領域のうち、まだ値のない場所にだけ `value` を書き込む。
    ///
    /// `value` のバイト列が [`MAX_VALUE_LEN`](crate::collection::disk::MAX_VALUE_LEN) を超えれば
    /// [`DiskError::ValueTooLarge`] を返す。
    pub async fn upsert(&mut self, target: impl SpatialId, value: V) -> Result<(), Error> {
        let value = EncodedValue::encode(&value)?;
        Ok(self.edit(target, Op::Upsert(value)).await?)
    }

    /// `target` の領域を空にする。
    pub async fn remove(&mut self, target: impl SpatialId) -> Result<(), Error> {
        Ok(self.edit(target, Op::Remove).await?)
    }

    /// `target` と重なる領域と値を、`target` との共通部分に切り取って返す。
    pub async fn get(&self, target: impl SpatialId) -> Result<Vec<(FlexId, V)>, Error> {
        let mut found = Vec::new();
        for ((root, region), targets) in self
            .roots
            .into_iter()
            .zip(FlexId::ROOTS)
            .zip(FlexId::group_by_root(target))
        {
            let Some(page_id) = root else {
                continue;
            };
            if targets.is_empty() {
                continue;
            }
            let node = self.read_page(page_id).await?;
            for target in &targets {
                self.collect(&node, region, target, &mut found).await?;
            }
        }
        Ok(found)
    }

    /// ここまでの変更をファイルに残す。
    pub async fn commit(&mut self) -> Result<(), Error> {
        self.file.sync().await?;
        Ok(self.write_meta(self.generation + 1).await?)
    }

    /// ファイルを閉じる。コミットしていない変更は捨てる。
    pub async fn close(self) -> Result<(), Error> {
        Ok(self.file.close().await?)
    }

    /// 領域 `this` の `node` のうち `target` と重なる領域と値を、`target` との共通部分に切り取って `found` に足す。
    async fn collect(
        &self,
        node: &Node,
        this: FlexId,
        target: &FlexId,
        found: &mut Vec<(FlexId, V)>,
    ) -> Result<(), DiskError> {
        let Some(overlap) = this.intersection(target) else {
            return Ok(());
        };
        match node {
            Node::Value(value) => found.push((overlap, V::decode(value.as_bytes())?)),
            Node::RefPage { page_id, .. } => {
                let child = self.read_page(*page_id).await?;
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
}

impl<V> FlexTreeCore<V> {
    /// 世代 `generation` のメタページに今のルートを書き、永続化する。
    async fn write_meta(&mut self, generation: u64) -> Result<(), DiskError> {
        let slot = MetaPage::SLOTS[(generation % 2) as usize];
        let meta = MetaPage::new(generation, self.roots, self.value_type);
        self.file.write(slot, Page::of(&meta)).await?;
        self.file.sync().await?;
        self.generation = generation;
        Ok(())
    }

    /// `target` の領域に `op` を施す。
    async fn edit(&mut self, target: impl SpatialId, op: Op) -> Result<(), DiskError> {
        let mut roots = self.roots;
        for ((root, region), targets) in roots
            .iter_mut()
            .zip(&FlexId::ROOTS)
            .zip(FlexId::group_by_root(target))
        {
            if targets.is_empty() {
                continue;
            }
            let mut node = match *root {
                Some(page_id) => Some(self.read_page(page_id).await?),
                None => None,
            };
            for target in &targets {
                node = self.edit_node(node, region, target, &op).await?;
            }
            *root = match node {
                Some(node) => Some(self.store(node).await?),
                None => None,
            };
        }
        self.roots = roots;
        Ok(())
    }

    /// `node` をページに収まるよう分けて書き、ルートのページ番号を返す。
    async fn store(&mut self, node: Node) -> Result<PageId, DiskError> {
        let (root_id, node_pages) = NodePage::split(node, || self.file.alloc());
        for (page_id, node_page) in node_pages {
            self.file.write(page_id, Page::of(&*node_page)).await?;
        }
        Ok(root_id)
    }

    /// ページ `page_id` の Node を読み出す。
    async fn read_page(&self, page_id: PageId) -> Result<Node, DiskError> {
        self.file
            .read(page_id)
            .await?
            .try_head_as::<NodePage>()?
            .decode()
    }
}

/// コミットのたびに書く、ルートの位置を残すページ。
///
/// ファイル先頭の 2 ページに世代の偶奇で交互に書く。書いている途中で落ちてもチェックサムが合わなくなるだけで、
/// もう一方に直前のコミットが残る。
#[derive(Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct MetaPage {
    generation: U64<BigEndian>,
    /// [`FlexId::ROOTS`] の各領域のルートのページ番号。0 はメタページの位置でルートには使われないので、ルートがないことを表す。
    roots: [U64<BigEndian>; 2],
    /// 値の型名のチェックサム。
    value_type: U32<BigEndian>,
}

impl MetaPage {
    /// メタページを置く 2 ページ。
    const SLOTS: [PageId; 2] = [PageId(0), PageId(1)];

    fn new(generation: u64, roots: [Option<PageId>; 2], value_type: u32) -> Self {
        MetaPage {
            generation: U64::new(generation),
            roots: roots.map(|root| U64::new(root.map_or(0, |page_id| page_id.0))),
            value_type: U32::new(value_type),
        }
    }

    /// 各領域のルートのページ。ルートがなければ [`None`]。
    fn roots(&self) -> [Option<PageId>; 2] {
        self.roots
            .map(|root| Some(root.get()).filter(|&raw| raw != 0).map(PageId))
    }
}
