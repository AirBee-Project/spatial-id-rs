use crate::{Error, RangeId, SingleId, SpatialIdMap, SpatialIdSet};

/// 図形が覆う [`SingleId`] を求めるTrait。
pub trait CoverSingleIds {
    /// ID と組にする値の元となる型。値を持たない図形では `()`。
    type Value<'a>
    where
        Self: 'a;

    // 実装では `f` を図形（GeoJSON なら Feature）ごとに 1 回だけ評価し、得た値を各 ID に clone して付ける。
    // 空間IDに展開した後で ID ごとに評価すると、ID の数だけ `f` が走って重くなる。
    /// 図形が覆う [`SingleId`] を、`f` で変換した値と組にしてイテレータとして返す。
    /// 空間IDより前の時点で `f` を評価し、値を紐づける。
    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a;

    /// 対象の図形が覆う [`SingleId`] を、その ID に対応する値と組にして返す。
    fn cover_single_ids_with_value<'a>(
        &'a self,
        z: impl Into<u8>,
    ) -> Result<impl Iterator<Item = (SingleId, Self::Value<'a>)> + 'a, Error>
    where
        Self::Value<'a>: Clone,
    {
        self.cover_single_ids_with(z, |v| v)
    }

    /// 対象の図形が覆う [`SingleId`] を返す。
    fn cover_single_ids(&self, z: impl Into<u8>) -> Result<impl Iterator<Item = SingleId>, Error> {
        Ok(self.cover_single_ids_with(z, |_| ())?.map(|(id, _)| id))
    }

    /// 図形が覆う空間IDを、`f` で変換した値と組にして [`SpatialIdMap`] にまとめる。
    fn cover_map_with<'a, V, F>(&'a self, z: impl Into<u8>, f: F) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        // 実装型が CoverRangeIds を実装している場合は、SingleId へ展開せずに
        // CoverRangeIds::cover_range_ids_with を用いてオーバーライドしたほうが効率的。
        let mut map = SpatialIdMap::new();
        for (id, value) in self.cover_single_ids_with(z, f)? {
            map.insert(id, value);
        }
        Ok(map)
    }

    /// 図形が覆う空間IDを、その ID に対応する値と組にして [`SpatialIdMap`] にまとめる。
    fn cover_map<'a>(&'a self, z: impl Into<u8>) -> Result<SpatialIdMap<Self::Value<'a>>, Error>
    where
        Self::Value<'a>: PartialEq + Clone + Send + Sync,
    {
        self.cover_map_with(z, |v| v)
    }

    /// 対象の図形が覆う [`SingleId`] を [`SpatialIdSet`] にまとめる。
    fn cover_set(&self, z: impl Into<u8>) -> Result<SpatialIdSet, Error> {
        Ok(self.cover_map_with(z, |_| ())?.into())
    }
}

/// 図形が覆う [`RangeId`] を求めるTrait。
///
/// [`CoverSingleIds`] の結果を単純に [`RangeId`] へ変換するラッパーではなく、
/// 実装内部で [`RangeId`] の出力を活かす処理を持つこと。
pub trait CoverRangeIds: CoverSingleIds {
    // 実装では `f` を図形（GeoJSON なら Feature）ごとに 1 回だけ評価し、得た値を各 ID に clone して付ける。
    // 空間IDに展開した後で ID ごとに評価すると、ID の数だけ `f` が走って重くなる。
    /// 図形が覆う [`RangeId`] を、`f` で変換した値と組にしてイテレータとして返す。
    /// 空間IDより前の時点で `f` を評価し、値を紐づける。
    fn cover_range_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        f: F,
    ) -> Result<impl Iterator<Item = (RangeId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a;

    /// 対象の図形が覆う [`RangeId`] を、その ID に対応する値と組にして返す。
    fn cover_range_ids_with_value<'a>(
        &'a self,
        z: impl Into<u8>,
    ) -> Result<impl Iterator<Item = (RangeId, Self::Value<'a>)> + 'a, Error>
    where
        Self::Value<'a>: Clone,
    {
        self.cover_range_ids_with(z, |v| v)
    }

    /// 対象の図形が覆う [`RangeId`] を返す。
    fn cover_range_ids(&self, z: impl Into<u8>) -> Result<impl Iterator<Item = RangeId>, Error> {
        Ok(self.cover_range_ids_with(z, |_| ())?.map(|(id, _)| id))
    }
}
