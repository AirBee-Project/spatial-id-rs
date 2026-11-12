[`SpatialIdSet`](crate::collection::SpatialIdSet) と [`SpatialIdMap`](crate::collection::SpatialIdMap) で空間IDをまとめて扱う方法。

大量の空間IDを扱うときは、コレクションにまとめる。

| 型 | 保持するもの | 向いている用途 |
| :--- | :--- | :--- |
| [`SpatialIdSet`](crate::collection::SpatialIdSet) | 空間IDの集合 | 領域の有無の判定、和・積・差の集合演算 |
| [`SpatialIdMap<V>`](crate::collection::SpatialIdMap) | 空間IDごとの値 | リスク値や属性の管理、値による絞り込み |

どちらも重なった空間IDを1つにし、まとめられる部分は大きな単位にまとめて保持する。
そのため、`SingleId` を1個ずつ持つより、少ないメモリで大きな領域を扱える。

# `SpatialIdSet`

## 挿入する

`insert` は [`SpatialId`](crate::SpatialId) を実装した型を受け取るので、`SingleId` と `RangeId` を混ぜて入れられる。

中身を確かめるには `flat_single_ids` を使う。集合を `SingleId` に展開して返す。
引数は時間の絞り込みで、`None` なら全時間を対象にする（[5章](crate::_tutorial::_05_temporal)）。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::collection::SpatialIdSet;
use spatial_id::{RangeId, SingleId};

let mut set = SpatialIdSet::new();
set.insert(SingleId::new(20, 9, 931389, 412900)?);
set.insert(RangeId::new(20, 9, [931390, 931391], 412900)?);

for id in set.flat_single_ids(None) {
    println!("{id},");
}
# Ok(())
# }
```

```console
20/9/931389/412900,
20/9/931390/412900,
20/9/931391/412900,
```

展開せずに取り出すときは `iter` を使う。
内部でまとまっている単位ごとに `RangeId` で返る。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdSet;
# use spatial_id::{RangeId, SingleId};
# let mut set = SpatialIdSet::new();
# set.insert(SingleId::new(20, 9, 931389, 412900)?);
# set.insert(RangeId::new(20, 9, [931390, 931391], 412900)?);
for range in set.iter() {
    println!("{range},");
}
# Ok(())
# }
```

```console
20/9/931389/412900,
20/9/931390:931391/412900,
```

## 取得する

`get` は、指定した範囲と重なる部分のうち、集合に実際に含まれる部分だけを返す。
戻り値は `RangeId` で返る。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdSet;
# use spatial_id::{RangeId, SingleId};
# let mut set = SpatialIdSet::new();
# set.insert(SingleId::new(20, 9, 931389, 412900)?);
# set.insert(RangeId::new(20, 9, [931390, 931391], 412900)?);
// 931390〜931392 を問い合わせる。931392 は集合に含まれない
let target = RangeId::new(20, 9, [931390, 931392], 412900)?;
for found in set.get(target) {
    println!("{found},");
}
# Ok(())
# }
```

```console
20/9/931390:931391/412900,
```

![集合（青）と問い合わせる範囲（橙）から、重なる部分（紫）だけが返る][collection_get]

## 削除する

`remove` は、指定した範囲と重なる部分を集合から取り除き、取り除いた部分を返す。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdSet;
# use spatial_id::{RangeId, SingleId};
# let mut set = SpatialIdSet::new();
# set.insert(SingleId::new(20, 9, 931389, 412900)?);
# set.insert(RangeId::new(20, 9, [931390, 931391], 412900)?);
let target = RangeId::new(20, 9, [931389, 931390], 412900)?;
for removed in set.remove(target) {
    println!("{removed},");
}

// 931391 だけが残る
for rest in set.flat_single_ids(None) {
    println!("{rest},");
}
# Ok(())
# }
```

```console
20/9/931389/412900,
20/9/931390/412900,
20/9/931391/412900,
```

## 集合演算

`SpatialIdSet` は `|`（和）・`&`（積）・`-`（差）の演算子に対応している。
東京駅の空間IDを中心に、X 方向に3個並んだ集合 `a` と、Y 方向に3個並んだ集合 `b` で試す。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::collection::SpatialIdSet;
use spatial_id::RangeId;

let mut a = SpatialIdSet::new();
a.insert(RangeId::new(20, 9, [931388, 931390], 412900)?);

let mut b = SpatialIdSet::new();
b.insert(RangeId::new(20, 9, 931389, [412899, 412901])?);

let print_ids = |s: SpatialIdSet| {
    for id in s.flat_single_ids(None) {
        println!("{id},");
    }
};

// 和（十字形の5個）・積（中心の1個）・差（a から中心を除いた2個）の順に出力する
print_ids(&a | &b);
println!();
print_ids(&a & &b);
println!();
print_ids(&a - &b);
# Ok(())
# }
```

```console
20/9/931389/412899,
20/9/931388/412900,
20/9/931389/412900,
20/9/931389/412901,
20/9/931390/412900,

20/9/931389/412900,

