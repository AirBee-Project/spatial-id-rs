use ::geojson::{Feature, GeoJson, Geometry, GeometryValue, Position};
use alloc::vec::Vec;
use hashbrown::HashSet;

use crate::{
    Coordinate, CoverSingleIds, Epsilon, Error, GeometryError, Line, Point, Polygon, SingleId,
    SpatialIdMap,
};

mod projected_ring;
#[cfg(test)]
mod test;

use projected_ring::ProjectedRing;

/// GeoJSON の全ジオメトリが覆う空間IDを求める。
///
/// - Position は `[経度, 緯度, 高度]` として読み、高度が省略された場合は 0 m とする。
/// - Polygon の穴は、穴の内部を除き、穴の境界線は含める。
/// - `geometry` が `null` の Feature は無視する。
/// - 値の元は Feature で、Geometry だけの GeoJSON では Feature がないので [`None`] になる。
/// - [`CoverSingleIds::cover_single_ids_with`] では空間IDへの展開前に Feature 単位で値を評価する。
/// - [`CoverSingleIds::cover_map_with`] では直接 [`SpatialIdMap`] を構築する。
///   複数の Feature が同じ ID を覆う場合は、後の Feature の値が残る。
///
/// # 動作例
/// ```
/// # use spatial_id::geometry::CoverSingleIds;
/// let geojson: geojson::GeoJson = r#"{
///     "type": "FeatureCollection",
///     "features": [{
///         "type": "Feature",
///         "properties": { "name": "A" },
///         "geometry": { "type": "Point", "coordinates": [139.0, 35.0] }
///     }]
/// }"#.parse().unwrap();
///
/// let map = geojson.cover_map_with(20, |feature| {
///     feature.unwrap().property("name").unwrap().as_str().unwrap().to_owned()
/// }).unwrap();
/// assert!(map.iter().all(|(_, v)| v == "A"));
/// ```
impl CoverSingleIds for GeoJson {
    type Value<'a> = Option<&'a Feature>;

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let z = z.into();
        let mut pairs = Vec::new();
        match self {
            GeoJson::Geometry(geometry) => {
                let value = f(None);
                for id in geometry.cover_single_ids(z)? {
                    pairs.push((id, value.clone()));
                }
            }
            GeoJson::Feature(feature) => {
                if let Some(geometry) = &feature.geometry {
                    let value = f(Some(feature));
                    for id in geometry.cover_single_ids(z)? {
                        pairs.push((id, value.clone()));
                    }
                }
            }
            GeoJson::FeatureCollection(collection) => {
                for feature in &collection.features {
                    if let Some(geometry) = &feature.geometry {
                        let value = f(Some(feature));
                        for id in geometry.cover_single_ids(z)? {
                            pairs.push((id, value.clone()));
                        }
                    }
                }
            }
        }
        Ok(pairs.into_iter())
    }

    fn cover_map_with<'a, V, F>(&'a self, z: impl Into<u8>, f: F) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        to_feature_collection_map(self, z.into(), f)
    }
}

