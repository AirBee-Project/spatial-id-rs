//! SpatialIdSet ベンチマーク共通のパターン生成関数。
//!
//! systematic_crud / systematic_setops / memory_usage から
//! `#[path = "patterns.rs"] mod patterns;` で共有される。

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use spatial_id::SingleId;
use spatial_id::collection::mem::SpatialIdSet;

pub const Z: u8 = 20;
pub const COUNTS: &[usize] = &[500, 2_000, 8_000];

pub type PatternFn = fn(u8, usize) -> Vec<SingleId>;

pub const PATTERNS: &[(&str, PatternFn)] = &[
    ("Dense", dense_cluster),
    ("Sparse", sparse_random),
    ("Linear", linear_path),
    ("MultiCluster", multi_cluster),
    ("Layered", layered_floors),
    ("Checkerboard", checkerboard),
];

// ────────────────────────────────────────────────────────────────
// パターン生成関数
// ────────────────────────────────────────────────────────────────

/// 連続した 3-D ブロック。FlexTree のノードマージが最大限発生するベストケース。
pub fn dense_cluster(z: u8, count: usize) -> Vec<SingleId> {
    let side = libm::ceil(libm::cbrt(count as f64)) as u32;
    let mut ids = Vec::with_capacity(count);
    'outer: for f in 0..side as i32 {
        for x in 0..side {
            for y in 0..side {
                if ids.len() >= count {
                    break 'outer;
                }
                ids.push(SingleId::new(z, f, x, y).unwrap());
            }
        }
    }
    ids
}

/// 全アドレス空間にランダム散在。空間的局所性がほぼゼロ。
pub fn sparse_random(z: u8, count: usize) -> Vec<SingleId> {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let max_xy = (1u64 << z).saturating_sub(1) as u32;
    (0..count)
        .map(|_| {
            let x = rng.random_range(0..=max_xy);
            let y = rng.random_range(0..=max_xy);
            let f = rng.random_range(-500..500i32);
            SingleId::new(z, f, x, y).unwrap()
        })
        .collect()
}

/// 対角線上の SingleId 列。道路・コリドーをモデル化。
pub fn linear_path(z: u8, count: usize) -> Vec<SingleId> {
    (0..count)
        .map(|i| SingleId::new(z, 0, i as u32, i as u32).unwrap())
        .collect()
}

/// 4×2 グリッド上の 8 クラスター。クラスター間は大きな隙間で分離。
pub fn multi_cluster(z: u8, count: usize) -> Vec<SingleId> {
    const NUM: usize = 8;
    let per = count / NUM;
    let side = libm::ceil(libm::cbrt(per as f64)) as u32;
    let gap = side * 3;
    let mut ids = Vec::with_capacity(count);
    'outer: for c in 0..NUM {
        let base_x = (c as u32 % 4) * (side + gap);
        let base_y = (c as u32 / 4) * (side + gap);
        for f in 0..side as i32 {
            for x in base_x..base_x + side {
                for y in base_y..base_y + side {
                    if ids.len() >= count {
                        break 'outer;
                    }
                    ids.push(SingleId::new(z, f, x, y).unwrap());
                }
            }
        }
    }
    ids
}

/// F 方向に SingleId 4 個間隔の水平スラブ 10 枚。多層ビルのフロアをモデル化。
pub fn layered_floors(z: u8, count: usize) -> Vec<SingleId> {
    const FLOORS: usize = 10;
    const GAP: i32 = 4;
    let per = count / FLOORS;
    let side = libm::ceil(libm::sqrt(per as f64)) as u32;
    let mut ids = Vec::with_capacity(count);
    'outer: for floor in 0..FLOORS {
        let f = floor as i32 * (1 + GAP);
        for x in 0..side {
            for y in 0..side {
                if ids.len() >= count {
                    break 'outer;
                }
                ids.push(SingleId::new(z, f, x, y).unwrap());
            }
        }
    }
    ids
}

/// 3-D 市松模様。ノードマージが一切発生しないワーストケース。
pub fn checkerboard(z: u8, count: usize) -> Vec<SingleId> {
    let side = libm::ceil(libm::cbrt((count * 2) as f64)) as u32;
    let mut ids = Vec::with_capacity(count);
    'outer: for f in 0..side as i32 {
        for x in 0..side {
            for y in 0..side {
                if (f as u32 + x + y).is_multiple_of(2) {
                    if ids.len() >= count {
                        break 'outer;
                    }
                    ids.push(SingleId::new(z, f, x, y).unwrap());
                }
            }
        }
    }
    ids
}

// ────────────────────────────────────────────────────────────────
// ヘルパー
// ────────────────────────────────────────────────────────────────

pub fn build_set(ids: &[SingleId]) -> SpatialIdSet {
    let mut set = SpatialIdSet::new();
    for id in ids {
        set.insert(*id);
    }
    set
}

/// 50% 要素重複を保証する 2 セットを生成する。
///
/// 2N 個の ID を生成し A = ids[0..N]、B = ids[N/2..3N/2] に分割する。
/// 空間的に順序付けられたパターンでは境界領域で実際の空間重複が発生する。
pub fn make_pair(pattern_fn: PatternFn, z: u8, count: usize) -> (SpatialIdSet, SpatialIdSet) {
    let all = pattern_fn(z, count.saturating_mul(2));
    let n = all.len();
    let end_a = n.min(count);
    let start_b = end_a / 2;
    let end_b = (start_b + count).min(n);
    (build_set(&all[..end_a]), build_set(&all[start_b..end_b]))
}
