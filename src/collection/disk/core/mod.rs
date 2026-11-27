mod compact;
mod edit;
mod iter;
mod node;
mod node_page;
use self::edit::Op;
use self::iter::{Leaves, OverlapTargets, Step};
use self::node::{EncodedValue, Node};
use self::node_page::NodePage;
use crate::collection::disk::DiskValue;
use crate::collection::disk::io::{Page, PageId, PagedFile};
use crate::collection::{mem, time_reconstruct};
use crate::id::dimension::Dimension;
use crate::{
    AllowedIntervals, CancellationToken, DiskError, Error, FlexId, RangeId, SingleId, ZoomLevel,
};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;
pub(crate) use iter::OverlappingIter;
pub use iter::{IntoIter, Iter};
use std::collections::HashMap;
use std::path::Path;
use zerocopy::byteorder::{BigEndian, U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};
#[cfg(test)]
mod tests;

/// 空間IDごとに値 `V` を持つ、ファイルに残るマップ。
///
/// 書き換えた Node は、ファイル上のページを上書きせず、ファイルの末尾へすぐ書き足す。
/// 書き換えていない部分木のページは、複数の親から共有されることがある。
/// [`commit`](Self::commit) でメタページを書き換えるまでは直前のコミットの状態がファイルに残るので、
/// 途中で落ちても直前のコミットに戻る。参照されなくなったページはファイルにそのまま残り、
/// [`compact`](Self::compact) を呼んだときにだけ、生きているページを先頭へ詰め直してファイルを切り詰める。
#[derive(Debug)]
pub(crate) struct FlexTreeCore<V> {
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
        Self::from_file(PagedFile::open(path).await?).await
    }

    /// `tree` と同じ中身のファイルを `path` に作り、コミットして返す。
    ///
    /// 既存のファイルは、値の型に関わらず中身を捨てて作り直す。
    pub(crate) async fn create_from_mem(
        path: impl AsRef<Path>,
        tree: &mem::core::FlexTreeCore<V>,
    ) -> Result<Self, Error> {
        let mut file = PagedFile::open(path).await?;
        file.truncate(0).await?;
        let mut disk = Self::from_file(file).await?;
        let mut roots = [None, None];
        for (root, mem_root) in roots.iter_mut().zip([&tree.upper_root, &tree.lower_root]) {
            if let Some(node) = mem_root {
                *root = Some(disk.store(Node::from_mem(node)?).await?);
            }
        }
        disk.roots = roots;
        disk.commit().await?;
        Ok(disk)
    }

    /// 開いた `file` のメタページを読む。空のファイルなら初期のメタページを書く。
    async fn from_file(mut file: PagedFile) -> Result<Self, Error> {
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
    pub async fn insert(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: V,
    ) -> Result<(), Error> {
        let value = EncodedValue::encode(&value)?;
        Ok(self.edit(target, Op::Insert(value)).await?)
    }

    /// `target` の領域のうち、まだ値のない場所にだけ `value` を書き込む。
    ///
    /// `value` のバイト列が [`MAX_VALUE_LEN`](crate::collection::disk::MAX_VALUE_LEN) を超えれば
    /// [`DiskError::ValueTooLarge`] を返す。
    pub async fn upsert(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: V,
    ) -> Result<(), Error> {
        let value = EncodedValue::encode(&value)?;
        Ok(self.edit(target, Op::Upsert(value)).await?)
    }

    /// `target` の領域を空にする。
    pub async fn remove(&mut self, target: impl IntoIterator<Item = FlexId>) -> Result<(), Error> {
        Ok(self.edit(target, Op::Remove).await?)
    }

    /// `target` と重なる領域と値を、`target` との共通部分に切り取って返す。
    pub async fn get(
        &self,
        target: impl IntoIterator<Item = FlexId>,
    ) -> Result<Vec<(FlexId, V)>, Error> {
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

    /// `target` と重なる領域と値を、切り取らずにそのまま返す。各領域は1回だけ返す。
    pub async fn get_overlapping(
        &self,
        target: impl IntoIterator<Item = impl Into<RangeId>>,
    ) -> Result<Vec<(FlexId, V)>, Error> {
        let targets = OverlapTargets::new(target);
        let mut found = Vec::new();
        self.visit_leaves(
            |region| targets.matches(region),
            |id, value| {
                found.push((id, V::decode(value.as_bytes())?));
                Ok(())
            },
        )
        .await?;
        Ok(found)
    }

    /// 値を持つ領域と値を、ページ単位で同期的に読み出しながら返すイテレーター。
    pub fn iter(&self) -> Iter<'_, V> {
        Iter::new(self)
    }

    /// `targets` と重なる領域と値を、ページ単位で同期的に読み出しながら返すイテレーター。
    pub(crate) fn overlapping_sync<'a>(
        &'a self,
        targets: &[RangeId],
        token: &CancellationToken,
    ) -> OverlappingIter<'a, V> {
        OverlappingIter::new(self, targets, token)
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

    /// 時間方向に隣接する同値の [`FlexId`] を結合した [`RangeId`] と値を返す。**ズームレベルは変えない**。
    pub async fn reconstructed_time_ranges(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<(RangeId, V)>, Error> {
        let mut rows = Vec::new();
        self.visit_leaves(
            |_| true,
            |id, value| {
                rows.push((id, value));
                Ok(())
            },
        )
        .await?;
        Self::time_ranges(rows, allowed_intervals)
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を同期的に読み出す（`Serialize` 実装用）。
    pub(crate) fn reconstructed_time_ranges_sync(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<(RangeId, V)>, Error> {
        let mut leaves = Leaves::new(self.roots, |_| true);
        let rows =
            core::iter::from_fn(|| leaves.next_sync(&self.file)).collect::<Result<Vec<_>, _>>()?;
        Self::time_ranges(rows, allowed_intervals)
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を、全体の最大ズームレベルに揃えた [`SingleId`] へ展開する。
    pub async fn flat_single_ids(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<(SingleId, V)>, Error>
    where
        V: Clone,
    {
        let ranges = self.reconstructed_time_ranges(allowed_intervals).await?;
        let Some(max_z) = ranges.iter().map(|(range, _)| range.z()).max() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (range, value) in ranges {
            let range = if range.z() == max_z {
                range
            } else {
                range
                    .spatial_children_at_zoom(max_z)
                    .expect("全体の最大ズームレベルは各RangeIdのズーム以上")
            };
            out.extend(range.single_ids().map(|id| (id, value.clone())));
        }
        Ok(out)
    }

    /// 葉の列 `rows` を時間方向に結合し、値をデコードする。値の比較はバイト列で行う。
    fn time_ranges(
        rows: Vec<(FlexId, EncodedValue)>,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<(RangeId, V)>, Error> {
        // ルートは時間で割っていないので、時間で割った領域があるのは時間のズームがルートより深い葉があるときに限る
        let has_temporal_split = rows.iter().any(|(id, _)| {
            id.finer_dimensions_than(&FlexId::ROOTS[id.root_index()])
                .contains(Dimension::T)
        });
        time_reconstruct::reconstruct(rows.into_iter(), has_temporal_split, allowed_intervals)
            .map(|(range, value)| Ok((range, V::decode(value.as_bytes())?)))
            .collect()
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

impl<V: DiskValue + Ord> FlexTreeCore<V> {
    /// 全ての値の範囲 `(最小, 最大)`。空なら [`None`]。
    pub async fn value_range(&self) -> Result<Option<(V, V)>, Error> {
        // V に Clone を求めないので、最大値が最小値と同じ間は最小値のバイト列を持ち、最後に 1 度だけデコードし直す
        let mut range: Option<(V, Result<V, EncodedValue>)> = None;
        self.visit_leaves(
            |_| true,
            |_, encoded| {
                let value = V::decode(encoded.as_bytes())?;
                range = Some(match range.take() {
                    None => (value, Err(encoded)),
                    Some((min, max)) => {
                        let upper = match &max {
                            Ok(max) => max,
                            Err(_) => &min,
                        };
                        if value < min {
                            (value, max.or(Ok(min)))
                        } else if value > *upper {
                            (min, Ok(value))
                        } else {
                            (min, max)
                        }
                    }
                });
                Ok(())
            },
        )
        .await?;
        Ok(match range {
            None => None,
            Some((min, Ok(max))) => Some((min, max)),
            Some((min, Err(encoded))) => {
                let max = V::decode(encoded.as_bytes())?;
                Some((min, max))
            }
        })
    }
}

impl<V> FlexTreeCore<V> {
    /// 空かどうかを返す。
    pub fn is_empty(&self) -> bool {
        self.roots == [None, None]
    }

    /// 全ての値を消す。永続化するには [`commit`](Self::commit) を呼ぶ。
    pub fn clear(&mut self) {
        self.roots = [None, None];
    }

    /// 値を持つ [`FlexId`] の数。
    pub async fn count(&self) -> Result<usize, Error> {
        let mut count = 0usize;
        self.visit_leaves(
            |_| true,
            |_, _| {
                count += 1;
                Ok(())
            },
        )
        .await?;
        Ok(count)
    }

    /// 値を持つ [`FlexId`] の F/X/Y のズームレベルのうち、最も高いもの。空なら [`None`]。
    pub async fn max_zoomlevel(&self) -> Result<Option<ZoomLevel>, Error> {
        let mut max_z: Option<ZoomLevel> = None;
        self.visit_leaves(
            |_| true,
            |id, _| {
                let z = id.spatial_max_zoomlevel();
                max_z = Some(max_z.map_or(z, |cur| cur.max(z)));
                Ok(())
            },
        )
        .await?;
        Ok(max_z)
    }

    /// すべての `(FlexId, V)` を非同期に読み出して返す。
    pub(crate) async fn entries(&self) -> Result<Vec<(FlexId, V)>, Error>
    where
        V: DiskValue,
    {
        let mut rows = Vec::new();
        self.visit_leaves(
            |_| true,
            |id, encoded| {
                rows.push((id, V::decode(encoded.as_bytes())?));
                Ok(())
            },
        )
        .await?;
        Ok(rows)
    }

    /// すべての [`FlexId`] を値をデコードせずに非同期で読み出して返す。
    pub(crate) async fn flex_ids(&self) -> Result<Vec<FlexId>, Error> {
        let mut ids = Vec::new();
        self.visit_leaves(
            |_| true,
            |id, _| {
                ids.push(id);
                Ok(())
            },
        )
        .await?;
        Ok(ids)
    }

    /// `allow` を満たす領域の葉を Z-order 昇順で辿る。
    async fn visit_leaves(
        &self,
        allow: impl Fn(&FlexId) -> bool,
        mut on_leaf: impl FnMut(FlexId, EncodedValue) -> Result<(), DiskError>,
    ) -> Result<(), DiskError> {
        let mut leaves = Leaves::new(self.roots, &allow);
        loop {
            match leaves.step(&allow)? {
                Step::Leaf(id, value) => on_leaf(id, value)?,
                Step::Read(pages) => {
                    let pages = self.file.read_many(&pages).await?;
                    leaves.supply(
                        pages
                            .iter()
                            .map(|page| NodePage::read(page))
                            .collect::<Result<_, _>>()?,
                    );
                }
                Step::Done => return Ok(()),
            }
        }
    }

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
    async fn edit(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        op: Op,
    ) -> Result<(), DiskError> {
        let mut roots = self.roots;
        let mut pages = HashMap::new();
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
            let mut changed = false;
            for target in &targets {
                let (edited, did_change) = self
                    .edit_node(node, region, target, &op, &mut pages)
                    .await?;
                node = edited;
                changed |= did_change;
            }
            if changed {
                *root = match node {
                    Some(node) => Some(self.store(node).await?),
                    None => None,
                };
            }
        }
        self.roots = roots;
        Ok(())
    }

    /// `node` をページに収まるよう分けて書き、ルートのページ番号を返す。
    async fn store(&mut self, node: Node) -> Result<PageId, DiskError> {
        // ルートが RefPage のページは読むときに弾くので、行き先のページをそのままルートにする
        if let Node::RefPage { page_id, .. } = node {
            return Ok(page_id);
        }
        let (root_id, node_pages) = NodePage::split(node, || self.file.alloc());
        for (page_id, node_page) in node_pages {
            self.file.write(page_id, Page::of(&*node_page)).await?;
        }
        Ok(root_id)
    }

    /// ページ `page_id` の Node を読み出す。
    async fn read_page(&self, page_id: PageId) -> Result<Node, DiskError> {
        NodePage::read(&*self.file.read(page_id).await?)
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
