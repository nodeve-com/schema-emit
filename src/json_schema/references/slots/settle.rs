//! Once a document has lowered: each model's slots, and the refusal of a slot where no struct's
//! type parameter can stand.

use crate::ir::{Base, Filled, Model, Schema, Slot, Ty};
use crate::json_schema::Lower;
use crate::Error;

impl Lower {
    /// The schema lowered, each model holding its slots; `tys` are the types the caller gets.
    pub(in crate::json_schema) fn finish(mut self, tys: &[&Ty]) -> Result<Schema, Error> {
        for m in 0..self.out.models.len() {
            let Some(home) = self.homes.get(m).filter(|h| !h.is_empty()).cloned() else {
                continue;
            };
            let names = self.reach(&Ty::of(Base::Model(m)), &home).open;
            let mut slots = Vec::new();
            for name in names {
                let ty = self.anchor(&name, &home, &name)?;
                slots.push(Slot { name, ty });
            }
            if let Some(model) = self.out.models.get_mut(m) {
                model.slots = slots;
            }
        }
        for m in self.out.models.iter().filter(|m| !m.slots.is_empty()) {
            let fields = m.fields.iter().map(|f| (&f.ty, !m.is_value()));
            let held = m.checks.iter().flat_map(|c| c.tys());
            let rest = held.chain(&m.others).map(|t| (t, false));
            for (ty, ok) in fields.chain(rest) {
                self.placed(ty, ok).map_err(|e| e.at(&m.name))?;
            }
        }
        match (tys.iter()).find(|t| !matches!(t.base, Base::Model(_)) && self.open(t)) {
            Some(_) => Err(misplaced()),
            None => Ok(self.out),
        }
    }

    /// Adds the model named `name` that is `model` with its slots holding `with`.
    pub(super) fn filled(&mut self, name: &str, model: usize, with: Vec<Ty>) -> usize {
        let of = Some(Filled { model, with });
        self.model(Model {
            name: name.into(),
            of,
            ..Model::default()
        })
    }

    /// Whether `ty` is a slot, or a model with slots.
    fn open(&self, ty: &Ty) -> bool {
        match &ty.base {
            Base::Slot(_) => true,
            Base::Model(m) => (self.out.models.get(*m)).is_some_and(|m| !m.slots.is_empty()),
            _ => ty.parts().into_iter().any(|t| self.open(t)),
        }
    }

    /// Refuses a slot, or a model with slots, in `ty` where `ok` is false, in a union, or in a
    /// check: a struct's field, a list's item, a map's value, and a tuple's position hold one.
    fn placed(&self, ty: &Ty, ok: bool) -> Result<(), Error> {
        let union = matches!(ty.base, Base::Union(_));
        let direct = !matches!(ty.base, Base::List(_) | Base::Map(_) | Base::Tuple { .. });
        if direct && self.open(ty) && (!ok || union) {
            return Err(misplaced());
        }
        ty.parts()
            .into_iter()
            .try_for_each(|t| self.placed(t, ok))?;
        let mut held = ty.checks.iter().flat_map(|c| c.tys());
        held.try_for_each(|t| self.placed(t, false))
    }
}

/// The refusal of a slot no type parameter can stand for.
fn misplaced() -> Error {
    Error::unsupported(
        "a slot outside an object's property: in a union, a branch, a key no field names, or a root that is no object",
    )
}
