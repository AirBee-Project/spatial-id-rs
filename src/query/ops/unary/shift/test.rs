use crate::{FlexId, RangeId, Source, SpatialIdMap};
use alloc::vec::Vec;
use proptest::prelude::*;

/// 移動量の組。負の向き、X の周回、F・Y の範囲外を含む。
const DELTAS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (0, -3, 0),
    (0, 0, 2),
    (2, 70, -1),
    (-5, -1, 3),
    (200, 0, 0),
];

/// ズーム 3〜6 の混ざった表。粗い要素が細かい要素に合わせて木の中で割られる。
fn arb_shift_entries() -> impl Strategy<Value = Vec<(FlexId, u32)>> {
    prop::collection::vec((FlexId::arb_within(3..=6), 0..4u32), 1..=20)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn shift_fxy_matches_chained_shifts(entries in arb_shift_entries()) {
        let table: SpatialIdMap<u32> = entries.into_iter().collect();
        for (f, x, y) in DELTAS {
            let fused = table
                .clone()
                .query()
                .shift_fxy(6, f, x, y)
                .get(..)
                .into_map();
            let chained = table
                .clone()
                .query()
                .shift_f(6, f)
                .shift_x(6, x)
                .shift_y(6, y)
                .get(..)
                .into_map();
            prop_assert_eq!(fused.is_ok(), chained.is_ok(), "({:?})", (f, x, y));
            if let (Ok(fused), Ok(chained)) = (fused, chained) {
                prop_assert_eq!(fused, chained, "({:?})", (f, x, y));
            }
        }
    }

    #[test]
    fn shift_fxy_partial_get_matches_whole(entries in arb_shift_entries()) {
        let table: SpatialIdMap<u32> = entries.into_iter().collect();
        let target = RangeId::new(4, [-3, 5], [2, 9], [4, 12]).unwrap();
        for (f, x, y) in DELTAS {
            let query = table.clone().query().shift_fxy(6, f, x, y);
            let Ok(expected) = table
                .clone()
                .query()
                .shift_fxy(6, f, x, y)
                .get(..)
                .into_map()
            else {
                continue;
            };
            let within = query.get(target).into_map().unwrap();
            let clip = |t: &SpatialIdMap<u32>| -> SpatialIdMap<u32> {
                t.get(target).map(|(id, v)| (id, *v)).collect()
            };
            prop_assert_eq!(clip(&within), clip(&expected), "({:?})", (f, x, y));
        }
    }
}
