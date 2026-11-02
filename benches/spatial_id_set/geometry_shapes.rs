//! ジオメトリ駆動の SpatialIdSet ベンチマーク。
//!
//! 全ベンチマークは 3D 形状から SpatialIdSet へのパイプライン全体を計測する：
//!   shape → cover_single_ids(z) → SpatialIdSet::insert
//!
//! 形状とシナリオは都市の一般的なオブジェクトを模している：
//!
//! | グループ       | 形状      | 都市的なアナロジー                  |
//! |----------------|-----------|-------------------------------------|
//! | Sphere         | 球体      | 給水タンクのドーム、球形貯水槽      |
//! | Building       | 直方体    | オフィスビル、高層ビル              |
//! | UrbanBlock     | 6 × 直方体| 都市ブロック（3列 × 2行グリッド）  |
//!
//! Cylinder は rough_solid() のコストが高いため spatial_id_set_geometry_cylinder に分離。
//! 集合演算ベンチマークは境界領域が一部重なる形状ペアを使用する。

use criterion::{Criterion, criterion_group, criterion_main};
use spatial_id::SingleId;
use spatial_id::collection::SpatialIdSet;
use spatial_id::geometry::{Coordinate, CoverSingleIds, Polygon, Solid, Sphere};
use std::hint::black_box;

// ────────────────────────────────────────────────────────────────
// 地理的アンカー：東京・丸の内
// ────────────────────────────────────────────────────────────────

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

// ────────────────────────────────────────────────────────────────
// 直方体ビルヘルパー
//
// コーナー座標から水密な 6 面 Solid を構築する。
// 辺の検証（略称: b=底面, t=上面, sw/se/ne/nw=コーナー）:
//   底面 [bsw,bnw,bne,bse] と上面 [tsw,tse,tne,tnw] は辺を共有しない。
//   4 本の垂直辺はそれぞれ隣接する 2 側面で共有される。
//   底面・上面の水平辺はそれぞれ底面または上面 + 1 側面で共有される。
//   合計: 12 本の一意な辺、それぞれの出現回数 = 2 → 水密条件を満たす ✓
// ────────────────────────────────────────────────────────────────

fn box_building(
    sw_lat: f64,
    sw_lon: f64,
    ne_lat: f64,
    ne_lon: f64,
    alt_low: f64,
    alt_high: f64,
) -> Solid {
    let bsw = coord(sw_lat, sw_lon, alt_low);
    let bse = coord(sw_lat, ne_lon, alt_low);
    let bne = coord(ne_lat, ne_lon, alt_low);
    let bnw = coord(ne_lat, sw_lon, alt_low);
    let tsw = coord(sw_lat, sw_lon, alt_high);
    let tse = coord(sw_lat, ne_lon, alt_high);
    let tne = coord(ne_lat, ne_lon, alt_high);
    let tnw = coord(ne_lat, sw_lon, alt_high);
    Solid::new(vec![
        Polygon::new(vec![bsw, bnw, bne, bse]), // 底面
        Polygon::new(vec![tsw, tse, tne, tnw]), // 上面
        Polygon::new(vec![bsw, bse, tse, tsw]), // 南面
        Polygon::new(vec![bse, bne, tne, tse]), // 東面
        Polygon::new(vec![bne, bnw, tnw, tne]), // 北面
        Polygon::new(vec![bnw, bsw, tsw, tnw]), // 西面
    ])
    .expect("直方体は水密である")
}

// ────────────────────────────────────────────────────────────────
// 球体ベンチマーク
// ────────────────────────────────────────────────────────────────

fn bench_sphere(c: &mut Criterion) {
    let center = coord(BASE_LAT, BASE_LON, 0.0);

    // (半径[m], ズームレベル, ラベル)
    // 半径とズームは生成されるボクSegment数が非自明になるよう選択:
    //   r=30m  z=20 → ボクSegmentサイズ ~38m → 軸方向 ~2–8 ボクSegment
    //   r=100m z=18 → ボクSegmentサイズ ~153m → 軸方向 ~1.3 ボクSegment
    //   r=100m z=20 → ボクSegmentサイズ ~38m  → 軸方向 ~5 ボクSegment
    //   r=300m z=18 → z=18 で軸方向 ~4 ボクSegment
    let cases: &[(f64, u8, &str)] = &[
        (30.0, 20, "dome_r30m_z20"),
        (100.0, 18, "dome_r100m_z18"),
        (100.0, 20, "dome_r100m_z20"),
        (300.0, 18, "sphere_r300m_z18"),
        (300.0, 20, "sphere_r300m_z20"),
    ];

    let mut group = c.benchmark_group("SpatialIdSet/Geometry/Sphere");
    for &(r, z, label) in cases {
        group.bench_function(label, |b| {
            b.iter(|| {
                let sphere = Sphere::new(center, r).unwrap();
                black_box(build_set(sphere.cover_single_ids(z).unwrap()))
            });
        });
    }
    group.finish();
}