20/9/931388/412900,
20/9/931390/412900,
```

| 和 `&a \| &b` | 積 `&a & &b` | 差 `&a - &b` |
| :---: | :---: | :---: |
| ![和][query_merge] | ![積][query_intersection] | ![差][query_difference] |

図は、青の集合と橙の集合（各図の左）から求めた結果（右）である。

# `SpatialIdMap`

`SpatialIdMap<V>` は、空間IDごとに値 `V` を持つ。
値の型には [`PartialEq`] と [`Clone`] が必要である。

## 挿入する

`insert` は、すでに値がある場所も上書きする。
`upsert` は、まだ値がない場所にだけ挿入する。

```rust
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::collection::SpatialIdMap;
use spatial_id::SingleId;

let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);

map.upsert(SingleId::new(20, 9, 931389, 412900)?, 99); // 値があるので変わらない
map.upsert(SingleId::new(20, 9, 931391, 412900)?, 80); // 新しく入る

for (id, v) in map.flat_single_ids(None) {
    println!("({id}, {v}),");
}
# Ok(())
# }
```

```console
(20/9/931389/412900, 30),
(20/9/931390/412900, 80),
(20/9/931391/412900, 80),
```

同じ値を持つ空間IDは、まとめられる部分が自動で1つにまとまる。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::SingleId;
# let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
# map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
# map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);
# map.insert(SingleId::new(20, 9, 931391, 412900)?, 80);
for (range, v) in map.iter() {
    println!("({range}, {v}),");
}
# Ok(())
# }
```

```console
(20/9/931389/412900, 30),
(20/9/931390:931391/412900, 80),
```

## 取得と削除

`get` と `remove` は `SpatialIdSet` と同じく、指定した範囲と重なる部分を値と組にして返す。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::{RangeId, SingleId};
# let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
# map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
# map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);
# map.insert(SingleId::new(20, 9, 931391, 412900)?, 80);
let target = RangeId::new(20, 9, [931389, 931390], 412900)?;
for (range, v) in map.get(target) {
    println!("({range}, {v}),");
}

println!();

for (range, v) in map.remove(SingleId::new(20, 9, 931390, 412900)?) {
    println!("({range}, {v}),");
}
println!();

for (id, v) in map.flat_single_ids(None) {
    println!("({id}, {v}),");
}
# Ok(())
# }
```

```console
(20/9/931389/412900, 30),
(20/9/931390/412900, 80),

(20/9/931390/412900, 80),

(20/9/931389/412900, 30),
(20/9/931391/412900, 80),
```

## 値で絞り込む

値の型が [`Ord`] を実装していれば、値の範囲や値の集合で絞り込める。
これらはマップ全体を走査して判定する。

```rust
# fn main() -> Result<(), spatial_id::Error> {
# use spatial_id::collection::SpatialIdMap;
# use spatial_id::SingleId;
# let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
# map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
# map.insert(SingleId::new(20, 9, 931390, 412900)?, 80);
# map.insert(SingleId::new(20, 9, 931391, 412900)?, 80);
// 最小値と最大値
println!("{:?}", map.value_range());

// 値が 50 以上の部分だけを取り出した新しいマップ
let high = map.filter_range(50..);
for (id, v) in high.flat_single_ids(None) {
    println!("({id}, {v}),");
}
# Ok(())
# }
```

```console
Some((30, 80))
(20/9/931390/412900, 80),
(20/9/931391/412900, 80),
```

![中心ほど値が大きいマップ（左）から、値が 40 以上 80 未満の部分を取り出した例（右）。赤ほど値が大きい][query_filter_in]

# JSON への保存と復元

`unstable` feature を有効にすると、`SpatialIdSet` と `SpatialIdMap` を [空間ID JSON スキーマ](https://airbee-project.github.io/schemas/json/v1.0.json) の形式で読み書きできる。
`serde` に対応しているので、`serde_json` でそのまま文字列にできる。

```rust
# #[cfg(feature = "unstable")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use spatial_id::collection::SpatialIdMap;
use spatial_id::SingleId;

let mut map: SpatialIdMap<u32> = SpatialIdMap::new();
map.insert(SingleId::new(20, 9, 931389, 412900)?, 30);
map.insert(SingleId::new(20, 9, 931391, 412900)?, 80);

let json = serde_json::to_string(&map)?;
let restored: SpatialIdMap<u32> = serde_json::from_str(&json)?;

for (id, v) in restored.flat_single_ids(None) {
    println!("({id}, {v}),");
}
# Ok(())
# }
# #[cfg(not(feature = "unstable"))]
# fn main() {}
```

```console
(20/9/931389/412900, 30),
(20/9/931391/412900, 80),
```

書き出した JSON では、同じ値を持つ空間IDが `value` 配列の1つの要素を `ref` で参照する。
値の重複が省かれるので、ファイルが小さくなる。

```json
{
  "$schema": "https://airbee-project.github.io/schemas/json/v1.0.json",
  "meta": { "version": "v1.0", "description": "" },
  "option": {},
  "data": [
    {
      "name": "",
      "value": [30, 80],
      "ids": [
        { "z": 20, "f": [9], "x": [931389], "y": [412900], "ref": 0 },
        { "z": 20, "f": [9], "x": [931391], "y": [412900], "ref": 1 }
      ]
    }
  ]
}
```
