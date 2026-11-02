# spatial-id

空間ID・時空間IDを扱う Rust ライブラリです。
仕様は [4次元時空間情報基盤 ガイドライン（IPA）](https://www.ipa.go.jp/digital/architecture/guidelines/4dspatio-temporal-guideline.html)に準拠しています。

![道路と建物のリスク値から作ったドローンリスクマップ](docs/images/riskmap_result.jpg)

## できること

| 機能 | 内容 |
| :--- | :--- |
| 図形 → 空間ID | 点・線・ポリゴン・立体・球・円柱・GeoJSON などを、解析的に高精度で空間IDへ変換 |
| Query | 押し出し・減衰拡散・集約・絞り込み・合成をメソッドチェーンで記述 |

## Query でリスクマップを作る

道路（`tran`）と建物（`bldg`）のリスク値を、それぞれ押し出して拡散させ、1枚のリスクマップに重ね合わせます。

![Query の処理の流れ](docs/images/riskmap_flow.jpg)

```rust
let tran_risk = tran
    .query()
    .extrude_f(25, 0, 50, Max)
    .falloff_x(25, 10, None, FalloffPattern::QuadraticIn, Max)
    .falloff_y(25, 10, None, FalloffPattern::QuadraticIn, Max);

let bldg_risk = bldg
    .query()
    .extrude_f(25, 0, 50, Max)
    .falloff_x(25, 10, None, FalloffPattern::QuadraticIn, Max)
    .falloff_y(25, 10, None, FalloffPattern::QuadraticIn, Max);

let risk = bldg_risk
    .merge(tran_risk, Max)
    .zoom_out(23, Average)
    .filter_in(0..100)
    .get(..)
    .into_map()?;
```

サンプルデータ（`sample/riskmap/`）で実際に動かせます。結果は `riskmap.json` に出力されます。

```sh
cargo run --release --example riskmap --features unstable
```

コード全体は [examples/riskmap.rs](examples/riskmap.rs) にあります。

## 図形 → 空間ID

```rust
use spatial_id::geometry::{Coordinate, CoverSingleIds, Sphere};

let center = Coordinate::new(35.681236, 139.767125, 50.0)?; // 緯度, 経度, 高度[m]
let sphere = Sphere::new(center, 30.0)?;                     // 半径 30 m
let set = sphere.cover_set(25)?;                             // ズームレベル 25 の空間IDの集合
```

`Line` `Triangle` `Polygon` `Solid` `Cylinder` `Tube` `geojson::GeoJson` も同じように変換できます。

## 空間ID ／ 時空間ID

```rust
use spatial_id::SingleId;

let id = SingleId::new(25, 0, 29800549, 13205107)?;                  // 25/0/29800549/13205107
let id = SingleId::new(12, 0, 3638, 1614)?.with_time(1800, 809712)?; // 12/0/3638/1614_1800/809712
```

時間を扱うかどうかは `temporal_id` feature で切り替えます（[詳細](docs/temporal-id-feature.md)）。

## インストール

```toml
[dependencies]
spatial-id = "0.1"
```

| feature | 既定 | 内容 |
| :--- | :---: | :--- |
| `std` | ✓ | 標準ライブラリを使う。GeoJSON からの変換に必要 |
| `rayon` | ✓ | 並列実行 |
| `temporal_id` | ✓ | 時空間ID。無効にすると空間IDのみ |
| `unstable` | | JSON 入出力など、破壊的変更があり得る API |
| `random` | | ランダムな空間IDの生成 |

## ドキュメント

- [用語](docs/terminology.md)
- [SpatialIdSet と SpatialIdMap の違い](docs/flex-tree-collections.md)
- [図形の関係](docs/geometry-relation.md)
- [時間の扱いと temporal_id feature](docs/temporal-id-feature.md)
- [コントリビュート](CONTRIBUTING.md)

## ライセンス

[MIT License](LICENSE)
