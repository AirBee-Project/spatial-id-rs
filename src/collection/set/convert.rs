use crate::{SpatialIdMap, SpatialIdSet};

impl<V> From<&SpatialIdMap<V>> for SpatialIdSet
where
    V: PartialEq + Clone,
{
    /// 値を捨て、占有空間だけを [`SpatialIdSet`] へ写し取る。元のテーブルは消費しない。
    fn from(table: &SpatialIdMap<V>) -> Self {
        table.iter().map(|(flex_id, _)| flex_id).collect()
    }
}

impl<V> From<SpatialIdMap<V>> for SpatialIdSet
where
    V: PartialEq + Clone,
{
    /// [`SpatialIdMap`] を、値を捨てて占有空間だけを持つ [`SpatialIdSet`] へ変換する。
    ///
    /// # 動作例
    ///
    /// 値付きテーブルから集合へ:
    /// ```
    /// use spatial_id::collection::{SpatialIdMap, SpatialIdSet};
    /// use spatial_id::SingleId;
    /// let mut table: SpatialIdMap<u8> = SpatialIdMap::new();
    /// table.insert(SingleId::new(20, 0, 0, 0).unwrap(), 7);
    /// table.insert(SingleId::new(20, 5, 0, 0).unwrap(), 9);
    ///
    /// let set = SpatialIdSet::from(table);
    /// assert!(set.get(SingleId::new(20, 0, 0, 0).unwrap()).next().is_some());
    /// assert!(set.get(SingleId::new(20, 5, 0, 0).unwrap()).next().is_some());
    /// assert!(set.get(SingleId::new(20, 9, 0, 0).unwrap()).next().is_none());
    /// ```
    fn from(table: SpatialIdMap<V>) -> Self {
        Self::from(&table)
    }
}
