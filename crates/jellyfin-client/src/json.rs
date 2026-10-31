//! Jellyfin JSON uses PascalCase by default and camelCase when the client asks
//! for that profile. Property names are normalized to PascalCase before serde.
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

pub(crate) fn parse<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(normalize_keys(value))
}

fn normalize_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(normalize_object(map)),
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_keys).collect()),
        other => other,
    }
}

fn normalize_object(map: Map<String, Value>) -> Map<String, Value> {
    let mut normalized = Map::with_capacity(map.len());
    for (key, value) in map {
        normalized.insert(pascal_key(&key), normalize_keys(value));
    }
    normalized
}

fn pascal_key(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => {
            let mut pascal = first.to_uppercase().collect::<String>();
            pascal.push_str(chars.as_str());
            pascal
        }
        None => String::new(),
    }
}

/// Missing and JSON null both become `T::default()`.
pub(crate) fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
