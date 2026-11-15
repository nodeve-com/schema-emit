//! A document's definitions: each one lowered together where the document has no root of its
//! own, and those its root reaches where it has.

use serde_json::{Map, Value};

use super::{references, Lower};
use crate::ir::{Schema, Ty};
use crate::Error;

/// Lowers each of `schema`'s `$defs`, by name, into one schema, its [`Schema::named`]: a
/// definition another names is the same model. `notes` names the annotation keywords to carry.
pub fn lower_defs(
    schema: &Value,
    shared: &Map<String, Value>,
    notes: &[&str],
) -> Result<Schema, Error> {
    let mut lower = Lower::new(Schema::default(), schema, shared, notes);
    let mut named = Vec::new();
    for name in def_names(schema) {
        let target = serde_json::json!({ "$ref": references::def_ref(&name) });
        let ty = lower.ty(&name, &target)?;
        named.push((name, ty));
    }
    let tys: Vec<&Ty> = named.iter().map(|(_, ty)| ty).collect();
    let mut out = lower.finish(&tys)?;
    out.named = named;
    Ok(out)
}

impl Lower {
    /// Each of `schema`'s own `$defs` entries lowering has made a type for, by name.
    pub(super) fn reached(&self, schema: &Value) -> Vec<(String, Ty)> {
        let found = def_names(schema).into_iter().filter_map(|name| {
            let target = Value::String(references::def_ref(&name));
            let t = references::resolve(self, &name, &target).ok()?;
            let ty = self.lowered.get(&t.key)?.clone();
            Some((name, ty))
        });
        found.collect()
    }
}

/// `document`'s own definitions by name, empty where it has none: what [`lower`](super::lower)'s
/// `shared` argument takes.
pub fn defs(document: &Value) -> Map<String, Value> {
    match document.get("$defs") {
        Some(Value::Object(defs)) => defs.clone(),
        _ => Map::new(),
    }
}

/// The keys of `schema`'s own `$defs`.
fn def_names(schema: &Value) -> Vec<String> {
    match schema.get("$defs") {
        Some(Value::Object(defs)) => defs.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// The document a type lowers from: the schema, the definitions shared beside it, and the
/// annotation keywords to carry.
pub struct Document<'a> {
    pub schema: &'a Value,
    pub shared: &'a Map<String, Value>,
    pub notes: &'a [&'a str],
}

/// Lowers `doc`'s schema of any type as `name`, adding its models to `out`; `shared` and `notes`
/// as in [`lower`].
pub fn lower_ty(name: &str, doc: &Document, out: Schema) -> Result<(Schema, Ty), Error> {
    let mut lower = Lower::new(out, doc.schema, doc.shared, doc.notes);
    let ty = lower.ty(name, doc.schema)?;
    Ok((lower.finish(&[&ty])?, ty))
}
