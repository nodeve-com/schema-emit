//! A branch over a model: each field the subschema names checked by its subschema, then its other
//! keywords on the model. A key no field names is a [`Check::Entry`] of the model's other keys.

use serde_json::{Map, Value};

use super::{onto, parent};
use crate::ir::{Base, Check, Field, Ty};
use crate::json_schema::{models, Lower};
use crate::Error;

/// A branch over model `m`.
pub(super) fn branch(
    lower: &mut Lower,
    key: &str,
    m: usize,
    nullable: bool,
    s: &Map<String, Value>,
) -> Result<Ty, Error> {
    if let Some(k) = [
        "minProperties",
        "maxProperties",
        "dependentRequired",
        "additionalProperties",
    ]
    .into_iter()
    .find(|k| s.contains_key(*k))
    {
        return Err(Error::unsupported(format!(
            "`{k}` in a `{key}` over an object"
        )));
    }
    let (required, props) = models::field_lists(key, s)?;
    let (fields, others) = lower
        .out
        .models
        .get(m)
        .map(|x| (x.fields.clone(), x.others.clone()))
        .ok_or_else(|| Error::invalid(format!("{key}: no model {m}")))?;
    let mut checks = Vec::new();
    for (name, p) in &props {
        let place = format!("{key}.{name}");
        let need = required.contains(name);
        checks.push(match fields.iter().position(|f| f.name == *name) {
            Some(index) => field(lower, &place, index, &fields, p, need)?,
            None => entry(lower, &place, name, others.as_ref(), Some(p), need)?,
        });
    }
    for name in required.iter().filter(|r| !props.contains_key(r.as_str())) {
        checks.push(match fields.iter().position(|f| f.name == *name) {
            Some(index) => field(lower, key, index, &fields, &Value::Bool(true), true)?,
            None => entry(lower, key, name, others.as_ref(), None, true)?,
        });
    }
    let names_others = checks.iter().any(|c| matches!(c, Check::Entry { .. }));
    if names_others && s.contains_key("unevaluatedProperties") {
        return Err(Error::unsupported(format!(
            "`unevaluatedProperties` in a `{key}` naming keys no field names"
        )));
    }
    let mut rest = s.clone();
    for k in ["type", "properties", "required"] {
        rest.remove(k);
    }
    let ty = Ty {
        nullable,
        checks,
        ..Ty::of(Base::Model(m))
    };
    lower.keywords(key, &rest, ty)
}

/// Field `index`, checked as subschema `p`.
fn field(
    lower: &mut Lower,
    place: &str,
    index: usize,
    fields: &[Field],
    p: &Value,
    required: bool,
) -> Result<Check, Error> {
    let fty = fields
        .get(index)
        .map(|f| parent(&f.ty))
        .ok_or_else(|| Error::invalid(format!("{place}: no field {index}")))?;
    let ty = onto(lower, place, &fty, p)?;
    Ok(Check::Field {
        index,
        ty: Box::new(ty),
        required,
    })
}

/// The value under key `name`, which no field names, checked as subschema `p`, if any. The
/// model must hold other keys, and a subschema judging their values needs them typed.
fn entry(
    lower: &mut Lower,
    place: &str,
    name: &str,
    others: Option<&Ty>,
    p: Option<&Value>,
    required: bool,
) -> Result<Check, Error> {
    let others = others.ok_or_else(|| {
        Error::unsupported(format!(
            "{place}: names `{name}`, which the object's fields lack"
        ))
    })?;
    let base = parent(others);
    let trivial = match p {
        None | Some(Value::Bool(true)) => true,
        Some(Value::Object(o)) => o.is_empty(),
        _ => false,
    };
    let ty = match p {
        Some(p) if !trivial => onto(lower, place, &base, p)?,
        _ => base,
    };
    Ok(Check::Entry {
        name: name.into(),
        ty: Box::new(ty),
        required,
    })
}
