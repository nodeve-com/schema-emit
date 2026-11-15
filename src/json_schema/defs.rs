//! Every definition of a document with no root of its own, lowered together.

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
    let names: Vec<String> = match schema.get("$defs") {
        Some(Value::Object(defs)) => defs.keys().cloned().collect(),
        _ => Vec::new(),
    };
    let mut named = Vec::new();
    for name in names {
        let target = serde_json::json!({ "$ref": references::def_ref(&name) });
        let ty = lower.ty(&name, &target)?;
        named.push((name, ty));
    }
    let tys: Vec<&Ty> = named.iter().map(|(_, ty)| ty).collect();
    let mut out = lower.finish(&tys)?;
    out.named = named;
    Ok(out)
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
