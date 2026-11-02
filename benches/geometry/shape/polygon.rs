use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use spatial_id::geometry::{Coordinate, CoverSingleIds, Polygon};
use std::hint::black_box;

fn bench_polygon(c: &mut Criterion) {
    let base_lat = 35.681000;
    let base_lon = 139.767000;
    let base_alt = 0.0;

    let mut zoom_group = c.benchmark_group("Polygon_ZoomLevel_Scaling");
    for z in 15..=22 {
        zoom_group.bench_with_input(BenchmarkId::from_parameter(z), &z, |b, &z| {
            b.iter(|| {
                let p1 = Coordinate::new(base_lat, base_lon, base_alt).unwrap();
                let p2 = Coordinate::new(base_lat + 0.001, base_lon, base_alt).unwrap();
                let p3 = Coordinate::new(base_lat + 0.001, base_lon + 0.001, base_alt).unwrap();
                let p4 = Coordinate::new(base_lat, base_lon + 0.001, base_alt).unwrap();
                let polygon = Polygon::new(vec![p1, p2, p3, p4]);
                let iter = polygon
                    .cover_single_ids(z as u8)
                    .expect("Failed to get iterator");
                black_box(iter.count());
            });
        });
    }
    zoom_group.finish();

    // 2. Size Scaling (Fixed Zoom: z=20)
    let mut size_group = c.benchmark_group("Polygon_Size_Scaling");
    let scales = [0.0001, 0.0005, 0.001, 0.005, 0.01];
    for &scale in &scales {
        size_group.bench_with_input(BenchmarkId::from_parameter(scale), &scale, |b, &scale| {
            b.iter(|| {
                let p1 = Coordinate::new(base_lat, base_lon, base_alt).unwrap();
                let p2 = Coordinate::new(base_lat + scale, base_lon, base_alt).unwrap();
                let p3 = Coordinate::new(base_lat + scale, base_lon + scale, base_alt).unwrap();
                let p4 = Coordinate::new(base_lat, base_lon + scale, base_alt).unwrap();
                let polygon = Polygon::new(vec![p1, p2, p3, p4]);
                let iter = polygon
                    .cover_single_ids(20)
                    .expect("Failed to get iterator");
                black_box(iter.count());
            });
        });
    }
    size_group.finish();
}

criterion_group!(benches, bench_polygon);
criterion_main!(benches);
