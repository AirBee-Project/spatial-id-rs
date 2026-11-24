# 公開 API ルール

このドキュメントは、クレートの公開方法（`pub mod` / `pub use`）を統一し、`rustdoc` と利用者体験を安定させるための規則である。

## 1. 基本方針

- 公開 API の起点は `src/lib.rs` とする。
- 利用者向けの主要型・主要トレイト・主要定数は `lib.rs` から直接 import できる状態を維持する。
- `pub use` は `lib.rs` に集約し、`lib.rs` は「公開 API 一覧」として読める構成にする。

## 2. 公開の原則

- 公開モジュールは `pub mod` で宣言する。
- 利用者に直接使ってほしい項目は `lib.rs` で `pub use` する。
- 内部実装都合のモジュール（実装分割用の `constructor` / `impls` / `ops` など）は原則として `lib.rs` では再公開しない。
- 中間 `mod.rs`（例: `id/mod.rs`, `geometry/mod.rs`）での再公開は最小限にする。

## 3. rustdoc 表示ルール

- `lib.rs` の `pub use` には `#[doc(inline)]` を付与する。
- これにより、`rustdoc` 上で Re-exports として埋もれず、通常の型・トレイト項目として表示されやすくなる。
- docs 専用の互換 API を追加する場合は `#[doc(hidden)]` を明示的に付ける。

## 4. lib.rs の並び順

`lib.rs` の公開順序は次のグループ順を基本とする。

1. `Error`。
2. geometry。

- 型（`Coordinate`, `Ecef`, `Line`, `Triangle`, `Polygon`, `Solid`, `Sphere`）。
- トレイト（`Point`, `Shape`, `Into*`, `To*`）。
- 定数（`WGS84_*`）。

3. spatial_id（モジュールは公開せず、項目を `lib.rs` 直下から公開する）。

- ID 型（`SingleId`, `RangeId`, `FlexId`）。
- コレクション型（`FlexTree*`）。
- トレイト（`SpatialId`, `Into*Ids`, `Iter*Ids`, `SpatialIdSet`, `SpatialIdMap`）。
- 定数（`F_*`, `XY_MAX`, `MAX_ZOOM_LEVEL`）。

4. time（`time` モジュールとして公開する）。

- 時間間隔と時間のズームレベル（`Interval`, `AllowedIntervals`, `TZoomLevel`）。

## 5. 追加・変更時のチェックリスト

- 新しい公開型を追加したら、`lib.rs` に `#[doc(inline)] pub use ...` を追加したか。
- 追加した公開項目が上記の並び順に従っているか。
- `cargo doc --no-deps` が通るか。
- broken intra-doc links が発生していないか。
- サンプル・Doc テストの import パスが壊れていないか。

## 6. 例

```rust
#[doc(inline)]
pub use geometry::point::coordinate::Coordinate;

#[doc(inline)]
pub use id::SingleId;
```

## 7. 互換性ポリシー

- 原則として、既存の `lib.rs` 直下 import は維持する。
