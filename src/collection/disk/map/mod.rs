use crate::collection::disk::DiskValue;
use crate::collection::disk::core::{FlexTreeCore, IntoIter, Iter};
use crate::collection::mem;
use crate::{AllowedIntervals, Error, FlexId, RangeId, SingleId, SpatialId, ZoomLevel};
use alloc::vec::Vec;
use core::iter::Map;
use std::path::Path;

pub(crate) mod json;
#[cfg(test)]
mod tests;

/// 空間IDごとに値を持つ、ディスク永続化マップ（unstable: `disk` feature）。
///
/// 変更は [`commit`](Self::commit) を呼ぶまで確定せず、コミットせずに閉じると直前のコミットに戻る。
/// 不要になった古いページは [`compact`](Self::compact) で詰め直せる。
///
/// ```
/// # compio::runtime::Runtime::new().unwrap().block_on(async {
/// use spatial_id::collection::disk::SpatialIdMap;
/// use spatial_id::SingleId;
///
/// let path = std::env::temp_dir().join("spatial-id-doctest-disk-map.bin");
/// let _ = std::fs::remove_file(&path);
///
/// let mut map = SpatialIdMap::<u64>::open(&path).await?;
/// let id = SingleId::new(20, 0, 0, 0)?;
/// map.insert(id, 42).await?;
/// map.commit().await?;
/// map.close().await?;
///
/// let map = SpatialIdMap::<u64>::open(&path).await?;
/// assert_eq!(map.get(id).await?.len(), 1);
/// map.close().await?;
/// let _ = std::fs::remove_file(&path);
/// # Ok::<(), spatial_id::Error>(())
/// # }).unwrap();
/// ```
#[derive(Debug)]
pub struct SpatialIdMap<V> {
    pub(in crate::collection) inner: FlexTreeCore<V>,
}

