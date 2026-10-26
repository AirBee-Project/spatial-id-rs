use crate::collection::core::{BitMask, FlexTreeCore, IntoIter, NoSummary, Summary, ValueSet};
use core::ops::RangeBounds;
#[cfg(feature = "unstable")]
pub(crate) mod json;
#[cfg(feature = "rayon")]
pub(crate) mod par;
#[cfg(test)]
mod test;

use crate::{AllowedIntervals, FlexId, RangeId, SingleId, SpatialId};

/// 空間IDごとに値を持つマップ。
///
/// 全ての操作は `S` を指定せずに使える。`S` は木の Branch が子孫の値についてキャッシュする
/// [Summary] で、値で絞り込む操作だけを速くする。値の範囲で絞り込む
/// [`filter_range`](Self::filter_range) を多用するなら [`MinMax`](crate::MinMax) を、
/// enum を種類で絞り込む [`filter_values`](Self::filter_values) を多用するなら [`ValueSet`] を指定する。
///
/// ```
/// use spatial_id::collection::SpatialIdMap;
/// use spatial_id::collection::summary::MinMax;
/// use spatial_id::SingleId;
///
/// let mut table = SpatialIdMap::new();
/// table.insert(SingleId::new(20, 0, 0, 0).unwrap(), 3.5_f64);
///
/// let mut indexed: SpatialIdMap<u32, MinMax<u32>> = SpatialIdMap::default();
/// indexed.insert(SingleId::new(20, 0, 0, 0).unwrap(), 80);
/// assert_eq!(indexed.filter_range(50..).count(), 1);
/// ```
///
/// ## Enum
///
/// enum に `#[derive(BitMask)]` を付け、型パラメーターに [ValueSet] を指定する。
/// これにより内部的にビットマスクが張られてフィルタリングが高速化する
///
/// ```
/// use spatial_id::collection::SpatialIdMap;
/// use spatial_id::collection::summary::{BitMask, ValueSet};
/// use spatial_id::SingleId;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, BitMask)]
/// enum Land {
///     Road,
///     Building,
///     Water,
/// }
///
/// let mut map: SpatialIdMap<Land, ValueSet<Land>> = SpatialIdMap::default();
/// map.insert(SingleId::new(20, 5, 100, 200).unwrap(), Land::Road);
/// map.insert(SingleId::new(20, 5, 101, 200).unwrap(), Land::Water);
///
/// assert!(map.value_set().contains(Land::Water));
/// let obstacles = map.filter_values([Land::Building, Land::Water].into_iter().collect());
/// assert_eq!(obstacles.count(), 1);
/// ```
///
/// ## 範囲と種類の両方で絞る
///
/// 2つの Summary をタプルで組み合わせる。型が長くなるので型エイリアスを付けるとよい。
///
/// ```
/// use spatial_id::collection::SpatialIdMap;
/// use spatial_id::collection::summary::{BitMask, MinMax, ValueSet};
/// use spatial_id::SingleId;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, BitMask)]
/// enum Risk {
///     Low,
///     Mid,
///     High,
/// }
///
/// let mut map = SpatialIdMap::<Risk, (MinMax<Risk>, ValueSet<Risk>)>::default();
/// map.insert(SingleId::new(20, 5, 100, 200).unwrap(), Risk::Low);
/// map.insert(SingleId::new(20, 5, 101, 200).unwrap(), Risk::High);
///
/// assert_eq!(map.filter_range(Risk::Mid..).count(), 1);
/// assert_eq!(map.filter_values(ValueSet::single(Risk::Low)).count(), 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialIdMap<V, S = NoSummary> {
    pub(crate) inner: FlexTreeCore<V, S>,
}

impl<V, S> Default for SpatialIdMap<V, S> {
    fn default() -> Self {
        Self {
            inner: FlexTreeCore::default(),
        }
    }
}

impl<V: PartialEq + Clone> SpatialIdMap<V> {
    /// 空の [SpatialIdMap] を作成する。
    ///
    /// [Summary] を指定するときは [`Default::default`] または [`with_summary`](Self::with_summary) を使う。
    pub fn new() -> Self {
        Self::default()
    }
}

