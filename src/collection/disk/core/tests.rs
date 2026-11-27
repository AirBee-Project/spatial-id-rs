use super::*;
use crate::SpatialIdMap;
use alloc::sync::Arc;
use std::path::PathBuf;
use std::pin::pin;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

/// 起こされるまでスレッドを止めて `future` を最後まで進める。
pub(super) fn block_on<T>(future: impl Future<Output = T>) -> T {
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

/// テストごとに別の、まだないファイルの場所。テストが終われば消す。
struct TempPath(PathBuf);

impl TempPath {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("spatial-id-disk-{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        TempPath(path)
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// テストを再現できるよう、種から決まる乱数列。
pub(super) struct Lcg(pub(super) u64);

impl Lcg {
    pub(super) fn below(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % u64::from(n)) as u32
    }

    /// F / X / Y のズームが 0〜6 の FlexId。F は正負どちらにもなる。
    pub(super) fn flex_id(&mut self) -> FlexId {
        let mut zoom_index = || {
            let z = self.below(7) as u8;
            (z, self.below(1 << z))
        };
        let (fz, f) = zoom_index();
        let (xz, x) = zoom_index();
        let (yz, y) = zoom_index();
        let f = f as i32 - if self.below(2) == 0 { 0 } else { 1 << fz };
        FlexId::new(fz, f, xz, x, yz, y).unwrap()
    }
}

/// 全体を読み出し、比べられるよう SpatialIdMap に集めたもの。
fn contents(core: &FlexTreeCore) -> SpatialIdMap<u64> {
    let mut map = SpatialIdMap::new();
    for target in FlexId::ROOTS {
        for (id, value) in block_on(core.get(target)).unwrap() {
            map.insert(id, value);
        }
    }
    map
}

/// insert / upsert / remove をランダムに施し、同じ操作を施した SpatialIdMap を返す。
fn apply_random(core: &mut FlexTreeCore, rng: &mut Lcg, count: usize) -> SpatialIdMap<u64> {
    let mut expected = contents(core);
    for _ in 0..count {
        let target = rng.flex_id();
        let value = u64::from(rng.below(30));
        match rng.below(3) {
            0 => {
                block_on(core.insert(target, value)).unwrap();
                expected.insert(target, value);
            }
            1 => {
                block_on(core.upsert(target, value)).unwrap();
                expected.upsert(target, value);
            }
            _ => {
                block_on(core.remove(target)).unwrap();
                let _ = expected.remove_flex_ids(target);
            }
        }
    }
    expected
}

/// コミットした内容は、開き直しても読める。
#[test]
fn committed_changes_survive_reopen() {
    let path = TempPath::new("reopen");
    let mut rng = Lcg(1);
    let mut core = block_on(FlexTreeCore::open(&path.0)).unwrap();
    let expected = apply_random(&mut core, &mut rng, 1000);
    assert_eq!(contents(&core), expected);
    block_on(core.commit()).unwrap();
    // コミットした後も、書き換えはファイルのページを読んで続けられる
    let expected = apply_random(&mut core, &mut rng, 1000);
    block_on(core.commit()).unwrap();
    block_on(core.close()).unwrap();

    let core = block_on(FlexTreeCore::open(&path.0)).unwrap();
    assert_eq!(contents(&core), expected);
    block_on(core.close()).unwrap();
}

/// コミットしていない変更は、開き直すと消えている。
#[test]
fn uncommitted_changes_are_discarded() {
    let path = TempPath::new("discard");
    let mut rng = Lcg(2);
    let mut core = block_on(FlexTreeCore::open(&path.0)).unwrap();
    let expected = apply_random(&mut core, &mut rng, 500);
    block_on(core.commit()).unwrap();
    apply_random(&mut core, &mut rng, 500);
    block_on(core.close()).unwrap();

    let core = block_on(FlexTreeCore::open(&path.0)).unwrap();
    assert_eq!(contents(&core), expected);
    block_on(core.close()).unwrap();
}

/// 書き換えは直前のコミットのページを上書きせず、ファイルの末尾へ書き足すだけ。
#[test]
fn writes_only_append() {
    let path = TempPath::new("append");
    let mut rng = Lcg(3);
    let mut core = block_on(FlexTreeCore::open(&path.0)).unwrap();
    apply_random(&mut core, &mut rng, 1000);
    block_on(core.commit()).unwrap();
    let committed_len = core.file.len();
    let committed: Vec<Box<Page>> = (MetaPage::SLOTS.len() as u64..committed_len)
        .map(|page_id| block_on(core.file.read(PageId(page_id))).unwrap())
        .collect();

    apply_random(&mut core, &mut rng, 1000);
    assert!(core.file.len() > committed_len, "書き換えは末尾へ書き足す");
    for (page_id, before) in (MetaPage::SLOTS.len() as u64..).zip(&committed) {
        let after = block_on(core.file.read(PageId(page_id))).unwrap();
        assert!(
            after[..] == before[..],
            "ページ {page_id} は上書きされていない"
        );
    }
    block_on(core.close()).unwrap();
}
