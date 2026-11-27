//! SpatialIdSet の構築ベンチマーク: 逐次 insert vs 並列バルク構築。
//!
//! 各空間パターン（Dense / Sparse / Linear / …）について、同じ入力を以下の2つの方法で構築し比較する。
//! - `sequential`: `insert` を1件ずつ
//! - `parallel`  : `par_iter().collect()`（[`FromParallelIterator`]）
//!
//! 並列構築は空間ソート→チャンク部分木化→union簡約で行い、要素数が多いほど高速化の効果が大きくなる。
//! パターンは systematic_* ベンチと共通の `patterns.rs` を使う。

#[path = "patterns.rs"]
#[allow(dead_code)]
mod patterns;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use patterns::{PATTERNS, Z, build_set};
use rayon::prelude::*;
use spatial_id::collection::mem::SpatialIdSet;
use std::hint::black_box;

const COUNTS: &[usize] = &[10_000, 100_000, 500_000];

fn bench_build(c: &mut Criterion) {
    for &(name, pattern_fn) in PATTERNS {
        let mut group = c.benchmark_group(format!("SpatialIdSet/Build/{name}"));
        group.sample_size(10);

        for &count in COUNTS {
            let ids = pattern_fn(Z, count);

            group.bench_with_input(BenchmarkId::new("sequential", count), &ids, |b, ids| {
                b.iter(|| black_box(build_set(ids)));
            });

            group.bench_with_input(BenchmarkId::new("parallel", count), &ids, |b, ids| {
                b.iter(|| black_box(ids.par_iter().cloned().collect::<SpatialIdSet>()));
            });
        }

        group.finish();
    }
}

criterion_group!(benches, bench_build);
criterion_main!(benches);
