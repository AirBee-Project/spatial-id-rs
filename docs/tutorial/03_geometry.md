# 幾何図形と空間IDの変換

建物や道路、飛行経路などの地理データを空間IDで扱うには、図形を空間IDに変換する必要がある。
この章では、座標を表す型と、図形が覆う空間IDの求め方を扱う。

| Line | Polygon | Tube | GeoJson |
| :---: | :---: | :---: | :---: |
| ![line][line_cover] | ![polygon][solid_cover] | ![tube][tube_cover] | ![geojson][geojson_cover] |

# 地図の歪み

空間IDは Web メルカトル図法を基準にしている。
楕円体の地球を正方形に引き伸ばしているため、現実の直線は空間IDの世界では曲線になる。
画像のラスタライズのように、平面上で塗りつぶすだけでは正確に変換できない。

このライブラリは地球を3次元の直交座標で扱って図形を計算し、その結果を空間IDに戻す。
こうすることで、歪みを含めて正確に変換している。

# 座標を表す型

## `Coordinate`

[`Coordinate`](crate::geometry::Coordinate) は、緯度・経度・高度（m）で表した点である。
ズームレベルを指定すると、その点を含む空間IDが求められる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::{Coordinate, Point};

// 東京駅付近（緯度, 経度, 高度[m]）
let coord = Coordinate::new(35.681236, 139.767125, 10.0)?;

let id = coord.single_id(20)?;
println!("{id}");
# Ok(())
# }
```

```console
20/0/931389/412906
```

## `Ecef`

[`Ecef`](crate::geometry::Ecef) は、地球の中心を原点とし、赤道面を XY 平面、地軸を Z 軸とする直交座標系（地心直交座標系）の点である。
図法の歪みがないので、図形の計算はこの座標系で行う。

![ECEF 座標系][geometry_ecef]

`Coordinate` と `Ecef` は相互に変換できる。
`Ecef::new` は、空間IDで扱える範囲（緯度 ±85.0511°、高度 ±33,554,432 m）の外にある点をエラーにする。
そのため、作れた `Ecef` は必ず `Coordinate` に戻せる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::{Coordinate, Ecef};

let coord = Coordinate::new(35.681236, 139.767125, 10.0)?;

let ecef = Ecef::from(coord);
let back = Coordinate::from(ecef);
// 浮動小数点の誤差がわずかに出る
println!("{}", back.altitude());

// 北極点の上空は空間IDの緯度の範囲外なので作れない
println!("{:?}", Ecef::new(0.0, 0.0, 6_400_000.0));
# Ok(())
# }
```

```console
9.999997209757566
Err(Geometry(LatitudeOutOfRange { latitude: 90.0 }))
```

## `FractionalId`

[`FractionalId`](crate::geometry::FractionalId) は、空間IDのインデックスを小数まで広げて点を表す型である。
整数部が空間IDを、小数部が空間IDの中での位置を表す。
図形を空間IDに変換する最後の段階で使う。

![FractionalId の整数部と小数部][geometry_fractional_id]

`Coordinate` からは、ズームレベルを指定して `fractional_id` で変換する。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::{Coordinate, Point};

let coord = Coordinate::new(35.681236, 139.767125, 10.0)?;
let frac = coord.fractional_id(20)?;

// 整数部が空間IDのインデックスになる
println!("x = {}", frac.x());
println!("{}", frac.round_to_single_id());
# Ok(())
# }
```

```console
x = 931389.2579555555
20/0/931389/412906
```

# 図形を空間IDに変換する

図形の型は [`CoverSingleIds`](crate::geometry::CoverSingleIds) トレイトを実装している。
ズームレベルを渡すと、図形が覆う空間IDが得られる。

| メソッド | 戻り値 |
| :--- | :--- |
| `cover_single_ids(z)` | `SingleId` のイテレータ |
| `cover_set(z)` | `SpatialIdSet` |
| `cover_map(z)` / `cover_map_with(z, f)` | 値を持たせた `SpatialIdMap` |

## 球

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::{Coordinate, CoverSingleIds, Sphere};

let center = Coordinate::new(35.681236, 139.767125, 50.0)?;
let sphere = Sphere::new(center, 30.0)?; // 半径 30m

// ズームレベル 25（約1m）の空間IDの数
println!("{} 個", sphere.cover_single_ids(25)?.count());

// SpatialIdSet にまとめたときの、内部の要素の数
let set = sphere.cover_set(25)?;
println!("{} 個", set.count());
# Ok(())
# }
```

