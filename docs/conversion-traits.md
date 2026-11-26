# 変換系 Trait

この文書は、図形や空間IDをあるズームレベルの精度で空間IDに変換したり、既存の空間IDを列挙したりする Trait 群の役割を整理したものである。

## 1. 役割の分離

本ライブラリには、大きく分けて 2 系統の Trait がある。

- 図形をあるズームレベルの精度で空間IDに変換するための Trait。
- すでに空間IDとして表現されている値を「列挙する」ための Trait。

前者は幾何学的な入力から空間IDを作るためのものであり、後者は空間IDそのもの、または空間IDの集合を扱うためのものである。

## 2. 変換系 Trait

`geometry::traits` に定義されている Trait である。

- `CoverSingleIds`
  - 指定したズームレベルの精度で `SingleId` を返す。
  - メソッドは `cover_single_ids(&self, z: u8) -> Result<impl Iterator<Item = SingleId>, Error>` である。
- `CoverRangeIds`
  - 指定したズームレベルの精度で `RangeId` を返す。
  - メソッドは `cover_range_ids(&self, z: u8) -> Result<impl Iterator<Item = RangeId>, Error>` である。
- `CoverFlexIds`
  - `FlexId` への変換に対応することを表すマーカ Trait である。

これらは `Coordinate`、`Ecef`、`Line`、`Triangle`、`Polygon`、`Solid`、`Sphere` などの図形型に実装される。

## 3. 列挙系 Trait

`id::traits` に定義されている Trait である。

- `IntoSingleIds`
  - `self` を消費して `SingleId` の列挙を返す。
- `IterSingleIds`
  - 参照から `SingleId` の列挙を返す。
- `IntoFlexIds`
  - `self` を消費して `FlexId` の列挙を返す。
- `IterFlexIds`
  - 参照から `FlexId` の列挙を返す。

`Into*` は所有権を受け取るため、変換元を破棄してよい場合に向く。`Iter*` は借用で使うため、元の値を保持したまま列挙したい場合に向く。

## 4. 使い分け

- 図形をあるズームレベルの精度で空間IDに変換するなら `Cover*` を使う。
- `SingleId` や `FlexId` を 1 要素または集合として扱うなら `Into*` / `Iter*` を使う。
- 変換結果を `Result` で返したい場合は `Cover*` を選ぶ。
- ただ列挙したいだけなら `Into*` / `Iter*` を選ぶ。

## 5. 例

```rust
use spatial_id::IntoFlexIds;
use spatial_id::geometry::{Coordinate, CoverSingleIds};
use spatial_id::SingleId;

let coord = Coordinate::new(35.0, 139.0, 0.0).unwrap();
let ids: Vec<SingleId> = coord.cover_single_ids(18).unwrap().collect();

let flex_ids: Vec<_> = ids[0].into_flex_ids().collect();
```
