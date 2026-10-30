#[cfg(test)]
mod tests {
    use super::super::{TableEntry, build_table};
    use crate::{RangeId, SingleId, SpatialIdMap};
    use alloc::vec::Vec;

    #[test]
    fn upsert_writes_into_empty_table() {
        let target = SingleId::new(4, 3, 2, 1).unwrap();
        let mut table = SpatialIdMap::new();

        table.upsert(target, 10);

        let actual_values: Vec<_> = table.get(target).map(|(_, value)| *value).collect();
        assert_eq!(actual_values, vec![10]);
    }

    #[test]
    fn upsert_keeps_the_existing_value() {
        let target = SingleId::new(4, 3, 2, 1).unwrap();
        let mut table = SpatialIdMap::new();

        table.insert(target, 10);
        table.upsert(target, 20);

        let actual_values: Vec<_> = table.get(target).map(|(_, value)| *value).collect();
        assert_eq!(actual_values, vec![10], "既存値が上書きされてはいけない");
        assert!(table.iter().find(|(_, value)| **value == 20).is_none());
    }

    /// target が既存領域と空き領域の両方にまたがる場合、占有済みの部分は既存値のまま、
    /// 空きの部分にだけ新値が入ること。
    #[test]
    fn upsert_fills_only_the_empty_part_of_a_partial_overlap() {
        let occupied = SingleId::new(4, 3, 2, 1).unwrap();
        let empty = SingleId::new(4, 3, 2, 2).unwrap();
        let both = RangeId::new(4, 3, 2, [1, 2]).unwrap();

        let mut table = build_table(&[TableEntry::Single(occupied, 10)]);
        table.upsert(both, 20);

        assert_eq!(
            table.get(occupied).map(|(_, v)| *v).collect::<Vec<_>>(),
            vec![10],
            "既存側は保たれる"
        );
        assert_eq!(
            table.get(empty).map(|(_, v)| *v).collect::<Vec<_>>(),
            vec![20],
            "空き側には新値が入る"
        );
    }

    /// target が既に全て埋まっている upsert は何も書かず、テーブルは元と等しいままであること。
    #[test]
    fn upsert_on_a_fully_occupied_target_changes_nothing() {
        let target = SingleId::new(4, 3, 2, 1).unwrap();

        let mut with_noop_upsert = SpatialIdMap::new();
        with_noop_upsert.insert(target, 10);
        with_noop_upsert.upsert(target, 20); // 既に埋まっているので何も書かないはず

        let plain = build_table(&[TableEntry::Single(target, 10)]);

        assert_eq!(
            with_noop_upsert, plain,
            "埋まっている場所への upsert でテーブルが変わった"
        );
        assert!(with_noop_upsert.iter().find(|(_, val)| **val == 20).is_none());
    }
}
