//! Which keywords of an object schema judge keys its `properties` doesn't list.

use serde_json::{Map, Value};

use crate::json_schema::unevaluated::branches;

/// Whether `s` bounds its count of properties.
pub(in crate::json_schema) fn counted(s: &Map<String, Value>) -> bool {
    s.contains_key("minProperties") || s.contains_key("maxProperties")
}

/// Whether `s` holds no key beyond its properties.
pub(in crate::json_schema) fn closed(s: &Map<String, Value>) -> bool {
    s.get("additionalProperties") == Some(&Value::Bool(false))
}

/// The patterns `s` gives values to.
pub(super) fn patterns(s: &Map<String, Value>) -> Vec<String> {
    s.get("patternProperties")
        .and_then(Value::as_object)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default()
}

/// Whether a keyword of `s` judges a key `props` doesn't list, so the model holds the others.
pub(in crate::json_schema) fn needed(
    s: &Map<String, Value>,
    props: &Map<String, Value>,
    required: &[String],
) -> bool {
    if !patterns(s).is_empty() {
        return true;
    }
    if closed(s) {
        return false;
    }
    let lacks = |n: &str| !props.contains_key(n);
    let depends = s
        .get("dependentRequired")
        .and_then(Value::as_object)
        .is_some_and(|d| {
            d.iter().any(|(when, then)| {
                lacks(when)
                    || then
                        .as_array()
                        .is_some_and(|t| t.iter().filter_map(Value::as_str).any(lacks))
            })
        });
    s.get("additionalProperties").is_some_and(Value::is_object)
        || !branch_names(s).iter().all(|n| props.contains_key(n))
        || s.get("propertyNames")
            .is_some_and(|v| *v != Value::Bool(true))
        || counted(s)
        || required.iter().any(|n| lacks(n))
        || depends
        || s.get("dependentSchemas")
            .and_then(Value::as_object)
            .is_some_and(|d| !d.is_empty())
}

/// The keys the branches of `s` name by `required` or `properties`: `anyOf`, `oneOf`, `if`,
/// `then`, `else`, and `dependentSchemas`.
pub fn branch_names(s: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for sub in branches(s) {
        let required = sub.get("required").and_then(Value::as_array);
        out.extend(
            required
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
        let props = sub.get("properties").and_then(Value::as_object);
        out.extend(props.into_iter().flat_map(Map::keys).cloned());
    }
    out
}

/// Whether a keyword of `s` judges an object's keys by name or count, so it is no plain map.
pub(in crate::json_schema) fn reads(s: &Map<String, Value>) -> bool {
    [
        "patternProperties",
        "propertyNames",
        "dependentSchemas",
        "dependentRequired",
        "minProperties",
        "maxProperties",
    ]
    .iter()
    .any(|k| s.contains_key(*k))
}
