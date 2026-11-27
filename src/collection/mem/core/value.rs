use core::{
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem::{self, ManuallyDrop, MaybeUninit},
    ops::Deref,
    ptr,
};
use triomphe::Arc;

/// 1 ワードに収まる値はインラインで、超える値は `Arc<V>` でヒープへ退避して保持する値スロット。
///
/// 格納方式は `V` のサイズとアライメントからコンパイル時に一意に決まるため、実行時の判別タグを持たない。
#[repr(transparent)]
pub(super) struct CompactValue<V> {
    storage: Storage<V>,
    _marker: PhantomData<V>,
}

#[repr(C)]
union Storage<V> {
    arc: ManuallyDrop<Arc<V>>,
    // ポインタを含む小さな `V`（`Box<T>` 等）をムーブしても provenance を失わないよう `*const ()` を使う
    inline: MaybeUninit<*const ()>,
}

// SAFETY: インライン時は `V` を、非インライン時は `Arc<V>` を保持する。
// `V: Send + Sync` なら `V`・`Arc<V>` いずれも `Send + Sync` となる。
unsafe impl<V: Send + Sync> Send for CompactValue<V> {}
unsafe impl<V: Send + Sync> Sync for CompactValue<V> {}

impl<V> CompactValue<V> {
    pub(super) const IS_INLINE: bool = mem::size_of::<V>() <= mem::size_of::<Arc<V>>()
        && mem::align_of::<V>() <= mem::align_of::<Arc<V>>();

    #[inline]
    pub(super) fn new(value: V) -> Self {
        if Self::IS_INLINE {
            let mut inline = MaybeUninit::<*const ()>::uninit();
            // SAFETY: `IS_INLINE` により `V` のサイズとアライメントは `inline` に収まる
            unsafe { inline.as_mut_ptr().cast::<V>().write(value) };
            Self {
                storage: Storage { inline },
                _marker: PhantomData,
            }
        } else {
            Self {
                storage: Storage {
                    arc: ManuallyDrop::new(Arc::new(value)),
                },
                _marker: PhantomData,
            }
        }
    }

    #[inline]
    pub(super) fn as_ref(&self) -> &V {
        if Self::IS_INLINE {
            // SAFETY: `IS_INLINE` のため `storage.inline` に有効な `V` が書き込まれている
            unsafe { &*self.storage.inline.as_ptr().cast::<V>() }
        } else {
            // SAFETY: `!IS_INLINE` のため `storage.arc` が有効
            unsafe { &self.storage.arc }
        }
    }

    /// 非インライン格納時のみ内部の `Arc<V>` への参照を返す。
    #[inline]
    pub(super) fn as_arc(&self) -> Option<&Arc<V>> {
        if Self::IS_INLINE {
            None
        } else {
            // SAFETY: `!IS_INLINE` のため `storage.arc` が有効
            Some(unsafe { &self.storage.arc })
        }
    }

    /// 非インライン格納の `Arc<V>` から直接構築する。
    #[cfg(any(feature = "unstable", test))]
    #[inline]
    pub(super) fn from_arc(arc: Arc<V>) -> Self {
        debug_assert!(!Self::IS_INLINE);
        Self {
            storage: Storage {
                arc: ManuallyDrop::new(arc),
            },
            _marker: PhantomData,
        }
    }

    /// 両方が非インライン格納で、同一の `Arc` を指しているなら `true`。
    #[inline]
    pub(super) fn ptr_eq<W>(&self, other: &CompactValue<W>) -> bool {
        match (self.as_arc(), other.as_arc()) {
            (Some(a), Some(b)) => ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            _ => false,
        }
    }
}

impl<V: Clone> CompactValue<V> {
    #[inline]
    pub(super) fn into_inner(self) -> V {
        let mut this = ManuallyDrop::new(self);
        if Self::IS_INLINE {
            // SAFETY: `ManuallyDrop` により `Drop` は走らず、インラインの `V` の所有権を1度だけ取り出す
            unsafe { this.storage.inline.as_ptr().cast::<V>().read() }
        } else {
            // SAFETY: `ManuallyDrop` により `Drop` は走らず、`arc` の所有権を1度だけ取り出す
            let arc = unsafe { ManuallyDrop::take(&mut this.storage.arc) };
            Arc::unwrap_or_clone(arc)
        }
    }
}

