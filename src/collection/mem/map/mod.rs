use crate::collection::core::{FlexTreeCore, IntoIter};
use alloc::collections::BTreeSet;
#[cfg(feature = "unstable")]
use core::hash::Hash;
use core::iter::FusedIterator;
use core::ops::RangeBounds;
#[cfg(feature = "unstable")]
pub(crate) mod json;
#[cfg(feature = "rayon")]
pub(crate) mod par;
#[cfg(test)]
mod test;

use crate::{AllowedIntervals, FlexId, RangeId, SingleId, SpatialId, ZoomLevel};

/// [`SpatialIdMap`] の各メソッドが返すイテレーター。
///
/// 既定では [`RangeId`] と値の組を返す。
/// 木が保持する [`FlexId`] のまま受け取るには [`MapIter::flex_ids`]、
/// 各要素を [`SingleId`] へ展開するには [`MapIter::single_ids`] を使う。
#[derive(Clone, Debug)]
pub struct MapIter<I>(I);

impl<I> MapIter<I> {
    pub(crate) fn new(inner: I) -> Self {
        Self(inner)
    }

    /// 木が保持する [`FlexId`] をそのまま返すイテレーターへ切り替える。
    pub fn flex_ids(self) -> I {
        self.0
    }
}

impl<V, I> MapIter<I>
where
    V: Clone,
    I: Iterator<Item = (FlexId, V)>,
{
    /// 各要素を [`SingleId`] と値の組へ展開して返すイテレーターへ切り替える。
    pub fn single_ids(self) -> impl Iterator<Item = (SingleId, V)> {
        self.0
            .flat_map(|(id, value)| id.single_ids().map(move |single| (single, value.clone())))
    }
}

impl<V, I> Iterator for MapIter<I>
where
    I: Iterator<Item = (FlexId, V)>,
{
    type Item = (RangeId, V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(id, v)| (RangeId::from(id), v))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item> {
        self.0.last().map(|(id, v)| (RangeId::from(id), v))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|(id, v)| (RangeId::from(id), v))
    }

    #[inline]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        self.0
            .fold(init, |acc, (id, v)| f(acc, (RangeId::from(id), v)))
    }
}

impl<V, I> ExactSizeIterator for MapIter<I>
where
    I: ExactSizeIterator<Item = (FlexId, V)>,
{
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<V, I> FusedIterator for MapIter<I> where I: FusedIterator<Item = (FlexId, V)> {}

/// 空間IDごとに値を持つマップ。
///
/// ```
/// use spatial_id::collection::SpatialIdMap;
/// use spatial_id::SingleId;
///
/// let mut table = SpatialIdMap::new();
/// table.insert(SingleId::new(20, 0, 0, 0).unwrap(), 3.5_f64);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialIdMap<V> {
    pub(in crate::collection) inner: FlexTreeCore<V>,
}

impl<V> Default for SpatialIdMap<V> {
    fn default() -> Self {
        Self {
            inner: FlexTreeCore::default(),
        }
    }
}

impl<V: PartialEq + Clone> SpatialIdMap<V> {
    /// 空の [SpatialIdMap] を作成する。
    pub fn new() -> Self {
        Self::default()
    }
}

