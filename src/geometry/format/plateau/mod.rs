//! [PLATEAU](https://www.mlit.go.jp/plateau/) の CityGML（EPSG:6697）。

pub mod bldg;
mod element;
mod geometry;
mod parser;
#[cfg(test)]
mod test;
pub mod tran;

use alloc::vec::Vec;
use std::io::BufRead;

use crate::{Error, SingleId, SpatialId, SpatialIdMap};
pub use element::Element;
use geometry::LodRings;
use parser::{Attribute, Parser};

/// 属性と形状を持つ地物。XML の読み込みと、ファイル全体の被覆を共通にする。
trait Feature: Sized + Send + Sync {
    type Attribute: Attribute + Send + Sync;

    /// [`SpatialIdMap`] の構築に使う ID。範囲で求められる形状は範囲のまま渡し、展開を避ける。
    type Id: SpatialId + Send;

    fn new(attribute: Self::Attribute, lods: LodRings) -> Self;

    fn attribute(&self) -> &Self::Attribute;

    fn single_ids(&self, z: u8) -> Result<impl Iterator<Item = SingleId>, Error>;

    fn map_ids(&self, z: u8) -> Result<impl Iterator<Item = Self::Id>, Error>;

    /// XML の地物をすべて読む。形状の検証が重いので、rayon があれば並列に変換する。
    fn read_all<R: BufRead>(reader: R) -> Result<Vec<Self>, Error> {
        let parsed = Parser::<R, Self::Attribute>::new(reader).collect::<Result<Vec<_>, _>>()?;
        let new = |(attribute, lods)| Self::new(attribute, lods);
        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            Ok(parsed.into_par_iter().map(new).collect())
        }
        #[cfg(not(feature = "rayon"))]
        {
            Ok(parsed.into_iter().map(new).collect())
        }
    }

    /// 地物ごとに `f` を 1 回だけ評価し、`ids` で求めた各 ID と組にする。
    fn cover_all<'a, I, V>(
        features: &'a [Self],
        mut f: impl FnMut(&'a Self::Attribute) -> V,
        ids: impl Fn(&Self) -> Result<Vec<I>, Error> + Sync,
    ) -> Result<impl Iterator<Item = (I, V)> + 'a, Error>
    where
        I: Send + 'a,
        V: Clone + 'a,
    {
        let values: Vec<V> = features.iter().map(|t| f(t.attribute())).collect();
        #[cfg(feature = "rayon")]
        let ids: Vec<Vec<I>> = {
            use rayon::prelude::*;
            features.par_iter().map(&ids).collect::<Result<_, _>>()?
        };
        #[cfg(not(feature = "rayon"))]
        let ids: Vec<Vec<I>> = features.iter().map(&ids).collect::<Result<_, _>>()?;
        Ok(ids
            .into_iter()
            .zip(values)
            .flat_map(|(ids, value)| ids.into_iter().map(move |id| (id, value.clone()))))
    }

    /// 複数の地物が同じ ID を覆う場合は、後の地物の値が残る。
    fn cover_map_all<'a, V>(
        features: &'a [Self],
        z: u8,
        mut f: impl FnMut(&'a Self::Attribute) -> V,
    ) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
    {
        let pairs = features.iter().map(|t| (t, f(t.attribute())));

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            // 後勝ちを保つため、順序を崩さない reduce で和を取る。
            pairs
                .collect::<Vec<_>>()
                .into_par_iter()
                .map(|(t, value)| {
                    let mut map = SpatialIdMap::new();
                    for id in t.map_ids(z)? {
                        map.insert(id, value.clone());
                    }
                    Ok(map)
                })
                .try_reduce(SpatialIdMap::new, |a, b| Ok(b.union(&a)))
        }

        #[cfg(not(feature = "rayon"))]
        {
            let mut map = SpatialIdMap::new();
            for (t, value) in pairs {
                for id in t.map_ids(z)? {
                    map.insert(id, value.clone());
                }
            }
            Ok(map)
        }
    }
}
