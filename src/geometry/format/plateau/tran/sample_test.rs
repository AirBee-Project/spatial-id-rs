//! 過去に不具合が出た交通の実データ（`sample/plateau/tran`）のスナップショット。

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use insta::glob;

use super::super::test::{counts_snapshot, ids_snapshot};
use super::TranFile;
use crate::CoverSingleIds;

const Z: u8 = 24;

fn tran(path: &Path) -> TranFile {
    TranFile::from_reader(BufReader::new(File::open(path).unwrap())).unwrap()
}

#[test]
fn tran_single() {
    glob!("../../../../../sample/plateau/tran", "*.gml", |path| {
        let file = tran(path);
        insta::assert_snapshot!(ids_snapshot(file.cover_single_ids(Z).unwrap()));
    });
}

#[test]
fn tran_single_with_attr() {
    glob!("../../../../../sample/plateau/tran", "*.gml", |path| {
        let file = tran(path);
        insta::assert_snapshot!(counts_snapshot(
            file.transportations.iter().map(|t| &t.attribute),
            file.cover_single_ids_with_value(Z).unwrap(),
        ));
    });
}