impl<V> SpatialIdMap<V>
where
    V: PartialEq + Clone,
{
    /// 空間に値を挿入する。既に値がある場所は上書きされる。
    pub fn insert<T: SpatialId>(&mut self, target: T, value: V) {
        self.inner.insert(target.flex_ids(), value);
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
            if self.iter().flex_ids().all(|(id, _)| id.time_is_whole()) {
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
                self.get(part)
                    .flex_ids()
                    .map(|(id, v)| (id, v.clone()))
                    .collect(),
            ))
        };
        Some((half(crate::Side::Lower)?, half(crate::Side::Upper)?))
    }

    /// まだ値のない場所にだけ挿入する（Upsert）。既に値がある場所はそのまま保つ。
    pub fn upsert<T: SpatialId>(&mut self, target: T, value: V) {
        self.insert_with(target, value, |existing, _| existing.clone());
    }

    /// `target` の領域に `value` を挿入する。既に値がある場所は `resolve(既存の値, value)` にする。
    ///
    pub fn insert_with(&mut self, target: impl SpatialId, value: V, resolve: impl Fn(&V, &V) -> V) {
        self.inner.insert_with(target.flex_ids(), value, resolve);
    }

    /// [`insert_with`](Self::insert_with) と同じだが、[`FlexId`] の列を渡せる。列の各領域へ同じ `value` を、複製せずに書き込む。
    pub fn insert_with_flex_ids(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: V,
        resolve: impl Fn(&V, &V) -> V,
    ) {
        self.inner.insert_with(target, value, resolve);
    }

    /// `other` を重ねる。両方に値がある場所は `resolve(self の値, other の値)` にする。
    pub fn merge_with(&mut self, other: &Self, resolve: impl Fn(&V, &V) -> V) {
        self.inner.merge_with(&other.inner, resolve);
    }

    /// 和を作る。両方に値がある場所は `self` の値を使う。
    pub fn union(&self, other: &Self) -> Self {
        Self {
            inner: self.inner.union(&other.inner),
        }
    }

    /// `other` にも値がある場所だけ、`self` の値を残したテーブルを作る。
    pub fn intersection<W: PartialEq>(&self, other: &SpatialIdMap<W>) -> Self {
        Self {
            inner: self.inner.intersection(&other.inner),
        }
    }

    /// `other` に値がある場所を `self` から取り除いたテーブルを作る。
    pub fn difference<W: PartialEq>(&self, other: &SpatialIdMap<W>) -> Self {
        Self {
            inner: self.inner.difference(&other.inner),
        }
    }

    /// 指定した空間と重なる領域を、指定した空間との共通部分に切り取って値への参照とともに返す。
    pub fn get<'a, T: SpatialId + 'a>(
        &'a self,
        target: T,
    ) -> MapIter<impl Iterator<Item = (FlexId, &'a V)> + 'a> {
        MapIter::new(self.inner.get(target.flex_ids()))
    }

    /// 指定した空間（target）をツリーからくり抜き、削除された領域とその値を返す。
    ///
    /// 削除は呼び出した時点で済んでおり、返すイテレーターはテーブルを借用しない。
    pub fn remove<T: SpatialId>(&mut self, target: T) -> MapIter<IntoIter<V>> {
        MapIter::new(self.inner.remove(target.flex_ids()).into_iter())
    }

    /// [`get`](Self::get) と異なり切り取りを行わず、target と重なった
    /// 空間IDと値をそのまま返す。
    pub fn get_overlapping<T: SpatialId>(
        &self,
        target: T,
    ) -> MapIter<impl Iterator<Item = (FlexId, &V)> + '_> {
        MapIter::new(self.inner.get_overlapping([target]))
    }

    /// [`remove`](Self::remove) と異なり切り取りを行わず、target と重なった
    /// 空間IDと値をそのまま取り除いて返す。
    ///
    /// 削除は呼び出した時点で済んでおり、返すイテレーターはテーブルを借用しない。
    pub fn remove_overlapping<T: SpatialId>(&mut self, target: T) -> MapIter<IntoIter<V>> {
        MapIter::new(self.inner.remove_overlapping(target.flex_ids()).into_iter())
    }

    /// 指定した空間IDと面で接している空間IDと値への参照を重複なく返す。入力された空間ID自身と重なる要素は除外する。
    pub fn spatial_face_neighbors<T: SpatialId>(
        &self,
        target: T,
    ) -> MapIter<impl Iterator<Item = (FlexId, &V)> + '_> {
        MapIter::new(self.inner.spatial_face_neighbors(target))
    }

    /// 保持している [FlexId] の総数を返す。
    pub fn count(&self) -> usize {
        self.inner.count()
    }

    /// 保持しているすべての [FlexId] のうち、最大のズームレベル値を返す。空なら [`None`]。
    ///
    /// キャッシュを持たず、すべての [FlexId] を走査する（O(n)）。
    pub fn max_zoomlevel(&self) -> Option<ZoomLevel> {
        self.inner.max_zoomlevel()
    }

    /// 時間方向に隣接する同値の [`FlexId`] を結合した [`RangeId`] と値を返す。**ズームレベルは変えない**。
    ///
    /// `allowed_intervals` が [`None`] なら、各区間はそれを表せる最も粗い単位（`gcd(開始秒, 幅)`）の1区間になる。
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

    /// テーブルのすべての値を削除する。
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// テーブルに保持されているすべての空間と値への参照のペアを返す。
    pub fn iter(
        &self,
    ) -> MapIter<impl ExactSizeIterator<Item = (FlexId, &V)> + FusedIterator + Clone + '_> {
        MapIter::new(self.inner.iter())
    }
}

