#[cfg(test)]
mod tests {
    use super::super::arb_random_set_case;
    use crate::SpatialIdSet;
    use proptest::prelude::*;
    use rand::seq::SliceRandom;
    use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};

    fn insert_all(set: &mut SpatialIdSet, inserts: &[super::super::RandomSetInsert]) {
        for insert in inserts {
            match insert {
                super::super::RandomSetInsert::Single(single_id) => set.insert(*single_id),
                super::super::RandomSetInsert::Range(range_id) => set.insert(*range_id),
            }
        }
    }

    /// 同じ要素を異なる固定順で挿入しても 2 つの Set が等しいことを検証する。
    #[test]
    fn equal_even_if_insert_order_differs_fixed_case() {
        let mut lhs = SpatialIdSet::new();
        lhs.insert(crate::SingleId::new(4, 3, 2, 1).unwrap());
        lhs.insert(crate::SingleId::new(4, 3, 2, 2).unwrap());
        lhs.insert(crate::RangeId::new(3, [0, 1], [1, 2], [3, 3]).unwrap());

        let mut rhs = SpatialIdSet::new();
        rhs.insert(crate::RangeId::new(3, [0, 1], [1, 2], [3, 3]).unwrap());
        rhs.insert(crate::SingleId::new(4, 3, 2, 2).unwrap());
        rhs.insert(crate::SingleId::new(4, 3, 2, 1).unwrap());

        assert!(lhs == rhs);
    }

    proptest! {
        #[test]
        /// 同じ要素をシード付きランダム順で挿入しても Set の等価性が保たれることをプロパティテストで検証する。
        fn equal_even_if_insert_order_is_random_proptest(
            case in arb_random_set_case(),
            shuffle_seed in any::<u64>(),
        ) {
            let mut lhs = SpatialIdSet::new();
            insert_all(&mut lhs, &case.inserts);

            let mut shuffled = case.inserts.clone();
            let mut rng = ChaCha8Rng::seed_from_u64(shuffle_seed);
            shuffled.shuffle(&mut rng);

            let mut rhs = SpatialIdSet::new();
            insert_all(&mut rhs, &shuffled);

            prop_assert!(lhs == rhs, "seed={}\n{}", shuffle_seed, case.debug_summary());
        }
    }
}
