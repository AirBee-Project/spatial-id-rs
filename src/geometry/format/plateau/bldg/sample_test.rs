//! 過去に不具合が出た建築物の実データ（`sample/plateau/bldg`）のスナップショット。

use alloc::vec::Vec;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use hashbrown::HashSet;
use insta::glob;

use super::super::test::{counts_snapshot, ids_snapshot};
use super::{BldgAttribute as Attribute, BldgFile, BldgUsage};
use crate::{CoverRangeIds, CoverSingleIds, SingleId};

const Z: u8 = 23;

/// 型付きのフィールドだけ。スナップショットを [`Attribute`] の `Debug` と同じ形にするため、同じ名前にする。
#[derive(Debug)]
#[allow(dead_code)]
struct BldgAttribute<'a> {
    gml_id: &'a str,
    uro_building_id: &'a str,
    uro_city_code: &'a str,
    class_code: &'a str,
    measured_height: Option<f64>,
    lod1_height_type: Option<i32>,
    uro_prefecture_code: Option<&'a str>,
    usage: Option<BldgUsage>,
}

fn typed(a: &Attribute) -> BldgAttribute<'_> {
    BldgAttribute {
        gml_id: &a.gml_id,
        uro_building_id: &a.uro_building_id,
        uro_city_code: &a.uro_city_code,
        class_code: &a.class_code,
        measured_height: a.measured_height,
        lod1_height_type: a.lod1_height_type,
        uro_prefecture_code: a.uro_prefecture_code.as_deref(),
        usage: a.usage,
    }
}

fn bldg(path: &Path) -> BldgFile {
    BldgFile::from_reader(BufReader::new(File::open(path).unwrap())).unwrap()
}

#[test]
fn bldg_single() {
    glob!("../../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(ids_snapshot(file.cover_single_ids(Z).unwrap()));
    });
}

#[test]
fn bldg_range() {
    glob!("../../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(ids_snapshot(file.cover_range_ids(Z).unwrap()));
    });
}

#[test]
fn bldg_range_expands_to_single() {
    glob!("../../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        let single: HashSet<SingleId> = file.cover_single_ids(Z).unwrap().collect();
        let range: HashSet<SingleId> = file
            .cover_range_ids(Z)
            .unwrap()
            .flat_map(|r| r.single_ids().collect::<Vec<_>>())
            .collect();
        assert!(single == range, "{}", path.display());
    });
}

#[test]
fn bldg_single_with_attr() {
    glob!("../../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(counts_snapshot(
            file.buildings.iter().map(|b| &b.attribute),
            file.cover_single_ids_with_value(Z).unwrap(),
            typed,
        ));
    });
}

#[test]
fn bldg_range_with_attr() {
    glob!("../../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(counts_snapshot(
            file.buildings.iter().map(|b| &b.attribute),
            file.cover_range_ids_with_value(Z).unwrap(),
            typed,
        ));
    });
}
