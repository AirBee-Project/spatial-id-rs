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

    /// `RangeId` 版は `_flex` 版の各要素を `RangeId` に直したものと一致する。
    #[test]
    fn range_id_views_match_flex_views() {
        let table = sample();
        let target = RangeId::new(5, [0, 2], [2, 5], [2, 3]).unwrap();

        let flex: Vec<_> = table.iter_flex_ids().map(|(id, v)| (id, *v)).collect();
        let range: Vec<_> = table.iter().map(|(id, v)| (id, *v)).collect();
        assert_eq!(range, as_range(flex));

        let flex: Vec<_> = table.get_flex_ids(target).map(|(id, v)| (id, *v)).collect();
        let range: Vec<_> = table.get(target).map(|(id, v)| (id, *v)).collect();
        assert_eq!(range, as_range(flex));

        let flex: Vec<_> = table
            .get_overlapping_flex_ids(target)
            .map(|(id, v)| (id, *v))
            .collect();
        let range: Vec<_> = table
            .get_overlapping(target)
            .map(|(id, v)| (id, *v))
            .collect();
        assert_eq!(range, as_range(flex));

        assert_eq!(
            table.clone().into_iter().collect::<Vec<_>>(),
            as_range(table.clone().into_iter_flex_ids().collect())
        );
    }

    /// 削除は呼び出した時点で済み、どちらの版でも同じ内容が取り除かれる。
    #[test]
    fn remove_variants_remove_the_same_content() {
        let target = RangeId::new(5, [0, 2], [2, 5], [2, 3]).unwrap();

        let (mut a, mut b) = (sample(), sample());
        let removed: Vec<_> = a.remove(target).collect();
        let removed_flex: Vec<_> = b.remove_flex_ids(target).collect();
        assert_eq!(removed, as_range(removed_flex));
        assert_eq!(a, b);

        let (mut a, mut b) = (sample(), sample());
        let removed: Vec<_> = a.remove_overlapping(target).collect();
        let removed_flex: Vec<_> = b.remove_overlapping_flex_ids(target).collect();
        assert_eq!(removed, as_range(removed_flex));
        assert_eq!(a, b);
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

        let from_flex: SpatialIdMap<i32> = expected.clone().into_iter_flex_ids().collect();
        assert_eq!(from_flex, expected);

        let from_single: SpatialIdMap<i32> = [(single, 1)].into_iter().collect();
        assert_eq!(from_single.count(), 1);
    }
}
