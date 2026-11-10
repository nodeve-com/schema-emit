//! Area: slots. A value of [`Base::Slot`](crate::ir::Base::Slot) checks by the function the
//! check takes for the slot, where the model being checked has it. Under a model that fills the
//! slot, or one another resource holds with the slot's own type, the type is known: its checks
//! are written in place, under the path so far.

use super::{Emitter, Walk};
use crate::ir::Ty;
use crate::Error;

impl Emitter<'_> {
    /// Enters `model` for a walk of its fields: what each of its slots holds is known from here
    /// in, unless the check being written takes it. Gives the count of known slots before, to
    /// leave with.
    pub(super) fn fill(&self, model: usize) -> usize {
        let mut fills = self.fills.borrow_mut();
        let kept = fills.len();
        let Some(m) = self.schema.models.get(model) else {
            return kept;
        };
        if let Some(of) = &m.of {
            let slots = self.schema.models.get(of.model).map(|g| g.slots.as_slice());
            let names = slots.unwrap_or_default().iter().map(|s| s.name.clone());
            fills.extend(names.zip(of.with.iter().cloned()));
            return kept;
        }
        for s in &m.slots {
            let taken = self.std.scope().iter().any(|o| o.name == s.name);
            if !taken && !fills.iter().any(|(n, _)| *n == s.name) {
                fills.push((s.name.clone(), s.ty.clone()));
            }
        }
        kept
    }

    /// What slot `slot` holds, where the walk knows.
    fn filled(&self, slot: &str) -> Option<Ty> {
        let fills = self.fills.borrow();
        let found = fills.iter().rev().find(|(n, _)| n == slot);
        found.map(|(_, ty)| ty.clone())
    }
}

impl Walk<'_, '_> {
    /// `value`, a `&` of what slot `slot` holds, checked by the slot's own check.
    pub(super) fn slot_value(&mut self, value: &str, slot: &str) -> Result<String, Error> {
        match self.e.filled(slot) {
            Some(ty) => self.value(value, &ty),
            None => Ok(format!("{}({value}, r);\n", self.e.std.slot_check(slot))),
        }
    }
}
