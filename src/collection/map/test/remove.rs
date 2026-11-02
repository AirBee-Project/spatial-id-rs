#[cfg(test)]
mod tests {
    use super::super::{TableEntry, build_table};
    use crate::SingleId;
    use alloc::vec::Vec;

    /// remove() 後に index と count() が整合していることを検証する。
    #[test]
    fn remove_updates_indexes_and_count() {
        let first = SingleId::new(4, 3, 2, 1).unwrap();
        let second = SingleId::new(4, 3, 2, 2).unwrap();
        let mut table = build_table(&[
            TableEntry::Single(first, 10),
            TableEntry::Single(second, 20),
        ]);

        let removed: Vec<_> = table.remove(first).collect();
        assert_eq!(removed.len(), 1);
        let (flex_id, value) = &removed[0];
        assert_eq!(*value, 10);
        assert_eq!(flex_id.f_zoomlevel(), 4);
        assert_eq!(flex_id.f_index(), 3);
        assert_eq!(flex_id.x_zoomlevel(), 4);
        assert_eq!(flex_id.x_index(), 2);
        assert_eq!(flex_id.y_zoomlevel(), 4);
        assert_eq!(flex_id.y_index(), 1);
        assert!(table.get(first).next().is_none());
        assert_eq!(
            table
                .iter()
                .filter(|(_, v)| **v == 20)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            second.into_iter().collect::<Vec<_>>()
        );
        assert_eq!(table.count(), table.iter().count());
    }

    /// 値を削除した後に、values() が残存値だけを返すことを検証する。
    #[test]
    fn remove_value_then_values_only_contains_remaining_values() {
        let first = SingleId::new(4, 3, 2, 1).unwrap();
        let second = SingleId::new(4, 3, 2, 2).unwrap();
        let mut table = build_table(&[
            TableEntry::Single(first, 10),
            TableEntry::Single(second, 20),
        ]);

        table.remove(first);
        assert_eq!(table.iter().map(|(_, v)| *v).collect::<Vec<_>>(), vec![20]);
    }
}
