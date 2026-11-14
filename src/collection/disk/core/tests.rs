use alloc::string::String;
use alloc::sync::Arc;
use alloc::task::Wake;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::path::PathBuf;
use std::thread::{self, Thread};

use super::FlexTreeCore;
use super::meta::Hemisphere;
use super::space::SegmentSize;
use crate::FlexId;
use crate::collection::disk::DiskValue;
use crate::collection::mem::core::FlexTreeCore as MemTree;

/// テスト用の最小限の executor。I/O は専用スレッドが進めるので、待つだけでよい。
fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

/// テストごとの一時ファイル。drop で消す。
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(alloc::format!(
            "spatial-id-disk-{name}-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// 上半分・下半分の両方にまたがる、ズーム 0〜7 の [FlexId]。
    fn id(&mut self) -> FlexId {
        let [fz, xz, yz] = [0, 0, 0].map(|_| (self.next() % 8) as u8);
        let f = (self.next() % (2 << fz)) as i32 - (1 << fz);
        let x = (self.next() % (1 << xz)) as u32;
        let y = (self.next() % (1 << yz)) as u32;
        FlexId::new(fz, f, xz, x, yz, y).unwrap()
    }

    /// ページが溢れるよう、長めの値。
    fn value(&mut self) -> String {
        "v".repeat((self.next() % 4) as usize * 20)
    }
}

const SMALL: SegmentSize = match SegmentSize::new(8) {
    Some(size) => size,
    None => panic!(),
};

impl<V: DiskValue + Clone + PartialEq> FlexTreeCore<V>
where
    V::Error: core::fmt::Debug,
{
    /// すべての値のある [FlexId] と値を、メモリ版と同じ正準な形に揃えたもの。
    ///
    /// ページをまたぐと、同じ中身の左右が Ref の違いでまとまらずに残ることがある。表す値は同じなので、
    /// メモリ版に入れ直して形を揃えてから比べる。
    async fn entries(&mut self) -> Vec<(FlexId, V)> {
        let mut canonical = MemTree::default();
        for (id, value) in self.raw_entries().await {
            canonical.insert([id], value);
        }
        mem_entries(&canonical)
    }

    async fn raw_entries(&mut self) -> Vec<(FlexId, V)> {
        let mut pending: Vec<_> = self.roots.iter().map(|root| root.page()).collect();
        while let Some(page) = pending.pop() {
            let refs = self.load(page).await.unwrap().refs().unwrap();
            pending.extend(refs.into_iter().map(|addr| addr.page()));
        }
        let mut out = Vec::new();
        for hemisphere in Hemisphere::ALL {
            if let Some(root) = self.roots[hemisphere] {
                self.pages
                    .get(root.page())
                    .collect(root.offset(), &hemisphere.region(), &self.pages, &mut out)
                    .unwrap();
            }
        }
        out
    }
}

fn mem_entries<V: Clone + PartialEq>(tree: &MemTree<V>) -> Vec<(FlexId, V)> {
    let mut out: Vec<_> = tree.iter().map(|(id, v)| (id, v.clone())).collect();
    out.sort_by_key(|(id, _)| id.encode());
    out
}

#[test]
fn matches_mem_across_pages() {
    let file = TempFile::new("across-pages");
    block_on(async {
        let mut disk = FlexTreeCore::<String>::open_with(&file.0, SMALL)
            .await
            .unwrap();
        let mut mem = MemTree::<String>::default();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for i in 0..1500 {
            let (id, value) = (rng.id(), rng.value());
            disk.insert([id], value.clone()).await.unwrap();
            mem.insert([id], value);
            if i % 100 == 99 {
                assert_eq!(disk.entries().await, mem_entries(&mem));
            }
            if i % 400 == 399 {
                disk.commit().await.unwrap();
            }
        }
        assert!(
            disk.pages.len() > 1,
            "1 ページに収まってしまい、ページをまたいでいない"
        );
    });
}

#[test]
fn reopen_restores_committed_state() {
    let file = TempFile::new("reopen");
    block_on(async {
        let mut mem = MemTree::<u64>::default();
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let mut disk = FlexTreeCore::<u64>::open_with(&file.0, SMALL)
            .await
            .unwrap();
        for _ in 0..800 {
            let (id, value) = (rng.id(), rng.next() % 5);
            disk.insert([id], value).await.unwrap();
            mem.insert([id], value);
        }
        disk.commit().await.unwrap();
        // コミットしていない変更は、開き直すと消える
        disk.insert([rng.id()], 99).await.unwrap();
        disk.close().await.unwrap();

        let mut disk = FlexTreeCore::<u64>::open(&file.0).await.unwrap();
        assert_eq!(disk.entries().await, mem_entries(&mem));
    });
}

#[test]
fn gc_keeps_file_from_growing() {
    let file = TempFile::new("gc");
    block_on(async {
        let mut mem = MemTree::<String>::default();
        let mut rng = Rng(0xD1B5_4A32_D192_ED03);
        let mut disk = FlexTreeCore::<String>::open_with(&file.0, SMALL)
            .await
            .unwrap();
        // 決まった ID だけを上書きし続けるので、生きているページの量はほぼ変わらない
        let ids: Vec<FlexId> = (0..60).map(|_| rng.id()).collect();
        let mut len_at_half = 0;
        for round in 0..300 {
            for _ in 0..5 {
                let id = ids[(rng.next() % ids.len() as u64) as usize];
                let value = rng.value();
                disk.insert([id], value.clone()).await.unwrap();
                mem.insert([id], value);
            }
            disk.commit().await.unwrap();
            if round == 150 {
                len_at_half = disk.file.len();
            }
        }
        assert!(
            disk.file.len() <= len_at_half * 5 / 4,
            "GC してもファイルが伸び続けている: {len_at_half} -> {}",
            disk.file.len()
        );
        assert_eq!(disk.entries().await, mem_entries(&mem));
        disk.close().await.unwrap();

        let mut disk = FlexTreeCore::<String>::open(&file.0).await.unwrap();
        assert_eq!(disk.entries().await, mem_entries(&mem));
    });
}