/// 0.001° ≈ 100m 離れた 2 つのドームで部分重複ペアを作る。
/// r > 50m のときに重複が発生する。
fn bench_sphere_set_ops(c: &mut Criterion) {
    let center_a = coord(BASE_LAT, BASE_LON, 0.0);
    let center_b = coord(BASE_LAT + 0.001, BASE_LON + 0.001, 0.0);

    let cases: &[(f64, u8, &str)] = &[
        (50.0, 20, "r50m_z20"),
        (150.0, 18, "r150m_z18"),
        (150.0, 20, "r150m_z20"),
    ];

    for &(r, z, label) in cases {
        let set_a = build_set(
            Sphere::new(center_a, r)
                .unwrap()
                .cover_single_ids(z)
                .unwrap(),
        );
        let set_b = build_set(
            Sphere::new(center_b, r)
                .unwrap()
                .cover_single_ids(z)
                .unwrap(),
        );

        let mut g = c.benchmark_group("SpatialIdSet/Geometry/Sphere_Union");
        g.bench_function(label, |b| b.iter(|| black_box(&set_a | &set_b)));
        g.finish();

        let mut g = c.benchmark_group("SpatialIdSet/Geometry/Sphere_Intersection");
        g.bench_function(label, |b| b.iter(|| black_box(&set_a & &set_b)));
        g.finish();

        let mut g = c.benchmark_group("SpatialIdSet/Geometry/Sphere_Difference");
        g.bench_function(label, |b| b.iter(|| black_box(&set_a - &set_b)));
        g.finish();
    }
}

// ────────────────────────────────────────────────────────────────
// 直方体ビルベンチマーク
// ────────────────────────────────────────────────────────────────

fn bench_building(c: &mut Criterion) {
    // 緯度方向の半幅: 0.00015° ≈ 17m、0.00025° ≈ 28m、0.00040° ≈ 44m
    // (半幅[度], 高さ[m], ズームレベル, ラベル)
    let solids: &[(f64, f64, u8, &str)] = &[
        (0.00015, 30.0, 20, "small_30x30x30_z20"),
        (0.00025, 80.0, 20, "medium_55x55x80_z20"),
        (0.00025, 80.0, 18, "medium_55x55x80_z18"),
        (0.00040, 200.0, 18, "tower_90x90x200_z18"),
        (0.00040, 200.0, 20, "tower_90x90x200_z20"),
    ];

    let prepared: Vec<_> = solids
        .iter()
        .map(|&(half, h, z, label)| {
            let solid = box_building(
                BASE_LAT - half,
                BASE_LON - half,
                BASE_LAT + half,
                BASE_LON + half,
                0.0,
                h,
            );
            (solid, z, label)
        })
        .collect();

    let mut group = c.benchmark_group("SpatialIdSet/Geometry/Building");
    for (solid, z, label) in &prepared {
        group.bench_function(*label, |b| {
            b.iter(|| {
                let ids = solid.cover_single_ids(*z).unwrap();
                black_box(build_set(ids))
            });
        });
    }
    group.finish();
}

// ────────────────────────────────────────────────────────────────
// 都市ブロック：6 棟を逐次挿入（3列 × 2行）
// ────────────────────────────────────────────────────────────────

/// 6 棟の直方体ビルを 1 つの SpatialIdSet に挿入して 3×2 都市ブロックを構築する。
///
/// ビル中心間隔は約 44m、フットプリント約 33m のため路地が約 11m 残る。
/// 高さは 30〜80m で変化させてビルごとのボクSegment数に差をつける。
fn make_urban_block(z: u8) -> SpatialIdSet {
    const STEP: f64 = 0.0004; // ビル中心間 ~44m
    const HALF: f64 = 0.00015; // ビルフットプリントの半幅 ~17m
    let mut block = SpatialIdSet::new();
    for row in 0..2i32 {
        for col in 0..3i32 {
            let lat = BASE_LAT + row as f64 * STEP;
            let lon = BASE_LON + col as f64 * STEP;
            let h = 30.0 + (row * 3 + col) as f64 * 10.0; // 30, 40, 50, 60, 70, 80 m
            let solid = box_building(lat - HALF, lon - HALF, lat + HALF, lon + HALF, 0.0, h);
            for id in solid.cover_single_ids(z).unwrap() {
                block.insert(id);
            }
        }
    }
    block
}

fn bench_urban_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("SpatialIdSet/Geometry/UrbanBlock");
    for z in [18u8, 20] {
        group.bench_function(format!("z={z}"), |b| {
            b.iter(|| black_box(make_urban_block(z)));
        });
    }
    group.finish();
}

/// 東に約 110m ずらした隣接都市ブロック同士で集合演算を計測する。
/// ブロックの東端・西端が境界ゾーンで重なる。
fn bench_urban_block_ops(c: &mut Criterion) {
    let block_a = make_urban_block(20);

    const STEP: f64 = 0.0004;
    const HALF: f64 = 0.00015;
    let mut block_b = SpatialIdSet::new();
    for row in 0..2i32 {
        for col in 0..3i32 {
            let lat = BASE_LAT + row as f64 * STEP;
            let lon = (BASE_LON + 0.001) + col as f64 * STEP; // 東へ約 110m
            let h = 40.0 + col as f64 * 15.0; // 40, 55, 70 m
            let solid = box_building(lat - HALF, lon - HALF, lat + HALF, lon + HALF, 0.0, h);
            for id in solid.cover_single_ids(20).unwrap() {
                block_b.insert(id);
            }
        }
    }

    let mut group = c.benchmark_group("SpatialIdSet/Geometry/UrbanBlock_Ops_z20");
    group.bench_function("Union", |b| b.iter(|| black_box(&block_a | &block_b)));
    group.bench_function("Intersection", |b| {
        b.iter(|| black_box(&block_a & &block_b))
    });
    group.bench_function("Difference", |b| b.iter(|| black_box(&block_a - &block_b)));
    group.finish();
}

criterion_group!(
    benches,
    bench_sphere,
    bench_sphere_set_ops,
    bench_building,
    bench_urban_block,
    bench_urban_block_ops,
);
criterion_main!(benches);
