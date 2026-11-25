use crate::{FlexId, RangeId};

mod private {
    use crate::{FlexId, RangeId, SingleId, SpatialId, SpatialIdSet};

    pub trait Sealed {
        fn into_flex_ids(self) -> impl Iterator<Item = FlexId>;
        fn into_range_ids(self) -> impl Iterator<Item = RangeId>;
    }

    pub trait TargetItem {
        fn item_flex_ids(self) -> impl Iterator<Item = FlexId>;
        fn item_range_id(self) -> RangeId;
    }

    impl<T: SpatialId> TargetItem for T {
        #[inline]
        fn item_flex_ids(self) -> impl Iterator<Item = FlexId> {
            self.flex_ids()
        }

        #[inline]
        fn item_range_id(self) -> RangeId {
            self.into()
        }
    }

    macro_rules! impl_target_item_ref {
        ($($ty:ty),* $(,)?) => {
            $(
                impl TargetItem for &$ty {
                    #[inline]
                    fn item_flex_ids(self) -> impl Iterator<Item = FlexId> {
                        (*self).flex_ids()
                    }

                    #[inline]
                    fn item_range_id(self) -> RangeId {
                        (*self).into()
                    }
                }
            )*
        };
    }

    impl_target_item_ref!(SingleId, RangeId, FlexId);

    macro_rules! impl_sealed_single {
        ($($ty:ty),* $(,)?) => {
            $(
                impl Sealed for $ty {
                    #[inline]
                    fn into_flex_ids(self) -> impl Iterator<Item = FlexId> {
                        TargetItem::item_flex_ids(self)
                    }

                    #[inline]
                    fn into_range_ids(self) -> impl Iterator<Item = RangeId> {
                        core::iter::once(TargetItem::item_range_id(self))
                    }
                }
            )*
        };
    }

    // FlexId は IntoIterator を実装するため、下の IntoIterator 向け実装で扱う
    impl_sealed_single!(SingleId, RangeId, &SingleId, &RangeId, &FlexId);

    // FlexId -> RangeId -> FlexId の往復を避けるため専用の実装を与える
    impl Sealed for &SpatialIdSet {
        #[inline]
        fn into_flex_ids(self) -> impl Iterator<Item = FlexId> {
            self.iter_flex_ids()
        }

        #[inline]
        fn into_range_ids(self) -> impl Iterator<Item = RangeId> {
            self.iter()
        }
    }

    impl<I> Sealed for I
    where
        I: IntoIterator,
        I::Item: TargetItem,
    {
        #[inline]
        fn into_flex_ids(self) -> impl Iterator<Item = FlexId> {
            self.into_iter().flat_map(TargetItem::item_flex_ids)
        }

        #[inline]
        fn into_range_ids(self) -> impl Iterator<Item = RangeId> {
            self.into_iter().map(TargetItem::item_range_id)
        }
    }
}

/// [`SpatialIdMap`](crate::collection::SpatialIdMap) や [`SpatialIdSet`](crate::collection::SpatialIdSet) の操作対象として渡せる型を表すトレイト。
///
/// [`SingleId`](crate::SingleId)・[`RangeId`](crate::RangeId)・[`FlexId`](crate::FlexId)（およびそれらの参照）、
/// [`&SpatialIdSet`](crate::collection::SpatialIdSet)、配列、スライス、`Vec`、およびそれらを要素に持つ
/// [`IntoIterator`] を受け取る。
///
/// ```
/// use spatial_id::collection::SpatialIdSet;
/// use spatial_id::SingleId;
///
/// let ids = [
///     SingleId::new(20, 0, 0, 0).unwrap(),
///     SingleId::new(20, 0, 1, 0).unwrap(),
/// ];
/// let mut set = SpatialIdSet::new();
/// set.insert(&ids[..]);
/// assert_eq!(set.count(), 1);
/// assert_eq!(set.get_overlapping(ids.iter().copied()).count(), 1);
/// ```
pub trait IntoSpatialIds: private::Sealed {}

impl<T: private::Sealed> IntoSpatialIds for T {}

#[inline]
pub(in crate::collection) fn into_flex_ids<T: IntoSpatialIds>(
    target: T,
) -> impl Iterator<Item = FlexId> {
    private::Sealed::into_flex_ids(target)
}

#[inline]
pub(in crate::collection) fn into_range_ids<T: IntoSpatialIds>(
    target: T,
) -> impl Iterator<Item = RangeId> {
    private::Sealed::into_range_ids(target)
}
