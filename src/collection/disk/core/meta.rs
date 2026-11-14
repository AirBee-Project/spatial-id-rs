//! ファイルの先頭に置くメタページと、そこに残す木のルート。
//!
//! メタページは 2 か所に交互に書く。コミットはデータを書いて永続化してから、古いほうの場所にメタページを書く。
//! 書いている途中で落ちてもチェックサムが合わなくなるだけで、もう一方の場所に直前の状態が残る。

use core::ops::{Index, IndexMut};

use alloc::boxed::Box;
use zerocopy::byteorder::{BigEndian, U16, U32, U64};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::address::NodeAddr;
use super::space::{FreeRun, SegmentSize, Space};
use crate::collection::disk::io::{Page, PagedFile};
use crate::{DiskError, Error, FlexId};

/// F の符号で分けた、全空間の上半分と下半分。メモリ版と同じく、それぞれにルートを 1 つずつ持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hemisphere {
    Upper = 0,
    Lower = 1,
}

impl Hemisphere {
    pub(super) const ALL: [Hemisphere; 2] = [Hemisphere::Upper, Hemisphere::Lower];

    /// `id` を含む側。
    pub(super) fn of(id: &FlexId) -> Self {
        if id.f_index().is_negative() {
            Hemisphere::Lower
        } else {
            Hemisphere::Upper
        }
    }

    /// この側全体の領域。
    pub(super) fn region(self) -> FlexId {
        match self {
            Hemisphere::Upper => FlexId::UPPER_MAX,
            Hemisphere::Lower => FlexId::LOWER_MAX,
        }
    }
}

/// 上半分と下半分のルート。値が 1 つもない側は [`None`]。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Roots([Option<NodeAddr>; 2]);

impl Roots {
    pub(super) fn iter(&self) -> impl Iterator<Item = NodeAddr> + '_ {
        self.0.iter().flatten().copied()
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = &mut NodeAddr> {
        self.0.iter_mut().flatten()
    }
}

impl Index<Hemisphere> for Roots {
    type Output = Option<NodeAddr>;

    fn index(&self, hemisphere: Hemisphere) -> &Self::Output {
        &self.0[hemisphere as usize]
    }
}

impl IndexMut<Hemisphere> for Roots {
    fn index_mut(&mut self, hemisphere: Hemisphere) -> &mut Self::Output {
        &mut self.0[hemisphere as usize]
    }
}

/// GC の時期を決めるために数える、書いたページの数。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct WriteCounts {
    /// 最後の GC で書いた、生きているページの数。
    pub(super) live: u64,
    /// 最後の GC の後に、コミットで書いたページの数。
    pub(super) since_gc: u64,
}

impl WriteCounts {
    /// GC の後に書いた分が、そのとき生きていた分を超えたら GC する。小さいうちは 1 セグメント分は待つ。
    ///
    /// GC で写す量がそれまでに書いた量を超えないので、書き込み 1 回あたりの GC の手間がならして一定になる。
    pub(super) fn gc_due(self, size: SegmentSize) -> bool {
        self.since_gc > self.live.max(u64::from(size.pages()))
    }
}

#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct Meta {
    magic: [u8; 8],
    version: U32<BigEndian>,
    segment_pages: U32<BigEndian>,
    /// コミットのたびに 1 つ増やす。2 か所のうち大きいほうが最新。
    generation: U64<BigEndian>,
    live_pages: U64<BigEndian>,
    pages_since_gc: U64<BigEndian>,
    roots: [RootSlot; 2],
    free_count: U16<BigEndian>,
    /// 空いているセグメント。載りきらない分は残さない。
    free: [FreeRunSlot; Meta::FREE_RUNS],
}

/// 連続して空いているセグメント 1 つ分。
#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct FreeRunSlot {
    first: U64<BigEndian>,
    len: U64<BigEndian>,
}

/// ルート 1 つ。
#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(super) struct RootSlot {
    /// 0 ならルートがない。
    present: u8,
    addr: U64<BigEndian>,
}

impl RootSlot {
    fn new(root: Option<NodeAddr>) -> Self {
        RootSlot {
            present: u8::from(root.is_some()),
            addr: U64::new(root.map_or(0, NodeAddr::to_bits)),
        }
    }

    fn get(self) -> Option<NodeAddr> {
        (self.present != 0).then(|| NodeAddr::from_bits(self.addr.get()))
    }
}

impl Meta {
    const MAGIC: [u8; 8] = *b"SPIDTREE";
    const VERSION: u32 = 2;
    /// メタページを置く 2 か所。
    const SLOTS: [u64; 2] = [0, 1];
    /// 残せる [FreeRun] の数。メタページに収まるだけ。
    const FREE_RUNS: usize = 240;

    pub(super) fn new(generation: u64, space: &Space, counts: WriteCounts, roots: &Roots) -> Self {
        let mut meta = Meta::new_zeroed();
        meta.magic = Self::MAGIC;
        meta.version = U32::new(Self::VERSION);
        meta.segment_pages = U32::new(space.size().pages());
        meta.generation = U64::new(generation);
        meta.live_pages = U64::new(counts.live);
        meta.pages_since_gc = U64::new(counts.since_gc);
        meta.roots = Hemisphere::ALL.map(|hemisphere| RootSlot::new(roots[hemisphere]));
        let mut count = 0;
        for (slot, run) in meta.free.iter_mut().zip(space.free_runs()) {
            *slot = FreeRunSlot {
                first: U64::new(run.first),
                len: U64::new(run.len),
            };
            count += 1;
        }
        meta.free_count = U16::new(count);
        meta
    }

    /// 2 か所のうち、壊れていない最新のメタページ。どちらも壊れていればエラー。
    pub(super) async fn read_latest(file: &PagedFile) -> Result<Self, Error> {
        let mut latest: Option<Meta> = None;
        for slot in Self::SLOTS.into_iter().filter(|&slot| slot < file.len()) {
            let page = file.read(slot).await?;
            let Ok(meta) = page.try_head_as::<Meta>() else {
                continue;
            };
            let valid = meta.magic == Self::MAGIC && meta.version.get() == Self::VERSION;
            if valid && latest.is_none_or(|latest| latest.generation() < meta.generation()) {
                latest = Some(*meta);
            }
        }
        Ok(latest.ok_or(DiskError::Corrupt)?)
    }

    /// このメタページを書く場所。
    pub(super) fn slot(&self) -> u64 {
        Self::SLOTS[(self.generation() % 2) as usize]
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation.get()
    }

    pub(super) fn segment_size(&self) -> Result<SegmentSize, Error> {
        Ok(SegmentSize::new(self.segment_pages.get()).ok_or(DiskError::Corrupt)?)
    }

    pub(super) fn counts(&self) -> WriteCounts {
        WriteCounts {
            live: self.live_pages.get(),
            since_gc: self.pages_since_gc.get(),
        }
    }

    pub(super) fn free_runs(&self) -> impl Iterator<Item = FreeRun> + '_ {
        self.free[..usize::from(self.free_count.get()).min(Self::FREE_RUNS)]
            .iter()
            .map(|slot| FreeRun {
                first: slot.first.get(),
                len: slot.len.get(),
            })
    }

    pub(super) fn roots(&self) -> Roots {
        let mut roots = Roots::default();
        for hemisphere in Hemisphere::ALL {
            roots[hemisphere] = self.roots[hemisphere as usize].get();
        }
        roots
    }
}

impl From<&Meta> for Box<Page> {
    fn from(meta: &Meta) -> Self {
        Page::of(meta)
    }
}
