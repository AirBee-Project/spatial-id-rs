#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{FlexId, RangeId, SingleId, SpatialId, SpatialIdMap, SpatialIdSet};

    fn ids() -> (SingleId, RangeId, FlexId) {
        let single = SingleId::new(4, 3, 2, 1).unwrap();
        let range = RangeId::new(4, [0, 1], [4, 5], [1, 1]).unwrap();
        let flex = FlexId::new(4, -1, 4, 7, 4, 3).unwrap();
        (single, range, flex)
    }

    fn expected_map() -> SpatialIdMap<i32> {
        let (single, range, flex) = ids();
        let mut table = SpatialIdMap::new();
        table.insert(single, 10);
        table.insert(range, 20);
        table.insert(flex, 30);
        table
    }

    fn collect_sorted<T: Ord>(iter: impl Iterator<Item = T>) -> Vec<T> {
        let mut items: Vec<_> = iter.collect();
        items.sort();
        items
    }

    fn as_sorted_ranges(items: Vec<(FlexId, i32)>) -> Vec<(RangeId, i32)> {
        collect_sorted(items.into_iter().map(|(id, v)| (RangeId::from(id), v)))
    }

    fn generic_insert_single<S: SpatialId>(table: &mut SpatialIdMap<i32>, id: S, value: i32) {
        table.insert(id, value);
    }

    fn generic_insert_iter<S: SpatialId>(
        table: &mut SpatialIdMap<i32>,
        ids: impl IntoIterator<Item = S>,
        value: i32,
    ) {
        table.insert(ids, value);
    }

    /// `insert`・`get`・`get_flex_ids` が配列・スライス・`Vec`・参照・イテレータ・集合・汎用 `SpatialId` を同様に扱う。
    #[test]
    fn insert_and_get_accept_all_target_forms() {
        let (single, range, flex) = ids();
        let single_pair = [single, SingleId::new(4, 3, 2, 2).unwrap()];
        let range_vec = vec![RangeId::from(single), range, RangeId::from(flex)];

        let mut from_array = SpatialIdMap::new();
        from_array.insert(single_pair, 7);

        let mut from_slice = SpatialIdMap::new();
        from_slice.insert(&single_pair[..], 7);

        let mut from_vec_ref = SpatialIdMap::new();
        let pair_vec = single_pair.to_vec();
        from_vec_ref.insert(&pair_vec, 7);

        let mut from_iter = SpatialIdMap::new();
        generic_insert_iter(&mut from_iter, pair_vec.iter().copied(), 7);

        assert_eq!(from_array, from_slice);
        assert_eq!(from_array, from_vec_ref);
        assert_eq!(from_array, from_iter);

        let mut generic = SpatialIdMap::new();
        generic_insert_single(&mut generic, single, 7);
        generic_insert_single(&mut generic, single_pair[1], 7);
        assert_eq!(from_array, generic);

        let table = expected_map();
        let target_set: SpatialIdSet = range_vec.iter().copied().collect();
        let via_vec: Vec<_> = table
            .get_flex_ids(range_vec.clone())
            .map(|(id, v)| (id, *v))
            .collect();
        let via_slice: Vec<_> = table
            .get_flex_ids(&range_vec[..])
            .map(|(id, v)| (id, *v))
            .collect();
        let via_vec_ref: Vec<_> = table
            .get_flex_ids(&range_vec)
            .map(|(id, v)| (id, *v))
            .collect();
        let via_iter: Vec<_> = table
            .get_flex_ids(range_vec.iter().copied())
            .map(|(id, v)| (id, *v))
            .collect();
        let via_set_ref: Vec<_> = table
            .get_flex_ids(&target_set)
            .map(|(id, v)| (id, *v))
            .collect();
        let via_set_owned: Vec<_> = table
            .get_flex_ids(target_set)
            .map(|(id, v)| (id, *v))
            .collect();
        let via_ref_single: Vec<_> = table.get(&single).map(|(id, v)| (id, *v)).collect();

        assert_eq!(via_slice, via_vec);
        assert_eq!(via_vec_ref, via_vec);
        assert_eq!(via_iter, via_vec);
        assert_eq!(
            as_sorted_ranges(via_set_ref),
            as_sorted_ranges(via_vec.clone())
        );
        assert_eq!(
            as_sorted_ranges(via_set_owned),
            as_sorted_ranges(via_vec.clone())
        );
        assert_eq!(
            via_ref_single,
            table
                .get(single)
                .map(|(id, v)| (id, *v))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            collect_sorted(table.get(&range_vec).map(|(id, v)| (id, *v))),
            as_sorted_ranges(via_vec)
        );
    }

    /// `remove`・`remove_flex_ids`・`remove_overlapping`・`remove_overlapping_flex_ids` は
    /// 呼び出し時点で削除を終えており、重なりを含む複数ターゲットでも重複除去されない。
    #[test]
    fn remove_variants_accept_multi_targets_without_borrowing_table() {
        let (single, range, _) = ids();
        let overlapping = SingleId::new(4, 0, 4, 1).unwrap();
        assert_eq!(
            expected_map().get(overlapping).next().map(|(_, v)| *v),
            Some(20)
        );

        let mut table = expected_map();
        let targets = vec![RangeId::from(single), range, RangeId::from(overlapping)];
        let removed_iter = table.remove(&targets);
        assert!(table.get(single).next().is_none());
        assert!(table.get(range).next().is_none());
        let after_remove = table.clone();
        table.clear();
        let removed: Vec<_> = removed_iter.collect();
        assert!(!removed.is_empty());

        let mut table_flex = expected_map();
        let removed_flex_iter = table_flex.remove_flex_ids(targets.iter());
        assert_eq!(
            collect_sorted(removed.into_iter()),
            collect_sorted(removed_flex_iter.map(|(id, v)| (RangeId::from(id), v)))
        );
        assert_eq!(after_remove, table_flex);

        let mut table_overlap = expected_map();
        let overlap_targets = vec![overlapping, single];
        let overlap_removed_iter = table_overlap.remove_overlapping(&overlap_targets);
        assert!(table_overlap.get(range).next().is_none());
        let overlap_removed: Vec<_> = overlap_removed_iter.collect();
        assert!(overlap_removed.iter().any(|(_, v)| *v == 10));
        assert!(overlap_removed.iter().any(|(_, v)| *v == 20));
    }

    /// `get_overlapping` 系は重なる複数ターゲットでも一致する要素を 1 回ずつ返す。
    #[test]
    fn get_overlapping_accepts_multi_targets_and_deduplicates() {
        let range = RangeId::new(4, [0, 1], [4, 5], [1, 1]).unwrap();
        let inner_a = SingleId::new(4, 0, 4, 1).unwrap();
        let inner_b = SingleId::new(4, 1, 5, 1).unwrap();
        let mut table = SpatialIdMap::new();
        table.insert(range, 99);

        let hits: Vec<_> = table
            .get_overlapping(&[inner_a, inner_b][..])
            .map(|(id, v)| (id, *v))
            .collect();
        let hits_flex: Vec<_> = table
            .get_overlapping_flex_ids([inner_a, inner_b].iter().copied())
            .map(|(id, v)| (RangeId::from(id), *v))
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, 99);
        assert_eq!(hits, hits_flex);
    }

    /// `insert_with` と `upsert` が複数ターゲットおよび重なり箇所を意図どおりに処理する。
    #[test]
    fn insert_with_and_upsert_handle_multi_and_overlapping_targets() {
        let a = SingleId::new(4, 0, 0, 0).unwrap();
        let b = SingleId::new(4, 0, 0, 1).unwrap();
        let both = RangeId::new(4, 0, 0, [0, 1]).unwrap();

        let mut table = SpatialIdMap::new();
        table.insert(a, 10);
        table.upsert(&[a, b][..], 5);
        assert_eq!(table.get(a).next().map(|(_, v)| *v), Some(10));
        assert_eq!(table.get(b).next().map(|(_, v)| *v), Some(5));

        table.insert_with([RangeId::from(a), both], 3, |old, new| old + new);
        // `a` は `[a, both]` の2要素に重なるため +3 が 2 回適用され、`b` は 1 回だけ適用される
        assert_eq!(table.get(a).next().map(|(_, v)| *v), Some(16));
        assert_eq!(table.get(b).next().map(|(_, v)| *v), Some(8));
    }

    /// 空のターゲット列はいずれの操作でも安全に空動作となる。
    #[test]
    fn empty_targets_are_noops() {
        let mut table = expected_map();
        let before = table.clone();
        let empty: [SingleId; 0] = [];
        let empty_set = SpatialIdSet::new();

        table.insert(empty, 1);
        table.upsert(core::iter::empty::<RangeId>(), 1);
        table.insert_with(&empty_set, 1, |a, b| a + b);
        assert_eq!(table, before);

        assert!(table.get(&empty[..]).next().is_none());
        assert!(table.get_overlapping(&empty[..]).next().is_none());
        assert!(table.remove(empty).next().is_none());
        assert!(table.remove_overlapping(&empty_set).next().is_none());
        assert_eq!(table, before);
    }
}
