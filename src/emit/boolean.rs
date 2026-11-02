//! Area: boolean schemas. A value of any kind, `Base::Any`, has no view to read: the embedder
//! types it as it chooses, and only the checks that need no view apply: `Not` of such a type,
//! and an empty choice, which fails every value.

use super::{guard, At, Emitter};
use crate::ir::{Base, Check, Ty};
use crate::Error;

pub(super) fn value(e: &Emitter, at: &mut At, ty: &Ty) -> Result<String, Error> {
    if !ty.checks.iter().all(viewless) {
        return Err(Error::unsupported(format!("checks on {:?}", ty.base)));
    }
    let conds = e.conds(&Base::Any, &ty.checks, "")?;
    Ok(guard(at, &conds))
}

/// Whether `check` needs no view: `Not` of a viewless type, or a choice with no literal.
fn viewless(check: &Check) -> bool {
    match check {
        Check::Not(inner) => inner.base == Base::Any && inner.checks.iter().all(viewless),
        Check::OneOf(lits) => lits.is_empty(),
        _ => false,
    }
}
