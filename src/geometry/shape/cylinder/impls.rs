use alloc::vec::Vec;

use crate::{
    Coordinate, CoverRangeIds, CoverSingleIds, Cylinder, Error, RangeId, Shape, SingleId,
    SpatialIdMap,
};

impl Shape for Cylinder {
    fn center(&self) -> Coordinate {
        Coordinate::centroid([self.start, self.end])
    }
}

/// 円柱（左）を、それが覆う空間ID（右）へ変換する。図はズームレベル 21 の例。
///
/// ![円柱と、それが覆う空間ID][cylinder_cover]
#[doc = doc_image!("cylinder_cover", "docs/images/cylinder_cover.jpg")]
impl CoverSingleIds for Cylinder {
    type Value<'a> = ();

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        let solid = self.rough_solid();
        #[allow(clippy::needless_collect)]
        let ids: Vec<_> = solid.cover_single_ids(z)?.collect();
        Ok(ids.into_iter().map(move |id| (id, value.clone())))
    }

    fn cover_map_with<'a, V, F>(&'a self, z: impl Into<u8>, f: F) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        // SingleId へ展開せず RangeId のままツリーを構築して負荷を抑える。
        let mut map = SpatialIdMap::new();
        for (id, value) in self.cover_range_ids_with(z, f)? {
            map.insert(id, value);
        }
        Ok(map)
    }
}

/// 円柱（左）を、それが覆う [`RangeId`](crate::RangeId)（右）へ変換する。図はズームレベル 21 の例。
///
/// ![円柱と、それが覆う範囲ID][cylinder_cover_range]
#[doc = doc_image!("cylinder_cover_range", "docs/images/cylinder_cover_range.jpg")]
impl CoverRangeIds for Cylinder {
    fn cover_range_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (RangeId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        let solid = self.rough_solid();
        #[allow(clippy::needless_collect)]
        let ids: Vec<_> = solid.cover_range_ids(z)?.collect();
        Ok(ids.into_iter().map(move |id| (id, value.clone())))
    }
}
