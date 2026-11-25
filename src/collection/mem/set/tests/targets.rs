#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{FlexId, RangeId, SingleId, SpatialId, SpatialIdSet};

    fn sample_ids() -> (SingleId, RangeId, FlexId) {
        let single = SingleId::new(4, 3, 2, 1).unwrap();
        let range = RangeId::new(4, [0, 1], [4, 5], [1, 1]).unwrap();
        let flex = FlexId::new(4, -1, 4, 7, 4, 3).unwrap();
        (single, range, flex)
    }

    fn generic_set_insert<S: SpatialId>(set: &mut SpatialIdSet, ids: impl IntoIterator<Item = S>) {
        set.insert(ids);
    }

    /// `SpatialIdSet` の各操作が単一・参照・配列・スライス・イテレータ・集合を同等に扱う。
    #[test]
    fn set_operations_accept_all_target_forms() {
        let (single, range, flex) = sample_ids();
        let targets = [RangeId::from(single), range, RangeId::from(flex)];

        let mut from_array = SpatialIdSet::new();
        from_array.insert(targets);

        let mut from_slice = SpatialIdSet::new();
        from_slice.insert(&targets[..]);

        let mut from_vec = SpatialIdSet::new();
        from_vec.insert(targets.to_vec());

        let mut from_iter = SpatialIdSet::new();
        generic_set_insert(&mut from_iter, targets.iter().copied());

        assert_eq!(from_array, from_slice);
        assert_eq!(from_array, from_vec);
        assert_eq!(from_array, from_iter);

        let mut via_set_ref = SpatialIdSet::new();
        via_set_ref.insert(&from_array);
        assert_eq!(via_set_ref, from_array);

        let mask: SpatialIdSet = [RangeId::from(single), RangeId::from(flex)]
            .into_iter()
            .collect();
        let flex_as_single = SingleId::new(4, -1, 7, 3).unwrap();
        assert_eq!(
            from_array.get(&mask).collect::<Vec<_>>(),
            from_array
                .get(&[single, flex_as_single][..])
                .collect::<Vec<_>>()
        );
        assert_eq!(
            from_array.get(&mask).collect::<Vec<_>>(),
            from_array.get(mask.iter()).collect::<Vec<_>>()
        );
        assert_eq!(
            from_array.get_flex_ids(&single).collect::<Vec<_>>(),
            from_array.get_flex_ids(single).collect::<Vec<_>>()
        );
    }

    /// `remove` 系は集合を借用せず返し、`get_overlapping` 系は重複せず返す。
    #[test]
    fn set_remove_does_not_borrow_set_and_overlapping_deduplicates() {
        let range = RangeId::new(4, [0, 1], [4, 5], [1, 1]).unwrap();
        let inner_a = SingleId::new(4, 0, 4, 1).unwrap();
        let inner_b = SingleId::new(4, 1, 5, 1).unwrap();

        let mut set = SpatialIdSet::new();
        set.insert(range);

        let hits: Vec<_> = set.get_overlapping(&[inner_a, inner_b][..]).collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits,
            set.get_overlapping_flex_ids([inner_a, inner_b].iter().copied())
                .map(RangeId::from)
                .collect::<Vec<_>>()
        );

        let mut set_a = set.clone();
        let parts = vec![inner_a, inner_b];
        let removed_iter = set_a.remove(&parts);
        assert!(set_a.get([inner_a, inner_b]).next().is_none());
        set_a.clear();
        assert_eq!(removed_iter.count(), 2);

        let mut set_b = set.clone();
        let mut removed_flex_iter = set_b.remove_overlapping_flex_ids(parts.iter());
        assert!(set_b.is_empty());
        assert!(removed_flex_iter.next().is_some());
    }

    /// 空のターゲット列はいずれの操作でも空動作となる。
    #[test]
    fn empty_targets_are_noops() {
        let (single, _, _) = sample_ids();
        let mut set = SpatialIdSet::new();
        set.insert(single);
        let before = set.clone();

        let empty: [FlexId; 0] = [];
        set.insert(empty);
        assert_eq!(set, before);
        assert!(set.get(&empty[..]).next().is_none());
        assert!(set.get_overlapping(&empty[..]).next().is_none());
        assert!(set.remove(&empty[..]).next().is_none());
        assert!(set.remove_overlapping(empty).next().is_none());
        assert_eq!(set, before);
    }
}
