//! PLATEAU の交通（`tran`）。道路、鉄道、広場、徒歩道。
//!
//! [`TranFile`] で交通の XML ファイル（`*_tran_*.gml`）を読み込み、[`CoverSingleIds`] で
//! 空間IDに変換する。値には地物ごとの [`TranAttribute`] の参照が渡る。
//!
//! 地物は面として覆う。面を持つ LOD のうち最も高いものを使い、`TrafficArea` などの子地物の面は
//! 親の地物に含める。
//!
//! # 動作例
//! ```no_run
//! use spatial_id::geometry::CoverSingleIds;
//! use spatial_id::geometry::format::plateau::tran::{TranFile, TranFunction};
//! use std::{fs::File, io::BufReader};
//!
//! let file = File::open("53394598_tran_6697_op.gml").unwrap();
//! let tran = TranFile::from_reader(BufReader::new(file)).unwrap();
//!
//! // 歩道かどうかを値として付ける。
//! let sidewalks = tran
//!     .cover_map_with(24, |attribute| attribute.function == Some(TranFunction::Sidewalk))
//!     .unwrap();
//! ```

#[cfg(test)]
mod sample_test;
#[cfg(test)]
pub(super) mod test;

use alloc::string::String;
use alloc::vec::Vec;
use core::str::FromStr;
use std::io::BufRead;

use super::Feature;
use super::geometry::{LodRings, Surfaces};
use super::parser::Attribute;
use crate::{CoverSingleIds, Error, Polygon, SingleId, SpatialIdMap};

/// 交通の XML ファイル（`*_tran_*.gml`）全体。
///
/// 複数の地物が同じ ID を覆う場合、[`CoverSingleIds::cover_map_with`] では後の地物の値が残る。
#[derive(Debug, Clone, Default)]
pub struct TranFile {
    pub transportations: Vec<Transportation>,
}

impl TranFile {
    /// XML を読み込む。XML として読めない場合だけエラーを返す。
    ///
    /// 形状は面を持つ LOD のうち最も高いものを使う。`TrafficArea` などの子地物の面は親の地物に含め、
    /// 属性は親の地物のものだけを読む。
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Self, Error> {
        Ok(Self {
            transportations: Transportation::read_all(reader)?,
        })
    }
}

impl FromStr for TranFile {
    type Err = Error;

    fn from_str(xml: &str) -> Result<Self, Error> {
        Self::from_reader(xml.as_bytes())
    }
}

/// PLATEAU の交通の地物 1 件（`Road`、`Railway`、`Square`、`Track`）。
#[derive(Debug, Clone)]
pub struct Transportation {
    pub attribute: TranAttribute,
    pub surfaces: Vec<Polygon>,
}

/// PLATEAU の交通の属性。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub struct TranAttribute {
    pub gml_id: String,
    pub class: Option<TranClass>,
    pub function: Option<TranFunction>,
    pub usage: Option<TranUsage>,
}

/// 道路の区分（`tran:class`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub enum TranClass {
    /// 一般国道
    NationalRoad,
    /// 都道府県道
    PrefecturalRoad,
    /// 市町村道
    MunicipalRoad,
    /// 高速自動車国道等
    Expressway,
    Other(String),
}

/// 道路の機能（`tran:function`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub enum TranFunction {
    /// 車道
    Carriageway,
    /// 歩道
    Sidewalk,
    /// 自転車道
    BicycleLane,
    /// 交差点
    Intersection,
    Other(String),
}

/// 道路の用途（`tran:usage`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub enum TranUsage {
    /// 自動車交通
    Car,
    /// 歩行者交通
    Pedestrian,
    /// 自転車交通
    Bicycle,
    Other(String),
}

impl TranClass {
    fn from_text(value: &str) -> Self {
        match value {
            "一般国道" => Self::NationalRoad,
            "都道府県道" => Self::PrefecturalRoad,
            "市町村道" => Self::MunicipalRoad,
            "高速自動車国道等" => Self::Expressway,
            _ => Self::Other(value.into()),
        }
    }
}

impl TranFunction {
    fn from_text(value: &str) -> Self {
        match value {
            "車道" => Self::Carriageway,
            "歩道" => Self::Sidewalk,
            "自転車道" => Self::BicycleLane,
            "交差点" => Self::Intersection,
            _ => Self::Other(value.into()),
        }
    }
}

impl TranUsage {
    fn from_text(value: &str) -> Self {
        match value {
            "自動車交通" => Self::Car,
            "歩行者交通" => Self::Pedestrian,
            "自転車交通" => Self::Bicycle,
            _ => Self::Other(value.into()),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Field {
    Class,
    Function,
    Usage,
}

impl Attribute for TranAttribute {
    type Field = Field;

    const FEATURES: &'static [&'static [u8]] = &[b"Road", b"Railway", b"Square", b"Track"];
    const PARTS: &'static [&'static [u8]] = &[b"TrafficArea", b"AuxiliaryTrafficArea"];

    fn field(local_name: &[u8]) -> Option<Field> {
        Some(match local_name {
            b"class" => Field::Class,
            b"function" => Field::Function,
            b"usage" => Field::Usage,
            _ => return None,
        })
    }

    fn set(&mut self, field: Field, text: &str) {
        match field {
            Field::Class => self.class = Some(TranClass::from_text(text)),
            Field::Function => self.function = Some(TranFunction::from_text(text)),
            Field::Usage => self.usage = Some(TranUsage::from_text(text)),
        }
    }

    fn set_gml_id(&mut self, id: String) {
        self.gml_id = id;
    }
}

impl Feature for Transportation {
    type Attribute = TranAttribute;
    type Id = SingleId;

    /// 面を持つ LOD のうち、最も高いものを使う。
    fn new(attribute: TranAttribute, lods: LodRings) -> Self {
        Self {
            attribute,
            surfaces: lods.highest_first().next().unwrap_or_default(),
        }
    }

    fn attribute(&self) -> &TranAttribute {
        &self.attribute
    }

    fn single_ids(&self, z: u8) -> Result<impl Iterator<Item = SingleId>, Error> {
        Ok(Surfaces(&self.surfaces).cover(z)?.into_iter())
    }

    fn map_ids(&self, z: u8) -> Result<impl Iterator<Item = SingleId>, Error> {
        self.single_ids(z)
    }
}

impl CoverSingleIds for TranFile {
    type Value<'a> = &'a TranAttribute;

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
        Transportation::cover_all(&self.transportations, f, move |t| {
            Ok(t.single_ids(z)?.collect())
        })
    }

    fn cover_map_with<'a, V, F>(&'a self, z: impl Into<u8>, f: F) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        Transportation::cover_map_all(&self.transportations, z.into(), f)
    }
}
