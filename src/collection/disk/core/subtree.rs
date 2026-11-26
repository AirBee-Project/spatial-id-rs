use crate::collection::disk::core::node::RefPage;
use crate::collection::disk::io::{Page, PageId};
use crate::{Error, FlexId};
use alloc::boxed::Box;
use alloc::vec::Vec;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// 1 ページに収まる部分木。ページの中身そのもので、ファイルにはこのまま書く。
///
/// 自身の PageId も、自身が受け持つ領域も持たない。どちらも親から辿ってきた
/// [FlexTreeCore](super::FlexTreeCore) が知っており、操作のたびに渡す。
// Todo: 先頭にルートの Node の種類と使っている長さを置き、続けて Node のバイト列を置く
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(transparent)]
pub(super) struct SubTree([u8; Page::CAPACITY]);

/// [SubTree] の中で済んだ結果と、別のページにある子へ渡す続き。
pub(super) struct Step<T> {
    pub(super) done: T,
    /// 子ごとの、子の [SubTree] があるページ、子のルートが受け持つ領域、その領域の中に切り取った対象。
    pub(super) forward: Vec<(PageId, FlexId, Vec<FlexId>)>,
}

impl SubTree {
    /// Node を持たない部分木。
    pub(super) fn new() -> Box<Self> {
        todo!()
    }

    /// Node を 1 つも持たないか。親は空になった子を [`relink`](Self::relink) で外す。
    pub(super) fn is_empty(&self) -> bool {
        todo!()
    }

    /// `page` に置いた自身を指す、親に置くための [RefPage]。
    pub(super) fn link(&self, page: PageId) -> Result<RefPage, Error> {
        todo!()
    }

    /// `old` を指す [RefPage] を `new` に差し替える。`new` が [`None`] なら取り除く。
    ///
    /// 子をコピーオンライトで別のページへ写したときと、子が空になったときに使う。
    pub(super) fn relink(&mut self, old: PageId, new: Option<RefPage>) -> Result<(), Error> {
        todo!()
    }

    /// 領域 `region` の部分木から、`targets` と重なる領域と値を共通部分に切り取って返す。
    pub(super) fn get(
        &self,
        region: &FlexId,
        targets: &[FlexId],
    ) -> Result<Step<Vec<(FlexId, u64)>>, Error> {
        todo!()
    }

    /// 領域 `region` の部分木の `targets` に `value` を書き込む。既に値がある場所は上書きする。
    ///
    /// ページに収まらなくなったら、中の部分木を `alloc` で得たページへ移して [RefPage] に置き換え、
    /// 移した部分木を `done` に返す。
    pub(super) fn insert(
        &mut self,
        region: &FlexId,
        targets: &[FlexId],
        value: u64,
        alloc: &mut impl FnMut() -> PageId,
    ) -> Result<Step<Vec<(PageId, Box<SubTree>)>>, Error> {
        todo!()
    }

    /// 領域 `region` の部分木の `targets` を空にする。
    ///
    /// `targets` に覆われた子のページは [RefPage] ごと外し、読まずに捨てる。
    pub(super) fn remove(
        &mut self,
        region: &FlexId,
        targets: &[FlexId],
    ) -> Result<Step<()>, Error> {
        todo!()
    }
}
