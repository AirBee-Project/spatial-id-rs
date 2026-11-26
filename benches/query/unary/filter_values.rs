use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use spatial_id::query::Source;

#[path = "../utils.rs"]
mod utils;

fn bench_filter_values(c: &mut Criterion) {
    let mut group = c.benchmark_group("Unary/FilterValues");
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(1));

    let table = utils::get_full_data();
    let thresholds = [1, 2, 3, 4, 5];

    for &threshold in &thresholds {
        group.bench_with_input(
            BenchmarkId::new("greater_than_or_equal", threshold),
            &threshold,
            |b, &t_val| {
                b.iter_batched(
                    || table.clone(),
                    |t| {
                        t.query()
                            .filter_values(move |v| *v >= t_val)
                            .get(..)
                            .into_map()
                            .unwrap()
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_filter_values);
criterion_main!(benches);
