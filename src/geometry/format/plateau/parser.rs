use alloc::string::ToString;
use alloc::vec::Vec;
use std::io::BufRead;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::{NsReader, XmlVersion};

use super::Element;
use super::geometry::{Lod, LodRings, Ring};
use crate::{Coordinate, Error, GeometryError};

/// 要素の木に入れない名前空間。形状と外観は座標やテクスチャの量が多く、属性ではないため。
const GEOMETRY_NAMESPACES: &[&str] = &[
    "http://www.opengis.net/gml",
    "http://www.opengis.net/citygml/appearance/2.0",
];

/// GML の名前空間のうち、属性として残す要素。
const GML_ATTRIBUTES: &[&str] = &["name", "description"];

/// 地物の型ごとの、要素の木からの属性の作り方。
pub(super) trait Attribute: From<Element> {
    /// 1 件の地物として切り出す要素のローカル名。
    const FEATURES: &'static [&'static str];
}

/// CityGML を地物 1 件ずつ読み出すパーサ。
pub(super) struct Parser<R, A> {
    reader: NsReader<R>,
    buf: Vec<u8>,
    /// 読んでいる地物の、開いている要素。先頭が地物そのもの。
    elements: Vec<Element>,
    /// 木に入れない部分木の中での深さ。0 なら木を組み立てる。
    skip_depth: usize,
    lod: Option<Lod>,
    in_pos_list: bool,
    pos_values: Vec<f64>,
    /// 読んでいる Polygon の外周と内周（穴）。
    rings: Vec<Ring>,
    lods: LodRings,
    attribute: core::marker::PhantomData<A>,
}

impl<R: BufRead, A: Attribute> Parser<R, A> {
    pub(super) fn new(reader: R) -> Self {
        Self {
            reader: NsReader::from_reader(reader),
            buf: Vec::with_capacity(8192),
            elements: Vec::new(),
            skip_depth: 0,
            lod: None,
            in_pos_list: false,
            pos_values: Vec::new(),
            rings: Vec::new(),
            lods: LodRings::default(),
            attribute: core::marker::PhantomData,
        }
    }

    fn start(&mut self, e: &BytesStart, geometry_ns: bool) {
        let local_name = e.local_name();
        let name = local_name.as_ref();
        if self.elements.is_empty() {
            if A::FEATURES.contains(&name) {
                self.lod = None;
                self.lods.clear();
                self.elements.push(Self::element(e));
            }
            return;
        }

        if let Some(lod) = Lod::from_element(name.as_bytes()) {
            self.lod = Some(lod);
        } else if name == "Polygon" {
            self.rings.clear();
        } else if name == "posList" {
            self.in_pos_list = true;
            self.pos_values.clear();
        }

        if self.skip_depth > 0 || (geometry_ns && !GML_ATTRIBUTES.contains(&name)) {
            self.skip_depth += 1;
        } else {
            self.elements.push(Self::element(e));
        }
    }

    fn element(e: &BytesStart) -> Element {
        Element {
            name: e.name().as_ref().into(),
            attributes: e
                .attributes()
                .flatten()
                .filter(|a| a.key.as_namespace_binding().is_none())
                .filter_map(|a| {
                    let value = a.normalized_value(XmlVersion::Explicit1_0).ok()?;
                    Some((a.key.as_ref().into(), value.into_owned()))
                })
                .collect(),
            ..Element::default()
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_pos_list {
            self.pos_values.extend(
                text.split_ascii_whitespace()
                    .filter_map(|v| v.parse::<f64>().ok()),
            );
        } else if self.skip_depth == 0
            && let Some(element) = self.elements.last_mut()
        {
            element.text.push_str(text);
        }
    }

    /// 文字参照と XML の定義済み実体だけを展開する。PLATEAU は DTD で実体を定義しないため。
    fn general_ref(&mut self, r: &BytesRef) {
        if self.skip_depth > 0 {
            return;
        }
        let Some(element) = self.elements.last_mut() else {
            return;
        };
        if let Ok(Some(c)) = r.resolve_char_ref() {
            element.text.push(c);
        } else if let Some(s) = resolve_predefined_entity(r) {
            element.text.push_str(s);
        }
    }

    fn end(&mut self, local_name: &str) -> Option<(A, LodRings)> {
        if self.elements.is_empty() {
            return None;
        }

        if Lod::from_element(local_name.as_bytes()).is_some() {
            self.lod = None;
        } else if local_name == "posList" {
            self.in_pos_list = false;
            self.push_ring();
        } else if local_name == "Polygon" {
            self.push_polygon();
        }

        if self.skip_depth > 0 {
            self.skip_depth -= 1;
            return None;
        }
        let mut element = self.elements.pop()?;
        element.close();
        match self.elements.last_mut() {
            Some(parent) => {
                if !element.is_empty() {
                    parent.children.push(element);
                }
                None
            }
            None => Some((A::from(element), core::mem::take(&mut self.lods))),
        }
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
            let (ns, event) = match self.reader.read_resolved_event_into(&mut buf) {
                Ok(resolved) => resolved,
                Err(err) => break Some(Err(Self::invalid_xml(err))),
            };
            let geometry_ns = matches!(ns, ResolveResult::Bound(Namespace(uri)) if GEOMETRY_NAMESPACES.contains(&uri));
            match event {
                Event::Start(e) => self.start(&e, geometry_ns),
                Event::Empty(e) => {
                    self.start(&e, geometry_ns);
                    if let Some(item) = self.end(e.local_name().as_ref()) {
                        break Some(Ok(item));
                    }
                }
                Event::Text(e) => self.text(&e),
                Event::CData(e) => self.text(&e),
                Event::GeneralRef(e) => self.general_ref(&e),
                Event::End(e) => {
                    if let Some(item) = self.end(e.local_name().as_ref()) {
                        break Some(Ok(item));
                    }
                }
                Event::Eof => break None,
                _ => {}
            }
        };
        self.buf = buf;
        item
    }
}
