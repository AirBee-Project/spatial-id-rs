#[cfg(test)]
mod tests {
    use super::super::{TableEntry, build_table};
    use crate::{RangeId, SingleId};
    use alloc::vec::Vec;

    /// get() が指定した空間に一致する ID と値だけを返すことを検証する。
    #[test]
    fn get_returns_expected_pairs() {
        let first = SingleId::new(4, 3, 2, 1).unwrap();
        let second = SingleId::new(4, 3, 2, 2).unwrap();
        let table = build_table(&[
            TableEntry::Single(first, 10),
            TableEntry::Single(second, 20),
        ]);

        let actual: Vec<_> = table
            .get(first)
            .map(|(flex_id, value)| (flex_id, *value))
            .collect();
        assert_eq!(actual.len(), 1);
        let (flex_id, value) = &actual[0];
        assert_eq!(*value, 10);
        assert_eq!(flex_id.f_zoomlevel(), 4);
        assert_eq!(flex_id.f_index(), 3);
        assert_eq!(flex_id.x_zoomlevel(), 4);
        assert_eq!(flex_id.x_index(), 2);
        assert_eq!(flex_id.y_zoomlevel(), 4);
        assert_eq!(flex_id.y_index(), 1);
    }

    /// get_range() は x軸の折り返し表現（`x[0] > x[1]`、経度方向の周期境界をまたぐ範囲）
    /// でも、同じ領域を昇順で指定した場合と同じ結果を返す（回帰テスト）。
    ///
    /// 木の枝刈り（`Node::overlapping_children_range`）が `target.x()[0]`/`[1]` を
    /// 単純な min/max として扱い、この折り返し規約（[`RangeId::set_x`]と同じ規約）を
    /// 考慮していなかったため、折り返し範囲を渡すと本来ヒットするはずのSegmentを
    /// 見失うことがあった。
    #[test]
    fn get_range_handles_wrapped_x() {
        let id = SingleId::new(2, 0, 2, 0).unwrap();
        let table = build_table(&[TableEntry::Single(id, 42)]);

        // [1, 0] は折り返し規約上 {1,2,3,0}（= z=2 の x 全域）を表す。
        let mut wrapped = RangeId::new(2, 0, 0, 0).unwrap();
        wrapped.set_x([1, 0]).unwrap();
        assert_eq!(wrapped.x(), [1, 0]);

        let full = RangeId::new(2, 0, [0, 3], 0).unwrap();
        let via_wrapped_count = table.get_overlapping(wrapped).count();
        let via_full_count = table.get_overlapping(full).count();

        assert_eq!(via_wrapped_count, 1, "折り返し範囲でも1件ヒットするはず");
        assert_eq!(via_wrapped_count, via_full_count);
    }

    /// iter() がテーブル中の全ての ID と値の組を返すことを検証する。
    #[test]
    fn iter_returns_all_pairs() {
        let table = build_table(&[
            TableEntry::Single(SingleId::new(4, 3, 2, 1).unwrap(), 10),
            TableEntry::Range(RangeId::new(3, [0, 1], [1, 1], [2, 2]).unwrap(), 30),
        ]);

        let actual: Vec<_> = table
            .iter()
            .map(|(flex_id, value)| (flex_id, *value))
            .collect();
        assert!(!actual.is_empty());
        assert!(actual.iter().any(|(_, value)| *value == 10));
        assert!(actual.iter().any(|(_, value)| *value == 30));
    }
}
