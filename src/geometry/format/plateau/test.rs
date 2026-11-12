//! 建築物と交通のテストで共通に使う補助関数と、XML の読み込みのテスト。

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Debug;

use hashbrown::HashMap;

use super::bldg::BldgFile;
use crate::{Error, GeometryError, SingleId};

pub(super) fn city_model(members: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<core:CityModel xmlns:core="c" xmlns:bldg="b" xmlns:tran="t" xmlns:gml="g" xmlns:uro="u">{members}</core:CityModel>"#
    )
}

pub(super) fn sorted_ids(ids: impl Iterator<Item = SingleId>) -> Vec<SingleId> {
    let mut ids: Vec<SingleId> = ids.collect();
    ids.sort();
    ids.dedup();
    ids
}

/// 重複を除いて並べた ID の一覧。
pub(super) fn ids_snapshot(ids: impl Iterator<Item = impl ToString>) -> String {
    let mut lines: Vec<String> = ids.map(|id| id.to_string()).collect();
    lines.sort();
    lines.dedup();
    lines.join(",\n")
}

/// 属性ごとの ID の数。ID を 1 つも覆わない地物も 0 として残す。
pub(super) fn counts_snapshot<'a, A: Debug + 'a, I>(
    attributes: impl Iterator<Item = &'a A>,
    pairs: impl Iterator<Item = (I, &'a A)>,
) -> String {
    let mut counts: HashMap<String, usize> = attributes
        .map(|attribute| (format!("{attribute:?}"), 0))
        .collect();
    for (_, attribute) in pairs {
        *counts.entry(format!("{attribute:?}")).or_default() += 1;
    }
    let mut lines: Vec<String> = counts
        .into_iter()
        .map(|(attribute, n)| format!("{attribute} - {n} IDs"))
        .collect();
    lines.sort();
    lines.join(",\n")
}

#[test]
fn invalid_xml_is_error() {
    let xml = "<core:CityModel><bldg:Building gml:id=\"b\"></bldg:Buildin></core:CityModel>";
    assert!(matches!(
        xml.parse::<BldgFile>(),
        Err(Error::Geometry(GeometryError::InvalidXml { .. }))
    ));
}