impl<V, S> SpatialIdMap<V, S>
where
    V: PartialEq + Clone,
    S: Summary<V>,
{
    /// 空間に値を挿入する。既に値がある場所は上書きされる。
    pub fn insert<T: SpatialId>(&mut self, target: T, value: V) {
        self.inner.insert(target, value);
    }

    #[cfg(feature = "unstable")]
    /// `region` に収まるシャードを2つの領域に割り、それぞれの領域で切り取った中身を `(下, 上)` で返す。
    /// 選んだ軸がこれ以上割れなければ [`None`]。
    ///
    /// 軸は F→X→Y の順に、各軸のズームの和で巡回して選び、シャードが1方向の薄い板に偏らないようにする。
    /// 全ての値が全時間なら T 軸では割らない。割っても両側が元と同じ件数になり、分割が収束しないため。
    pub fn split_shard(&self, region: FlexId) -> Option<((FlexId, Self), (FlexId, Self))> {
        let spatial = u32::from(region.f_zoomlevel())
            + u32::from(region.x_zoomlevel())
            + u32::from(region.y_zoomlevel());
        let split: fn(&FlexId, crate::Side) -> Option<FlexId> =
            if self.iter().all(|(id, _)| id.is_whole_time()) {
                match spatial % 3 {
                    0 => FlexId::split_f,
                    1 => FlexId::split_x,
                    _ => FlexId::split_y,
                }
            } else {
                match (spatial + u32::from(region.t_zoomlevel())) % 4 {
                    0 => FlexId::split_f,
                    1 => FlexId::split_x,
                    2 => FlexId::split_y,
                    _ => FlexId::split_t,
                }
            };
        let half = |side: crate::Side| {
            let part = split(&region, side)?;
            Some((
                part,
                self.get(part).map(|(id, v)| (id, v.clone())).collect(),
            ))
        };
        Some((half(crate::Side::Lower)?, half(crate::Side::Upper)?))
    }

    /// まだ値のない場所にだけ挿入する（Upsert）。既に値がある場所はそのまま保つ。
    pub fn upsert<T: SpatialId>(&mut self, target: T, value: V) {
        self.inner
            .insert_with(target, value, |existing, _| existing.clone());
    }

    /// 指定した空間と重なる領域を、指定した空間との共通部分に切り取って値への参照とともに返す。
    pub fn get<'a, T: SpatialId<IntoIter: 'a>>(
        &'a self,
        target: T,
    ) -> impl Iterator<Item = (FlexId, &'a V)> + 'a {
        self.inner.get(target)
    }

    /// 指定した空間（target）をツリーからくり抜き、削除された領域とその値を返す。
    ///
    /// 削除は呼び出した時点で済んでおり、返すイテレーターはテーブルを借用しない。
    pub fn remove<T: SpatialId>(&mut self, target: T) -> IntoIter<V, S> {
        self.inner.remove(target).into_iter()
    }

    /// [`get`](Self::get) と異なり切り取りを行わず、target と重なった
    /// [`FlexId`] と値をそのまま返す。
    pub fn get_overlapping<T: SpatialId>(
        &self,
        target: T,
    ) -> impl Iterator<Item = (FlexId, &V)> + '_ {
        self.inner.get_overlapping([target])
    }

    /// [`remove`](Self::remove) と異なり切り取りを行わず、target と重なった
    /// [`FlexId`] と値をそのまま取り除いて返す。
    ///
    /// 削除は呼び出した時点で済んでおり、返すイテレーターはテーブルを借用しない。
    pub fn remove_overlapping<T: SpatialId>(&mut self, target: T) -> IntoIter<V, S> {
        self.inner.remove_overlapping(target).into_iter()
    }

    /// 指定した単体の空間 ID と面で接している [`FlexId`] と値への参照を重複なく返す。入力された空間 ID 自身と重なる要素は除外する。
    pub fn spatial_face_neighbors<T: SpatialId>(
        &self,
        target: T,
    ) -> impl Iterator<Item = (FlexId, &V)> + '_ {
        self.inner.spatial_face_neighbors(target)
    }

    /// 保持している [FlexId] の総数を返す。
    pub fn count(&self) -> usize {
        self.inner.count()
    }

    /// 保持しているすべての [FlexId] のうち、最大のズームレベル値を返す。空なら [`None`]。
    ///
    /// キャッシュを持たず、すべての [FlexId] を走査する（O(n)）。
    pub fn max_zoomlevel(&self) -> Option<u8> {
        self.inner.max_zoomlevel()
    }

    /// 時間方向に隣接する同値のSegmentを結合した [`RangeId`] と値を返す。**空間解像度は変えない**。
    ///
    /// `allowed_intervals` が [`None`] なら、各区間はそれを表せる最も粗い単位（`gcd(開始秒, 幅)`）の1Segmentになる。
    /// [`AllowedIntervals`] を渡すと、その候補のうち区間を割り切る最も粗い単位で表す。
    pub fn reconstructed_time_ranges<'a>(
        &'a self,
        allowed_intervals: Option<&'a AllowedIntervals>,
    ) -> impl Iterator<Item = (RangeId, &'a V)> + 'a {
        self.inner.reconstructed_time_ranges(allowed_intervals)
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を、テーブル全体の最大ズームレベルに揃えた [`SingleId`] へ展開する。
    pub fn flat_single_ids<'a>(
        &'a self,
        allowed_intervals: Option<&'a AllowedIntervals>,
    ) -> impl Iterator<Item = (SingleId, &'a V)> + 'a {
        self.inner.flat_single_ids(allowed_intervals)
    }

    /// テーブルが空かどうかを判定する。
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 中身はそのままに、[Summary] を `T` に付け替えたテーブルを作成する。
    pub fn with_summary<T: Summary<V>>(&self) -> SpatialIdMap<V, T> {
        SpatialIdMap {
            inner: self.inner.with_summary(),
        }
    }

    /// テーブルのすべての値を削除する。
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// テーブルに保持されているすべての空間と値への参照のペアを返す。
    pub fn iter(&self) -> impl Iterator<Item = (FlexId, &V)> + Clone + '_ {
        self.inner.iter()
    }
}

