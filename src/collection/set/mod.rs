use crate::collection::core::{FlexTreeCore, IntoIter};
use crate::{AllowedIntervals, FlexId, RangeId, SingleId, SpatialId};
use core::iter::Map;
pub(crate) mod convert;
pub(crate) mod impls;
#[cfg(feature = "unstable")]
pub(crate) mod json;
pub(crate) mod ops;
#[cfg(test)]
mod tests;

/// 空間IDの集合を表す型。
///
/// [`SpatialIdSet`] は、保持する値が空間IDそのものだけであるため、「どの空間が存在するか」を表すための型として機能する。
///
/// - ある場所に対する空間IDを「存在しない」もしくは「一意に定まる」状態を維持する
/// - 集合同士の演算や、集合に対する単項演算を提供する
///
/// # 使い分け
/// - 空間ごとに値を持たせたい場合は [`SpatialIdMap`](crate::SpatialIdMap) を使用する。
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct SpatialIdSet {
    pub(in crate::collection) inner: FlexTreeCore<()>,
}

impl SpatialIdSet {
    /// 新しい集合を作成する。
    ///
    /// # Examples
    ///
    /// ```
    /// use spatial_id::collection::SpatialIdSet;
    ///
    /// let set = SpatialIdSet::new();
    /// assert!(set.is_empty());
    /// ```
    pub fn new() -> Self {
        SpatialIdSet::default()
    }

    /// 集合に対して空間IDを挿入する。[SpatialId] Traitが実装されていれば挿入ができる。
    /// 挿入した際に重なりがある空間IDが既に存在する場合は自動的に重なりを解消する。
    ///
    /// # Examples
    ///
    /// ```
    /// use spatial_id::collection::SpatialIdSet;
    /// use spatial_id::{FlexId, RangeId, SingleId};
    ///
    /// let mut set = SpatialIdSet::new();
    ///
    /// // SingleId の挿入
    /// let single = SingleId::new(23, 0, 7451089, 3303245).unwrap();
    /// set.insert(single);
    ///
    /// // RangeId の挿入
    /// let range = RangeId::new(23, [0, 0], [7451089, 7451089], [3303245, 3303245]).unwrap();
    /// set.insert(range);
    ///
    /// // FlexId の挿入
    /// let flex = FlexId::new(23, 0, 24, 7451089, 23, 3303245).unwrap();
    /// set.insert(flex);
    /// ```
    pub fn insert<S: SpatialId>(&mut self, target: S) {
        self.inner.insert(target, ());
    }

