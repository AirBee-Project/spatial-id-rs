use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::{Error, QueryError};

/// [`CancellationToken::check_amortized`] が実際に確認する間隔（呼び出し回数）。
const AMORTIZED_CHECK_INTERVAL: u32 = 0xFFF;

#[derive(Clone, Debug)]
enum Inner {
    /// 割り当てなしの、決してキャンセルされない状態。
    Never,
    Shared(Arc<AtomicBool>),
}

/// クエリ実行への協調的キャンセルを伝えるトークン。複製は状態を共有する。
#[derive(Clone, Debug)]
pub struct CancellationToken(Inner);

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self(Inner::Shared(Arc::new(AtomicBool::new(false))))
    }

    /// 決してキャンセルされないトークン。[`cancel()`](Self::cancel) は無視される。
    pub fn never() -> Self {
        Self(Inner::Never)
    }

    pub fn cancel(&self) {
        if let Inner::Shared(flag) = &self.0 {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match &self.0 {
            Inner::Never => false,
            Inner::Shared(flag) => flag.load(Ordering::Relaxed),
        }
    }

    /// タイトループ用。`counter` を進め、一定間隔でだけ実際に [`is_cancelled`](Self::is_cancelled)を確認する。
    #[inline]
    pub(crate) fn check_amortized(&self, counter: &mut u32) -> Result<(), Error> {
        *counter = counter.wrapping_add(1);
        if *counter & AMORTIZED_CHECK_INTERVAL != 0 {
            return Ok(());
        }
        if self.is_cancelled() {
            Err(Error::Query(QueryError::Cancelled))
        } else {
            Ok(())
        }
    }
}
