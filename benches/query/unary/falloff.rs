use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use spatial_id::query::FalloffPattern;
use spatial_id::query::Source;
use spatial_id::query::merge_policy::Max;

#[path = "../utils.rs"]
mod utils;

fn bench_falloff(c: &mut Criterion) {
    let mut group = c.benchmark_group("Unary/Falloff");
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(1));

    let table = utils::get_full_data();
    let distances = [1, 5, 10, 15];

    // 個別の次元で行う関数 (X)
    for &dist in &distances {
        group.bench_with_input(BenchmarkId::new("falloff_x", dist), &dist, |b, &d| {
            b.iter_batched(
                || table.clone(),
                |t| {
                    t.query()
                        .falloff_x(24, -d..=d, FalloffPattern::Linear, Max)
                        .get(..)
                        .into_map()
                        .unwrap()
                },
                BatchSize::SmallInput,
            );
        });
    }

    // 個別の次元で行う関数 (Y)
    for &dist in &distances {
        group.bench_with_input(BenchmarkId::new("falloff_y", dist), &dist, |b, &d| {
            b.iter_batched(
                || table.clone(),
                |t| {
                    t.query()
                        .falloff_y(24, -d..=d, FalloffPattern::Linear, Max)
                        .get(..)
                        .into_map()
                        .unwrap()
                },
                BatchSize::SmallInput,
            );
        });
    }

    // 個別の次元で行う関数 (F)
    for &dist in &distances {
        group.bench_with_input(BenchmarkId::new("falloff_f", dist), &dist, |b, &d| {
            b.iter_batched(
                || table.clone(),
                |t| {
                    t.query()
                        .falloff_f(24, -d..=d, FalloffPattern::Linear, Max)
                        .get(..)
                        .into_map()
                        .unwrap()
                },
                BatchSize::SmallInput,
            );
        });
    }

    // 全ての次元で行う関数
    for &dist in &distances {
        group.bench_with_input(BenchmarkId::new("falloff_all", dist), &dist, |b, &d| {
            b.iter_batched(
                || table.clone(),
                |t| {
                    t.query()
                        .falloff_x(24, -d..=d, FalloffPattern::Linear, Max)
                        .falloff_y(24, -d..=d, FalloffPattern::Linear, Max)
                        .falloff_f(24, -d..=d, FalloffPattern::Linear, Max)
                        .get(..)
                        .into_map()
                        .unwrap()
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_falloff);
criterion_main!(benches);
