//! Dynamic references: the parts a model leaves open, and a model that fills them.

use super::{Base, Check, Schema, Ty};

/// A part a model leaves open, by name.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicRef {
    pub name: String,
    /// What the dynamic reference holds where nothing fills it.
    pub ty: Ty,
}

/// A model that is another one with its dynamic references filled.
#[derive(Clone, Debug, PartialEq)]
pub struct Filled {
    /// The model with the dynamic references, [`Schema::models`]' index.
    pub model: usize,
    /// What each of that model's dynamic references holds here, in its dynamic references' order.
    pub with: Vec<Ty>,
}

/// The types check `$check` holds, each borrowed by `$borrow` (`as_ref` or `as_mut`), `$iter` the
/// matching iteration over a list of them.
macro_rules! held {
    ($check:expr, $iter:ident, $borrow:ident) => {
        match $check {
            Check::Contains { ty, .. }
            | Check::Not(ty)
            | Check::Field { ty, .. }
            | Check::Entry { ty, .. }
            | Check::Matching { ty, .. }
            | Check::Unmatched { ty, .. }
            | Check::Names(ty) => vec![ty.$borrow()],
            Check::ExactlyOne(subs) | Check::AtLeastOne(subs) => subs.$iter().collect(),
            Check::Conditional {
                when,
                then,
                otherwise,
            } => [Some(when), then.$borrow(), otherwise.$borrow()]
                .into_iter()
                .flatten()
                .map(|t| t.$borrow())
                .collect(),
            _ => Vec::new(),
        }
    };
}

impl Check {
    /// The types this check holds.
    pub fn tys(&self) -> Vec<&Ty> {
        held!(self, iter, as_ref)
    }

    /// The types this check holds, to change.
    pub fn tys_mut(&mut self) -> Vec<&mut Ty> {
        held!(self, iter_mut, as_mut)
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

impl Schema {
    /// The model `model` is: the one it fills, else itself.
    pub fn filling(&self, model: usize) -> usize {
        let of = self.models.get(model).and_then(|m| m.of.as_ref());
        of.map_or(model, |of| of.model)
    }
}
