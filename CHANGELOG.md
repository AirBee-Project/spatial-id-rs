# Changelog

このプロジェクトの重要な変更はすべてこのファイルに記録する。

形式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に基づき、
バージョン番号は [Semantic Versioning](https://semver.org/lang/ja/) に従う。

## [Unreleased]

## [0.2.0](https://github.com/AirBee-Project/spatial-id-rs/compare/v0.1.1...v0.2.0) - 2026-11-28

### Added

- [**breaking**] 出力イテレーターをラッパー型へ統合
- *(id)* FlexIdのランダム生成と境界付きRangeIdを追加
- *(geometry)* Polygon同士が同一平面上にあるか判定するis_coplanarとis_coplanar_epsilonを追加
- *(id)* ShortRelativeFlexIdからの変換を追加
- *(dimension)* DimensionSetからu8への変換を追加
- *(query)* try_filter_mapを追加し変換を簡素化
- [**breaking**] 公開enumにnon_exhaustiveを付与
- *(collection)* 同じ構造を共有するcompactを追加
- [**breaking**] SpatialIdMap の value_set を削除
- Query に into_map と into_set を追加

### Changed

- *(geometry)* is_coplanar_epsilonの法線計算をVec3Ecefの外積で書き換え
- *(geometry)* [**breaking**] with_epsilonをnew_epsilonに改名し_epsilon命名規則を統一
- *(query)* [**breaking**] Filter系APIと実装を整理
- *(geometry)* line_dda を Vec3FractionalId で実装
- *(collection)* compactのレビュー指摘を反映
- ソース読出しと木走査イテレーターを高速化
- *(collection)* 木の走査で子をプリフェッチする
- *(query)* 全体評価でソースの交差判定を省く
- *(query)* into_mapとZoomOutの親計算を高速化
- *(id)* FlexIdの4軸演算・包含交差判定・Z-order計算を高速化
- *(id)* difference の子の Vec 確保をなくす
- *(id)* 次元の集合を DimensionSet で表す
- *(collection)* Node の空を Option で表す
- *(collection)* インメモリコレクションを mem ディレクトリに集約
- [**breaking**] SpatialId から IntoIterator を外し flex_ids を追加
- [**breaking**] QueryOutput の要素を RangeId にする
- [**breaking**] SpatialIdSet と SpatialIdMap の出力を RangeId にする
- [**breaking**] time_with_span などの秒区間指定 API を削除
- [**breaking**] time_interval を廃止し i() を追加
- [**breaking**] 時間のみのメソッドを time_ 接頭辞に改名

### Fixed

- geojson を専用 feature に分離
- no_std+rayonとwasm32並列に対応
- *(geometry)* [**breaking**] Polygon の生成時に頂点数・面積・自己交差を検証する

## [0.1.1](https://github.com/AirBee-Project/spatial-id-rs/compare/v0.1.0...v0.1.1) - 2026-11-06

### Added

- SpatialId に型判定メソッドを追加

## [0.1.0] - 2026-11-04

### Added

- 初回リリース。
