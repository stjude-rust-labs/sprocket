//! Strict JSON parsing per the WDL module spec.
//!
//! `serde_json` is strict on trailing commas, comments, and BOM by
//! default, but it silently uses the last value when an object contains
//! duplicate keys. The module spec requires implementations to reject
//! duplicate keys outright. The wrapper in this module performs a
//! strict pre-pass that rejects them, then deserializes the typed value.

use std::collections::HashSet;
use std::fmt;

use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Parses `bytes` strictly. Any duplicate object key at any depth fails
/// the parse. The typed value is then deserialized from the resulting
/// document.
pub(crate) fn from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    let CheckedValue(value) = serde_json::from_slice(bytes)?;
    serde_json::from_value(value)
}

/// A `serde_json::Value` whose `Deserialize` impl rejects duplicate keys
/// at every nested object level.
struct CheckedValue(serde_json::Value);

impl<'de> Deserialize<'de> for CheckedValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Visitor).map(CheckedValue)
    }
}

/// A `Visitor` that builds a `serde_json::Value` while rejecting duplicate
/// keys.
struct Visitor;

impl<'de> serde::de::Visitor<'de> for Visitor {
    type Value = serde_json::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any valid JSON value")
    }

    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Bool(v))
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Number(v.into()))
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Number(v.into()))
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(serde_json::Number::from_f64(v)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null))
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(serde_json::Value::String(v.to_string()))
    }

    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(serde_json::Value::String(v))
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }

    fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        let CheckedValue(v) = Deserialize::deserialize(d)?;
        Ok(v)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(CheckedValue(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(serde_json::Value::Array(items))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut obj = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key `{key}`"
                )));
            }
            let CheckedValue(value) = map.next_value()?;
            obj.insert(key, value);
        }
        Ok(serde_json::Value::Object(obj))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Probe {
        name: String,
        nested: Nested,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Nested {
        value: u32,
    }

    #[test]
    fn accepts_duplicate_free_input() {
        let p: Probe = from_slice(br#"{"name":"x","nested":{"value":7}}"#).unwrap();
        assert_eq!(
            p,
            Probe {
                name: "x".to_string(),
                nested: Nested { value: 7 },
            }
        );
    }

    #[test]
    fn rejects_top_level_duplicate() {
        let err =
            from_slice::<Probe>(br#"{"name":"x","name":"y","nested":{"value":7}}"#).unwrap_err();
        assert!(err.to_string().contains("duplicate object key"));
    }

    #[test]
    fn rejects_nested_duplicate() {
        let err =
            from_slice::<Probe>(br#"{"name":"x","nested":{"value":1,"value":2}}"#).unwrap_err();
        assert!(err.to_string().contains("duplicate object key"));
    }
}
