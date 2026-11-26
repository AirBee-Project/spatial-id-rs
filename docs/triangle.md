# Triangle 被覆のデバッグ手順

`Triangle::cover_single_ids` の結果を、三角形の内部を細かくサンプリングして求めた参照結果と比べ、取りこぼしや余分な ID がないか、どれだけ速いかを確認するための手順。

## 1. 参照実装を一時的に追加する

`src/geometry/shape/triangle/mod.rs` の `impl Triangle` に、次の関数を一時的に追加する（確認が終わったら削除する）。

三角形を ECEF 上で細かい格子点に分割し、各点が属する `SingleId` を重複なく列挙する。格子の間隔は、3頂点のうち最も赤道に近い緯度での `SingleId` 1辺の幅の 1/4 とする。

```rs
// 追加で必要な import:
// use core::f64::consts::PI;
// use crate::{Point, WGS84_A, ZoomLevel};

/// サンプリングによる参照実装（デバッグ用）。
pub fn single_ids_sampling(&self, z: u8) -> Result<impl Iterator<Item = SingleId>, Error> {
    let z = ZoomLevel::new(z)?.get();

    let a: Vec3Ecef = self.points[0].into();
    let b: Vec3Ecef = self.points[1].into();
    let c: Vec3Ecef = self.points[2].into();

    let min_lat_rad = libm::fabs(self.points[0].latitude())
        .min(libm::fabs(self.points[1].latitude()))
        .min(libm::fabs(self.points[2].latitude()))
        .to_radians();

    // サンプリング間隔 [m]
    let d = PI * WGS84_A * libm::cos(min_lat_rad) * libm::pow(2.0, (-2 - z as i32) as f64);

    let l1 = (c - b).norm();
    let l2 = (a - c).norm();
    let l3 = (a - b).norm();
    let steps = libm::ceil(l1.max(l2).max(l3) / d) as usize;

    let mut seen = HashSet::new();
    let iter = (0..=steps)
        .flat_map(move |i| {
            let t = i as f64 / steps as f64;
            let line1 = a.scale(1.0 - t) + b.scale(t);
            let line2 = a.scale(1.0 - t) + c.scale(t);

            (0..=i).filter_map(move |j| {
                let p = if i == 0 {
                    a
                } else {
                    let s = j as f64 / i as f64;
                    line1.scale(1.0 - s) + line2.scale(s)
                };
                // 範囲外の点は飛ばす
                Coordinate::try_from(p).ok()?.single_id(z).ok()
            })
        })
        .filter(move |id| seen.insert(id.clone()));

    Ok(iter)
}
```

## 2. 比較用のプログラムを実行する

次の内容を `examples/triangle_debug.rs` として保存し、`cargo run --release --example triangle_debug` で実行する。乱数生成には dev-dependencies の `rand` / `rand_chacha` を使うので、feature の指定は不要。

固定の三角形（東京・池袋・品川）と、ランダムに生成した小さな三角形について、両者の結果の差分と速度比を表示し、`triangle_debug.csv` に追記する。

```rs
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

use spatial_id::geometry::{Coordinate, CoverSingleIds, Triangle};
use spatial_id::SingleId;
use rand::RngExt;
use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};

const MIN_LAT: f64 = 20.0;
const MAX_LAT: f64 = 22.0;
const MIN_LON: f64 = 137.0;
const MAX_LON: f64 = 139.0;
const MIN_ALT: f64 = 0.0;
const MAX_ALT: f64 = 1000.0;

/// 中心点の周囲 ±`spread_deg` 度の範囲に頂点を持つ三角形を生成する。
fn random_triangle(rng: &mut ChaCha8Rng, spread_deg: f64) -> Result<Triangle, spatial_id::Error> {
    let lat0 = rng.random_range(MIN_LAT..MAX_LAT);
    let lon0 = rng.random_range(MIN_LON..MAX_LON);
    let mut vertex = || {
        Coordinate::new(
            lat0 + rng.random_range(-spread_deg..spread_deg),
            lon0 + rng.random_range(-spread_deg..spread_deg),
            rng.random_range(MIN_ALT..MAX_ALT),
        )
    };
    Ok(Triangle::new([vertex()?, vertex()?, vertex()?]))
}

fn save_result_to_csv(
    area: f64,
    z: u8,
    speedup: f64,
    only_sampling: usize,
    only_cover: usize,
    file_path: &str,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(file_path)?;
    if file.metadata()?.len() == 0 {
        writeln!(file, "area,z,speedup,only_sampling,only_cover")?;
    }
    writeln!(file, "{area},{z},{speedup},{only_sampling},{only_cover}")?;
    file.flush()
}

/// 1つの三角形について、cover_single_ids と参照実装を比較する。
fn compare(z: u8, tri: Triangle) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let reference: HashSet<SingleId> = tri.single_ids_sampling(z)?.collect();
    let t_reference = start.elapsed();

    let start = Instant::now();
    let covered: HashSet<SingleId> = tri.cover_single_ids(z)?.collect();
    let t_cover = start.elapsed();

    let common = covered.intersection(&reference).count();
    // 参照実装にはあるが cover_single_ids にない ID（取りこぼし）
    let only_sampling = reference.difference(&covered).count();
    // cover_single_ids にだけある ID（余分。境界付近では多少出てよい）
    let only_cover = covered.difference(&reference).count();
    let speedup = t_reference.as_secs_f64() / t_cover.as_secs_f64();

    println!(
        "面積 {:.1} m², 共通 {common}, 取りこぼし {only_sampling}, 余分 {only_cover}, \
         参照 {t_reference:?} / cover {t_cover:?}（{speedup:.1} 倍）",
        tri.area()
    );
    save_result_to_csv(tri.area(), z, speedup, only_sampling, only_cover, "triangle_debug.csv")?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let z = 22;

    // 固定の三角形
    let tokyo = Coordinate::new(35.681382, 139.766084, 0.0)?;
    let ikebukuro = Coordinate::new(35.728926, 139.71038, 100.0)?;
    let shinagawa = Coordinate::new(35.630152, 139.74044, 50.0)?;
    compare(z, Triangle::new([tokyo, ikebukuro, shinagawa]))?;

    // ランダムな三角形（シード固定で再現可能）
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    for _ in 0..10 {
        // 一辺が最大 2 km 程度の三角形（z = 22 では参照実装の計算に数秒かかることがある）
        compare(z, random_triangle(&mut rng, 0.01)?)?;
    }
    Ok(())
}
```

`取りこぼし` が 0 でない場合は、`cover_single_ids` が三角形の内部にある `SingleId` を見落としている可能性がある。
