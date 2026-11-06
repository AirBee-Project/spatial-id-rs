//! PLATEAU の建築物（`bldg`）。
//!
//! [`BldgFile`] で建築物の XML ファイル（`*_bldg_*.gml`）を読み込み、[`CoverSingleIds`] や
//! [`CoverRangeIds`] で空間IDに変換する。値には建築物ごとの [`BldgAttribute`] の参照が渡る。
//!
//! 建築物は LOD の高い順に、閉じた立体になる形状を使い、内部まで覆う。
//! どの LOD でも閉じなければ、面だけを覆う（[`BldgShape::Surfaces`]）。
//!
//! # 動作例
//! ```no_run
//! use spatial_id::geometry::CoverSingleIds;
//! use spatial_id::geometry::format::plateau::bldg::{BldgFile, BldgUsage};
//! use std::{fs::File, io::BufReader};
//!
//! let file = File::open("53394598_bldg_6697_op.gml").unwrap();
//! let bldg = BldgFile::from_reader(BufReader::new(file)).unwrap();
//!
//! // 空間ごとの建築物の用途。商業施設が占める空間だけを取り出す。
//! let usages = bldg.cover_map_with(24, |attribute| attribute.usage).unwrap();
//! let commercial = usages.filter_values(&[Some(BldgUsage::Commercial)].into());
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use core::str::FromStr;
use std::io::BufRead;

use super::Feature;
use super::geometry::{LodRings, Surfaces};
use super::parser::Attribute;
use crate::{
    CoverRangeIds, CoverSingleIds, Error, Polygon, RangeId, SingleId, Solid, SpatialIdMap,
};

/// 建築物の XML ファイル（`*_bldg_*.gml`）全体。
///
/// 複数の建築物が同じ ID を覆う場合、[`CoverSingleIds::cover_map_with`] では後の建築物の値が残る。
#[derive(Debug, Clone, Default)]
pub struct BldgFile {
    pub buildings: Vec<Building>,
}

impl BldgFile {
    /// XML を読み込む。XML として読めない場合だけエラーを返す。
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Self, Error> {
        Ok(Self {
            buildings: Building::read_all(reader)?,
        })
    }
}

impl FromStr for BldgFile {
    type Err = Error;

    fn from_str(xml: &str) -> Result<Self, Error> {
        Self::from_reader(xml.as_bytes())
    }
}

/// PLATEAU の建築物 1 件。
#[derive(Debug, Clone)]
pub struct Building {
    pub attribute: BldgAttribute,
    pub shape: BldgShape,
}

/// 建築物の形状。
#[derive(Debug, Clone)]
pub enum BldgShape {
    /// 閉じた立体。内部まで覆う。
    Solid(Solid),
    /// どの LOD でも閉じた立体にならなかった建築物の、最も高い LOD の面。
    Surfaces(Vec<Polygon>),
}

impl BldgShape {
    /// LOD の高い順に、閉じた立体になるものを使う。
    fn from_lods(lods: LodRings) -> Self {
        let mut highest = None;
        for polygons in lods.highest_first() {
            // Solid::new は面を消費するので、立体にならなかったときに使う最も高い LOD の面を残す。
            if highest.is_none() {
                highest = Some(polygons.clone());
            }
            if let Ok(solid) = Solid::new(polygons) {
                return Self::Solid(solid);
            }
        }
        Self::Surfaces(highest.unwrap_or_default())
    }
}

/// PLATEAU の建築物の属性。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct BldgAttribute {
    pub gml_id: String,
    pub uro_building_id: String,
    pub uro_city_code: String,
    pub class_code: String,
    pub measured_height: Option<f64>,
    pub lod1_height_type: Option<i32>,
    pub uro_prefecture_code: Option<String>,
    pub usage: Option<BldgUsage>,
}

/// PLATEAU の建築物の用途（`bldg:usage` のコード）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub enum BldgUsage {
    /// 1000
    Residential,
    /// 1010
    Commercial,
    /// 1020
    Public,
    /// 1030
    Industrial,
    /// 1040
    Agricultural,
    /// 1050
    Sports,
    /// 1070
    TrafficOrStorage,
    /// 3001
    OrdinaryBuilding,
    /// 3002
    RobustBuilding,
    /// 3003
    OrdinaryWallLess,
    /// 3004
    RobustWallLess,
    /// 411: 物品販売業店舗
    Retail,
    /// 412: 飲食店
    Restaurant,
    /// 413: 卸売業店舗
    Wholesale,
    /// 454: 遊技場
    Amusement,
    /// 461: 宿泊施設
    Hotel,
    /// 1060, 3000: 分類しない
    Unclassified,
    /// 上記以外のコード
    Other(i32),
}

