//! Area: models and optionality. An `object` with `properties` is a model, its fields in the
//! order written; `required` names the fields absence fails. `additionalProperties: false`
//! checks nothing more: a model holds only its fields. A `$ref` is `references.rs`'s.
//!
//! `minProperties` and `maxProperties` count a model's keys present: its fields, and the others
//! ([`super::others`]) where it holds any. On any other type they check nothing.
//!
//! `dependentRequired` on a model checks each key named, when the key beside it is present. A
//! name the model's fields and other keys lack is never present, so it fails whenever its
//! trigger is. On any other type it checks nothing.
//!
//! An `object` with no `properties` and `additionalProperties` a schema is a map, each value of
//! that schema, unless a keyword names or counts its keys.
//!
//! Stub: the rest of the object keywords are unsupported.

use serde_json::{Map, Value};

use super::bounds::count;
use super::others::{closed, needed, reads};
use super::{Done, Key, Lower};
use crate::ir::{Base, Check, Field, Model, Ty};
use crate::Error;

/// Whether a value of `schema` may hold a key no field names: an object not closed, one with
/// such a field or item, or a `$ref` (not followed here, so counted as open).
pub(super) fn open(schema: &Value) -> bool {
    let Value::Object(s) = schema else {
        return false;
    };
    if s.contains_key("$ref") {
        return true;
    }
    match s.get("type").and_then(Value::as_str) {
        Some("object") => {
            !closed(s)
                || s.get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(|p| p.values().any(open))
        }
        Some("array") => s.get("items").is_some_and(open),
        _ => false,
    }
}

/// `dependentRequired` as pairs of a field and the fields it requires, or `None` if malformed.
fn dependents(value: &Value) -> Option<Vec<(String, Vec<String>)>> {
    value
        .as_object()?
        .iter()
        .map(|(when, then)| {
            let names = then
                .as_array()?
                .iter()
                .map(|x| x.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()?;
            Some((when.clone(), names))
        })
        .collect()
}

/// `s`'s `required` names and its `properties`, as an object keyword reads them.
pub(super) fn field_lists(
    name: &str,
    s: &Map<String, Value>,
) -> Result<(Vec<String>, Map<String, Value>), Error> {
    let required = match s.get("required") {
        None => Vec::new(),
        Some(Value::Array(xs)) => xs
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| Error::invalid(format!("{name}: `required` holds {x}")))
            })
            .collect::<Result<_, _>>()?,
        Some(other) => return Err(Error::invalid(format!("{name}: `required` of {other}"))),
    };
    let props = match s.get("properties") {
        None => Map::new(),
        Some(Value::Object(p)) => p.clone(),
        Some(other) => return Err(Error::invalid(format!("{name}: `properties` of {other}"))),
    };
    Ok((required, props))
}

pub(super) fn base(lower: &mut Lower, name: &str, s: &Map<String, Value>) -> Result<Base, Error> {
    let (required, props) = field_lists(name, s)?;
    let item = s.get("additionalProperties").filter(|v| v.is_object());
    let plain = props.is_empty() && required.is_empty() && !reads(s);
    if let Some(item) = item.filter(|_| plain) {
        return Ok(Base::Map(Box::new(lower.ty(&format!("{name}.*"), item)?)));
    }
    if props.contains_key("") {
        return Err(Error::unsupported(format!(
            "{name}: a property named empty, which a non-object root's model holds its value in"
        )));
    }
    let index = lower.reserve(name);
    let mut fields = Vec::new();
    for (field, schema) in &props {
        let ty = lower.ty(&format!("{name}.{field}"), schema)?;
        fields.push(Field {
            name: field.clone(),
            ty,
            required: required.contains(field),
        });
    }
    lower.fill(
        index,
        Model {
            name: name.into(),
            fields,
            checks: Vec::new(),
            others: needed(s, &props, &required).then(|| Ty::of(Base::Any)),
        },
    );
    Ok(Base::Model(index))
}

pub(super) fn keyword(k: &mut Key) -> Done {
    let model = matches!(k.ty.base, Base::Model(_));
    match k.key {
        "properties" | "$ref" | "$defs" => Some(Ok(())),
        "additionalProperties" if !model || *k.value == Value::Bool(false) => Some(Ok(())),
        "minProperties" | "maxProperties" => {
            let Some(n) = count(k.value) else {
                return k.refuse();
            };
            if model {
                let (min, max) = if k.key == "minProperties" {
                    (Some(n), None)
                } else {
                    (None, Some(n))
                };
                k.ty.checks.push(Check::Properties { min, max });
            }
            Some(Ok(()))
        }
        "dependentRequired" => {
            if model {
                let Some(pairs) = dependents(k.value) else {
                    return k.refuse();
                };
                for (when, then) in pairs {
                    k.ty.checks.push(Check::Dependent { when, then });
                }
            }
            Some(Ok(()))
        }
        _ => None,
    }
}
