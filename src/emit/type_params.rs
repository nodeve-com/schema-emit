//! Area: type parameters. A value of [`Base::DynamicRef`](crate::ir::Base::DynamicRef) checks by the function the
//! check takes for the type parameter, where the model being checked has it. Under a model that fills the
//! type parameter, or one another resource holds with the type parameter's own type, the type is known: its checks
//! are written in place, under the path so far.

use super::{Emitter, Walk};
use crate::ir::Ty;
use crate::Error;

impl Emitter<'_> {
    /// Enters `model` for a walk of its fields: what each of its type parameters holds is known from here
    /// in, unless the check being written takes it. Gives the count of known type parameters before, to
    /// leave with.
    pub(super) fn fill(&self, model: usize) -> usize {
        let mut fills = self.fills.borrow_mut();
        let kept = fills.len();
        let Some(m) = self.schema.models.get(model) else {
            return kept;
        };
        if let Some(of) = &m.of {
            let dynamic_refs = self
                .schema
                .models
                .get(of.model)
                .map(|g| g.dynamic_refs.as_slice());
            let names = dynamic_refs
                .unwrap_or_default()
                .iter()
                .map(|s| s.name.clone());
            fills.extend(names.zip(of.with.iter().cloned()));
            return kept;
        }
        for s in &m.dynamic_refs {
            let taken = self.std.scope().iter().any(|o| o.name == s.name);
            if !taken && !fills.iter().any(|(n, _)| *n == s.name) {
                fills.push((s.name.clone(), s.ty.clone()));
            }
        }
        kept
    }

    /// What type parameter `anchor` holds, where the walk knows.
    fn filled(&self, anchor: &str) -> Option<Ty> {
        let fills = self.fills.borrow();
        let found = fills.iter().rev().find(|(n, _)| n == anchor);
        found.map(|(_, ty)| ty.clone())
    }
}

impl Walk<'_, '_> {
    /// `value`, a `&` of what type parameter `anchor` holds, checked by the type parameter's own check.
    pub(super) fn param_value(&mut self, value: &str, anchor: &str) -> Result<String, Error> {
        match self.e.filled(anchor) {
            Some(ty) => self.value(value, &ty),
            None => Ok(format!("{}({value}, r);\n", self.e.std.param_check(anchor))),
        }
    }
}
