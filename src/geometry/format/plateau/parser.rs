use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::geometry::{Lod, LodRings, Ring};
use crate::{Coordinate, Error, GeometryError};

/// 地物ごとに異なる属性の読み取り方。
pub(super) trait Attribute: Default {
    type Field: Copy;

    /// 1 件の地物として切り出す要素のローカル名。
    const FEATURES: &'static [&'static [u8]];

    /// 地物の中にある子地物のローカル名。形状は親の地物に含め、属性は読まない。
    const PARTS: &'static [&'static [u8]];

    fn field(local_name: &[u8]) -> Option<Self::Field>;

    fn set(&mut self, field: Self::Field, text: &str);

    fn set_gml_id(&mut self, id: String);
}

/// CityGML を地物 1 件ずつ読み出すパーサ。
pub(super) struct Parser<R, A: Attribute> {
    reader: Reader<R>,
    buf: Vec<u8>,
    in_feature: bool,
    part_depth: usize,
    attribute: A,
    field: Option<A::Field>,
    lod: Option<Lod>,
    in_pos_list: bool,
    pos_values: Vec<f64>,
    /// 読んでいる Polygon の外周と内周（穴）。
    rings: Vec<Ring>,
    lods: LodRings,
}

impl<R: BufRead, A: Attribute> Parser<R, A> {
    pub(super) fn new(reader: R) -> Self {
        let mut reader = Reader::from_reader(reader);
        reader.config_mut().trim_text(true);
        Self {
            reader,
            buf: Vec::with_capacity(8192),
            in_feature: false,
            part_depth: 0,
            attribute: A::default(),
            field: None,
            lod: None,
            in_pos_list: false,
            pos_values: Vec::new(),
            rings: Vec::new(),
            lods: LodRings::default(),
        }
    }

    fn start(&mut self, e: &BytesStart) {
        let local_name = e.local_name();
        let name = local_name.as_ref().as_bytes();
        if A::FEATURES.contains(&name) {
            self.in_feature = true;
            self.part_depth = 0;
            self.attribute = A::default();
            self.lod = None;
            self.lods.clear();
            if let Some(id) = e
                .attributes()
                .flatten()
                .find(|a| a.key.local_name().as_ref() == "id")
                .and_then(|a| a.normalized_value(XmlVersion::Explicit1_0).ok())
            {
                self.attribute.set_gml_id(id.into_owned());
            }
            return;
        }
        if !self.in_feature {
            return;
        }
        if A::PARTS.contains(&name) {
            self.part_depth += 1;
        } else if let Some(lod) = Lod::from_element(name) {
            self.lod = Some(lod);
        } else if name == b"Polygon" {
            self.rings.clear();
        } else if name == b"posList" {
            self.in_pos_list = true;
            self.pos_values.clear();
        } else if self.part_depth == 0 {
            self.field = A::field(name);
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_pos_list {
            self.pos_values.extend(
                text.split_ascii_whitespace()
                    .filter_map(|v| v.parse::<f64>().ok()),
            );
        } else if let Some(field) = self.field {
            self.attribute.set(field, text);
        }
    }

    fn end(&mut self, local_name: &[u8]) -> Option<(A, LodRings)> {
        self.field = None;
        if !self.in_feature {
            return None;
        }
        if A::FEATURES.contains(&local_name) {
            self.in_feature = false;
            return Some((
                core::mem::take(&mut self.attribute),
                core::mem::take(&mut self.lods),
            ));
        }
        if A::PARTS.contains(&local_name) {
            self.part_depth = self.part_depth.saturating_sub(1);
        } else if Lod::from_element(local_name).is_some() {
            self.lod = None;
        } else if local_name == b"posList" {
            self.in_pos_list = false;
            self.push_ring();
        } else if local_name == b"Polygon" {
            self.push_polygon();
        }
        None
    }

    fn push_ring(&mut self) {
        if self.lod.is_none() {
            return;
        }
        // EPSG:6697 の座標順は 緯度 経度 楕円体高。
        let points = self
            .pos_values
            .as_chunks::<3>()
            .0
            .iter()
            .filter_map(|&[lat, lon, alt]| Coordinate::new(lat, lon, alt).ok())
            .collect();
        self.rings.push(Ring::new(points));
    }

    /// 最初のリングが外周、残りが穴。
    fn push_polygon(&mut self) {
        let Some(lod) = self.lod else { return };
        let mut rings = core::mem::take(&mut self.rings).into_iter();
        if let Some(outer) = rings.next() {
            self.lods.push(lod, outer.with_holes(rings));
        }
    }

    fn invalid_xml(err: impl ToString) -> Error {
        GeometryError::InvalidXml {
            message: err.to_string(),
        }
        .into()
    }
}

impl<R: BufRead, A: Attribute> Iterator for Parser<R, A> {
    type Item = Result<(A, LodRings), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // イベントは buf を借用するので、self と分けて持つ。
        let mut buf = core::mem::take(&mut self.buf);
        let item = loop {
            buf.clear();
            match self.reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => self.start(&e),
                Ok(Event::Text(e)) => self.text(&e),
                Ok(Event::End(e)) => {
                    if let Some(item) = self.end(e.local_name().as_ref().as_bytes()) {
                        break Some(Ok(item));
                    }
                }
                Ok(Event::Eof) => break None,
                Ok(_) => {}
                Err(err) => break Some(Err(Self::invalid_xml(err))),
            }
        };
        self.buf = buf;
        item
    }
}
