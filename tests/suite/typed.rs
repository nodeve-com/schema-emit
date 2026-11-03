//! The suite's schemas for a typed world. A suite schema often omits `type`, its keywords then
//! applying to whichever kind the instance is. For each instance, give every schema the instance
//! reaches the instance's kind as its `type`: for a value of that kind, `S` and `S` with that
//! `type` accept the same values, so the verdict stands. A null is a value of kind `null`, which
//! a schema holds only if it admits null. An instance no typed value can be is n/a: a list of
//! mixed kinds no `prefixItems` tuple makes one of, a key a closed object lacks. A value of another kind than a stated `type` (or a
//! null the schema doesn't admit) has a typed value no decode builds: the emitted decode refuses
//! it, and that refusal is the verdict.

use serde_json::{Map, Value};

#[path = "keys.rs"]
mod keys;

/// The schema and instance as a typed check sees them, or why the instance is n/a. A schema a
/// `$ref` of its own reaches keeps its kind open: other values than the instance meet it too.
pub fn fit(schema: &Value, data: &Value) -> Result<(Value, Value), String> {
    if refers_to_itself(schema, schema.get("$id").and_then(Value::as_str)) {
        return Ok((schema.clone(), data.clone()));
    }
    fit_in(schema, data)
}

/// Whether `schema` holds a `$ref` to the whole schema: `#`, or the root's `$id`.
fn refers_to_itself(schema: &Value, id: Option<&str>) -> bool {
    match schema {
        Value::Object(m) => m.iter().any(|(k, v)| match (k.as_str(), v) {
            ("$ref", Value::String(r)) => r == "#" || Some(r.as_str()) == id,
            _ => refers_to_itself(v, id),
        }),
        Value::Array(xs) => xs.iter().any(|x| refers_to_itself(x, id)),
        _ => false,
    }
}

fn fit_in(schema: &Value, data: &Value) -> Result<(Value, Value), String> {
    let Value::Object(s) = schema else {
        return Ok((schema.clone(), data.clone()));
    };
    if s.contains_key("$ref") {
        return Ok((schema.clone(), data.clone()));
    }
    let mut s = s.clone();
    if !s.contains_key("type") {
        s.insert("type".into(), Value::String(kind(data).into()));
    }
    let ty = s
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match (ty.as_str(), data) {
        ("object", Value::Object(o)) => object(s, o),
        ("array", Value::Array(xs)) => array(s, xs),
        _ => Ok((Value::Object(s), data.clone())),
    }
}

/// The object subschemas of `s`'s `allOf`, one level down.
fn all_of(s: &Map<String, Value>) -> Vec<Map<String, Value>> {
    s.get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_object().cloned())
        .collect()
}

fn object(mut s: Map<String, Value>, o: &Map<String, Value>) -> Result<(Value, Value), String> {
    let subs = all_of(&s);
    // A key an `allOf` subschema names is one the value may hold, though `properties` lacks it.
    let sub_names: Vec<&String> = subs
        .iter()
        .filter_map(|m| m.get("properties").and_then(Value::as_object))
        .flat_map(|p| p.keys())
        .collect();
    let mut props = match s.get("properties") {
        Some(Value::Object(p)) => p.clone(),
        _ => Map::new(),
    };
    let (mut kept, loose) = split_keys(&mut props, &sub_names, o)?;
    let shut = shut_of(&s, &subs);
    kept.extend(keys::fit(&mut s, &props, &loose, shut)?);
    // A field the instance lacks checks nothing: any kind will do.
    for (k, p) in props.iter_mut() {
        if !o.contains_key(k) {
            untyped(p);
        }
    }
    if !props.is_empty() {
        s.insert("properties".into(), Value::Object(props));
    }
    Ok((Value::Object(s), Value::Object(kept)))
}

/// The keys an object keeps, and the loose ones.
type Split = (Map<String, Value>, Map<String, Value>);

/// The keys of `o` the object keeps, each property fitted to its schema, and the loose ones no
/// property or `allOf` subschema names.
fn split_keys(
    props: &mut Map<String, Value>,
    sub_names: &[&String],
    o: &Map<String, Value>,
) -> Result<Split, String> {
    let (mut kept, mut loose) = (Map::new(), Map::new());
    for (k, v) in o {
        match props.get(k) {
            Some(p) => {
                let (p, v) = fit_in(p, v)?;
                props.insert(k.clone(), p);
                kept.insert(k.clone(), v);
            }
            None if sub_names.contains(&k) => {
                kept.insert(k.clone(), v.clone());
            }
            None => {
                loose.insert(k.clone(), v.clone());
            }
        }
    }
    Ok((kept, loose))
}

