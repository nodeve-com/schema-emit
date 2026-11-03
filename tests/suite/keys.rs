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

/// How shut an object is to the keys it never names.
#[derive(Clone, Copy, PartialEq)]
pub enum Shut {
    /// Keys may be there.
    Open,
    /// `additionalProperties: false`: only the keys a pattern names.
    Closed,
    /// `unevaluatedProperties: false`: no key at all.
    Strict,
}

/// The keys the typed value holds, with the values the rules that reach them fit.
struct Typed<'a> {
    loose: &'a Map<String, Value>,
    kept: Map<String, Value>,
}

impl Typed<'_> {
    /// Fits `rule` and the values of `keys` to one type; the keys keep the fitted values. Gives
    /// the fitted rule.
    fn fit_rule(&mut self, rule: &Value, keys: &[&String]) -> Result<Value, String> {
        let values: Vec<Value> = keys.iter().map(|k| self.loose[*k].clone()).collect();
        let (rule, values) = same(rule, &values)?;
        for (key, value) in keys.iter().zip(values) {
            self.kept.insert((*key).clone(), value);
        }
        Ok(rule)
    }
}

/// The `patternProperties` of `s`, each pattern with its rule.
fn pattern_rules(s: &Map<String, Value>) -> Vec<(String, Value)> {
    s.get("patternProperties")
        .and_then(Value::as_object)
        .map(|p| p.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// The keys of `loose` each of `rules` reaches, and those none reaches. `None` where a pattern
/// is one the harness can't read, which the front end then judges.
fn reaching<'a>(
    rules: &[(String, Value)],
    loose: &'a Map<String, Value>,
) -> Option<(Vec<Vec<&'a String>>, Vec<&'a String>)> {
    let mut reach: Vec<Vec<&String>> = vec![Vec::new(); rules.len()];
    let mut rest = Vec::new();
    for key in loose.keys() {
        let mut hit = false;
        for ((pattern, _), keys) in rules.iter().zip(reach.iter_mut()) {
            if schema_emit::matching::matches(pattern, key).ok()? {
                hit = true;
                keys.push(key);
            }
        }
        if !hit {
            rest.push(key);
        }
    }
    Some((reach, rest))
}

/// Fits each of `rules` and the keys it reaches, narrowing `s`'s `patternProperties` to the rules
/// that reach any. Whether some rule did.
fn fit_patterns(
    s: &mut Map<String, Value>,
    rules: &[(String, Value)],
    reach: &[Vec<&String>],
    typed: &mut Typed,
) -> Result<bool, String> {
    let mut fitted = Map::new();
    for ((pattern, rule), keys) in rules.iter().zip(reach) {
        if !keys.is_empty() {
            fitted.insert(pattern.clone(), typed.fit_rule(rule, keys)?);
        }
    }
    let ruled = !fitted.is_empty();
    if s.contains_key("patternProperties") {
        s.insert("patternProperties".into(), Value::Object(fitted));
    }
    Ok(ruled)
}

/// Whether the values of `kept` are of more than one kind.
fn mixed(kept: &Map<String, Value>) -> bool {
    let kinds: Vec<&str> = kept.values().map(kind).collect();
    kinds.windows(2).any(|w| w[0] != w[1])
}

/// The `loose` keys (those neither `props` nor an `allOf` subschema names) the typed value
/// holds, fitted to the rules that reach them, with `s`'s rules narrowed to those.
pub fn fit(
    s: &mut Map<String, Value>,
    props: &Map<String, Value>,
    loose: &Map<String, Value>,
    shut: Shut,
) -> Result<Map<String, Value>, String> {
    let rules = pattern_rules(s);
    if let Some(k) = loose.keys().next() {
        if shut == Shut::Strict || (shut == Shut::Closed && rules.is_empty()) {
            return Err(format!("key `{k}`, which a closed object lacks"));
        }
    }
    if !reads(s, props) {
        return Ok(Map::new());
    }
    let Some((reach, rest)) = reaching(&rules, loose) else {
        return Ok(loose.clone());
    };
    let mut typed = Typed {
        loose,
        kept: loose.clone(),
    };
    let mut ruled = fit_patterns(s, &rules, &reach, &mut typed)?;
    if let Some(rule) = s.get("additionalProperties").filter(|v| v.is_object()) {
        if rest.is_empty() {
            s.remove("additionalProperties");
        } else {
            ruled = true;
            let fitted = typed.fit_rule(&rule.clone(), &rest)?;
            s.insert("additionalProperties".into(), fitted);
        }
    }
    if ruled && mixed(&typed.kept) {
        return Err("keys no field names, of mixed kinds".into());
    }
    Ok(typed.kept)
}
