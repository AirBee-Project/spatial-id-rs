# Changelog

このプロジェクトの重要な変更はすべてこのファイルに記録する。

形式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に基づき、
バージョン番号は [Semantic Versioning](https://semver.org/lang/ja/) に従う。

## [Unreleased]

## [0.2.0](https://github.com/AirBee-Project/spatial-id-rs/compare/v0.1.1...v0.2.0) - 2026-11-07

### Added

- [**breaking**] SpatialIdMap の value_set を削除
- Query に into_map と into_set を追加

### Changed

- [**breaking**] SpatialId から IntoIterator を外し flex_ids を追加
- [**breaking**] QueryOutput の要素を RangeId にする
- [**breaking**] SpatialIdSet と SpatialIdMap の出力を RangeId にする
- [**breaking**] time_with_span などの秒区間指定 API を削除
- [**breaking**] time_interval を廃止し i() を追加
- [**breaking**] 時間のみのメソッドを time_ 接頭辞に改名

## [0.1.1](https://github.com/AirBee-Project/spatial-id-rs/compare/v0.1.0...v0.1.1) - 2026-11-06

### Added

- SpatialId に型判定メソッドを追加

## [0.1.0] - 2026-11-04

### Added

- 初回リリース。
