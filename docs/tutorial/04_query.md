[`Query`](crate::query::Query) は、コレクションに対する演算をメソッドチェーンで組み立て、まとめて評価する仕組みである。
押し出し、減衰拡散、集約、絞り込み、合成などを組み合わせて、空間データを加工できる。

この章の最後では道路と建物のリスク値からドローンのリスクマップを作る。

![Query の処理の流れ][riskmap_flow]

# Query の基本

`SpatialIdSet` と `SpatialIdMap` は [`Source`](crate::query::Source) トレイトを実装していて、`query()` で Query を始められる。
演算をつないだあと、`get` で評価する範囲を指定し、`into_map` か `into_set` で結果を受け取る。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::collection::SpatialIdMap;
use spatial_id::query::Source;
use spatial_id::SingleId;

let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);
map.insert(SingleId::new(20, 9, 931391, 412900)?, 100);

// 値が 50 以上の部分だけを残す
let high = map.query().filter_in(50..).get(..).into_map()?;

for (id, v) in high.flat_single_ids(None) {
    println!("{id} => {v}");
}
# Ok(())
# }
```

```console
20/9/931390/412900 => 80
20/9/931391/412900 => 100
```

`query()` は元のコレクションを消費する。元のデータを残しておきたいときは、`map.clone().query()` のように複製してから始める。

## 評価する範囲

`get` に `..` を渡すと全体を評価する。
空間IDを渡すと、その範囲の結果を出すのに必要な部分だけを評価するので、広いデータの一部だけを見たいときに速い。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
# let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
# map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
# map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);
# map.insert(SingleId::new(20, 9, 931391, 412900)?, 100);
let target = SingleId::new(20, 9, 931390, 412900)?;
let part = map.query().get(target).into_map()?;
println!("{} 個", part.flat_single_ids(None).count());
# Ok(())
# }
```

```console
1 個
```

評価は結果を読み出すときに始まり、読んだ分だけ進む。途中で止めたいときは [`get_cancellable`](crate::query::Query::get_cancellable) を使う。

# 演算

## 移動 `shift_*`

指定したズームレベルのインデックス単位で、データ全体を平行移動する。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);

// ズームレベル 20 で東に2個ずらす
let moved = map.query().shift_x(20, 2).get(..).into_map()?;

for (id, _) in moved.flat_single_ids(None) {
    println!("{id},");
}
# Ok(())
# }
```

```console
20/9/931391/412900,
```

![L 字形の立体（左）を X 方向へ動かした例（右。灰色は元の位置）][query_shift_x]

## 押し出し `extrude_*`

対象の軸の位置を無視して、指定した範囲いっぱいに値を引き伸ばす。地表のデータを高さ方向に伸ばすときなどに使う。

最後の引数は、引き伸ばした値どうしが重なったときの扱い（[マージポリシー](#マージポリシー)）である。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
use spatial_id::query::merge_policy::Max;

let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 0, 931389, 412900)?, 100);

// f = 0〜3 に引き伸ばす
let extruded = map.query().extrude_f(20, 0, 3, Max).get(..).into_map()?;

for (id, _) in extruded.flat_single_ids(None) {
    println!("{id},");
}
# Ok(())
# }
```

```console
20/0/931389/412900,
20/1/931389/412900,
20/2/931389/412900,
20/3/931389/412900,
```

![地表の L 字形の敷地（左）を F 方向へ押し出した例（右）][query_extrude_f]

## 減衰拡散 `falloff_*`

値を周囲へ広げながら、距離に応じて減らしていく。指定した距離で 0 になる。
建物や道路のまわりに、リスクのバッファを設けるときに使う。

3つ目の引数は広げる向きである。`None` なら両側に、`Some(Side::Upper)` なら座標が大きい側だけに広げる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
use spatial_id::query::{merge_policy::Max, FalloffPattern};

let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 0, 931389, 412900)?, 100);

// X 方向に、4個先で 0 になるよう直線的に減らす
let spread = map
    .query()
    .falloff_x(20, 4, None, FalloffPattern::Linear, Max)
    .get(..)
    .into_map()?;

