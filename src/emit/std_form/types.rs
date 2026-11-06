//! Helpers for the Rust types of the std form.

use super::{unions, StdForm};
use crate::ir::{Base, Int, Ty};

/// The Rust type of an integer of width `w`.
pub(super) fn rust_int(w: Int) -> &'static str {
    match w {
        Int::I8 => "i8",
        Int::I16 => "i16",
        Int::I32 => "i32",
        Int::I64 => "i64",
        Int::I128 => "i128",
        Int::U8 => "u8",
        Int::U16 => "u16",
        Int::U32 => "u32",
        Int::U64 => "u64",
        Int::U128 => "u128",
    }
}

/// The models a value of `ty` holds in its own size: its own, or a tuple's.
pub(super) fn inline(ty: &Ty) -> Vec<usize> {
    match &ty.base {
        Base::Model(m) => vec![*m],
        Base::Union(u) => unions::models(&u.variants).collect(),
        Base::Tuple { prefix, .. } => prefix.iter().flat_map(inline).collect(),
        _ => Vec::new(),
    }
}

/// A Rust tuple of `parts`, whose one part needs a comma to be a tuple.
pub(super) fn tuple_of(parts: &[String]) -> String {
    match parts {
        [one] => format!("({one},)"),
        _ => format!("({})", parts.join(", ")),
    }
}

/// Every model a value of `ty` names, at any depth.
fn named(ty: &Ty, out: &mut Vec<usize>) {
    match &ty.base {
        Base::Model(m) => out.push(*m),
        Base::Union(u) => u.variants.iter().for_each(|v| named(v, out)),
        Base::Tuple { prefix, rest } => {
            prefix.iter().for_each(|p| named(p, out));
            rest.iter().for_each(|r| named(r, out));
        }
        Base::List(item) | Base::Map(item) => named(item, out),
        _ => {}
    }
}

impl StdForm<'_> {
    /// Whether model `m` is a value model written as an alias of its value's type: its root is no
    /// object, and it never names itself, directly or through other models, which no alias can.
    pub(crate) fn is_alias(&self, m: usize) -> bool {
        let Some(model) = self.schema.models.get(m).filter(|m| m.is_value()) else {
            return false;
        };
        let mut todo = Vec::new();
        model.fields.iter().for_each(|f| named(&f.ty, &mut todo));
        let mut seen: Vec<usize> = Vec::new();
        while let Some(n) = todo.pop() {
            if n == m {
                return false;
            }
            if seen.contains(&n) {
                continue;
            }
            seen.push(n);
            if let Some(next) = self.schema.models.get(n) {
                let held = next.fields.iter().map(|f| &f.ty).chain(next.others.iter());
                held.for_each(|t| named(t, &mut todo));
            }
        }
        true
    }
}
