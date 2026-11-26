mod node;
mod subtree;
use self::subtree::SubTree;
use crate::collection::disk::io::{PageId, PagedFile};
use crate::{Error, FlexId, SpatialId};
use alloc::boxed::Box;
use alloc::vec::Vec;
use std::collections::HashMap;
use std::path::Path;
use zerocopy::byteorder::{BigEndian, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub struct FlexTreeCore {
    file: PagedFile,
    /// 最後にコミットした世代。
    generation: u64,
    /// [FlexId::UPPER_MAX] を受け持つルートのページ。値がなければ [`None`]。
    upper_root: Option<PageId>,
    /// [FlexId::LOWER_MAX] を受け持つルートのページ。値がなければ [`None`]。
    lower_root: Option<PageId>,
    /// 最後のコミットの後に書き換えた、まだファイルにないページ。
    dirty: HashMap<PageId, Box<SubTree>>,
    /// 使っていないページ。
    // Todo: コミットで参照されなくなったページを、次のコミットが同期した後に戻す
    free: Vec<PageId>,
}

impl FlexTreeCore {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        todo!()
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// `target` の領域に `value` を書き込む。既に値がある場所は上書きする。
    pub async fn insert(&mut self, target: impl SpatialId, value: u64) -> Result<(), Error> {
        todo!()
    }

    /// `target` の領域を空にする。
    pub async fn remove(&mut self, target: impl SpatialId) -> Result<(), Error> {
        todo!()
    }

    /// `target` と重なる領域と値を、`target` との共通部分に切り取って返す。
    pub async fn get(&self, target: impl SpatialId) -> Result<Vec<(FlexId, u64)>, Error> {
        todo!()
    }

    /// ここまでの変更をファイルに残す。
    ///
    /// 失敗したときは、ファイルは直前にコミットした状態のまま残る。
    pub async fn commit(&mut self) -> Result<(), Error> {
        todo!()
    }

    /// ファイルを閉じる。コミットしていない変更は捨てる。
    pub async fn close(self) -> Result<(), Error> {
        todo!()
    }

    /// `page` の [SubTree] を読む。書き換え中ならその内容を返す。
    async fn load(&self, page: PageId) -> Result<Box<SubTree>, Error> {
        todo!()
    }

    /// `page` の [SubTree] を書き換えるために取り出し、置くページと合わせて返す。
    ///
    /// ファイルにあるページなら、新しいページを割り当てる（コピーオンライト）。
    /// 呼び出し側は、書き換えた後で `dirty` に戻し、ページが変わったなら親を [`relink`](SubTree::relink) する。
    async fn checkout(&mut self, page: PageId) -> Result<(PageId, Box<SubTree>), Error> {
        todo!()
    }

    fn alloc_page(&mut self) -> PageId {
        todo!()
    }

    fn free_page(&mut self, page: PageId) {
        todo!()
    }

    /// `target` を含む方のルートと、その領域を返す。
    fn root_for(&mut self, target: &FlexId) -> (&mut Option<PageId>, FlexId) {
        if target.f_index().is_negative() {
            (&mut self.lower_root, FlexId::LOWER_MAX)
        } else {
            (&mut self.upper_root, FlexId::UPPER_MAX)
        }
    }
}

/// コミットのたびに書く、ルートの位置を残すページ。
///
/// ファイル先頭の 2 ページに世代の偶奇で交互に書く。書いている途中で落ちてもチェックサムが合わなくなるだけで、
/// もう一方に直前のコミットが残る。
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct Meta {
    generation: U64<BigEndian>,
    /// 0 ならルートがない。0 はメタページの位置なので、ルートには使われない。
    upper_root: U64<BigEndian>,
    lower_root: U64<BigEndian>,
}

impl Meta {
    /// メタページを置く 2 ページ。
    const SLOTS: [PageId; 2] = [PageId(0), PageId(1)];

    /// 2 か所のうち、壊れていない最新のメタページ。どちらも壊れていればエラー。
    async fn read_latest(file: &PagedFile) -> Result<Self, Error> {
        todo!()
    }

    /// このメタページを書く場所。
    fn slot(&self) -> PageId {
        Self::SLOTS[(self.generation.get() % 2) as usize]
    }
}
