//! SpatialIdSet::insert のヒープメモリ使用量ベンチマーク。
//!
//! カスタムアロケータで追跡窓を制御し、パターン生成を除いた
//! Insert 操作のみのヒープ使用量を計測する。
//!
//! 出力は Markdown テーブル形式。GitHub Actions の Job Summary に
//! リダイレクトするとそのまま表示される：
//!   cargo bench --bench spatial_id_set_memory >> $GITHUB_STEP_SUMMARY
//!
//! ローカル実行:
//!   cargo bench --bench spatial_id_set_memory

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};

#[path = "patterns.rs"]
#[allow(dead_code)]
mod patterns;

use patterns::{COUNTS, PATTERNS, Z};

use spatial_id::SingleId;
use spatial_id::collection::mem::SpatialIdSet;

// ────────────────────────────────────────────────────────────────
// カスタムアロケータ
//
// TRACKING フラグが true の間だけヒープ増減を記録する。
// フラグ外（パターン生成の Vec など）はカウントしない。
// ────────────────────────────────────────────────────────────────

static TRACKING: AtomicBool = AtomicBool::new(false);
static CURRENT: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && TRACKING.load(Ordering::Relaxed) {
            let after = CURRENT.fetch_add(layout.size() as isize, Ordering::Relaxed)
                + layout.size() as isize;
            if after > 0 {
                let after_u = after as usize;
                let mut peak = PEAK.load(Ordering::Relaxed);
                while after_u > peak {
                    match PEAK.compare_exchange_weak(
                        peak,
                        after_u,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(p) => peak = p,
                    }
                }
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        if TRACKING.load(Ordering::Relaxed) {
            CURRENT.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn start_tracking() {
    CURRENT.store(0, Ordering::SeqCst);
    PEAK.store(0, Ordering::SeqCst);
    TRACKING.store(true, Ordering::SeqCst);
}

/// 追跡を終了して (最終使用量, ピーク使用量) のバイト数を返す。
fn stop_tracking() -> (usize, usize) {
    TRACKING.store(false, Ordering::SeqCst);
    let current = CURRENT.load(Ordering::SeqCst).max(0) as usize;
    let peak = PEAK.load(Ordering::SeqCst);
    (current, peak)
}

// ────────────────────────────────────────────────────────────────
// 計測ロジック
// ────────────────────────────────────────────────────────────────

struct MemResult {
    final_bytes: usize,
    peak_bytes: usize,
}

/// IDs を SpatialIdSet に Insert し、追跡窓内のメモリ統計を返す。
/// IDs 生成は計測対象外。
fn measure_insert(ids: &[SingleId]) -> MemResult {
    start_tracking();
    let mut set = SpatialIdSet::new();
    for id in ids {
        set.insert(*id);
    }
    let (final_bytes, peak_bytes) = stop_tracking();
    drop(set); // 追跡停止後のため計測に影響しない
    MemResult {
        final_bytes,
        peak_bytes,
    }
}

fn fmt_bytes(b: usize) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
    } else if b >= 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{} B", b)
    }
}

// ────────────────────────────────────────────────────────────────
// エントリポイント
// ────────────────────────────────────────────────────────────────

fn main() {
    println!("## SpatialIdSet Insert ヒープメモリ使用量 (z={Z})");
    println!();
    println!("| パターン | ID数 | 最終使用量 | ピーク使用量 | bytes/ID |");
    println!("|:---|---:|---:|---:|---:|");

    for &(name, pattern_fn) in PATTERNS {
        for &count in COUNTS {
            // パターン生成は計測窓の外
            let ids = pattern_fn(Z, count);
            let actual_count = ids.len();

            // ウォームアップ（アロケータ内部構造を安定させる）
            let _ = measure_insert(&ids);

            // 本計測 3 回の中央値
            let mut results: Vec<MemResult> = (0..3).map(|_| measure_insert(&ids)).collect();
            results.sort_unstable_by_key(|r| r.final_bytes);
            let mid = &results[1];

            let bytes_per_id = if actual_count > 0 {
                mid.final_bytes as f64 / actual_count as f64
            } else {
                0.0
            };

            println!(
                "| {} | {} | {} | {} | {:.1} |",
                name,
                actual_count,
                fmt_bytes(mid.final_bytes),
                fmt_bytes(mid.peak_bytes),
                bytes_per_id,
            );
        }
    }

    println!();
    println!("**最終使用量**: Insert 完了後に SpatialIdSet が占有するヒープ");
    println!("**ピーク使用量**: Insert 中の最大ヒープ（一時ノード込み）");
    println!("**bytes/ID**: 最終使用量 ÷ ID数（小さいほどメモリ効率が高い）");
    println!();
    println!("> Dense はノードマージにより bytes/ID が最小になります。");
    println!("> Checkerboard はマージが発生しないため bytes/ID が最大になります。");
}
