use crate::AllowedIntervals;
use crate::collection::disk::{DiskValue, SpatialIdMap};
use crate::collection::json::{deserialize_with_values, try_serialize_with_values};
use alloc::string::String;
use serde::de::{self, DeserializeOwned};
use serde::ser;
use serde::{Deserialize, Serialize, Serializer};
use std::io::{Read, Write};
use std::path::Path;

impl<V> Serialize for SpatialIdMap<V>
where
    V: DiskValue + PartialEq + Serialize,
{
    fn serialize<Ser: Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        let ranges = self
            .inner
            .reconstructed_time_ranges_sync(Some(&AllowedIntervals::default()))
            .map_err(ser::Error::custom)?;
        try_serialize_with_values::<V, V, core::convert::Infallible, Ser>(
            ranges.into_iter().map(Ok),
            serializer,
        )
    }
}

impl<V: DiskValue> SpatialIdMap<V> {
    /// スキーマ準拠の JSON 文字列からディスク版 [`SpatialIdMap`] を構築し、コミットして返す。
    pub async fn from_json_str<'de>(
        path: impl AsRef<Path>,
        json: &'de str,
    ) -> Result<Self, serde_json::Error>
    where
        V: PartialEq + Clone + Deserialize<'de>,
    {
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let entries = deserialize_with_values(&mut deserializer)?;
        deserializer.end()?;
        Self::from_json_entries(path, entries).await
    }

    /// スキーマ準拠の JSON リーダーからディスク版 [`SpatialIdMap`] を構築し、コミットして返す。
    pub async fn from_json_reader(
        path: impl AsRef<Path>,
        reader: impl Read,
    ) -> Result<Self, serde_json::Error>
    where
        V: PartialEq + Clone + DeserializeOwned,
    {
        let mut deserializer = serde_json::Deserializer::from_reader(reader);
        let entries = deserialize_with_values(&mut deserializer)?;
        deserializer.end()?;
        Self::from_json_entries(path, entries).await
    }

    async fn from_json_entries(
        path: impl AsRef<Path>,
        entries: alloc::vec::Vec<(crate::RangeId, V)>,
    ) -> Result<Self, serde_json::Error>
    where
        V: PartialEq + Clone,
    {
        Self::from_mem(path, &entries.into_iter().collect())
            .await
            .map_err(de::Error::custom)
    }

    /// スキーマ準拠の JSON 文字列へ非同期に書き出す。
    pub async fn to_json_string(&self) -> Result<String, serde_json::Error>
    where
        V: PartialEq + Serialize,
    {
        let mut buf = alloc::vec::Vec::new();
        self.to_json_writer(&mut buf).await?;
        Ok(String::from_utf8(buf).expect("serde_json outputs valid UTF-8"))
    }

    /// スキーマ準拠の JSON を `writer` へ非同期に書き出す。
    pub async fn to_json_writer(&self, writer: impl Write) -> Result<(), serde_json::Error>
    where
        V: PartialEq + Serialize,
    {
        let ranges = self
            .reconstructed_time_ranges(Some(&AllowedIntervals::default()))
            .await
            .map_err(ser::Error::custom)?;
        let mut serializer = serde_json::Serializer::new(writer);
        try_serialize_with_values::<V, V, core::convert::Infallible, _>(
            ranges.into_iter().map(Ok),
            &mut serializer,
        )
    }
}
