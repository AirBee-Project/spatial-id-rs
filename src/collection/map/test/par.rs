#[cfg(all(test, feature = "rayon"))]
mod par_build {
    use crate::{FlexId, SingleId, SpatialIdMap};
    use alloc::vec::Vec;
    use rayon::prelude::*;

    /// 互いに素なSegmentに値を割り当てた入力（同値の共有あり・空間の衝突なし）。
    fn items() -> Vec<(FlexId, i32)> {
        (0..800i32)
            .flat_map(|i| {
                SingleId::new(20, 0, (i * 2) as u32, (i % 11) as u32)
                    .unwrap()
                    .into_iter()
                    .map(move |f| (f, i % 13)) // 値は 0..13 を循環＝同値が多数
            })
            .collect()
    }

    fn sorted(table: &SpatialIdMap<i32>) -> Vec<(FlexId, i32)> {
        let mut v: Vec<_> = table.iter().map(|(f, val)| (f, *val)).collect();
        v.sort_by_key(|(id, _)| id.field_key());
        v
    }

    #[test]
    fn par_matches_sequential() {
        let its = items();
        let seq: SpatialIdMap<i32> = its.iter().cloned().collect();
        let par: SpatialIdMap<i32> = its.par_iter().cloned().collect();

        assert_eq!(seq.count(), par.count());
        assert_eq!(sorted(&seq), sorted(&par));

        // 値クエリも一致する（ランク割り当ては違っても値は同じ）。
        for v in 0..13 {
            let mut a: Vec<FlexId> = seq
                .iter()
                .filter(|(_, val)| **val == v)
                .map(|(id, _)| id)
                .collect();
            let mut b: Vec<FlexId> = par
                .iter()
                .filter(|(_, val)| **val == v)
                .map(|(id, _)| id)
                .collect();
            a.sort_by_key(FlexId::field_key);
            b.sort_by_key(FlexId::field_key);
            assert_eq!(a, b, "value_get({v}) mismatch");
        }
    }
}
