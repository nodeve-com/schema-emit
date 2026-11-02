//! The suite's keys no field names. An object schema may judge keys its `properties` doesn't list
//! (`patternProperties`, `additionalProperties` as a schema, `propertyNames`, counts, and
//! dependencies on them). Each such key of the instance keeps its place, typed by the rules that
//! reach it. A rule no key reaches checks nothing of this instance, so it leaves the schema. The
//! values of these keys share one type, so they are one kind where a rule gives them a type. A
//! key the schema never reads is dropped: a typed value holds only the keys something reads.

use serde_json::{Map, Value};

use super::{kind, same};

/// Whether a keyword of `s` judges a key `props` doesn't list.
pub fn reads(s: &Map<String, Value>, props: &Map<String, Value>) -> bool {
    let named = |k: &str| s.contains_key(k);
    let lacks = |v: &Value| v.as_str().is_some_and(|n| !props.contains_key(n));
    let required = s
        .get("required")
        .and_then(Value::as_array)
        .is_some_and(|r| r.iter().any(lacks));
    let depends = s
        .get("dependentRequired")
        .and_then(Value::as_object)
        .is_some_and(|d| {
            d.iter().any(|(when, then)| {
                !props.contains_key(when) || then.as_array().is_some_and(|t| t.iter().any(lacks))
            })
        });
    s.get("patternProperties")
        .and_then(Value::as_object)
        .is_some_and(|p| !p.is_empty())
        || s.get("additionalProperties").is_some_and(Value::is_object)
        || s.get("propertyNames")
            .is_some_and(|v| *v != Value::Bool(true))
        || named("dependentSchemas")
        || !schema_emit::json_schema::branch_names(s)
            .iter()
            .all(|n| props.contains_key(n))
        || named("minProperties")
        || named("maxProperties")
        || required
        || depends
}

/// Whether `s` names a keyword that judges an object's keys by name or count: no plain map.
pub fn any(s: &Map<String, Value>) -> bool {
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

/// The `loose` keys (those neither `props` nor an `allOf` subschema names) the typed value
/// holds, fitted to the rules that reach them, with `s`'s rules narrowed to those. `closed`
/// forbids keys no pattern names; `strict` forbids every one.
pub fn fit(
    s: &mut Map<String, Value>,
    props: &Map<String, Value>,
    loose: &Map<String, Value>,
    closed: bool,
    strict: bool,
) -> Result<Map<String, Value>, String> {
    let rules: Vec<(String, Value)> = s
        .get("patternProperties")
        .and_then(Value::as_object)
        .map(|p| p.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    if let Some(k) = loose.keys().next() {
        if strict || (closed && rules.is_empty()) {
            return Err(format!("key `{k}`, which a closed object lacks"));
        }
    }
    if !reads(s, props) {
        return Ok(Map::new());
    }
    // Which rule reaches which key; a pattern the harness can't read is left to the front end.
    let mut reach: Vec<Vec<&String>> = vec![Vec::new(); rules.len()];
    let mut rest: Vec<&String> = Vec::new();
    for key in loose.keys() {
        let mut hit = false;
        for (i, (pattern, _)) in rules.iter().enumerate() {
            match schema_emit::matching::matches(pattern, key) {
                Ok(true) => {
                    hit = true;
                    reach[i].push(key);
                }
                Ok(false) => {}
                Err(_) => return Ok(loose.clone()),
            }
        }
        if !hit {
            rest.push(key);
        }
    }
    let mut kept = loose.clone();
    let mut fitted = Map::new();
    let mut ruled = false;
    for ((pattern, rule), keys) in rules.iter().zip(&reach) {
        if keys.is_empty() {
            continue;
        }
        ruled = true;
        let values: Vec<Value> = keys.iter().map(|k| loose[*k].clone()).collect();
        let (rule, values) = same(rule, &values)?;
        fitted.insert(pattern.clone(), rule);
        for (key, value) in keys.iter().zip(values) {
            kept.insert((*key).clone(), value);
        }
    }
    if s.contains_key("patternProperties") {
        s.insert("patternProperties".into(), Value::Object(fitted));
    }
    if let Some(rule) = s.get("additionalProperties").filter(|v| v.is_object()) {
        if rest.is_empty() {
            s.remove("additionalProperties");
        } else {
            ruled = true;
            let values: Vec<Value> = rest.iter().map(|k| loose[*k].clone()).collect();
            let (rule, values) = same(&rule.clone(), &values)?;
            s.insert("additionalProperties".into(), rule);
            for (key, value) in rest.iter().zip(values) {
                kept.insert((*key).clone(), value);
            }
        }
    }
    let kinds: Vec<&str> = kept.values().map(kind).collect();
    if ruled && kinds.windows(2).any(|w| w[0] != w[1]) {
        return Err("keys no field names, of mixed kinds".into());
    }
    Ok(kept)
}
