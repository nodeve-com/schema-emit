//! Helpers for the Rust types of the std form.

use super::unions;
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