    /// 集合から指定した空間IDと重なる空間IDを切り出して返す。
    pub fn get<'a, S: SpatialId<IntoIter: 'a>>(
        &'a self,
        target: S,
    ) -> impl Iterator<Item = FlexId> + 'a {
        self.inner.get(target).map(|(flex_id, _)| flex_id)
    }

    /// 集合から指定した空間IDと重なる空間IDを切り出して削除する。削除した部分の空間IDを返す。
    ///
    /// 削除は呼び出した時点で済んでおり、返すイテレーターは集合を借用しない。
    pub fn remove<S: SpatialId>(&mut self, target: S) -> impl Iterator<Item = FlexId> + use<S> {
        self.inner
            .remove(target)
            .into_iter()
            .map(|(flex_id, _)| flex_id)
    }

    /// 指定した空間IDと接触していたすべての空間IDを返す。
    /// [`get`](Self::get) と異なり切り取りを行わず、target と重なった [`FlexId`] をそのままの返す。
    pub fn get_overlapping<S: SpatialId>(&self, target: S) -> impl Iterator<Item = FlexId> + '_ {
        self.inner
            .get_overlapping([target])
            .map(|(flex_id, _)| flex_id)
    }

    /// 指定した空間IDと接触していたすべての空間IDを削除する。削除した空間IDを返す。
    /// [`remove`](Self::remove) と異なり切り取りを行わず、target と重なった [`FlexId`] をそのまま返す。
    ///
    /// 削除は呼び出した時点で済んでおり、返すイテレーターは集合を借用しない。
    pub fn remove_overlapping<S: SpatialId>(
        &mut self,
        target: S,
    ) -> impl Iterator<Item = FlexId> + use<S> {
        self.inner
            .remove_overlapping(target)
            .into_iter()
            .map(|(flex_id, _)| flex_id)
    }

    /// 指定した単体の空間 IDと面で接している[`FlexId`] を重複なく返す。入力された空間ID自身と重なる空間IDは除外する。
    pub fn spatial_face_neighbors<S: SpatialId>(
        &self,
        target: S,
    ) -> impl Iterator<Item = FlexId> + '_ {
        self.inner
            .spatial_face_neighbors(target)
            .map(|(flex_id, _)| flex_id)
    }

    /// 集合の内部にある[FlexId]の個数を返す。
    pub fn count(&self) -> usize {
        self.inner.count()
    }

    /// 集合の内部にある全ての[FlexId]のうち、最大のズームレベル値を返す。
    /// 内部に空間 ID が存在しない場合は [`None`] を返す。
    ///
    /// キャッシュを持たず、全ての[FlexId]を走査する（O(n)）。
    pub fn max_zoomlevel(&self) -> Option<u8> {
        self.inner.max_zoomlevel()
    }

    /// 時間方向に結合した [`RangeId`] として読み出す。**空間解像度は変えない**。
    ///
    /// `allowed_intervals` が [`None`] なら、各区間はそれを表せる最も粗い単位（`gcd(開始秒, 幅)`）の1Segmentになる。
    /// [`AllowedIntervals`] を渡すと、その候補のうち**区間を割り切る最も粗いもの**で表す
    /// （＝候補の中でSegment数が最小）。[`AllowedIntervals`] は必ず全区間を表せる候補を含むので失敗しない。
    ///
    /// [`iter`](Self::iter) が返す生の [`FlexId`] は木の2分岐Segmentそのもの
    /// （`_8/182185424` のような断片）なので、人間が読む用途にはこちらを使う。
    ///
    /// ```
    /// # #[cfg(feature = "temporal_id")]
    /// # {
    /// # use spatial_id::collection::SpatialIdSet;
    /// # use spatial_id::{SingleId, time::{AllowedIntervals, Interval}};
    /// let mut set = SpatialIdSet::new();
    /// set.insert(SingleId::new(12, 0, 3638, 1614).unwrap().with_time(Interval::HOUR, 0).unwrap());
    /// set.insert(SingleId::new(12, 0, 3638, 1614).unwrap().with_time(Interval::HOUR, 1).unwrap());
    ///
    /// // 既定（gcd）では 2 時間ぶんが「7200 秒 × 1 TimeSegment」になる。
    /// assert_eq!(set.reconstructed_time_ranges(None).next().unwrap().to_string(), "12/0/3638/1614_7200/0");
    ///
    /// // 暦の単位に正規化すると「3600 秒 × 2 TimeSegment」になる。
    /// let got = set.reconstructed_time_ranges(Some(AllowedIntervals::calendar())).next().unwrap();
    /// assert_eq!(got.to_string(), "12/0/3638/1614_3600/0:1");
    /// # }
    /// ```
    pub fn reconstructed_time_ranges<'a>(
        &'a self,
        allowed_intervals: Option<&'a AllowedIntervals>,
    ) -> impl Iterator<Item = RangeId> + 'a {
        self.inner
            .reconstructed_time_ranges(allowed_intervals)
            .map(|(range_id, _)| range_id)
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を、集合全体の最大ズームレベルに揃えた [`SingleId`] へ展開する。
    pub fn flat_single_ids<'a>(
        &'a self,
        allowed_intervals: Option<&'a AllowedIntervals>,
    ) -> impl Iterator<Item = SingleId> + 'a {
        self.inner
            .flat_single_ids(allowed_intervals)
            .map(|(single_id, _)| single_id)
    }

    /// [SpatialIdSet] 内部の空間 ID をすべて削除する。
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// [SpatialIdSet] 内部が空かどうかを判定する。
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = FlexId> + Clone + '_ {
        self.inner.iter().map(|(flex_id, _)| flex_id)
    }
}

impl IntoIterator for SpatialIdSet {
    type Item = FlexId;
    type IntoIter = Map<IntoIter<()>, fn((FlexId, ())) -> FlexId>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter().map(|(flex_id, ())| flex_id)
    }
}