impl<V, S> SpatialIdMap<V, S>
where
    V: Ord + Clone,
    S: Summary<V>,
{
    /// テーブル全体に存在する値の範囲 `(最小, 最大)`。空なら [`None`]。
    ///
    /// [`MinMax`](crate::MinMax) を持つテーブルなら O(1)、そうでなければ全ての値を走査する。
    pub fn value_range(&self) -> Option<(&V, &V)> {
        self.inner.value_range()
    }

    /// 値が `range` に含まれる領域だけを残したテーブルを作る。
    ///
    /// [`MinMax`](crate::MinMax) を持つテーブルなら、値がすべて `range` の内側・外側にある部分は辿らない。
    pub fn filter_range<R: RangeBounds<V>>(&self, range: R) -> Self {
        Self {
            inner: self.inner.filter_range(range),
        }
    }
}

impl<V, S> SpatialIdMap<V, S>
where
    V: BitMask + PartialEq + Clone,
    S: Summary<V>,
{
    /// テーブル全体に現れる値の集合。
    ///
    /// [`ValueSet`] を持つテーブルなら O(1)、そうでなければ全ての値を走査する。
    pub fn value_set(&self) -> ValueSet<V> {
        self.inner.value_set()
    }

    /// 値が `values` に含まれる領域だけを残したテーブルを作る。
    ///
    /// [`ValueSet`] を持つテーブルなら、値がすべて `values` の内側・外側にある部分は辿らない。
    pub fn filter_values(&self, values: ValueSet<V>) -> Self {
        Self {
            inner: self.inner.filter_values(values),
        }
    }
}

impl<V: Clone, S> IntoIterator for SpatialIdMap<V, S> {
    type Item = (FlexId, V);
    type IntoIter = IntoIter<V, S>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<V: PartialEq + Clone, S: Summary<V>> FromIterator<(FlexId, V)> for SpatialIdMap<V, S> {
    fn from_iter<T: IntoIterator<Item = (FlexId, V)>>(iter: T) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}

impl<V: PartialEq + Clone, S: Summary<V>> Extend<(FlexId, V)> for SpatialIdMap<V, S> {
    fn extend<T: IntoIterator<Item = (FlexId, V)>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}
