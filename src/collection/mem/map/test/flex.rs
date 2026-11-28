#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{FlexId, RangeId, SingleId, SpatialIdMap};

    fn sample() -> SpatialIdMap<i32> {
        let mut table = SpatialIdMap::new();
        table.insert(SingleId::new(6, 3, 5, 9).unwrap(), 10);
        table.insert(RangeId::new(5, [-2, 4], [1, 7], [2, 3]).unwrap(), 20);
        table
    }

    fn as_range(items: Vec<(FlexId, i32)>) -> Vec<(RangeId, i32)> {
        items
            .into_iter()
            .map(|(id, v)| (RangeId::from(id), v))
            .collect()
    }

    fn as_single(items: Vec<(FlexId, i32)>) -> Vec<(SingleId, i32)> {
        items
            .into_iter()
            .flat_map(|(id, v)| id.single_ids().map(move |s| (s, v)))
            .collect()
    }

    /// 既定の `RangeId` 版・`.flex_ids()`・`.single_ids()` が同じ領域と値を返す。
    #[test]
    fn range_id_views_match_flex_views() {
        let table = sample();
        let target = RangeId::new(5, [0, 2], [2, 5], [2, 3]).unwrap();

        let flex: Vec<_> = table.iter().flex_ids().map(|(id, v)| (id, *v)).collect();
        let range: Vec<_> = table.iter().map(|(id, v)| (id, *v)).collect();
        let single: Vec<_> = table.iter().single_ids().map(|(id, v)| (id, *v)).collect();
        assert_eq!(range, as_range(flex.clone()));
        assert_eq!(single, as_single(flex));
        assert_eq!(table.iter().len(), table.count());
        assert_eq!(table.iter().flex_ids().len(), table.count());

        let flex: Vec<_> = table
            .get(target)
            .flex_ids()
            .map(|(id, v)| (id, *v))
            .collect();
        let range: Vec<_> = table.get(target).map(|(id, v)| (id, *v)).collect();
        let single: Vec<_> = table
            .get(target)
            .single_ids()
            .map(|(id, v)| (id, *v))
            .collect();
        assert_eq!(range, as_range(flex.clone()));
        assert_eq!(single, as_single(flex));

        let flex: Vec<_> = table
            .get_overlapping(target)
            .flex_ids()
            .map(|(id, v)| (id, *v))
            .collect();
        let range: Vec<_> = table
            .get_overlapping(target)
            .map(|(id, v)| (id, *v))
            .collect();
        let single: Vec<_> = table
            .get_overlapping(target)
            .single_ids()
            .map(|(id, v)| (id, *v))
            .collect();
        assert_eq!(range, as_range(flex.clone()));
        assert_eq!(single, as_single(flex));

        let flex: Vec<_> = table
            .spatial_face_neighbors(target)
            .flex_ids()
            .map(|(id, v)| (id, *v))
            .collect();
        let range: Vec<_> = table
            .spatial_face_neighbors(target)
            .map(|(id, v)| (id, *v))
            .collect();
        let single: Vec<_> = table
            .spatial_face_neighbors(target)
            .single_ids()
            .map(|(id, v)| (id, *v))
            .collect();
        assert_eq!(range, as_range(flex.clone()));
        assert_eq!(single, as_single(flex));

        let into_flex: Vec<_> = table.clone().into_iter().flex_ids().collect();
        assert_eq!(
            table.clone().into_iter().collect::<Vec<_>>(),
            as_range(into_flex.clone())
        );
        assert_eq!(
            table.clone().into_iter().single_ids().collect::<Vec<_>>(),
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

    /// `FromIterator` と `Extend` は `SingleId`・`RangeId`・`FlexId` のどれでも受け取る。
    #[test]
    fn from_iterator_accepts_every_id_type() {
        let single = SingleId::new(6, 3, 5, 9).unwrap();
        let range = RangeId::new(5, [-2, 4], [1, 7], [2, 3]).unwrap();
        let flex = FlexId::new(4, 1, 4, 3, 4, 2).unwrap();

        let mut expected = SpatialIdMap::new();
        expected.insert(single, 1);
        expected.insert(range, 2);
        expected.insert(flex, 3);

        let mut from_ranges = SpatialIdMap::new();
        from_ranges.extend([(RangeId::from(single), 1), (range, 2)]);
        from_ranges.extend([(RangeId::from(flex), 3)]);
        assert_eq!(from_ranges, expected);

        let from_flex: SpatialIdMap<i32> = expected.clone().into_iter().flex_ids().collect();
        assert_eq!(from_flex, expected);

        let from_single: SpatialIdMap<i32> = [(single, 1)].into_iter().collect();
        assert_eq!(from_single.count(), 1);
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn compact_preserves_map_entries_and_queries() {
        let mut map = SpatialIdMap::new();
        for dx in [0_u32, 10_000] {
            map.insert(SingleId::new(20, 0, dx, 0).unwrap(), 10);
            map.insert(SingleId::new(20, 0, dx + 1, 1).unwrap(), 20);
            map.insert(SingleId::new(20, 0, dx + 2, 0).unwrap(), 10);
        }
        let expected: Vec<_> = map.iter().map(|(id, v)| (id, *v)).collect();
        map.compact();
        assert_eq!(map.count(), expected.len());
        assert_eq!(
            map.iter().map(|(id, v)| (id, *v)).collect::<Vec<_>>(),
            expected
        );
        let q = SingleId::new(20, 0, 10_001, 1).unwrap();
        assert_eq!(map.get(q).map(|(_, v)| *v).collect::<Vec<_>>(), [20]);
    }
}
