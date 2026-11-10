//! Area: `unevaluatedProperties` and `unevaluatedItems`. Each applies to what the schema and its
//! applied subschemas left unread, which a typed value mostly answers at lowering.
//!
//! `unevaluatedProperties: false` over a model checks that no field the schema's scope doesn't
//! evaluate is present ([`Check::Unevaluated`]): `properties`, `allOf`, and `$ref` evaluate
//! statically, and an `unevaluatedProperties` or `additionalProperties` beside evaluates every
//! key. A key no field names has no typed value, so the object is closed. `true` checks nothing.
//! A schema value, which would check keys no field names, is refused where fields are left
//! unevaluated, as is a branch (`anyOf`, `oneOf`, `if`, `dependentSchemas`) that evaluates a
//! name nothing else does, and `patternProperties`: which branch passed isn't known at lowering.
//!
//! `unevaluatedItems` on an array nothing else evaluates reads as `items`: [`fold_items`] rewrites
//! the schema before it lowers. With `items` beside, every item is evaluated and it checks
//! nothing. `contains`, or an applied subschema that might evaluate items, is refused. In an
//! applied subschema, where the type came from the parent, only `true` is lowered.

use std::borrow::Cow;

use serde_json::{Map, Value};

use super::{Done, Key};
use crate::ir::{Base, Check};
use crate::Error;

mod names;

/// What a schema's scope evaluates of an object's keys.
pub(super) enum Evaluated {
    Everything,
    Names(Vec<String>),
}

/// `s`, an array's, with `unevaluatedItems` read as `items` where nothing else evaluates items.
pub(super) fn fold_items<'a>(
    name: &str,
    s: &'a Map<String, Value>,
) -> Result<Cow<'a, Map<String, Value>>, Error> {
    let Some(rest) = s.get("unevaluatedItems") else {
        return Ok(Cow::Borrowed(s));
    };
    let mut out = s.clone();
    out.remove("unevaluatedItems");
    if s.contains_key("items") {
        return Ok(Cow::Owned(out));
    }
    if ["contains", "$ref", "$dynamicRef"]
        .iter()
        .any(|k| s.contains_key(*k))
        || applied(s).any(|sub| reads_items(sub) || applies_items(sub))
    {
        return Err(Error::unsupported(format!(
            "{name}: `unevaluatedItems` beside what evaluates items"
        )));
    }
    out.insert("items".into(), rest.clone());
    Ok(Cow::Owned(out))
}

/// Whether `s` holds a keyword that might evaluate items.
fn reads_items(s: &Map<String, Value>) -> bool {
    [
        "contains",
        "unevaluatedItems",
        "$ref",
        "$dynamicRef",
        "prefixItems",
    ]
    .iter()
    .any(|k| s.contains_key(*k))
        || s.contains_key("items")
}

/// Whether a subschema applied to `s`'s value, however deep, might evaluate items.
fn applies_items(s: &Map<String, Value>) -> bool {
    applied(s).any(|sub| reads_items(sub) || applies_items(sub))
}

/// The object subschemas `s`'s `allOf` applies.
fn all_of(s: &Map<String, Value>) -> impl Iterator<Item = &Map<String, Value>> {
    s.get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

/// The object subschemas `s` applies when they pass: its branches, `then`, and `else`.
pub(super) fn branches(s: &Map<String, Value>) -> impl Iterator<Item = &Map<String, Value>> {
    let lists = ["anyOf", "oneOf"]
        .into_iter()
        .filter_map(|k| s.get(k)?.as_array())
        .flatten();
    let singles = ["if", "then", "else"].into_iter().filter_map(|k| s.get(k));
    let deps = s
        .get("dependentSchemas")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::values);
    lists
        .chain(singles)
        .chain(deps)
        .filter_map(Value::as_object)
}

fn applied(s: &Map<String, Value>) -> impl Iterator<Item = &Map<String, Value>> {
    all_of(s).chain(branches(s))
}

pub(super) fn keyword(k: &mut Key) -> Done {
    match k.key {
        "unevaluatedItems" => Some(items(k)),
        "unevaluatedProperties" => Some(properties(k)),
        _ => None,
    }
}

/// `unevaluatedItems` that [`fold_items`] left: in an applied subschema, or on a value that
/// isn't an array.
fn items(k: &Key) -> Result<(), Error> {
    let array = matches!(k.ty.base, Base::List(_) | Base::Tuple { .. });
    if !array || *k.value == Value::Bool(true) || k.siblings.contains_key("items") {
        return Ok(());
    }
    Err(Error::unsupported(
        "`unevaluatedItems` in an applied subschema",
    ))
}

fn properties(k: &mut Key) -> Result<(), Error> {
    if !matches!(k.ty.base, Base::Model(_)) || *k.value == Value::Bool(true) {
        return Ok(());
    }
    match names::evaluated(k.lower, k.siblings)? {
        Evaluated::Everything => Ok(()),
        Evaluated::Names(_) if *k.value != Value::Bool(false) => Err(Error::unsupported(
            "`unevaluatedProperties` as a schema: keys no field names",
        )),
        Evaluated::Names(n) => {
            k.ty.checks.push(Check::Unevaluated(n));
            Ok(())
        }
    }
}
