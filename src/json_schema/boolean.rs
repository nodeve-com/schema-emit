//! Area: boolean schemas. `true` admits every value and `false` none. Each is a type of its own,
//! base `Any`: `false` carries `Not` of `true`, which fails every value, so an `Any` is only ever
//! checked for presence, whatever type the embedder gives it.

use crate::ir::{Base, Check, Ty};

pub(super) fn ty(admits: bool) -> Ty {
    let mut ty = Ty::of(Base::Any);
    if !admits {
        ty.checks.push(Check::Not(Box::new(Ty::of(Base::Any))));
    }
    ty
}
