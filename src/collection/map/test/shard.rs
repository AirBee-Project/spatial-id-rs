#![cfg(test)]

use crate::{FlexId, Interval, SingleId, SpatialIdMap};

/// 分割した2つの中身を合わせると元のマップに戻り、それぞれが自分の領域に収まること。
#[test]
fn split_shard_partitions_the_map_into_its_two_halves() {
    let mut map = SpatialIdMap::new();
    map.insert(SingleId::new(4, 3, 1, 2).unwrap(), 1);
    map.insert(SingleId::new(4, 13, 9, 12).unwrap(), 2);
    map.insert(FlexId::new(1, 0, 0, 0, 0, 0).unwrap(), 3);

    let region = FlexId::new(0, 0, 0, 0, 0, 0).unwrap();
    let ((lower_region, lower), (upper_region, upper)) = map.split_shard(region).unwrap();

    for (part_region, part) in [(lower_region, &lower), (upper_region, &upper)] {
        assert!(region.contains(&part_region));
        assert!(part.iter().all(|(id, _)| part_region.contains(&id)));
    }
    assert!(!lower_region.intersects(&upper_region));

    let rejoined: SpatialIdMap<i32> = lower.into_iter().chain(upper).collect();
    assert_eq!(rejoined, map);
}

/// 全ての値が全時間なら、T 軸では割らない。
#[test]
fn split_shard_does_not_split_time_when_every_value_is_whole_time() {
    let mut map = SpatialIdMap::new();
    map.insert(SingleId::new(4, 3, 1, 2).unwrap(), 1);

    let mut region = FlexId::new(0, 0, 0, 0, 0, 0).unwrap();
    for _ in 0..12 {
        let ((lower, _), _) = map.split_shard(region).unwrap();
        assert_eq!(lower.t_zoomlevel(), 0);
        region = lower;
    }
}

/// 時間方向に区切られた値があれば、T 軸でも割る。
#[test]
fn split_shard_splits_time_when_values_are_time_segmented() {
    let mut map = SpatialIdMap::new();
    let id = SingleId::new(4, 3, 1, 2).unwrap();
    map.insert(id.with_time(Interval::new(1).unwrap(), 0).unwrap(), 1);
    map.insert(id.with_time(Interval::new(1).unwrap(), 1).unwrap(), 2);

    let mut region = FlexId::new(0, 0, 0, 0, 0, 0).unwrap();
    let mut split_time = false;
    for _ in 0..8 {
        let ((lower, _), _) = map.split_shard(region).unwrap();
        split_time |= lower.t_zoomlevel() > region.t_zoomlevel();
        region = lower;
    }
    assert!(split_time);
}