impl<V: Clone> Clone for CompactValue<V> {
    #[inline]
    fn clone(&self) -> Self {
        if Self::IS_INLINE {
            Self::new(self.as_ref().clone())
        } else {
            // SAFETY: `!IS_INLINE` のため `storage.arc` が有効
            let arc = unsafe { Arc::clone(&self.storage.arc) };
            Self {
                storage: Storage {
                    arc: ManuallyDrop::new(arc),
                },
                _marker: PhantomData,
            }
        }
    }
}

impl<V> Drop for CompactValue<V> {
    #[inline]
    fn drop(&mut self) {
        if Self::IS_INLINE {
            // SAFETY: `IS_INLINE` のため `storage.inline` に有効な `V` があり、ここで1度だけ解放する
            unsafe { ptr::drop_in_place(self.storage.inline.as_mut_ptr().cast::<V>()) };
        } else {
            // SAFETY: `!IS_INLINE` のため `storage.arc` が有効であり、ここで1度だけ解放する
            unsafe { ManuallyDrop::drop(&mut self.storage.arc) };
        }
    }
}

impl<V> Deref for CompactValue<V> {
    type Target = V;

    #[inline]
    fn deref(&self) -> &V {
        self.as_ref()
    }
}

impl<V: fmt::Debug> fmt::Debug for CompactValue<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_ref(), f)
    }
}

impl<V: PartialEq> PartialEq for CompactValue<V> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<V: Eq> Eq for CompactValue<V> {}

impl<V: Hash> Hash for CompactValue<V> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::CompactValue;
    use alloc::{boxed::Box, string::String};
    use core::mem::{align_of, size_of};
    use triomphe::Arc;

    #[repr(align(16))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Align16(u8);

    #[repr(align(32))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct AlignedZst;

    #[test]
    fn inline_threshold_respects_size_and_alignment() {
        const {
            assert!(CompactValue::<()>::IS_INLINE);
            assert!(CompactValue::<u8>::IS_INLINE);
            assert!(CompactValue::<u32>::IS_INLINE);
            assert!(CompactValue::<[u8; size_of::<usize>()]>::IS_INLINE);
            assert!(CompactValue::<Box<u32>>::IS_INLINE);

            assert!(!CompactValue::<[u8; size_of::<usize>() + 1]>::IS_INLINE);
            assert!(!CompactValue::<String>::IS_INLINE);
            assert!(!CompactValue::<Align16>::IS_INLINE);
            assert!(!CompactValue::<AlignedZst>::IS_INLINE);
        }

        assert_eq!(size_of::<CompactValue<()>>(), size_of::<Arc<()>>());
        assert_eq!(size_of::<CompactValue<u32>>(), size_of::<Arc<u32>>());
        assert_eq!(size_of::<CompactValue<String>>(), size_of::<Arc<String>>());
        assert_eq!(
            size_of::<CompactValue<[u64; 64]>>(),
            size_of::<Arc<[u64; 64]>>()
        );
        assert_eq!(
            size_of::<CompactValue<Align16>>(),
            size_of::<Arc<Align16>>()
        );
    }

    #[test]
    fn inline_and_arc_values_round_trip_and_keep_alignment() {
        let boxed = CompactValue::new(Box::new(42_u64));
        assert!(boxed.as_arc().is_none());
        assert_eq!(**boxed, 42);
        let cloned_box = boxed.clone();
        assert_eq!(boxed.into_inner(), Box::new(42));
        assert_eq!(cloned_box.into_inner(), Box::new(42));

        let large = CompactValue::new(Align16(9));
        assert!(large.as_arc().is_some());
        assert_eq!(
            (large.as_ref() as *const Align16 as usize) % align_of::<Align16>(),
            0
        );
        let cloned_large = large.clone();
        assert!(large.ptr_eq(&cloned_large));
        assert_eq!(large.into_inner(), Align16(9));
        assert_eq!(cloned_large.into_inner(), Align16(9));

        let zst = CompactValue::new(AlignedZst);
        assert!(zst.as_arc().is_some());
        assert_eq!(
            (zst.as_ref() as *const AlignedZst as usize) % align_of::<AlignedZst>(),
            0
        );
        assert_eq!(zst.into_inner(), AlignedZst);
    }

    #[test]
    fn large_nan_preserves_partial_eq_semantics() {
        let a = CompactValue::new([f64::NAN; 4]);
        let b = a.clone();
        assert!(a.ptr_eq(&b));
        assert_ne!(a, b);
    }

    #[test]
    fn send_and_sync_bounds_hold() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CompactValue<u32>>();
        assert_send_sync::<CompactValue<String>>();
        assert_send_sync::<CompactValue<[u64; 16]>>();
    }
}
