//! Slots: the parts a model leaves open, and a model that fills them.

use super::{Base, Check, Model, Schema, Ty};

/// A part a model leaves open, by name.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    /// What the slot holds where nothing fills it.
    pub ty: Ty,
}

/// A model that is another one with its slots filled.
#[derive(Clone, Debug, PartialEq)]
pub struct Filled {
    /// The model with the slots, [`Schema::models`]' index.
    pub model: usize,
    /// What each of that model's slots holds here, in its slots' order.
    pub with: Vec<Ty>,
}

impl Check {
    /// The types this check holds.
    pub fn tys(&self) -> Vec<&Ty> {
        match self {
            Check::Contains { ty, .. }
            | Check::Not(ty)
            | Check::Field { ty, .. }
            | Check::Entry { ty, .. }
            | Check::Matching { ty, .. }
            | Check::Unmatched { ty, .. }
            | Check::Names(ty) => vec![ty],
            Check::ExactlyOne(subs) | Check::AtLeastOne(subs) => subs.iter().collect(),
            Check::Conditional {
                when,
                then,
                otherwise,
            } => [Some(when), then.as_ref(), otherwise.as_ref()]
                .into_iter()
                .flatten()
                .map(|t| &**t)
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl Ty {
    /// The types a value of this one holds: a list's or a map's item, a tuple's positions, a
    /// union's variants.
    pub fn parts(&self) -> Vec<&Ty> {
        match &self.base {
            Base::List(t) | Base::Map(t) => vec![t],
            Base::Tuple { prefix, rest } => prefix.iter().chain(rest.as_deref()).collect(),
            Base::Union(u) => u.variants.iter().collect(),
            _ => Vec::new(),
        }
    }
}

impl Model {
    /// The model as a filler holds it: each field that is a slot taking the type `with` gives the
    /// slot. A field of a model with slots keeps that model, which [`Schema::filled`] fills.
    fn with(&self, slots: &[Slot], with: &[Ty]) -> Model {
        let mut out = self.clone();
        for f in &mut out.fields {
            let filled = slots.iter().zip(with).find_map(|(s, ty)| {
                matches!(&f.ty.base, Base::Slot(n) if *n == s.name).then(|| ty.clone())
            });
            if let Some(ty) = filled {
                f.ty = ty;
            }
        }
        out.slots = Vec::new();
        out
    }
}

impl Schema {
    /// The model `model` is: the one it fills, else itself.
    pub fn filling(&self, model: usize) -> usize {
        let of = self.models.get(model).and_then(|m| m.of.as_ref());
        of.map_or(model, |of| of.model)
    }

    /// Model `model` with every slot filled: a filler as the fields of the model it fills, each
    /// slot's field of the filler's type, and a model with slots holding each slot's own type.
    /// `None` where the model has no slot and fills none.
    pub fn filled(&self, model: usize) -> Option<Model> {
        let m = self.models.get(model)?;
        let (of, with) = match &m.of {
            Some(f) => (self.models.get(f.model)?, f.with.clone()),
            None if m.slots.is_empty() => return None,
            None => (m, m.slots.iter().map(|s| s.ty.clone()).collect()),
        };
        let mut out = of.with(&of.slots, &with);
        out.name.clone_from(&m.name);
        Some(out)
    }
}
