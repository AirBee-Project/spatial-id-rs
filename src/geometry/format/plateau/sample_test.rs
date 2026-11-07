//! 過去に不具合が出た PLATEAU の実データ（`sample/plateau`）のスナップショット。

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Debug;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use hashbrown::{HashMap, HashSet};
use insta::glob;

use super::bldg::BldgFile;
use super::tran::TranFile;
use crate::{CoverRangeIds, CoverSingleIds, SingleId};

const BLDG_Z: u8 = 23;
const TRAN_Z: u8 = 24;

fn bldg(path: &Path) -> BldgFile {
    BldgFile::from_reader(BufReader::new(File::open(path).unwrap())).unwrap()
}

fn tran(path: &Path) -> TranFile {
    TranFile::from_reader(BufReader::new(File::open(path).unwrap())).unwrap()
}

/// 重複を除いて並べた ID の一覧。
fn ids_snapshot(ids: impl Iterator<Item = impl ToString>) -> String {
    let mut lines: Vec<String> = ids.map(|id| id.to_string()).collect();
    lines.sort();
    lines.dedup();
    lines.join(",\n")
}

/// 属性ごとの ID の数。ID を 1 つも覆わない地物も 0 として残す。
fn counts_snapshot<'a, A: Debug + 'a, I>(
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
fn bldg_single() {
    glob!("../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(ids_snapshot(file.cover_single_ids(BLDG_Z).unwrap()));
    });
}

#[test]
fn bldg_range() {
    glob!("../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(ids_snapshot(file.cover_range_ids(BLDG_Z).unwrap()));
    });
}

#[test]
fn bldg_range_expands_to_single() {
    glob!("../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        let single: HashSet<SingleId> = file.cover_single_ids(BLDG_Z).unwrap().collect();
        let range: HashSet<SingleId> = file
            .cover_range_ids(BLDG_Z)
            .unwrap()
            .flat_map(|r| r.single_ids().collect::<Vec<_>>())
            .collect();
        assert!(single == range, "{}", path.display());
    });
}

#[test]
fn bldg_single_with_attr() {
    glob!("../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(counts_snapshot(
            file.buildings.iter().map(|b| &b.attribute),
            file.cover_single_ids_with_value(BLDG_Z).unwrap(),
        ));
    });
}

#[test]
fn bldg_range_with_attr() {
    glob!("../../../../sample/plateau/bldg", "*.gml", |path| {
        let file = bldg(path);
        insta::assert_snapshot!(counts_snapshot(
            file.buildings.iter().map(|b| &b.attribute),
            file.cover_range_ids_with_value(BLDG_Z).unwrap(),
        ));
    });
}

#[test]
fn tran_single() {
    glob!("../../../../sample/plateau/tran", "*.gml", |path| {
        let file = tran(path);
        insta::assert_snapshot!(ids_snapshot(file.cover_single_ids(TRAN_Z).unwrap()));
    });
}

#[test]
fn tran_single_with_attr() {
    glob!("../../../../sample/plateau/tran", "*.gml", |path| {
        let file = tran(path);
        insta::assert_snapshot!(counts_snapshot(
            file.transportations.iter().map(|t| &t.attribute),
            file.cover_single_ids_with_value(TRAN_Z).unwrap(),
        ));
    });
}
