use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use spatial_id::geometry::{Coordinate, CoverSingleIds, Polygon, Solid};
use std::hint::black_box;

fn bench_solid(c: &mut Criterion) {
    let base_lat = 35.681000;
    let base_lon = 139.767000;
    let base_alt = 0.0;
    let top_alt = 50.0;

    let mut zoom_group = c.benchmark_group("Solid_ZoomLevel_Scaling");
    for z in 15..=20 {
        // ズームレベル20以上は重いので20まで
        zoom_group.bench_with_input(BenchmarkId::from_parameter(z), &z, |b, &z| {
            b.iter(|| {
                let p1_b = Coordinate::new(base_lat, base_lon, base_alt).unwrap();
                let p2_b = Coordinate::new(base_lat + 0.001, base_lon, base_alt).unwrap();
                let p3_b = Coordinate::new(base_lat + 0.001, base_lon + 0.001, base_alt).unwrap();
                let p4_b = Coordinate::new(base_lat, base_lon + 0.001, base_alt).unwrap();

                let p1_t = Coordinate::new(base_lat, base_lon, top_alt).unwrap();
                let p2_t = Coordinate::new(base_lat + 0.001, base_lon, top_alt).unwrap();
                let p3_t = Coordinate::new(base_lat + 0.001, base_lon + 0.001, top_alt).unwrap();
                let p4_t = Coordinate::new(base_lat, base_lon + 0.001, top_alt).unwrap();

                let solid = Solid::new(vec![
                    Polygon::new(vec![p1_b, p4_b, p3_b, p2_b]), // Bottom
                    Polygon::new(vec![p1_t, p2_t, p3_t, p4_t]), // Top
                    Polygon::new(vec![p1_b, p2_b, p2_t, p1_t]), // Side
                    Polygon::new(vec![p2_b, p3_b, p3_t, p2_t]), // Side
                    Polygon::new(vec![p3_b, p4_b, p4_t, p3_t]), // Side
                    Polygon::new(vec![p4_b, p1_b, p1_t, p4_t]), // Side
                ])
                .unwrap();

                let iter = solid
                    .cover_single_ids(z as u8)
                    .expect("Failed to get iterator");
                black_box(iter.count());
            });
        });
    }
    zoom_group.finish();

    // 2. Size Scaling (Fixed Zoom: z=18, as z=20 is too heavy for very large solids)
    let mut size_group = c.benchmark_group("Solid_Size_Scaling");
    let scales = [0.0001, 0.0005, 0.001, 0.005, 0.01];
    for &scale in &scales {
        size_group.bench_with_input(BenchmarkId::from_parameter(scale), &scale, |b, &scale| {
            b.iter(|| {
                let p1_b = Coordinate::new(base_lat, base_lon, base_alt).unwrap();
                let p2_b = Coordinate::new(base_lat + scale, base_lon, base_alt).unwrap();
                let p3_b = Coordinate::new(base_lat + scale, base_lon + scale, base_alt).unwrap();
                let p4_b = Coordinate::new(base_lat, base_lon + scale, base_alt).unwrap();

                let p1_t = Coordinate::new(base_lat, base_lon, top_alt).unwrap();
                let p2_t = Coordinate::new(base_lat + scale, base_lon, top_alt).unwrap();
                let p3_t = Coordinate::new(base_lat + scale, base_lon + scale, top_alt).unwrap();
                let p4_t = Coordinate::new(base_lat, base_lon + scale, top_alt).unwrap();

                let solid = Solid::new(vec![
                    Polygon::new(vec![p1_b, p4_b, p3_b, p2_b]),
                    Polygon::new(vec![p1_t, p2_t, p3_t, p4_t]),
                    Polygon::new(vec![p1_b, p2_b, p2_t, p1_t]),
                    Polygon::new(vec![p2_b, p3_b, p3_t, p2_t]),
                    Polygon::new(vec![p3_b, p4_b, p4_t, p3_t]),
                    Polygon::new(vec![p4_b, p1_b, p1_t, p4_t]),
                ])
                .unwrap();

                let iter = solid
                    .cover_single_ids(18) // 18 is manageable even at 0.01 degree scale
                    .expect("Failed to get iterator");
                black_box(iter.count());
            });
        });
    }
    size_group.finish();
}

criterion_group!(benches, bench_solid);
criterion_main!(benches);
