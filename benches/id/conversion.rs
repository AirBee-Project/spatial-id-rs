use criterion::{Criterion, criterion_group, criterion_main};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use spatial_id::{FlexId, RangeId, SingleId, collection::SpatialIdSet, time::Interval};
use std::hint::black_box;

fn bench_id_conversions(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let num_ids = 1_000;

    let mut single_ids = Vec::with_capacity(num_ids);
    let mut single_ids_1800 = Vec::with_capacity(num_ids);
    let mut flex_ids = Vec::with_capacity(num_ids);
    let mut range_ids_medium = Vec::with_capacity(100);
    let mut range_ids_small = Vec::with_capacity(100);

    for i in 0..num_ids {
        let z = 20u8;
        let max_xy = (1u32 << z) - 1;
        let f = rng.random_range(-512..512i32);
        let x = rng.random_range(0..=max_xy);
        let y = rng.random_range(0..=max_xy);

        let single = SingleId::new(z, f, x, y)
            .unwrap()
            .time_with(1024u64, (i as u64) + 1)
            .unwrap();
        single_ids.push(single);

        let single_1800 = SingleId::new(z, f, x, y)
            .unwrap()
            .time_with(1800u64, 800_000 + (i as u64))
            .unwrap();
        single_ids_1800.push(single_1800);

        let fz = rng.random_range(12..=20u8);
        let xz = rng.random_range(12..=20u8);
        let yz = rng.random_range(12..=20u8);
        let tz = rng.random_range(15..=30u8);
        let flex = FlexId::new(fz, f >> (20 - fz), xz, x >> (20 - xz), yz, y >> (20 - yz))
            .unwrap()
            .time_with_segment(tz, (i as u64) & ((1u64 << tz) - 1))
            .unwrap();
        flex_ids.push(flex);
    }

    for i in 0..100 {
        let z = 18u8;
        let f0 = rng.random_range(-100..100i32);
        let x0 = rng.random_range(100..10_000u32);
        let y0 = rng.random_range(100..10_000u32);

        let r_med = RangeId::new(z, [f0, f0 + 15], [x0, x0 + 31], [y0, y0 + 31])
            .unwrap()
            .time_with(1024u64, (i as u64) + 1)
            .unwrap();
        range_ids_medium.push(r_med);

        let r_small = RangeId::new(z, [f0, f0 + 3], [x0, x0 + 3], [y0, y0 + 3])
            .unwrap()
            .time_with(Interval::HOUR, [i as u64, (i as u64) + 1])
            .unwrap();
        range_ids_small.push(r_small);
    }

    let mut temporal_set = SpatialIdSet::new();
    for id in &single_ids_1800[..200] {
        temporal_set.insert(*id);
    }

    let mut group = c.benchmark_group("Id_Conversion");

    group.bench_function("FlexId_to_RangeId", |b| {
        b.iter(|| {
            for flex in &flex_ids {
                black_box(RangeId::from(black_box(flex)));
            }
        });
    });

    group.bench_function("SingleId_to_RangeId", |b| {
        b.iter(|| {
            for single in &single_ids {
                black_box(RangeId::from(black_box(single)));
            }
        });
    });

    group.bench_function("SingleId_to_FlexIds_Pow2", |b| {
        b.iter(|| {
            for &single in &single_ids {
                for flex in black_box(single).flex_ids() {
                    black_box(flex);
                }
            }
        });
    });

    group.bench_function("SingleId_to_FlexIds_1800s", |b| {
        b.iter(|| {
            for &single in &single_ids_1800 {
                for flex in black_box(single).flex_ids() {
                    black_box(flex);
                }
            }
        });
    });

    group.bench_function("RangeId_to_FlexIds", |b| {
        b.iter(|| {
            for &range in &range_ids_medium {
                for flex in black_box(range).flex_ids() {
                    black_box(flex);
                }
            }
        });
    });

    group.bench_function("RangeId_to_SingleIds", |b| {
        b.iter(|| {
            for &range in &range_ids_small {
                for single in black_box(range).single_ids() {
                    black_box(single);
                }
            }
        });
    });

    group.bench_function("SingleId_Siblings", |b| {
        b.iter(|| {
            for single in &single_ids {
                black_box(black_box(single).spatial_siblings());
            }
        });
    });

    group.bench_function("SingleId_Children_ZPlus2", |b| {
        b.iter(|| {
            for single in &single_ids[..200] {
                for child in black_box(single).spatial_children_at_zoom(22).unwrap() {
                    black_box(child);
                }
            }
        });
    });

    group.bench_function("FlexId_Parent_At_Zoom", |b| {
        b.iter(|| {
            for flex in &flex_ids {
                black_box(black_box(flex).spatial_parent_at_zoom(14).unwrap());
            }
        });
    });

    group.bench_function("RangeId_Children_And_Parent_At_Zoom", |b| {
        b.iter(|| {
            for range in &range_ids_medium {
                let c = black_box(range).spatial_children_at_zoom(22).unwrap();
                let p = black_box(&c).spatial_parent_at_zoom(14).unwrap();
                black_box(p);
            }
        });
    });

    group.bench_function("Reconstructed_Time_Ranges", |b| {
        b.iter(|| {
            for range in temporal_set.reconstructed_time_ranges(None) {
                black_box(range);
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_id_conversions);
criterion_main!(benches);
