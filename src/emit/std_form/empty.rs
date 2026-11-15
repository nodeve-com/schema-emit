//! Fields a program fills later: each named by its path from a root model, and held as an option.

use super::StdForm;
use crate::ir::{Base, Schema};
use crate::Error;

impl StdForm<'_> {
    /// This form, each field `paths` names one the program fills later, `root` the model the paths
    /// start from. A path is property names joined by `.`, as `total` or `place.key`: each names a
    /// field of the model so far, and the next a field of that field's model, through a `$ref` or
    /// what fills a dynamic reference. A path naming no field fails, naming the path.
    pub fn with_empty(mut self, root: usize, paths: &[&str]) -> Result<Self, Error> {
        let filled = self.schema.filled();
        for path in paths {
            let field = place(&filled, root, path)
                .ok_or_else(|| Error::invalid(format!("`{path}` names no field")))?;
            if !self.empty.contains(&field) {
                self.empty.push(field);
            }
        }
        Ok(self)
    }

    /// Whether model `model`'s struct holds field `field` as `Option<T>`, so the field can be
    /// empty: it is optional, or one the program fills later, and has no default. A filler's
    /// field is the field of the model it fills. False where the model has no such field.
    pub fn can_be_empty(&self, model: usize, field: usize) -> bool {
        let model = self.schema.filling(model);
        let f = self
            .schema
            .models
            .get(model)
            .and_then(|m| m.fields.get(field));
        f.is_some_and(|f| f.ty.default.is_none() && (!f.required || self.later(model, field)))
    }

    /// Whether field `field` of model `model` is one the program fills later.
    pub(crate) fn later(&self, model: usize, field: usize) -> bool {
        let model = self.schema.filling(model);
        self.empty.contains(&(model, field))
    }
}

/// The field `path` names from model `root` of `filled`, a [`Schema::filled`]: the model whose
/// struct holds it, and its index.
fn place(filled: &Schema, root: usize, path: &str) -> Option<(usize, usize)> {
    let mut steps = path.split('.');
    let (mut at, mut step) = (root, steps.next()?);
    loop {
        let m = filled.models.get(at)?;
        let k = (m.fields.iter()).position(|f| !step.is_empty() && f.name == step)?;
        let Some(next) = steps.next() else {
            return Some((filled.filling(at), k));
        };
        at = match m.fields.get(k)?.ty.base {
            Base::Model(inner) => inner,
            _ => return None,
        };
        step = next;
    }
}