impl<V: DiskValue> SpatialIdMap<V> {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    ///
    /// 別の値の型で作ったファイルなら [`DiskError::Incompatible`](crate::DiskError::Incompatible) を返す。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            inner: FlexTreeCore::open(path).await?,
        })
    }

    /// メモリ版の [`mem::SpatialIdMap`] からディスク版を作成し、コミットして返す。
    ///
    /// `path` に既存のファイルがあれば、値の型に関わらず中身を捨てて作り直す。
    pub async fn from_mem(
        path: impl AsRef<Path>,
        map: &mem::SpatialIdMap<V>,
    ) -> Result<Self, Error> {
        Ok(Self {
            inner: FlexTreeCore::create_from_mem(path, &map.inner).await?,
        })
    }

    /// 内容をメモリ版の [`mem::SpatialIdMap`] へ非同期に読み出す。
    pub async fn to_mem(&self) -> Result<mem::SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone,
    {
        let entries = self.inner.entries().await?;
        Ok(entries.into_iter().collect())
    }

    /// 空間に値を挿入する。既に値がある場所は上書きされる。
    pub async fn insert<T: SpatialId>(&mut self, target: T, value: V) -> Result<(), Error> {
        self.inner.insert(target.flex_ids(), value).await
    }

    /// まだ値のない場所にだけ挿入する（Upsert）。既に値がある場所はそのまま保つ。
    pub async fn upsert<T: SpatialId>(&mut self, target: T, value: V) -> Result<(), Error> {
        self.inner.upsert(target.flex_ids(), value).await
    }

    /// 指定した空間をツリーから削除する。
    pub async fn remove<T: SpatialId>(&mut self, target: T) -> Result<(), Error> {
        self.inner.remove(target.flex_ids()).await
    }

    /// 指定した空間と重なる領域を、指定した空間との共通部分に切り取って値とともに返す。
    pub async fn get<T: SpatialId>(&self, target: T) -> Result<Vec<(RangeId, V)>, Error> {
        Ok(self
            .get_flex_ids(target)
            .await?
            .into_iter()
            .map(|(id, v)| (RangeId::from(id), v))
            .collect())
    }

    /// [`get`](Self::get) と同じだが、結果を [`FlexId`] で返す。
    pub async fn get_flex_ids<T: SpatialId>(&self, target: T) -> Result<Vec<(FlexId, V)>, Error> {
        self.inner.get(target.flex_ids()).await
    }

    /// [`get`](Self::get) と異なり切り取りを行わず、`target` と重なった空間IDと値をそのまま返す。
    pub async fn get_overlapping<T: SpatialId>(
        &self,
        target: T,
    ) -> Result<Vec<(RangeId, V)>, Error> {
        Ok(self
            .get_overlapping_flex_ids(target)
            .await?
            .into_iter()
            .map(|(id, v)| (RangeId::from(id), v))
            .collect())
    }

    /// [`get_overlapping`](Self::get_overlapping) と同じだが、結果を [`FlexId`] で返す。
    pub async fn get_overlapping_flex_ids<T: SpatialId>(
        &self,
        target: T,
    ) -> Result<Vec<(FlexId, V)>, Error> {
        self.inner.get_overlapping([target]).await
    }

    /// 時間方向に隣接する同値の [`FlexId`] を結合した [`RangeId`] と値を返す。**ズームレベルは変えない**。
    ///
    /// `allowed_intervals` が [`None`] なら、各区間はそれを表せる最も粗い単位（`gcd(開始秒, 幅)`）の1区間になる。
    /// [`AllowedIntervals`] を渡すと、その候補のうち区間を割り切る最も粗い単位で表す。
    pub async fn reconstructed_time_ranges(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<(RangeId, V)>, Error> {
        self.inner
            .reconstructed_time_ranges(allowed_intervals)
            .await
    }

    /// [`reconstructed_time_ranges`](Self::reconstructed_time_ranges) を、マップ全体の最大ズームレベルに揃えた [`SingleId`] へ展開する。
    pub async fn flat_single_ids(
        &self,
        allowed_intervals: Option<&AllowedIntervals>,
    ) -> Result<Vec<(SingleId, V)>, Error>
    where
        V: Clone,
    {
        self.inner.flat_single_ids(allowed_intervals).await
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

    /// マップに保持されているすべての空間と値のペアを、ページ単位で同期的に読み出しながら返す。
    ///
    /// ページ読み込み時に呼び出しスレッドをブロックするため、非同期ランタイムのワーカースレッド上で
    /// 大量に走査する場合は注意すること。
    pub fn iter(&self) -> impl Iterator<Item = Result<(RangeId, V), Error>> + '_ {
        self.iter_flex_ids()
            .map(|res| res.map(|(id, v)| (RangeId::from(id), v)))
    }

    /// [`iter`](Self::iter) と同じだが、木が保持する [`FlexId`] をそのまま返す。
    pub fn iter_flex_ids(&self) -> impl Iterator<Item = Result<(FlexId, V), Error>> + '_ {
        self.inner.iter()
    }

    /// [`into_iter`](IntoIterator::into_iter) と同じだが、木が保持する [`FlexId`] をそのまま返す。
    pub fn into_iter_flex_ids(self) -> IntoIter<V> {
        self.inner.into_iter()
    }
}

impl<V> SpatialIdMap<V> {
    /// 保持している [`FlexId`] の総数を返す。
    pub async fn count(&self) -> Result<usize, Error> {
        self.inner.count().await
    }

    /// 保持しているすべての [`FlexId`] のうち、最大のズームレベル値を返す。空なら [`None`]。
    pub async fn max_zoomlevel(&self) -> Result<Option<ZoomLevel>, Error> {
        self.inner.max_zoomlevel().await
    }

    /// マップが空かどうかを判定する。
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// マップのすべての値を削除する。永続化するには [`commit`](Self::commit) を呼ぶ。
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<V: DiskValue + Ord> SpatialIdMap<V> {
    /// マップ全体に存在する値の範囲 `(最小, 最大)`。空なら [`None`]。
    pub async fn value_range(&self) -> Result<Option<(V, V)>, Error> {
        self.inner.value_range().await
    }
}

impl<V: DiskValue> IntoIterator for SpatialIdMap<V> {
    type Item = Result<(RangeId, V), Error>;
    type IntoIter = Map<IntoIter<V>, fn(Result<(FlexId, V), Error>) -> Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner
            .into_iter()
            .map(|res| res.map(|(id, value)| (RangeId::from(id), value)))
    }
}

impl<'a, V: DiskValue> IntoIterator for &'a SpatialIdMap<V> {
    type Item = Result<(RangeId, V), Error>;
    type IntoIter = Map<Iter<'a, V>, fn(Result<(FlexId, V), Error>) -> Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner
            .iter()
            .map(|res| res.map(|(id, value)| (RangeId::from(id), value)))
    }
}