```console
133264 個
5372 個
```

## 線分

[`Line`](crate::geometry::Line) は2点を結ぶ線分である。
線分が通る空間IDだけが、重複なく返る。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::{Coordinate, CoverSingleIds, Line};

let start = Coordinate::new(35.681236, 139.767125, 10.0)?;
let end = Coordinate::new(35.681536, 139.767525, 50.0)?;
let line = Line::new([start, end]);

for id in line.cover_single_ids(20)? {
    println!("{id},");
}
# Ok(())
# }
```

```console
20/0/931389/412906,
20/0/931389/412905,
20/1/931389/412905,
20/1/931390/412905,
```

線分は`Ecef` 上で細かく分けてから `FractionalId` に戻し、それぞれに DDA（Digital Differential Analyzer）を当てて空間IDを求めている。

## 三角形・ポリゴン・立体

[`Triangle`](crate::geometry::Triangle) は3点、[`Polygon`](crate::geometry::Polygon) は複数の点で囲んだ面である。
[`Solid`](crate::geometry::Solid) は、複数の `Polygon` を面に持つ立体で、建物のデータなどに使う。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::{Coordinate, CoverSingleIds, Polygon};

// 東京駅周辺の地表を囲む四角形
let polygon = Polygon::new(vec![
    Coordinate::new(35.6810, 139.7665, 0.0)?,
    Coordinate::new(35.6810, 139.7675, 0.0)?,
    Coordinate::new(35.6818, 139.7675, 0.0)?,
    Coordinate::new(35.6818, 139.7665, 0.0)?,
]);

let set = polygon.cover_set(22)?;
println!("{} 個", set.count());
# Ok(())
# }
```

```console
12 個
```

`Polygon` はイヤークリッピング法で三角形に分けてから変換する。
三角形は`Ecef` 上で下の図のように分割してから `FractionalId` に変換する。

![Triangle の分割][geometry_triangle_divide]

## 変換できる図形の一覧

| 型 | 図形 | 変換の様子 |
| :--- | :--- | :---: |
| [`Coordinate`](crate::geometry::Coordinate) | 点 | ![点][point_cover] |
| [`Line`](crate::geometry::Line) | 線分 | ![線分][line_cover] |
| [`Triangle`](crate::geometry::Triangle) | 三角形 | ![三角形][triangle_cover] |
| [`Polygon`](crate::geometry::Polygon) | 多角形 | ![多角形][polygon_cover] |
| [`Solid`](crate::geometry::Solid) | 多面体 | ![多面体][solid_cover] |
| [`Sphere`](crate::geometry::Sphere) | 球 | ![球][sphere_cover] |
| [`Cylinder`](crate::geometry::Cylinder) | 円柱 | ![円柱][cylinder_cover] |
| [`Tube`](crate::geometry::Tube) | 点列を半径で太らせた管。空中の経路などに使う | ![管][tube_cover] |

## 範囲のまま受け取る

`Solid` と `Cylinder` は [`CoverRangeIds`](crate::geometry::CoverRangeIds) も実装している。
`cover_range_ids` を使うと、`SingleId` に展開せずに、まとまった `RangeId` で受け取れる。
大きな立体では、返る要素の数が少なくなる。

![多面体を RangeId で覆った様子][solid_cover_range]

# GeoJSON

`std` と `unstable` の feature を有効にすると、`geojson` クレートの `GeoJson` をそのまま変換できる。
`cover_map_with` を使えば、Feature の属性を値として持たせた `SpatialIdMap` を作れる。

```rust
# #[cfg(all(feature = "std", feature = "unstable"))]
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::geometry::CoverSingleIds;

let geojson: geojson::GeoJson = r#"{
    "type": "FeatureCollection",
    "features": [{
        "type": "Feature",
        "properties": { "risk": 80 },
        "geometry": { "type": "Point", "coordinates": [139.767125, 35.681236, 10.0] }
    }]
}"#
.parse()
.unwrap();

// Feature の "risk" を値にする
let map = geojson.cover_map_with(20, |feature| {
    feature
        .and_then(|f| f.property("risk"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
})?;

println!("{:?}", map.value_range());
# Ok(())
# }
# #[cfg(not(all(feature = "std", feature = "unstable")))]
# fn main() {}
```

```console
Some((80, 80))
```


![GeoJSON と、それが覆う空間ID][geojson_cover]

次章: [4. Query](crate::_tutorial::_04_query)