impl BldgUsage {
    fn from_code(code: i32) -> Self {
        match code {
            1000 => Self::Residential,
            1010 => Self::Commercial,
            1020 => Self::Public,
            1030 => Self::Industrial,
            1040 => Self::Agricultural,
            1050 => Self::Sports,
            1060 | 3000 => Self::Unclassified,
            1070 => Self::TrafficOrStorage,
            411 => Self::Retail,
            412 => Self::Restaurant,
            413 => Self::Wholesale,
            454 => Self::Amusement,
            461 => Self::Hotel,
            3001 => Self::OrdinaryBuilding,
            3002 => Self::RobustBuilding,
            3003 => Self::OrdinaryWallLess,
            3004 => Self::RobustWallLess,
            _ => Self::Other(code),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Field {
    BuildingId,
    City,
    Class,
    MeasuredHeight,
    Lod1HeightType,
    Prefecture,
    Usage,
}

impl Attribute for BldgAttribute {
    type Field = Field;

    const FEATURES: &'static [&'static [u8]] = &[b"Building"];
    const PARTS: &'static [&'static [u8]] = &[b"BuildingPart"];

    fn field(local_name: &[u8]) -> Option<Field> {
        Some(match local_name {
            b"buildingID" => Field::BuildingId,
            b"city" => Field::City,
            b"class" => Field::Class,
            b"measuredHeight" => Field::MeasuredHeight,
            b"lod1HeightType" => Field::Lod1HeightType,
            b"prefecture" => Field::Prefecture,
            b"usage" => Field::Usage,
            _ => return None,
        })
    }

    fn set(&mut self, field: Field, text: &str) {
        match field {
            Field::BuildingId => self.uro_building_id = text.into(),
            Field::City => self.uro_city_code = text.into(),
            Field::Class => self.class_code = text.into(),
            Field::MeasuredHeight => self.measured_height = text.parse().ok(),
            Field::Lod1HeightType => self.lod1_height_type = text.parse().ok(),
            Field::Prefecture => self.uro_prefecture_code = Some(text.into()),
            Field::Usage => self.usage = text.parse().ok().map(BldgUsage::from_code),
        }
    }

    fn set_gml_id(&mut self, id: String) {
        self.gml_id = id;
    }
}

impl CoverSingleIds for BldgShape {
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
        let z: u8 = z.into();
        let (solid, surfaces) = match self {
            Self::Solid(solid) => (Some(solid.cover_single_ids(z)?), None),
            Self::Surfaces(polygons) => (None, Some(Surfaces(polygons).cover(z)?)),
        };
        Ok(solid
            .into_iter()
            .flatten()
            .chain(surfaces.into_iter().flatten())
            .map(move |id| (id, value.clone())))
    }
}

impl CoverRangeIds for BldgShape {
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
        let z: u8 = z.into();
        let (solid, surfaces) = match self {
            Self::Solid(solid) => (Some(solid.cover_range_ids(z)?), None),
            Self::Surfaces(polygons) => (None, Some(Surfaces(polygons).cover(z)?)),
        };
        Ok(solid
            .into_iter()
            .flatten()
            .chain(surfaces.into_iter().flatten().map(RangeId::from))
            .map(move |id| (id, value.clone())))
    }
}

impl Feature for Building {
    type Attribute = BldgAttribute;
    type Id = RangeId;

    fn new(attribute: BldgAttribute, lods: LodRings) -> Self {
        Self {
            attribute,
            shape: BldgShape::from_lods(lods),
        }
    }

    fn attribute(&self) -> &BldgAttribute {
        &self.attribute
    }

    fn single_ids(&self, z: u8) -> Result<impl Iterator<Item = SingleId>, Error> {
        self.shape.cover_single_ids(z)
    }

    fn map_ids(&self, z: u8) -> Result<impl Iterator<Item = RangeId>, Error> {
        self.shape.cover_range_ids(z)
    }
}

impl CoverSingleIds for BldgFile {
    type Value<'a> = &'a BldgAttribute;

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let z = z.into();
        Building::cover_all(&self.buildings, f, move |b| Ok(b.single_ids(z)?.collect()))
    }

    fn cover_map_with<'a, V, F>(&'a self, z: impl Into<u8>, f: F) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        Building::cover_map_all(&self.buildings, z.into(), f)
    }
}

impl CoverRangeIds for BldgFile {
    fn cover_range_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        f: F,
    ) -> Result<impl Iterator<Item = (RangeId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let z = z.into();
        Building::cover_all(&self.buildings, f, move |b| Ok(b.map_ids(z)?.collect()))
    }
}