/// How shut `s`, or an `allOf` subschema of it, is to keys it never names.
fn shut_of(s: &Map<String, Value>, subs: &[Map<String, Value>]) -> keys::Shut {
    let false_in = |key: &str| {
        s.get(key) == Some(&Value::Bool(false))
            || subs.iter().any(|m| m.get(key) == Some(&Value::Bool(false)))
    };
    if false_in("unevaluatedProperties") {
        keys::Shut::Strict
    } else if false_in("additionalProperties") {
        keys::Shut::Closed
    } else {
        keys::Shut::Open
    }
}

fn array(mut s: Map<String, Value>, xs: &[Value]) -> Result<(Value, Value), String> {
    if let Some(Value::Array(prefix)) = s.get("prefixItems").cloned() {
        return tuple(s, prefix, xs);
    }
    let items = s
        .get(rest_key(&s))
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let (mut items, out) = same(&items, xs)?;
    untyped(&mut items);
    s.insert(rest_key(&s).into(), items);
    Ok((Value::Object(s), Value::Array(out)))
}

/// A list under `prefixItems`: each position fits its own schema, and the items past them fit
/// `items` as a list does. A list of mixed kinds fits this way where a tuple makes it one. Items
/// that are `true` or absent are unchecked; `false` admits none past the prefix.
fn tuple(
    mut s: Map<String, Value>,
    prefix: Vec<Value>,
    xs: &[Value],
) -> Result<(Value, Value), String> {
    let n = prefix.len();
    let (mut positions, mut out) = (Vec::new(), Vec::new());
    for (i, mut p) in prefix.into_iter().enumerate() {
        match xs.get(i) {
            Some(x) => {
                let (p, x) = fit_in(&p, x)?;
                positions.push(p);
                out.push(x);
            }
            None => {
                untyped(&mut p);
                positions.push(p);
            }
        }
    }
    let trailing = xs.get(n..).unwrap_or_default();
    let (rest, tail) = match s.get(rest_key(&s)).cloned() {
        None | Some(Value::Bool(true)) => (Value::Bool(true), trailing.to_vec()),
        Some(Value::Bool(false)) if !trailing.is_empty() => {
            return Err("more items than a fixed tuple holds".into())
        }
        Some(Value::Bool(false)) => (Value::Bool(false), Vec::new()),
        Some(items) => {
            let (mut items, tail) = same(&items, trailing)?;
            untyped(&mut items);
            (items, tail)
        }
    };
    out.extend(tail);
    s.insert("prefixItems".into(), Value::Array(positions));
    s.insert(rest_key(&s).into(), rest);
    Ok((Value::Object(s), Value::Array(out)))
}

/// Each item fitted to `items`, which they must all fit to one schema of: the fitted schema,
/// and the items.
fn same(items: &Value, xs: &[Value]) -> Result<(Value, Vec<Value>), String> {
    let mut fitted: Option<Value> = None;
    let mut out = Vec::new();
    for x in xs {
        let (p, x) = fit_in(items, x)?;
        if fitted.as_ref().is_some_and(|f| *f != p) {
            return Err("a list of mixed kinds".into());
        }
        fitted = Some(p);
        out.push(x);
    }
    Ok((fitted.unwrap_or_else(|| items.clone()), out))
}

/// Gives a schema no instance reaches a kind, if it lacks one.
fn untyped(p: &mut Value) {
    if let Value::Object(m) = p {
        if m.contains_key("$ref") {
            return;
        }
        m.entry("type").or_insert(Value::String("boolean".into()));
    }
}

fn kind(data: &Value) -> &'static str {
    match data {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) if whole(data) => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A whole number an `i64` or `u64` holds.
fn whole(data: &Value) -> bool {
    data.as_i64().is_some()
        || data.as_u64().is_some()
        || data
            .as_f64()
            .is_some_and(|f| f == f.trunc() && f.abs() < 9.2e18)
}

/// The keyword that types an array items past its prefix: `items`, else `unevaluatedItems`
/// where nothing else already evaluates them.
fn rest_key(s: &Map<String, Value>) -> &'static str {
    if !s.contains_key("items") && s.contains_key("unevaluatedItems") {
        "unevaluatedItems"
    } else {
        "items"
    }
}

/// Whether `schema` is a map, an object of no `properties` and `additionalProperties` a schema,
/// which lowers to no model.
pub fn map(schema: &Value) -> bool {
    let item = schema
        .get("additionalProperties")
        .is_some_and(Value::is_object);
    item && schema.get("properties").is_none() && !schema.as_object().is_some_and(keys::any)
}
