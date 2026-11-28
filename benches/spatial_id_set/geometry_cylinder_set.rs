//! SpatialIdSet に対する Cylinder 形状のベンチマーク。
//!
//! Cylinder::cover_single_ids は内部で rough_solid()（100分割ポリゴン構築）を
//! 呼ぶため他の形状より反復あたりのコストが高く、単独ジョブに分離している。

use criterion::{Criterion, criterion_group, criterion_main};
use spatial_id::SingleId;
use spatial_id::collection::mem::SpatialIdSet;
use spatial_id::geometry::{Coordinate, CoverSingleIds, Cylinder};
use std::hint::black_box;

const BASE_LAT: f64 = 35.681_382;
const BASE_LON: f64 = 139.766_084;

fn coord(lat: f64, lon: f64, alt: f64) -> Coordinate {
    Coordinate::new(lat, lon, alt).unwrap()
}

fn build_set(ids: impl IntoIterator<Item = SingleId>) -> SpatialIdSet {
    let mut set = SpatialIdSet::new();
    for id in ids {
        set.insert(id);
    }
    set
}

fn bench_cylinder(c: &mut Criterion) {
    let base = coord(BASE_LAT, BASE_LON, 0.0);

    // (高さ[m], 半径[m], ズームレベル, ラベル)
    let cases: &[(f64, f64, u8, &str)] = &[
        (200.0, 5.0, 18, "tower_h200r5_z18"),
        (200.0, 5.0, 20, "tower_h200r5_z20"),
        (50.0, 20.0, 18, "pillar_h50r20_z18"),
        (50.0, 20.0, 20, "pillar_h50r20_z20"),
        (20.0, 50.0, 18, "stadium_h20r50_z18"),
        (20.0, 50.0, 20, "stadium_h20r50_z20"),
    ];

    let mut group = c.benchmark_group("SpatialIdSet/Geometry/Cylinder");
    for &(h, r, z, label) in cases {
        let top = coord(BASE_LAT, BASE_LON, h);
        group.bench_function(label, |b| {
            b.iter(|| {
                let cyl = Cylinder::new(base, top, r).unwrap();
                black_box(build_set(cyl.cover_single_ids(z).unwrap()))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_cylinder);
criterion_main!(benches);
