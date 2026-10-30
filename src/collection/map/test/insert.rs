#[cfg(test)]
mod tests {
    use super::super::{TableEntry, build_table};
    use crate::{RangeId, SingleId, SpatialIdMap};
    use alloc::vec::Vec;

    /// 同じターゲットへ再挿入したときに、古い値が新しい値へ置き換わることを検証する。
    #[test]
    fn insert_same_target_replaces_previous_value() {
        let target = SingleId::new(4, 3, 2, 1).unwrap();
        let mut table = SpatialIdMap::new();

        table.insert(target, 10);
        table.insert(target, 20);

        let actual_values: Vec<_> = table.get(target).map(|(_, value)| *value).collect();
        assert_eq!(actual_values, vec![20]);
        assert!(table.iter().find(|(_, val)| **val == 10).is_none());
        assert_eq!(
            table
                .iter()
                .filter(|(_, val)| **val == 20)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            target.into_iter().collect::<Vec<_>>()
        );
    }

    /// 同じ値を持つ複数ターゲットが同じ値インデックスに集約されることを検証する。
    #[test]
    fn insert_same_value_groups_targets_under_one_value() {
        let first = SingleId::new(4, 3, 2, 1).unwrap();
        let second = SingleId::new(4, 3, 2, 2).unwrap();

        let table = build_table(&[
            TableEntry::Single(first, 10),
            TableEntry::Single(second, 10),
        ]);

        assert_eq!(
            table
                .iter()
                .filter(|(_, val)| **val == 10)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            first.into_iter().chain(second).collect::<Vec<_>>()
        );
    }

    /// RangeId の挿入結果が flat_single_ids() で単一Segmentに展開されることを検証する。
    #[test]
    fn insert_range_is_reflected_in_flat_single_ids() {
        let range = RangeId::new(3, [0, 1], [1, 1], [2, 2]).unwrap();
        let table = build_table(&[TableEntry::Range(range, 30)]);

        let actual: Vec<_> = table
            .flat_single_ids(None)
            .map(|(single_id, value)| (single_id, *value))
            .collect();
        let expected: Vec<_> = range
            .single_ids()
            .map(|single_id| (single_id, 30))
            .collect();

        assert_eq!(actual, expected);
    }
}
