use crate::collection::disk::core::{FlexTreeCore, IntoIter, Iter};
use crate::collection::disk::{DiskValue, SpatialIdMap};
use crate::collection::mem;
use crate::{AllowedIntervals, Error, FlexId, RangeId, SingleId, SpatialId, ZoomLevel};
use alloc::vec::Vec;
use core::iter::Map;
use std::path::Path;

pub(crate) mod json;
#[cfg(test)]
mod tests;

/// 空間IDの集合を表す、ディスク永続化コレクション（unstable: `disk` feature）。
///
/// 変更は [`commit`](Self::commit) を呼ぶまで確定せず、コミットせずに閉じると直前のコミットに戻る。
/// 不要になった古いページは [`compact`](Self::compact) で詰め直せる。
///
/// # 使い分け
/// - 空間ごとに値を持たせたい場合は [`SpatialIdMap`] を使用する。
/// - メモリ上で扱う場合は [`crate::collection::mem::SpatialIdSet`] を使用する。
///
/// ```
/// # compio::runtime::Runtime::new().unwrap().block_on(async {
/// use spatial_id::collection::disk::SpatialIdSet;
/// use spatial_id::SingleId;
///
/// let path = std::env::temp_dir().join("spatial-id-doctest-disk-set.bin");
/// let _ = std::fs::remove_file(&path);
///
/// let mut set = SpatialIdSet::open(&path).await?;
/// let id = SingleId::new(23, 0, 7451089, 3303245)?;
/// set.insert(id).await?;
/// set.commit().await?;
/// set.close().await?;
///
/// let set = SpatialIdSet::open(&path).await?;
/// assert_eq!(set.get(id).await?.len(), 1);
/// set.close().await?;
/// let _ = std::fs::remove_file(&path);
/// # Ok::<(), spatial_id::Error>(())
/// # }).unwrap();
/// ```
#[derive(Debug)]
pub struct SpatialIdSet {
    pub(in crate::collection) inner: FlexTreeCore<()>,
}