impl<V: Ord + Clone> SpatialIdMap<V> {
    /// テーブル全体に存在する値の範囲 `(最小, 最大)`。空なら [`None`]。
    pub fn value_range(&self) -> Option<(&V, &V)> {
        self.inner.value_range()
    }

    /// 値が `range` に含まれる領域だけを残したテーブルを作る。
    pub fn filter_range<R: RangeBounds<V>>(&self, range: R) -> Self {
        Self {
            inner: self.inner.filter_range(range),
        }
    }

    /// 値が `values` に含まれる領域だけを残したテーブルを作る。
    pub fn filter_values(&self, values: &BTreeSet<V>) -> Self {
        Self {
            inner: self.inner.filter_values(values),
        }
    }
}

#[cfg(feature = "unstable")]
impl<V: Eq + Hash + Clone> SpatialIdMap<V> {
    /// 同じ形状と値を持つ領域群でメモリ上の実体を共有し、メモリ使用量を削減する。
    ///
    /// 保持している空間IDと値の対応は変わらない。既にクローン等で他のインスタンスと共有されている領域があっても、
    /// 他のインスタンスには影響しない。圧縮後に変更すると、共有している領域はその時点で複製される。
    ///
    /// [`Eq`] で等しい値は同一とみなし、いずれか1つのインスタンスを共有する。
    /// 一部のフィールドだけを比較する [`Eq`] 実装では、どのインスタンスが残るかは規定しない。
    ///
    /// ユニークな内部構成要素数を n として O(n) 時間で動作し、実行中のみ O(n) の作業メモリを使用する。
    pub fn compact(&mut self) {
        self.inner.compact();
    }
}

impl<V: Clone> IntoIterator for SpatialIdMap<V> {
    type Item = (RangeId, V);
    type IntoIter = MapIter<IntoIter<V>>;

    fn into_iter(self) -> Self::IntoIter {
        MapIter::new(self.inner.into_iter())
    }
}

/// `(SpatialId, V)` 列から作る。[`SingleId`] / [`RangeId`] / [`FlexId`] のいずれも渡せる。
impl<S: SpatialId, V: PartialEq + Clone> FromIterator<(S, V)> for SpatialIdMap<V> {
    fn from_iter<T: IntoIterator<Item = (S, V)>>(iter: T) -> Self {
        Self {
            inner: iter
                .into_iter()
                .flat_map(|(id, value)| id.flex_ids().map(move |flex_id| (flex_id, value.clone())))
                .collect(),
        }
    }
}

impl<S: SpatialId, V: PartialEq + Clone> Extend<(S, V)> for SpatialIdMap<V> {
    fn extend<T: IntoIterator<Item = (S, V)>>(&mut self, iter: T) {
        self.inner
            .extend(iter.into_iter().flat_map(|(id, value)| {
                id.flex_ids().map(move |flex_id| (flex_id, value.clone()))
            }));
    }
}
