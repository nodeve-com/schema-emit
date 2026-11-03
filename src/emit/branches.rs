//! Area: `anyOf`, `oneOf`, and `if`. Each branch is the value's base with its own checks. On a
//! scalar's or text's view, its validity is a condition like any check's ([`Emitter::conds`]).
//! On a model or a list, the branch walks the value into a `Holds` report, whose verdict is the
//! condition. `anyOf` holds where some branch's condition does, `oneOf` where exactly one does,
//! and `if`'s where the branch its condition picks does.

use super::{guard, Emitter, Walk, STD};
use crate::ir::{Check, Ty};
use crate::Error;

pub(super) fn cond(e: &Emitter, check: &Check, view: &str) -> Option<Result<String, Error>> {
    let holds = |ty: &Ty| holds(e, ty, view);
    join(check, holds)
}

impl Walk<'_, '_> {
    /// Each branch check of `checks` on `value`, a `&` of a present model or list.
    pub(super) fn branches(&mut self, value: &str, checks: &[Check]) -> Result<String, Error> {
        let mut conds = Vec::new();
        for check in checks {
            let mut walk = |ty: &Ty| self.walks(value, ty);
            conds.push(join(check, &mut walk).unwrap_or_else(|| {
                Err(Error::unsupported(format!("{check:?} on a model or list")))
            })?);
        }
        Ok(guard(&self.at, &conds))
    }

    /// A boolean expression, true where branch `ty` holds of `value`: its checks report to a
    /// `Holds` in place of the value's report.
    fn walks(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        let q = self.at.local("q");
        let body = self.branch(value, ty)?;
        Ok(format!(
            "{{ let mut {q} = {STD}::Holds::default(); {{ use {STD}::Report as _; let r = &mut {q};\n\
             {body}}} {q}.held() }}"
        ))
    }
}

/// `check`'s condition, each branch's by `holds`; `None` where it isn't a branch check.
fn join(
    check: &Check,
    mut holds: impl FnMut(&Ty) -> Result<String, Error>,
) -> Option<Result<String, Error>> {
    let mut all = |subs: &[Ty]| subs.iter().map(&mut holds).collect::<Result<Vec<_>, _>>();
    Some(match check {
        Check::ExactlyOne(subs) => all(subs).map(|hs| {
            format!(
                "([{}].iter().filter(|ok| **ok).count() == 1)",
                hs.join(", ")
            )
        }),
        Check::AtLeastOne(subs) => all(subs).map(|hs| format!("({})", hs.join(" || "))),
        Check::Conditional {
            when,
            then,
            otherwise,
        } => (|| {
            let when = holds(when)?;
            let then = then.as_deref().map_or(Ok("true".into()), &mut holds)?;
            let otherwise = otherwise.as_deref().map_or(Ok("true".into()), &mut holds)?;
            Ok(format!("(if {when} {{ {then} }} else {{ {otherwise} }})"))
        })(),
        _ => return None,
    })
}

/// A boolean expression, true where `ty`'s checks all hold of the view, a scalar's or text's.
fn holds(e: &Emitter, ty: &Ty, view: &str) -> Result<String, Error> {
    if ty.base.is_structured() {
        return Err(Error::unsupported(format!("a branch of {:?}", ty.base)));
    }
    let conds = e.conds(&ty.base, &ty.checks, view)?;
    Ok(if conds.is_empty() {
        "true".into()
    } else {
        format!("({})", conds.join(" && "))
    })
}
