use crate::query::merge_policy::Sum;
use crate::{SingleId, Source, SpatialIdMap};

fn time_segment(x: u32, v: i32) -> (SingleId, i32) {
    (SingleId::new(20, 0, x, 0).unwrap(), v)
}

#[test]
fn merge_resolves_overlap_and_keeps_non_overlapping() {
    let mut a = SpatialIdMap::new();
    let mut b = SpatialIdMap::new();

    let (only_a, av) = time_segment(100, 7);
    let (both, av2) = time_segment(101, 3);
    let (_, bv2) = time_segment(101, 4);
    let (only_b, bv) = time_segment(102, 5);

    a.insert(only_a, av);
    a.insert(both, av2);
    b.insert(both, bv2);
    b.insert(only_b, bv);

    let out = a.query().merge(b.query(), Sum).get(..).into_map().unwrap();

    assert_eq!(out.get(only_a).next().unwrap().1, &7);
    assert_eq!(out.get(both).next().unwrap().1, &7);
    assert_eq!(out.get(only_b).next().unwrap().1, &5);

    let neither = time_segment(103, 0).0;
    assert!(out.get(neither).next().is_none());
}

#[test]
fn merge_of_two_empty_tables_is_noop() {
    let a = SpatialIdMap::<i32>::new();
    let b = SpatialIdMap::<i32>::new();

    let out = a.query().merge(b.query(), Sum).get(..).into_map().unwrap();

    assert_eq!(out.iter().count(), 0);
}
