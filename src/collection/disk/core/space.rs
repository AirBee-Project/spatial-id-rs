//! セグメント単位のファイル領域の割り当て。
//!
//! ページは 1 つずつ空きを探さず、セグメントの中に先頭から詰めて書く。GC は生きているページをすべて
//! 新しいセグメントへ写し、それ以外のセグメントをまとめて空けるので、ファイルが虫食いにならない。
//!
//! 空いているセグメントは、連続するものをまとめた [FreeRun] の並びとしてメタページに残す。
//! 載りきらない分は残さず、次の GC で空きとして見つけ直す。空きを忘れても、使わないだけで壊れはしない。

use alloc::collections::BTreeSet;

/// セグメント 1 つのページ数。2 の冪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SegmentSize(u32);

impl SegmentSize {
    /// 2MiB。
    pub(super) const DEFAULT: Self = SegmentSize(512);

    /// 2 の冪でないか、メタページ 2 枚が収まらなければ [`None`]。
    pub(super) const fn new(pages: u32) -> Option<Self> {
        if pages.is_power_of_two() && pages >= 2 {
            Some(SegmentSize(pages))
        } else {
            None
        }
    }

    pub(super) fn pages(self) -> u32 {
        self.0
    }
}

/// ファイルをセグメントの大きさで区切った 1 区画。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Segment(u64);

impl Segment {
    /// 先頭のセグメントはメタページに使う。
    const FIRST_DATA: Segment = Segment(1);

    fn page(self, offset: u32, size: SegmentSize) -> u64 {
        self.0 << size.0.trailing_zeros() | u64::from(offset)
    }
}

/// 連続して空いているセグメント。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FreeRun {
    pub(super) first: u64,
    pub(super) len: u64,
}

#[derive(Debug)]
pub(super) struct Space {
    size: SegmentSize,
    free: BTreeSet<Segment>,
    /// これより後ろのセグメントは、まだ一度も使っていない。
    end: Segment,
    /// 書き込み中のセグメントと、その中で次に使う位置。
    open: Option<(Segment, u32)>,
    /// [`restart`](Self::restart) の後に書き始めたセグメント。
    taken: BTreeSet<Segment>,
}

impl Space {
    /// 長さ `file_pages` のファイルで、`free` のセグメントが空いている。
    pub(super) fn new(
        size: SegmentSize,
        file_pages: u64,
        free: impl IntoIterator<Item = FreeRun>,
    ) -> Self {
        let end = Segment(
            file_pages
                .div_ceil(u64::from(size.0))
                .max(Segment::FIRST_DATA.0),
        );
        Space {
            size,
            free: free
                .into_iter()
                .flat_map(|run| run.first..run.first + run.len)
                .map(Segment)
                .filter(|&segment| Segment::FIRST_DATA <= segment && segment < end)
                .collect(),
            end,
            open: None,
            taken: BTreeSet::new(),
        }
    }

    /// 空いているセグメントを、先頭から順に連続するものをまとめて返す。
    pub(super) fn free_runs(&self) -> impl Iterator<Item = FreeRun> + '_ {
        let mut segments = self.free.iter().map(|segment| segment.0).peekable();
        core::iter::from_fn(move || {
            let first = segments.next()?;
            let mut len = 1;
            while segments.next_if_eq(&(first + len)).is_some() {
                len += 1;
            }
            Some(FreeRun { first, len })
        })
    }

    pub(super) fn size(&self) -> SegmentSize {
        self.size
    }

    /// 次に書くページの、ファイル上の位置。
    pub(super) fn allocate(&mut self) -> u64 {
        let (segment, offset) = match self.open {
            Some((segment, offset)) if offset < self.size.0 => (segment, offset),
            _ => (self.take_segment(), 0),
        };
        self.open = Some((segment, offset + 1));
        segment.page(offset, self.size)
    }

    /// GC を始める。この後に書くページは、新しいセグメントに置く。
    pub(super) fn restart(&mut self) {
        self.open = None;
        self.taken.clear();
    }

    /// GC を終える。[`restart`](Self::restart) の後に書いていないセグメントを、すべて空ける。
    ///
    /// GC が書いたページを指すメタページを永続化してから呼ぶこと。それより前に空けると、
    /// 障害で戻る先の古い状態のページを上書きしてしまう。
    pub(super) fn finish(&mut self) {
        self.free = (Segment::FIRST_DATA.0..self.end.0)
            .map(Segment)
            .filter(|segment| !self.taken.contains(segment))
            .collect();
    }

    fn take_segment(&mut self) -> Segment {
        let segment = self.free.pop_first().unwrap_or_else(|| {
            let segment = self.end;
            self.end = Segment(segment.0 + 1);
            segment
        });
        self.taken.insert(segment);
        segment
    }
}
