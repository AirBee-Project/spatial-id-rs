# spatial-id

spatial-id-rs は時空間IDを Rust で扱うためのライブラリです。

## 使い方

```sh
cargo add spatial-id
```

```rust
use spatial_id::SingleId;
use spatial_id::collection::SpatialIdMap;

fn main() {
    let mut map: SpatialIdMap<f64> = SpatialIdMap::new();

    map.insert(SingleId::new(20, 9, 931389, 412900).unwrap(), 3.2);

    for (id, value) in map.flat_single_ids(None) {
        println!("{} => {},", id, value)
    }
}
```

## Features

| feature       | 既定 | 内容                                                                       |
| ------------- | ---- | -------------------------------------------------------------------------- |
| `std`         | 有効 | 標準ライブラリを使う。無効にすると `no_std`（`alloc` は必要）で動作する。  |
| `rayon`       | 有効 | [rayon](https://docs.rs/rayon) による並列処理。`std` を含む。              |
| `temporal_id` | 有効 | 時間区間付きの空間ID。                                                     |
| `random`      | 無効 | [rand](https://docs.rs/rand) によるランダムな空間IDの生成。                |
| `unstable`    | 無効 | 互換性を保証しない実験的な API（serde / JSON 変換、GeoJSON、RegionMesh）。 |

## Contributing

開発手順とコミット規約は [CONTRIBUTING.md](https://github.com/AirBee-Project/spatial-id-rs/blob/main/CONTRIBUTING.md) を参照。
