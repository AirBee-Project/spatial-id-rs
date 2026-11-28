#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{FlexId, RangeId, SingleId, SpatialIdSet};

    fn sample() -> SpatialIdSet {
        let mut set = SpatialIdSet::new();
        set.insert(SingleId::new(6, 3, 5, 9).unwrap());
        set.insert(RangeId::new(5, [-2, 4], [1, 7], [2, 3]).unwrap());
        set
    }

    fn as_range(items: Vec<FlexId>) -> Vec<RangeId> {
        items.into_iter().map(RangeId::from).collect()
    }

    fn as_single(items: Vec<FlexId>) -> Vec<SingleId> {
        items.into_iter().flat_map(FlexId::single_ids).collect()
    }

    /// 既定の `RangeId` 版・`.flex_ids()`・`.single_ids()` が同じ領域を返す。
    #[test]
    fn range_id_views_match_flex_views() {
        let set = sample();
        let target = RangeId::new(5, [0, 2], [2, 5], [2, 3]).unwrap();

        let iter_flex: Vec<_> = set.iter().flex_ids().collect();
        assert_eq!(set.iter().collect::<Vec<_>>(), as_range(iter_flex.clone()));
        assert_eq!(
            set.iter().single_ids().collect::<Vec<_>>(),
            as_single(iter_flex)
        );
        assert_eq!(set.iter().len(), set.count());
        assert_eq!(set.iter().flex_ids().len(), set.count());

        let get_flex: Vec<_> = set.get(target).flex_ids().collect();
        assert_eq!(
            set.get(target).collect::<Vec<_>>(),
            as_range(get_flex.clone())
        );
        assert_eq!(
            set.get(target).single_ids().collect::<Vec<_>>(),
            as_single(get_flex)
        );

        let ov_flex: Vec<_> = set.get_overlapping(target).flex_ids().collect();
        assert_eq!(
            set.get_overlapping(target).collect::<Vec<_>>(),
            as_range(ov_flex.clone())
        );
        assert_eq!(
            set.get_overlapping(target).single_ids().collect::<Vec<_>>(),
            as_single(ov_flex)
        );

        let nb_flex: Vec<_> = set.spatial_face_neighbors(target).flex_ids().collect();
        assert_eq!(
            set.spatial_face_neighbors(target).collect::<Vec<_>>(),
            as_range(nb_flex.clone())
        );
        assert_eq!(
            set.spatial_face_neighbors(target)
                .single_ids()
                .collect::<Vec<_>>(),
            as_single(nb_flex)
        );

        let into_flex: Vec<_> = set.clone().into_iter().flex_ids().collect();
        assert_eq!(
            set.clone().into_iter().collect::<Vec<_>>(),
            as_range(into_flex.clone())
        );
        assert_eq!(
            set.clone().into_iter().single_ids().collect::<Vec<_>>(),
            as_single(into_flex)
        );
    }

    /// 削除は呼び出した時点で完了し、既定・`.flex_ids()`・`.single_ids()` のどれでも同じ内容が取り除かれる。
    #[test]
    fn remove_variants_remove_the_same_content() {
        let target = RangeId::new(5, [0, 2], [2, 5], [2, 3]).unwrap();

        let (mut a, mut b, mut c) = (sample(), sample(), sample());
        let removed: Vec<_> = a.remove(target).collect();
        let removed_flex: Vec<_> = b.remove(target).flex_ids().collect();
        let removed_single: Vec<_> = c.remove(target).single_ids().collect();
        assert_eq!(removed, as_range(removed_flex.clone()));
        assert_eq!(removed_single, as_single(removed_flex));
        assert_eq!(a, b);
        assert_eq!(a, c);

        let (mut a, mut b, mut c) = (sample(), sample(), sample());
        let removed: Vec<_> = a.remove_overlapping(target).collect();
        let removed_flex: Vec<_> = b.remove_overlapping(target).flex_ids().collect();
        let removed_single: Vec<_> = c.remove_overlapping(target).single_ids().collect();
        assert_eq!(removed, as_range(removed_flex.clone()));
        assert_eq!(removed_single, as_single(removed_flex));
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn compact_preserves_set_elements() {
        let mut set = SpatialIdSet::new();
        for dx in [0_u32, 10_000] {
            set.insert(SingleId::new(20, 0, dx, 0).unwrap());
            set.insert(SingleId::new(20, 0, dx + 1, 1).unwrap());
        }
        let expected: Vec<_> = set.iter().collect();
        set.compact();
        assert_eq!(set.count(), expected.len());
        assert_eq!(set.iter().collect::<Vec<_>>(), expected);
        let q = SingleId::new(20, 0, 10_001, 1).unwrap();
        assert_eq!(set.get(q).count(), 1);
    }
}
