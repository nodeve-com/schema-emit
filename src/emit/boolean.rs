//! Area: boolean schemas. A value of any kind, `Base::Any`, has no view to read: the embedder
//! types it as it chooses, and only the checks that need no view apply: `Not` of such a type,
//! and an empty choice, which fails every value.

use super::{guard, Walk};
use crate::ir::{Base, Check, Ty};
use crate::Error;

impl Walk<'_, '_> {
    /// The checks on a value of any kind: those that need no view.
    pub(super) fn any(&self, ty: &Ty) -> Result<String, Error> {
        if !ty.checks.iter().all(viewless) {
            return Err(Error::unsupported(format!("checks on {:?}", ty.base)));
        }
        let conds = self.e.conds(&Base::Any, &ty.checks, "")?;
        Ok(guard(&self.at, &conds))
    }
}

/// Whether `check` needs no view: `Not` of a viewless type, or a choice with no literal.
fn viewless(check: &Check) -> bool {
    match check {
        Check::Not(inner) => inner.base == Base::Any && inner.checks.iter().all(viewless),
        Check::OneOf(lits) => lits.is_empty(),
        _ => false,
    }
}
