//! A document lowered as one model, and what its root's type says.

use serde_json::{Map, Value};

use super::Lower;
use crate::ir::{Base, Check, Model, Schema};
use crate::Error;

/// Lowers `schema` as model `name`: the schema and the model's index in it. An object is the
/// model's fields; any other root, a scalar, list, map, or nullable, is a model of one required
/// field named empty ([`Model::is_value`]) holding it, so a path to the value is the model's own.
/// `#/$defs/<name>` refers to the schema's own `$defs`, else to `shared`. `notes` names the
/// annotation keywords to carry in [`crate::ir::Notes`].
pub fn lower(
    name: &str,
    schema: &Value,
    shared: &Map<String, Value>,
    notes: &[&str],
) -> Result<(Schema, usize), Error> {
    let mut lower = Lower::new(Schema::default(), schema, shared, notes);
    let ty = lower.ty(name, schema)?;
    if let (Base::Model(m), false) = (&ty.base, ty.nullable) {
        if let Some(c) = ty.checks.iter().find(|c| !is_model_check(c)) {
            return Err(Error::unsupported(format!("{name}: {c:?} on a model")));
        }
        if let Some(model) = lower.out.models.get_mut(*m) {
            model.checks = ty.checks.clone();
        }
        return Ok((lower.finish(&[])?, *m));
    }
    let root = lower.model(Model::value(name, ty));
    Ok((lower.finish(&[])?, root))
}

/// A `type` list: one kind beside `null` is that kind, nullable; `null` alone is null. Two kinds
/// at once are unsupported.
pub(super) fn union<'a>(name: &str, ts: &'a [Value]) -> Result<(&'a str, bool), Error> {
    let mut kinds = Vec::new();
    for t in ts {
        let k = t
            .as_str()
            .ok_or_else(|| Error::invalid(format!("{name}: type {t}")))?;
        kinds.push(k);
    }
    let nullable = kinds.contains(&"null");
    let rest: Vec<&str> = kinds.into_iter().filter(|k| *k != "null").collect();
    match rest.as_slice() {
        [kind] => Ok((kind, nullable)),
        [] if nullable => Ok(("null", false)),
        _ => Err(Error::unsupported(format!(
            "{name}: type {}",
            Value::Array(ts.to_vec())
        ))),
    }
}

/// Whether `c` is a check on a model as a whole, which the model carries as its own.
fn is_model_check(c: &Check) -> bool {
    c.is_branch()
        || matches!(
            c,
            Check::Properties { .. }
                | Check::Dependent { .. }
                | Check::Present(_)
                | Check::Matching { .. }
                | Check::Unmatched { .. }
                | Check::Names(_)
                | Check::Unevaluated(_)
                | Check::OneOf(_)
        )
}
