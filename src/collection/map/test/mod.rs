#![cfg_attr(test, allow(dead_code))]

pub(crate) mod insert;
pub(crate) mod par;
pub(crate) mod query;
pub(crate) mod remove;
pub(crate) mod upsert;

#[cfg(test)]
use crate::{RangeId, SingleId, SpatialIdMap};

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) enum TableEntry {
    Single(SingleId, i32),
    Range(RangeId, i32),
}

#[cfg(test)]
impl TableEntry {
    fn insert_into(&self, table: &mut SpatialIdMap<i32>) {
        match self {
            TableEntry::Single(single_id, value) => table.insert(*single_id, *value),
            TableEntry::Range(range_id, value) => table.insert(*range_id, *value),
        }
    }
}

#[cfg(test)]
pub(crate) fn build_table(entries: &[TableEntry]) -> SpatialIdMap<i32> {
    let mut table = SpatialIdMap::new();
    for entry in entries {
        entry.insert_into(&mut table);
    }
    table
}