let values: Vec<u32> = spread.flat_single_ids(None).map(|(_, v)| *v).collect();
println!("{values:?}");
# Ok(())
# }
```

```console
[0, 25, 50, 75, 100, 75, 50, 25, 0]
```

![値 100 の板（左）から、X 方向へ値を減らしながら広げた例（右）。赤ほど値が大きい][query_falloff_x]

減り方は [`FalloffPattern`](crate::query::FalloffPattern) で選ぶ。

| パターン | 減り方 |
| :--- | :--- |
| `Linear` | 距離に比例して減る |
| `QuadraticIn` | 近くではゆっくり、遠くで急に減る |
| `QuadraticOut` | 近くで急に、遠くではゆっくり減る |

![FalloffPattern ごとの、距離に応じて残る値の割合][query_falloff_pattern]

## 集約 `zoom_out`

ズームレベルを下げて、細かいデータを粗くまとめる。
1つの親にまとまる値はマージポリシーで1つにする。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
use spatial_id::query::merge_policy::Max;

let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);
map.insert(SingleId::new(20, 9, 931391, 412900)?, 100);

// 2つは同じ親に入るので、大きいほうの 100 が残る
let coarse = map.query().zoom_out(19, Max).get(..).into_map()?;

for (id, v) in coarse.flat_single_ids(None) {
    println!("{id} => {v}");
}
# Ok(())
# }
```

```console
19/4/465695/206450 => 100
```

![ズームレベル 21 の値（左）を、ズームレベル 19 の平均（Average）にまとめた例（右）。赤ほど値が大きい][query_zoom_out]

## 絞り込みと変換

| メソッド | 内容 |
| :--- | :--- |
| `filter_eq` / `filter_in` / `filter_not_in` | 値で絞り込む |
| `map_values` / `filter_values` / `filter_map_values` | 値を関数で変換・選別する |
| `map` / `filter` / `filter_map` | 空間IDと値の組を関数で変換・選別する |

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);

let doubled = map.query().map_values(|v| v * 2).get(..).into_map()?;
println!("{:?}", doubled.value_range());
# Ok(())
# }
```

```console
Some((60, 60))
```

![中心ほど値が大きいマップ（左）から、filter_in(40..80) で絞り込んだ例（右）][query_filter_in]

# 合成

`merge`・`intersection`・`difference` で、2つの Query を合成する。
`merge` は2つを重ね合わせ、重なった部分の値をマージポリシーで1つにする。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::query::Source;
# use spatial_id::SingleId;
use spatial_id::query::merge_policy::Sum;

let mut a: SpatialIdMap<u32> = SpatialIdMap::new();
a.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
a.insert(SingleId::new(20, 9, 931390, 412900)?, 80);

let mut b: SpatialIdMap<u32> = SpatialIdMap::new();
b.insert(SingleId::new(20, 9, 931390, 412900)?, 5);

// 重なった 931390 は 80 + 5 = 85 になる
let merged = a.query().merge(b, Sum).get(..).into_map()?;

let values: Vec<u32> = merged.flat_single_ids(None).map(|(_, v)| *v).collect();
println!("{values:?}");
# Ok(())
# }
```

```console
[30, 85]
```

`merge` の引数にはコレクションをそのまま渡しても、別の Query を渡してもよい。

| `merge` | `intersection` | `difference` |
| :---: | :---: | :---: |
| ![merge][query_merge] | ![intersection][query_intersection] | ![difference][query_difference] |

図は、青の A と橙の B（各図の左）を合成した結果（右）である。

## マージポリシー

値が重なったときの扱いは、[`merge_policy`](crate::query::merge_policy) の型で選ぶ。

| 型 | 結果 |
| :--- | :--- |
| `Max` / `Min` | 大きいほう / 小さいほう |
| `Average` | 平均 |
| `Sum` | 和。整数型では型の最大値で止まる |
| `SaturatingSub` | 差。結果が表せないときは既定値（数値型では 0） |
| `Overwrite` | 後から来た値で上書き |
| `KeepExisting` | 既存の値を残す |

# ドローンのリスクマップを作る

最後に、ここまでの演算を組み合わせる。
リポジトリの `sample/riskmap/` にある、道路（`tran_risk.json`）と建物（`bldg_risk.json`）のリスク値を使う。

1. 道路と建物のそれぞれを高さ方向へ押し出し、水平方向へ減衰させながら広げる。
2. 2つを重ね合わせる。
3. ズームレベル 23 に集約し、値が 0〜99 の範囲だけを残す。

```rust,no_run
# #[cfg(feature = "unstable")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use spatial_id::collection::SpatialIdMap;
use spatial_id::query::merge_policy::{Average, Max};
use spatial_id::query::{FalloffPattern, Source};

let load = |path: &str| -> Result<SpatialIdMap<u32>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
};
let tran = load("sample/riskmap/tran_risk.json")?;
let bldg = load("sample/riskmap/bldg_risk.json")?;

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

std::fs::write("riskmap.json", serde_json::to_string(&risk)?)?;
# Ok(())
# }
# #[cfg(not(feature = "unstable"))]
# fn main() {}
```

![完成したドローンリスクマップ][riskmap_result]

同じコードは `examples/riskmap.rs` にもある。処理が重いので、`--release` を付けて実行する。

```sh
cargo run --release --example riskmap --features unstable
```

次章: [5. 時空間ID](crate::_tutorial::_05_temporal)
