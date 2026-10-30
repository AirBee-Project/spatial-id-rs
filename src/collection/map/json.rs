use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{AllowedIntervals, SpatialIdMap};

use super::super::json::{deserialize_with_values, serialize_with_values};

impl<V> Serialize for SpatialIdMap<V>
where
    V: PartialEq + Clone + Serialize,
{
    fn serialize<Ser: Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        serialize_with_values(
            self.reconstructed_time_ranges(Some(&AllowedIntervals::default())),
            serializer,
        )
    }
}

impl<'de, V> Deserialize<'de> for SpatialIdMap<V>
where
    V: PartialEq + Clone + Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut table = Self::default();
        for (range_id, value) in deserialize_with_values(deserializer)? {
            table.insert(range_id, value);
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use crate::{SingleId, SpatialIdMap};
    use alloc::string::{String, ToString};

    #[test]
    fn serialize_lists_unique_values_and_refs() {
        let mut table = SpatialIdMap::<i32>::new();
        table.insert(SingleId::new(20, 0, 0, 0).unwrap(), 10);
        table.insert(SingleId::new(20, 1, 0, 0).unwrap(), 20);
        table.insert(SingleId::new(20, 2, 0, 0).unwrap(), 10); // 値 10 を再利用

        let json = serde_json::to_string(&table).unwrap();

        // 値は数値としてそのまま（クォートされない）、重複は1つに集約。
        assert!(json.contains("\"value\":[10,20]"));
        assert!(json.contains("\"ref\":0"));
        assert!(json.contains("\"ref\":1"));
        assert!(json.contains("\"z\":20"));
    }

    #[test]
    fn serialize_quotes_and_escapes_string_values() {
        let mut table = SpatialIdMap::<String>::new();
        table.insert(SingleId::new(20, 0, 0, 0).unwrap(), "a\"b".to_string());

        let json = serde_json::to_string(&table).unwrap();
        // 文字列値はクォートされ、" がエスケープされる。
        assert!(json.contains("\"value\":[\"a\\\"b\"]"));
    }

    #[test]
    fn serialize_empty_table_has_empty_value_and_ids() {
        let table = SpatialIdMap::<i32>::new();
        let json = serde_json::to_string(&table).unwrap();
        assert!(json.contains("\"value\":[]"));
        assert!(json.contains("\"ids\":[]"));
    }

    #[test]
    fn deserialize_round_trips_values_and_ids() {
        let mut table = SpatialIdMap::<i32>::new();
        table.insert(SingleId::new(20, 0, 0, 0).unwrap(), 10);
        table.insert(SingleId::new(20, 1, 0, 0).unwrap(), 20);
        table.insert(SingleId::new(20, 2, 0, 0).unwrap(), 10);

        let json = serde_json::to_string(&table).unwrap();
        let restored: SpatialIdMap<i32> = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.count(), table.count());
        for (flex_id, value) in table.iter() {
            let (_, restored_value) = restored.get(flex_id).next().unwrap();
            assert_eq!(restored_value, value);
        }
    }

    #[test]
    fn deserialize_rejects_out_of_range_zoom_level() {
        let json = r#"{"$schema":"https://airbee-project.github.io/schemas/json/v1.0.json","meta":{"version":"v1.0","description":""},"option":{},"data":[{"name":"","value":[10],"ids":[{"z":68,"f":[0],"x":[0],"y":[0],"ref":0}]}]}"#;
        assert!(serde_json::from_str::<SpatialIdMap<i32>>(json).is_err());
    }
}
