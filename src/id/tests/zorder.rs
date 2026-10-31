use crate::id::helpers::Side;
use crate::{Dimension, FlexId, SpatialIdMap};
use alloc::vec::Vec;
use core::cmp::Ordering;

/// 再現可能な疑似乱数（xorshift64）。
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// ズームの浅い FlexId を作る。浅いので互いに重なりやすく、木の中で割られる。
fn random_id(state: &mut u64) -> FlexId {
    let mut zoom = || (next(state) % 5) as u8;
    let (fz, xz, yz) = (zoom(), zoom(), zoom());
    let f = (next(state) % (2 << fz)) as i32 - (1 << fz);
    let x = (next(state) % (1 << xz)) as u32;
    let y = (next(state) % (1 << yz)) as u32;
    let id = FlexId::new(fz, f, xz, x, yz, y).unwrap();
    if cfg!(feature = "temporal_id") && next(state).is_multiple_of(2) {
        let tz = (next(state) % 4) as u8;
        id.with_time_segment(tz, next(state) % (1 << tz)).unwrap()
    } else {
        id
    }
}

#[test]
fn table_iterates_in_partial_order() {
    let mut state = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..200 {
        let table: SpatialIdMap<u64> = (0..40)
            .map(|_| (random_id(&mut state), next(&mut state) % 3))
            .collect();
        let ids: Vec<FlexId> = table.iter().map(|(id, _)| id).collect();
        for pair in ids.windows(2) {
            assert_eq!(pair[0].partial_cmp(&pair[1]), Some(Ordering::Less));
            assert!(pair[0].zorder_key() < pair[1].zorder_key());
        }
    }
}

#[test]
fn partial_order_is_antisymmetric_and_transitive() {
    let mut state = 0x2545_F491_4F6C_DD1D;
    let ids: Vec<FlexId> = (0..300).map(|_| random_id(&mut state)).collect();
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

/// 同じ形状なら、区切り方や入れる順によらず、FlexTreeCore から同じカノニカル列が出てくる。
#[test]
fn table_yields_same_canonical_sequence_for_same_shape() {
    let mut state = 0xD1B5_4A32_D192_ED03;
    for _ in 0..50 {
        let table: SpatialIdMap<u64> = (0..40)
            .map(|_| (random_id(&mut state), next(&mut state) % 3))
            .collect();
        let expected: Vec<(FlexId, u64)> = table.iter().map(|(id, v)| (id, *v)).collect();

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
        let actual: Vec<(FlexId, u64)> = rebuilt.iter().map(|(id, v)| (id, *v)).collect();
        assert_eq!(actual, expected);
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
