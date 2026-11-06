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

    /// `RangeId` 版は `_flex` 版の各要素を `RangeId` に直したものと一致する。
    #[test]
    fn range_id_views_match_flex_views() {
        let set = sample();
        let target = RangeId::new(5, [0, 2], [2, 5], [2, 3]).unwrap();

        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            as_range(set.iter_flex_ids().collect())
        );
        assert_eq!(
            set.get(target).collect::<Vec<_>>(),
            as_range(set.get_flex_ids(target).collect())
        );
        assert_eq!(
            set.get_overlapping(target).collect::<Vec<_>>(),
            as_range(set.get_overlapping_flex_ids(target).collect())
        );
        assert_eq!(
            set.clone().into_iter().collect::<Vec<_>>(),
            as_range(set.clone().into_iter_flex_ids().collect())
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
}
