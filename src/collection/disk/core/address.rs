//! ページと Node の位置。

use crate::collection::disk::io::Page;

/// ページの場所。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PageAddr {
    /// ファイル上の位置。先頭から何ページ目か。
    Stored(u64),
    /// まだ書いていない、メモリ上のページの仮の番号。コミットでファイル上の位置に置き換える。
    Pending(u64),
}

/// Node を指す位置。ページの場所とページ内の位置を 1 つの u64 に詰め、Ref に書く。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NodeAddr(u64);

impl NodeAddr {
    /// ページ内の位置に使う下位のビット数。
    const OFFSET_BITS: u32 = Page::SIZE.trailing_zeros();
    /// [`PageAddr::Pending`] を表す最上位のビット。
    const PENDING: u64 = 1 << 63;

    pub(super) fn new(page: PageAddr, offset: usize) -> Self {
        debug_assert!(offset < Page::SIZE);
        let page = match page {
            PageAddr::Stored(index) => index,
            PageAddr::Pending(id) => id | Self::PENDING >> Self::OFFSET_BITS,
        };
        NodeAddr(page << Self::OFFSET_BITS | offset as u64)
    }

    pub(super) fn from_bits(bits: u64) -> Self {
        NodeAddr(bits)
    }

    pub(super) fn to_bits(self) -> u64 {
        self.0
    }

    pub(super) fn page(self) -> PageAddr {
        let page = (self.0 & !Self::PENDING) >> Self::OFFSET_BITS;
        if self.0 & Self::PENDING == 0 {
            PageAddr::Stored(page)
        } else {
            PageAddr::Pending(page)
        }
    }

    pub(super) fn offset(self) -> usize {
        (self.0 & ((1 << Self::OFFSET_BITS) - 1)) as usize
    }

    /// 同じページ内の位置のまま、ページをファイル上の `index` に移した位置。
    pub(super) fn moved_to(self, index: u64) -> Self {
        Self::new(PageAddr::Stored(index), self.offset())
    }
}
