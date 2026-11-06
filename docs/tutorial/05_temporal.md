空間IDに時間を加えたものが時空間IDである。
「この空間を、この時間帯に」という単位でデータを扱えるので、ドローンの飛行計画や時間帯ごとの規制などを表せる。

時空間IDは `temporal_id` feature（既定で有効）で使える。

# 時間を持つ空間ID

時空間IDの文字列は空間IDのあとに `_{i}/{t}` を付けた形である。

- `i`: 時間間隔（秒）。`3600` なら1時間単位。
- `t`: 時間インデックス。1970-01-01 00:00 UTC から数えて何番目の時間間隔か。

時間は型を変えずに `time_with` で付ける。

```rust
# #[cfg(feature = "temporal_id")]
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::{SingleId, SpatialId};

// 1時間単位で 494000 番目（2026-05-10 08:00〜09:00 UTC）
let id = SingleId::new(20, 9, 931389, 412900)?.time_with(3600, 494000)?;
println!("{id}");

// 表す時間を Unix 秒の区間 [start, end) で得る
println!("{:?}", id.seconds_range());

// ある時刻を含むか
println!("{}", id.time_contains(1_778_401_234));
# Ok(())
# }
# #[cfg(not(feature = "temporal_id"))]
# fn main() {}
```

```console
20/9/931389/412900_3600/494000
(1778400000, 1778403600)
true
```

時間間隔には [`Interval`](crate::time::Interval) の定数も使える。
`time_with_at` を使うと、Unix 秒から時間インデックスを計算してくれる。

```rust
# #[cfg(feature = "temporal_id")]
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::time::Interval;
use spatial_id::SingleId;

let id = SingleId::new(20, 9, 931389, 412900)?.time_with_at(Interval::HOUR, 1_778_401_234)?;
println!("{id}");
# Ok(())
# }
# #[cfg(not(feature = "temporal_id"))]
# fn main() {}
```

```console
20/9/931389/412900_3600/494000
```

時間を付けていない空間IDは、全時間を表す。

## 時間の範囲

`RangeId` では、時間インデックスも範囲で指定できる。

```rust
# #[cfg(feature = "temporal_id")]
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::time::Interval;
use spatial_id::RangeId;

// 3時間分
let range = RangeId::new(20, 9, 931389, 412900)?.time_with(Interval::HOUR, [494000, 494002])?;
println!("{range}");
# Ok(())
# }
# #[cfg(not(feature = "temporal_id"))]
# fn main() {}
```

```console
20/9/931389/412900_3600/494000:494002
```

# コレクションで時間を扱う

コレクションにはそのまま時空間IDを入れられる。
`get` は、空間と時間の両方が重なる部分だけを返す。

```rust
# #[cfg(feature = "temporal_id")]
# fn main() -> Result<(), spatial_id::Error> {
use spatial_id::collection::SpatialIdSet;
use spatial_id::time::Interval;
use spatial_id::{RangeId, SingleId};

let mut set = SpatialIdSet::new();
set.insert(RangeId::new(20, 9, 931389, 412900)?.time_with(Interval::HOUR, [494000, 494002])?);

// 同じ空間でも、時間が重ならなければ何も返らない
let inside = SingleId::new(20, 9, 931389, 412900)?.time_with(3600, 494001)?;
let outside = SingleId::new(20, 9, 931389, 412900)?.time_with(3600, 494005)?;
println!("494001 と重なる: {}", set.get(inside).next().is_some());
println!("494005 と重なる: {}", set.get(outside).next().is_some());

// 読み出すと、元の時間間隔の表記に戻る
for range in set.reconstructed_time_ranges(None) {
    println!("{range},");
}
# Ok(())
# }
# #[cfg(not(feature = "temporal_id"))]
# fn main() {}
```

```console
494001 と重なる: true
494005 と重なる: false
20/9/931389/412900_3600/494000:494002,
```

![時間の範囲を持つ時空間IDと、get で問い合わせる時刻の関係][temporal_range]

コレクションの内部では時間を2のべき乗秒の区間に分けて持っている。
`reconstructed_time_ranges` と `flat_single_ids` は読み出すときに隣り合う区間をつなぎ、元の表記に戻して返す。
引数に [`AllowedIntervals`](crate::time::AllowedIntervals) を渡すと、戻すときに使う時間間隔を選べる。

# 時間を使わないとき

空間IDだけを扱うなら、`temporal_id` feature を無効にする。
空間IDから時間の情報がなくなり、メモリの使用量も減る。

```toml
[dependencies]
spatial-id = { version = "0.1", default-features = false, features = ["std", "rayon"] }
```

時間の扱いの詳細な仕様は [`time`](crate::time) モジュールにまとめている。

次章: [6. FlexTree の仕組み](crate::_tutorial::_06_flextree)
