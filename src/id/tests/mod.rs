pub mod overflow;
mod zorder;

mod flex_ids {
    use alloc::vec::Vec;

    use crate::{FlexId, RangeId, SingleId, SpatialIdSet};

    /// `flex_ids` が返す `FlexId` を全部足すと、元の空間IDの領域をちょうど覆う。
    #[test]
    fn flex_ids_cover_the_original_region() {
        let single = SingleId::new(5, 3, 2, 10).unwrap();
        let range = RangeId::new(4, [-3, 6], [8, 9], [5, 10]).unwrap();
        let flex = FlexId::new(4, 1, 4, 3, 4, 2).unwrap();

        assert_eq!(flex.flex_ids().collect::<Vec<_>>(), [flex]);
        assert_eq!(
            single.flex_ids().collect::<SpatialIdSet>(),
            SpatialIdSet::from_iter([single])
        );
        assert_eq!(
            range.flex_ids().collect::<SpatialIdSet>(),
            SpatialIdSet::from_iter([range])
        );
    }
}
