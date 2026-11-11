use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{AllowedIntervals, SpatialIdSet};

use super::super::json::{deserialize_without_values, serialize_without_values};

impl Serialize for SpatialIdSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_without_values(
            self.reconstructed_time_ranges(Some(&AllowedIntervals::default())),
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for SpatialIdSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut set = Self::new();
        for range_id in deserialize_without_values(deserializer)? {
            set.insert(range_id);
        }
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use crate::{SingleId, SpatialIdSet};

    #[test]
    fn serialize_has_schema_envelope_and_ids() {
        let mut set = SpatialIdSet::new();
        set.insert(SingleId::new(20, 0, 0, 0).unwrap());

        let json = serde_json::to_string(&set).unwrap();

        assert!(json.starts_with(
            "{\"$schema\":\"https://airbee-project.github.io/schemas/json/v1.0.json\""
        ));
        assert!(json.contains("\"meta\":{\"version\":\"v1.0\",\"description\":\"\"}"));
        assert!(json.contains("\"option\":{}"));
        assert!(json.contains("\"data\":[{\"name\":\"\",\"ids\":["));
        assert!(json.contains("\"z\":20"));
        assert!(json.contains("\"f\":[0]"));
        // 値を持たない集合は value を出さない。
        assert!(!json.contains("\"value\""));
        // 非 temporal では i / t を出さない。
        assert!(!json.contains("\"i\":"));
        assert!(!json.contains("\"t\":"));
    }

    #[test]
    fn serialize_empty_set_has_empty_ids() {
        let set = SpatialIdSet::new();
        assert!(serde_json::to_string(&set).unwrap().contains("\"ids\":[]"));
    }

    #[test]
    fn serialize_collapses_and_expands_ranges() {
        use crate::RangeId;
        let mut set = SpatialIdSet::new();
        // x は範囲 [0,1] を持つ（z20）。f/y は単一。
        set.insert(RangeId::new(20, [0, 0], [0, 1], [0, 0]).unwrap());

        let json = serde_json::to_string(&set).unwrap();
        assert!(json.contains("\"x\":[0,1]"));
        assert!(json.contains("\"f\":[0]"));
    }

    #[test]
    fn deserialize_round_trips_a_set() {
        let mut set = SpatialIdSet::new();
        set.insert(SingleId::new(20, 0, 0, 0).unwrap());
        set.insert(SingleId::new(20, 1, 0, 0).unwrap());

        let json = serde_json::to_string(&set).unwrap();
        let restored: SpatialIdSet = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.count(), set.count());
        for flex_id in set.iter() {
            assert!(restored.get(flex_id).next().is_some());
        }
    }

    #[test]
    fn deserialize_rejects_malformed_pair_length() {
        let json = r#"{"$schema":"https://airbee-project.github.io/schemas/json/v1.0.json","meta":{"version":"v1.0","description":""},"option":{},"data":[{"name":"","ids":[{"z":20,"f":[0,1,2],"x":[0],"y":[0]}]}]}"#;
        assert!(serde_json::from_str::<SpatialIdSet>(json).is_err());
    }
}