impl SpatialIdSet {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    ///
    /// 別の値の型で作ったファイルなら [`DiskError::Incompatible`](crate::DiskError::Incompatible) を返す。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            inner: FlexTreeCore::open(path).await?,
        })
    }

    /// メモリ版の [`mem::SpatialIdSet`] からディスク版を作成し、コミットして返す。
    ///
    /// `path` に既存のファイルがあれば、値の型に関わらず中身を捨てて作り直す。
    pub async fn from_mem(path: impl AsRef<Path>, set: &mem::SpatialIdSet) -> Result<Self, Error> {
        Ok(Self {
            inner: FlexTreeCore::create_from_mem(path, &set.inner).await?,
        })
    }

    /// 内容をメモリ版の [`mem::SpatialIdSet`] へ非同期に読み出す。
    pub async fn to_mem(&self) -> Result<mem::SpatialIdSet, Error> {
        let ids = self.inner.flex_ids().await?;
        Ok(ids.into_iter().collect())
    }

    /// ディスク版の [`SpatialIdMap`] から値を捨てて占有空間だけの [`SpatialIdSet`] を作成し、コミットして返す。
    ///
    /// `path` に既存のファイルがあれば、値の型に関わらず中身を捨てて作り直す。
    pub async fn from_map<V: DiskValue>(
        path: impl AsRef<Path>,
        map: &SpatialIdMap<V>,
    ) -> Result<Self, Error> {
        let set = map.inner.flex_ids().await?.into_iter().collect();
        Self::from_mem(path, &set).await
    }

    /// 集合に対して空間IDを挿入する。[`SpatialId`] が実装されていれば挿入できる。
    pub async fn insert<S: SpatialId>(&mut self, target: S) -> Result<(), Error> {
        self.inner.insert(target.flex_ids(), ()).await
    }

    /// 集合から指定した空間IDと重なる空間IDを削除する。
    pub async fn remove<S: SpatialId>(&mut self, target: S) -> Result<(), Error> {
        self.inner.remove(target.flex_ids()).await
    }

    /// 集合から指定した空間IDと重なる空間IDを切り出して返す。
    pub async fn get<S: SpatialId>(&self, target: S) -> Result<Vec<RangeId>, Error> {
        Ok(self
            .get_flex_ids(target)
            .await?
            .into_iter()
            .map(RangeId::from)
            .collect())
    }

    /// [`get`](Self::get) と同じだが、結果を [`FlexId`] で返す。
    pub async fn get_flex_ids<S: SpatialId>(&self, target: S) -> Result<Vec<FlexId>, Error> {
        Ok(self
            .inner
            .get(target.flex_ids())
            .await?
            .into_iter()
            .map(|(id, ())| id)
            .collect())
    }

    /// 指定した空間IDと接触していたすべての空間IDを返す。
    /// [`get`](Self::get) と異なり切り取りを行わず、`target` と重なった空間IDをそのまま返す。
    pub async fn get_overlapping<S: SpatialId>(&self, target: S) -> Result<Vec<RangeId>, Error> {
        Ok(self
            .get_overlapping_flex_ids(target)
            .await?
            .into_iter()
            .map(RangeId::from)
            .collect())
    }

    /// [`get_overlapping`](Self::get_overlapping) と同じだが、結果を [`FlexId`] で返す。
    pub async fn get_overlapping_flex_ids<S: SpatialId>(
        &self,
        target: S,
    ) -> Result<Vec<FlexId>, Error> {
        Ok(self
            .inner
            .get_overlapping([target])
            .await?
            .into_iter()
            .map(|(id, ())| id)
            .collect())
    }

    /// 集合の内部にある [`FlexId`] の個数を返す。
    pub async fn count(&self) -> Result<usize, Error> {
        self.inner.count().await
    }

    /// 集合の内部にあるすべての [`FlexId`] のうち、最大のズームレベル値を返す。空なら [`None`]。
    pub async fn max_zoomlevel(&self) -> Result<Option<ZoomLevel>, Error> {
        self.inner.max_zoomlevel().await
    }

    /// 時間方向に結合した [`RangeId`] として読み出す。**ズームレベルは変えない**。
    ///
    /// `allowed_intervals` が [`None`] なら、各区間はそれを表せる最も粗い単位（`gcd(開始秒, 幅)`）の1区間になる。
    /// [`AllowedIntervals`] を渡すと、その候補のうち区間を割り切る最も粗い単位で表す。
    pub async fn reconstructed_time_ranges(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<RangeId>, Error> {
        Ok(self
            .inner
            .reconstructed_time_ranges(allowed_intervals)
            .await?
            .into_iter()
            .map(|(range, ())| range)
            .collect())
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を、集合全体の最大ズームレベルに揃えた [`SingleId`] へ展開する。
    pub async fn flat_single_ids(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<SingleId>, Error> {
        Ok(self
            .inner
            .flat_single_ids(allowed_intervals)
            .await?
            .into_iter()
            .map(|(id, ())| id)
            .collect())
    }

    /// ここまでの変更をファイルに残す。
    pub async fn commit(&mut self) -> Result<(), Error> {
        self.inner.commit().await
    }

    /// ここまでの変更をコミットし、ファイルを生きているページだけに詰め直す。
    pub async fn compact(&mut self) -> Result<(), Error> {
        self.inner.compact().await
    }

    /// ファイルを閉じる。コミットしていない変更は捨てる。
    pub async fn close(self) -> Result<(), Error> {
        self.inner.close().await
    }

    /// 集合の内部の空間IDをすべて削除する。永続化するには [`commit`](Self::commit) を呼ぶ。
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// 集合が空かどうかを判定する。
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 集合が保持するすべての空間IDを、ページ単位で同期的に読み出しながら返す。
    ///
    /// ページ読み込み時に呼び出しスレッドをブロックするため、非同期ランタイムのワーカースレッド上で
    /// 大量に走査する場合は注意すること。
    pub fn iter(&self) -> impl Iterator<Item = Result<RangeId, Error>> + '_ {
        self.iter_flex_ids().map(|res| res.map(RangeId::from))
    }

    /// [`iter`](Self::iter) と同じだが、木が保持する [`FlexId`] をそのまま返す。
    pub fn iter_flex_ids(&self) -> impl Iterator<Item = Result<FlexId, Error>> + '_ {
        self.inner.iter().map(|res| res.map(|(id, ())| id))
    }

    /// [`into_iter`](IntoIterator::into_iter) と同じだが、木が保持する [`FlexId`] をそのまま返す。
    pub fn into_iter_flex_ids(self) -> impl Iterator<Item = Result<FlexId, Error>> {
        self.inner.into_iter().map(|res| res.map(|(id, ())| id))
    }
}

impl IntoIterator for SpatialIdSet {
    type Item = Result<RangeId, Error>;
    type IntoIter = Map<IntoIter<()>, fn(Result<(FlexId, ()), Error>) -> Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner
            .into_iter()
            .map(|res| res.map(|(id, ())| RangeId::from(id)))
    }
}

impl<'a> IntoIterator for &'a SpatialIdSet {
    type Item = Result<RangeId, Error>;
    type IntoIter = Map<Iter<'a, ()>, fn(Result<(FlexId, ()), Error>) -> Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner
            .iter()
            .map(|res| res.map(|(id, ())| RangeId::from(id)))
    }
}
