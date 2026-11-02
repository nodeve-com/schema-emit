//! Area: unions. A value of [`Base::Union`] is one variant, each a branch's type. Its check matches
//! the variant, read through [`Access::variant`], and runs that branch's check at the union's own
//! path, so a failure inside reports the path the branch gives.

use std::fmt::Write as _;

use super::{At, Emitter};
use crate::ir::{Base, Ty, Union};
use crate::Error;

/// `value`, a `&` of `union`, checked as the variant it holds.
pub(super) fn union(
    e: &Emitter,
    at: &mut At,
    value: &str,
    ty: &Ty,
    union: &Union,
) -> Result<String, Error> {
    if !ty.checks.is_empty() {
        return Err(Error::unsupported("a check on a union"));
    }
    let mut out = String::new();
    for (i, variant) in union.variants.iter().enumerate() {
        let held = at.local("u");
        let body = e.value(at, &held, variant)?;
        let read = e.access.variant(value, union, i)?;
        let _ = writeln!(out, "if let Some({held}) = {read} {{ {body}}}");
    }
    Ok(out)
}

/// The function `name` checking a `&` of `union`.
pub(super) fn function(e: &Emitter, union: &Union, name: &str) -> Result<String, Error> {
    let ty = Ty::of(Base::Union(union.clone()));
    let rust = e.access.rust(e.schema, &ty)?;
    e.function(name, &rust, "Union", &ty)
}
