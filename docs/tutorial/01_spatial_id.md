# 空間IDとは
空間IDは地球上の空間をグリッド状に区切り、それぞれに `{z}/{f}/{x}/{y}` という番号を振ったものである。

![東京駅の周辺をグリッド状に区切った空間ID（ズームレベル 20）][spatial_id_grid]

- `z`: ズームレベル。大きいほど立方体が小さくなる。ズームレベル 25 で1辺約1m。
- `f`: 高さ方向のインデックス。0 は高度 0m から上の1層目で、負の値は高度 0m より下を表す。
- `x`, `y`: 東西・南北方向のインデックス。Web メルカトルのタイル番号と同じ。

# 標準的な空間IDを表す `SingleId`

[`SingleId`](crate::SingleId) は、ガイドラインで定義された標準的な空間IDを表す。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::SingleId;

// 東京駅の上空
let id = SingleId::new(20, 9, 931389, 412900)?;
println!("{id}");

// 文字列から読み込むこともできる
let parsed: SingleId = "20/9/931389/412900".parse()?;
assert_eq!(parsed, id);
# Ok(())
# }
```

```console
20/9/931389/412900
```

`SingleId::new` は、各インデックスがズームレベルの範囲に収まっているかを検証する。
範囲外の値を渡すと [`Error`](crate::Error) を返す。

```rust,no_run
use spatial_id::SingleId;

fn main() -> Result<(), spatial_id::Error> {
    // ズームレベル 3 では、f は -8〜7 の範囲しか取れない
    let id = SingleId::new(3, 9, 931389, 412900)?;
    # Ok(())
}
```

```console
Error: SpatialId(FOutOfRange { z: 3, f: 9 })
```

## 親の空間ID
ズームレベルを下げると、その空間を含む大きな空間IDが得られる。
```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::SingleId;

let id = SingleId::new(20, 9, 931389, 412900)?;

let parent = id.spatial_parent_at_zoom(18)?;
println!("{}",parent);

# Ok(())
# }
```

```console
18/2/232847/103225
```

![空間ID（左）と、それを含むズームレベル 18 の親（右の橙）][spatial_id_parent]

## 子の空間ID

ズームレベルを上げると、その空間を分割した小さな空間IDが得られる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::SingleId;

let id = SingleId::new(20, 9, 931389, 412900)?;

// ズームレベルを1つ上げると、各軸が2分割されて8個になる
for child in id.spatial_children_at_zoom(21)? {
    println!("{child},");
}
# Ok(())
# }
```

```console
21/18/1862778/825800,
21/18/1862778/825801,
21/18/1862779/825800,
21/18/1862779/825801,
21/19/1862778/825800,
21/19/1862778/825801,
21/19/1862779/825800,
21/19/1862779/825801,
```

![空間ID（左）と、ズームレベル 21 の 8 個の子（右。塗り分けている）][spatial_id_children]

## 移動

`move_f`・`move_x`・`move_y` で空間IDを平行移動できる。
`move_f` と `move_y` は範囲外に出るとエラーになる。`move_x` は東西が地球を一周してつながっているため、常に成功する。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{SingleId, SpatialId};

let mut id = SingleId::new(20, 9, 931389, 412900)?;
id.move_f(10)?;
println!("{id}");
# Ok(())
# }
```

```console
20/19/931389/412900
```

## 隣接

面を共有して隣り合う6個の空間IDは `spatial_face_neighbors` で求められる。
辺や頂点を共有するものまで含めるときは、`spatial_edge_neighbors`・`spatial_vertex_neighbors`・`spatial_all_neighbors` を使う。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::SingleId;

let id = SingleId::new(20, 9, 931389, 412900)?;
// 上・下・東・西・南・北の順に返る
for neighbor in id.spatial_face_neighbors() {
    println!("{neighbor},");
}
# Ok(())
# }
```

```console
20/10/931389/412900,
20/8/931389/412900,
20/9/931390/412900,
20/9/931388/412900,
20/9/931389/412901,
20/9/931389/412899,
```

![中心の空間ID（青）と、面で隣り合う 6 個（橙）][spatial_id_neighbors]

## 実際の大きさ

空間IDが表す立方体の大きさ（メートル）を求められる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{SingleId, SpatialId};

let id = SingleId::new(20, 9, 931389, 412900)?;

println!("高さ: {} m", id.length_f_meters());
# Ok(())
# }
```

```console
高さ: 32 m
```

## 位置

中心の座標も求められる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{SingleId, SpatialId};

let id = SingleId::new(20, 9, 931389, 412900)?;

let center = id.spatial_center();
println!("中心: 緯度 {:.4}, 経度 {:.4}", center.latitude(), center.longitude());
# Ok(())
# }
```

```console
中心: 緯度 35.6828, 経度 139.7672
```

# 範囲をまとめて表す `RangeId`

[`RangeId`](crate::RangeId) は、F・X・Y の各軸を範囲 `[min, max]` で持つ型である。
連続した複数の空間IDを、1つの直方体としてまとめて表せる。
文字列では範囲を `min:max` と書く。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::RangeId;

// X 方向に 9 個並んだ空間ID
let range = RangeId::new(20, 9, [931385, 931393], 412900)?;
println!("{range}");
# Ok(())
# }
```

```console
20/9/931385:931393/412900
```

![X 方向に並んだ 9 個の SingleId（左。塗り分けている）と、それをまとめた 1 個の RangeId（右）][spatial_id_range]

## `SingleId` との変換

`SingleId` はそのまま `RangeId` に変換できる。
逆に `RangeId` は`single_ids` で含まれるすべての `SingleId` に展開できる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{RangeId, SingleId};

let id = SingleId::new(20, 9, 931389, 412900)?;
println!("{}", RangeId::from(id));

let range = RangeId::new(20, 9, [931385, 931393], 412900)?;
let ids: Vec<SingleId> = range.single_ids().collect();
println!("{} 個: {} 〜 {}", ids.len(), ids[0], ids[ids.len() - 1]);
# Ok(())
# }
```

```console
20/9/931389/412900
9 個: 20/9/931385/412900 〜 20/9/931393/412900
```

`RangeId` も `SingleId` と同じように移動できる。範囲全体がそのまま平行移動する。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{RangeId, SpatialId};

let mut range = RangeId::new(20, 9, [931385, 931393], 412900)?;
range.move_f(5)?;
println!("{range}");
# Ok(())
# }
```

```console
20/14/931385:931393/412900
```

![RangeId（左）を move_f(5) で上へ動かした例（右。灰色は元の位置）][spatial_id_range_move]

# 共通の操作を定める `SpatialId` トレイト

ここまでに使った `move_f` や `length_f_meters`、`spatial_center` は、[`SpatialId`](crate::SpatialId) トレイトのメソッドである。
`SingleId` と `RangeId` はどちらもこのトレイトを実装しているので、両方を受け取る関数を書ける。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{RangeId, SingleId, SpatialId};

/// 空間IDが覆う範囲の高さ（m）を返す。
fn height(id: &impl SpatialId) -> f64 {
    id.length_f_meters()
}

println!("{} m", height(&SingleId::new(20, 9, 931389, 412900)?));
println!("{} m", height(&RangeId::new(20, [9, 10], 931389, 412900)?));
# Ok(())
# }
```

```console
32 m
64 m
```

次章で扱うコレクションの `insert` や `get` も `SpatialId` を受け取るので、どちらの型でも渡せる。

次章: [2. コレクション](crate::_tutorial::_02_collection)
