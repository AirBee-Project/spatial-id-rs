use crate::id::helpers::Side;
use crate::{Dimension, FlexId, SpatialIdMap};
use alloc::vec::Vec;
use core::cmp::Ordering;
use proptest::prelude::*;
use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};

fn arb_shallow_entries() -> impl Strategy<Value = Vec<(FlexId, u64)>> {
    prop::collection::vec((FlexId::arb_with_time(0..=4, 0..=3), 0..3u64), 0..=40)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn table_iterates_in_partial_order(entries in arb_shallow_entries()) {
        let table: SpatialIdMap<u64> = entries.into_iter().collect();
        let ids: Vec<FlexId> = table.iter_flex_ids().map(|(id, _)| id).collect();
        for pair in ids.windows(2) {
            prop_assert_eq!(pair[0].partial_cmp(&pair[1]), Some(Ordering::Less));
            prop_assert!(pair[0].zorder_key() < pair[1].zorder_key());
        }
    }

    /// 同じ形状なら、区切り方や入れる順によらず、FlexTreeCore から同じカノニカル列が出てくる。
    #[test]
    fn table_yields_same_canonical_sequence_for_same_shape(entries in arb_shallow_entries()) {
        let table: SpatialIdMap<u64> = entries.into_iter().collect();
        let expected: Vec<(FlexId, u64)> = table.iter_flex_ids().map(|(id, v)| (id, *v)).collect();

        let dimensions = [Dimension::F, Dimension::X, Dimension::Y];
        let mut pieces: Vec<(FlexId, u64)> = Vec::new();
        for (i, &(id, v)) in expected.iter().enumerate() {
            let dimension = dimensions[i % dimensions.len()];
            match (
                id.split_on(dimension, Side::Lower),
                id.split_on(dimension, Side::Upper),
            ) {
                (Some(lower), Some(upper)) => pieces.extend([(upper, v), (lower, v)]),
                _ => pieces.push((id, v)),
            }
        }
        pieces.reverse();
        let mut rebuilt = SpatialIdMap::new();
        for (id, v) in pieces {
            rebuilt.insert(id, v);
        }
        let actual: Vec<(FlexId, u64)> = rebuilt.iter_flex_ids().map(|(id, v)| (id, *v)).collect();
        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn partial_order_is_antisymmetric_and_transitive() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x2545_F491_4F6C_DD1D);
    let ids: Vec<FlexId> = (0..300)
        .map(|_| {
            FlexId::random_within_using(&mut rng, 0..=4).time_random_within_using(&mut rng, 0..=3)
        })
        .collect();
    for a in &ids {
        for b in &ids {
            let ab = a.partial_cmp(b);
            assert_eq!(ab.map(Ordering::reverse), b.partial_cmp(a), "{a:?} {b:?}");
            assert_eq!(ab == Some(Ordering::Equal), a == b, "{a:?} {b:?}");
            if ab != Some(Ordering::Less) {
                continue;
            }
            assert!(!a.intersects(b), "前にある組が重なる: {a:?} {b:?}");
            assert!(a.zorder_key() < b.zorder_key(), "{a:?} {b:?}");
            for c in ids.iter().filter(|c| b < *c) {
                assert!(a < c, "推移律: {a:?} {b:?} {c:?}");
            }
        }
    }
}

#[test]
fn coarser_element_across_finer_split_is_unordered() {
    // a は F を割っていないが、b が F を割るので、木では a が F で 2 つに割られて b を挟む
    let a = FlexId::new(0, 0, 1, 0, 0, 0).unwrap();
    let b = FlexId::new(2, 1, 1, 1, 0, 0).unwrap();
    assert_eq!(a.partial_cmp(&b), None);
    assert_eq!(b.partial_cmp(&a), None);

    let a_lower = FlexId::new(1, 0, 1, 0, 0, 0).unwrap();
    let a_upper = FlexId::new(1, 1, 1, 0, 0, 0).unwrap();
    assert_eq!(a_lower.partial_cmp(&b), Some(Ordering::Less));
    assert_eq!(b.partial_cmp(&a_upper), Some(Ordering::Less));
}

#[test]
fn overlapping_ids_are_unordered() {
    let parent = FlexId::new(1, 0, 1, 0, 1, 0).unwrap();
    let child = FlexId::new(2, 1, 2, 0, 2, 1).unwrap();
    assert_eq!(parent.partial_cmp(&child), None);
    assert_eq!(parent.partial_cmp(&parent), Some(Ordering::Equal));
}

#[test]
fn upper_root_comes_first() {
    let [upper, lower] = FlexId::ROOTS;
    assert_eq!(upper.partial_cmp(&lower), Some(Ordering::Less));
    assert!(upper.zorder_key() < lower.zorder_key());
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "QueryIter の制約違反")]
fn new_panics_on_unordered_input() {
    let [upper, lower] = FlexId::ROOTS;
    crate::query::QueryIter::new([Ok((lower, ())), Ok((upper, ()))].into_iter()).for_each(drop);
}
