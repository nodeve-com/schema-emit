//! The source a build script writes: a schema's std form and the checks it asks for.

use super::{names, StdForm};
use crate::emit::Emitter;
use crate::ir::{Model, Schema};
use crate::Error;

/// `schema`'s std form, its structs named by [`names`], with a check function per `(model,
/// name)` in `checks`. A model `external` gives a module path is read through that module.
pub fn source(
    schema: &Schema,
    external: &dyn Fn(&Model) -> Option<String>,
    checks: &[(usize, &str)],
) -> Result<String, Error> {
    let named = names(schema);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let theirs = |i: usize| schema.models.get(i).and_then(external);
    let std = StdForm::new(schema, &name).with_external(&theirs);
    let mut out = std.items()?;
    for (model, check) in checks {
        out.push_str(&Emitter::new(schema, &std).check(*model, check)?);
    }
    Ok(out)
}
