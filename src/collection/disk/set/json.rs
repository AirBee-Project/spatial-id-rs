use crate::AllowedIntervals;
use crate::collection::disk::SpatialIdSet;
use crate::collection::json::{deserialize_without_values, try_serialize_without_values};
use alloc::string::String;
use serde::de;
use serde::ser;
use serde::{Serialize, Serializer};
use std::io::{Read, Write};
use std::path::Path;

impl Serialize for SpatialIdSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let ranges = self
            .inner
            .reconstructed_time_ranges_sync(Some(&AllowedIntervals::default()))
            .map_err(ser::Error::custom)?;
        try_serialize_without_values::<core::convert::Infallible, S>(
            ranges.into_iter().map(|(range, ())| Ok(range)),
            serializer,
        )
    }
}

impl SpatialIdSet {
    /// スキーマ準拠の JSON 文字列からディスク版 [`SpatialIdSet`] を構築し、コミットして返す。
    pub async fn from_json_str(
        path: impl AsRef<Path>,
        json: &str,
    ) -> Result<Self, serde_json::Error> {
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let entries = deserialize_without_values(&mut deserializer)?;
        deserializer.end()?;
        Self::from_json_entries(path, entries).await
    }

    /// スキーマ準拠の JSON リーダーからディスク版 [`SpatialIdSet`] を構築し、コミットして返す。
    pub async fn from_json_reader(
        path: impl AsRef<Path>,
        reader: impl Read,
    ) -> Result<Self, serde_json::Error> {
        let mut deserializer = serde_json::Deserializer::from_reader(reader);
        let entries = deserialize_without_values(&mut deserializer)?;
        deserializer.end()?;
        Self::from_json_entries(path, entries).await
    }

    async fn from_json_entries(
        path: impl AsRef<Path>,
        entries: alloc::vec::Vec<crate::RangeId>,
    ) -> Result<Self, serde_json::Error> {
        Self::from_mem(path, &entries.into_iter().collect())
            .await
            .map_err(de::Error::custom)
    }

    /// スキーマ準拠の JSON 文字列へ非同期に書き出す。
    pub async fn to_json_string(&self) -> Result<String, serde_json::Error> {
        let mut buf = alloc::vec::Vec::new();
        self.to_json_writer(&mut buf).await?;
        Ok(String::from_utf8(buf).expect("serde_json outputs valid UTF-8"))
    }

    /// スキーマ準拠の JSON を `writer` へ非同期に書き出す。
    pub async fn to_json_writer(&self, writer: impl Write) -> Result<(), serde_json::Error> {
        let ranges = self
            .reconstructed_time_ranges(Some(&AllowedIntervals::default()))
            .await
            .map_err(ser::Error::custom)?;
        let mut serializer = serde_json::Serializer::new(writer);
        try_serialize_without_values::<core::convert::Infallible, _>(
            ranges.into_iter().map(Ok),
            &mut serializer,
        )
    }
}