#[cfg(not(feature = "rayon"))]
fn to_feature_collection_map<'a, V, F>(
    geojson: &'a GeoJson,
    z: u8,
    mut f: F,
) -> Result<SpatialIdMap<V>, Error>
where
    V: PartialEq + Clone,
    F: FnMut(Option<&'a Feature>) -> V,
{
    let mut map = SpatialIdMap::new();
    match geojson {
        GeoJson::Geometry(geometry) => {
            let value = f(None);
            for id in geometry.cover_single_ids(z)? {
                map.insert(id, value.clone());
            }
        }
        GeoJson::Feature(feature) => {
            if let Some(geometry) = &feature.geometry {
                let value = f(Some(feature));
                for id in geometry.cover_single_ids(z)? {
                    map.insert(id, value.clone());
                }
            }
        }
        GeoJson::FeatureCollection(collection) => {
            for feature in &collection.features {
                let geometry = match &feature.geometry {
                    Some(g) => g,
                    None => continue,
                };
                let value = f(Some(feature));
                for id in geometry.cover_single_ids(z)? {
                    map.insert(id, value.clone());
                }
            }
        }
    }
    Ok(map)
}

#[cfg(feature = "rayon")]
fn to_feature_collection_map<'a, V, F>(
    geojson: &'a GeoJson,
    z: u8,
    mut f: F,
) -> Result<SpatialIdMap<V>, Error>
where
    V: PartialEq + Clone + Send + Sync,
    F: FnMut(Option<&'a Feature>) -> V,
{
    use rayon::prelude::*;
    let later_wins = |a: SpatialIdMap<V>, b: SpatialIdMap<V>| b.union(&a);

    match geojson {
        GeoJson::Geometry(geometry) => {
            let value = f(None);
            let mut map = SpatialIdMap::new();
            for id in geometry.cover_single_ids(z)? {
                map.insert(id, value.clone());
            }
            Ok(map)
        }
        GeoJson::Feature(feature) => {
            let mut map = SpatialIdMap::new();
            if let Some(geometry) = &feature.geometry {
                let value = f(Some(feature));
                for id in geometry.cover_single_ids(z)? {
                    map.insert(id, value.clone());
                }
            }
            Ok(map)
        }
        GeoJson::FeatureCollection(collection) => {
            let pairs: Vec<(&Geometry, V)> = collection
                .features
                .iter()
                .filter_map(|feature| {
                    let geometry = feature.geometry.as_ref()?;
                    Some((geometry, f(Some(feature))))
                })
                .collect();

            pairs
                .into_par_iter()
                .map(|(geometry, value)| {
                    let mut map = SpatialIdMap::new();
                    for id in geometry.cover_single_ids(z)? {
                        map.insert(id, value.clone());
                    }
                    Ok(map)
                })
                .try_reduce(SpatialIdMap::new, |a, b| Ok(later_wins(a, b)))
        }
    }
}

impl CoverSingleIds for Geometry {
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
        let z = z.into();
        let mut ids = HashSet::new();
        match &self.value {
            GeometryValue::Point { coordinates } => {
                ids.insert(Coordinate::try_from(coordinates)?.single_id(z)?);
            }
            GeometryValue::MultiPoint { coordinates } => {
                for position in coordinates {
                    ids.insert(Coordinate::try_from(position)?.single_id(z)?);
                }
            }
            GeometryValue::LineString { coordinates } => {
                let coords: Vec<Coordinate> = coordinates
                    .iter()
                    .map(Coordinate::try_from)
                    .collect::<Result<_, _>>()?;
                ids.extend(Line::cover_polyline(&coords, z)?);
            }
            GeometryValue::MultiLineString { coordinates } => {
                for line_string in coordinates {
                    let coords: Vec<Coordinate> = line_string
                        .iter()
                        .map(Coordinate::try_from)
                        .collect::<Result<_, _>>()?;
                    ids.extend(Line::cover_polyline(&coords, z)?);
                }
            }
            GeometryValue::Polygon { coordinates } => {
                ids.extend(cover_polygon(coordinates, z)?);
            }
            GeometryValue::MultiPolygon { coordinates } => {
                for rings in coordinates {
                    ids.extend(cover_polygon(rings, z)?);
                }
            }
            GeometryValue::GeometryCollection { geometries } => {
                for geometry in geometries {
                    ids.extend(geometry.cover_single_ids(z)?);
                }
            }
        }
        Ok(ids.into_iter().map(move |id| (id, value.clone())))
    }
}

/// 外周リングと穴（内周リング）からなる Polygon が覆う空間IDを求める。
///
/// 穴が外周の内側に収まっていない場合は [`GeometryError::HoleOutsideExterior`] を返す。
fn cover_polygon(rings: &[Vec<Position>], z: u8) -> Result<HashSet<SingleId>, Error> {
    let mut polygons = rings
        .iter()
        .map(|ring| {
            let coords = ring
                .iter()
                .map(Coordinate::try_from)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Polygon::new(coords))
        })
        .collect::<Result<Vec<_>, Error>>()?
        .into_iter();

    let Some(exterior) = polygons.next() else {
        return Ok(HashSet::new());
    };
    if exterior.vertices().is_empty() {
        return Ok(HashSet::new());
    }

    let mut ids: HashSet<SingleId> = exterior.cover_single_ids(z)?.collect();
    let mut projected = None;
    for (index, hole) in polygons.enumerate() {
        let vertices = hole.vertices();
        let Some(&first) = vertices.first() else {
            continue;
        };
        let projected = projected.get_or_insert_with(|| ProjectedRing::new(exterior.vertices()));
        if !projected.contains_ring(vertices, Epsilon::DEFAULT.as_meters()) {
            return Err(GeometryError::HoleOutsideExterior { index }.into());
        }

        let mut closed = vertices.clone();
        closed.push(first);
        let boundary: HashSet<SingleId> = Line::cover_polyline(&closed, z)?.collect();
        for id in hole.cover_single_ids(z)? {
            if !boundary.contains(&id) {
                ids.remove(&id);
            }
        }
    }
    Ok(ids)
}

impl TryFrom<&Position> for Coordinate {
    type Error = Error;

    fn try_from(position: &Position) -> Result<Self, Self::Error> {
        if position.len() < 2 {
            return Err(GeometryError::PositionTooShort {
                len: position.len(),
            }
            .into());
        }
        let lon = position[0];
        let lat = position[1];
        let alt = if position.len() > 2 { position[2] } else { 0.0 };
        Coordinate::new(lat, lon, alt)
    }
}
