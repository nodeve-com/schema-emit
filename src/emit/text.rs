//! Area: text, its length and pattern. Text's view is a `&str`; text the embedder can't read as
//! UTF-8 fails `cast`.
//!
//! A pattern is a `static` table pattern-emit built as ECMA-262 reads it, searched on the text;
//! a pattern its guard or cap refuses is unsupported, and a table that proves unusable at run
//! time fails `range`. The check's `_load` function checks its tables ahead of any value.

use super::{guard, readable, At, Emitter};
use crate::ir::{Base, Check, Count, Ty};
use crate::Error;

pub(super) fn text(e: &Emitter, at: &mut At, value: &str, ty: &Ty) -> Result<String, Error> {
    let view = at.local("t");
    let conds = e.conds(&ty.base, &ty.checks, &view)?;
    if conds.is_empty() {
        return Ok(String::new());
    }
    let body = guard(at, &conds);
    Ok(readable(at, &e.access.text(value), &view, &body))
}

pub(super) fn cond(
    e: &Emitter,
    check: &Check,
    base: &Base,
    view: &str,
) -> Option<Result<String, Error>> {
    match check {
        Check::Length { .. } | Check::Pattern(_) if *base != Base::Text => {
            Some(Err(Error::invalid(format!("{check:?} on {base:?}"))))
        }
        Check::Length { min, max, of } => {
            let len = match of {
                Count::Chars => format!("{}::chars({view})", super::STD),
                Count::Bytes => format!("({view}.len() as u64)"),
            };
            let mut conds = Vec::new();
            if let Some(min) = min {
                conds.push(format!("{len} >= {min}"));
            }
            if let Some(max) = max {
                conds.push(format!("{len} <= {max}"));
            }
            if conds.is_empty() {
                conds.push("true".into());
            }
            Some(Ok(format!("({})", conds.join(" && "))))
        }
        Check::Pattern(p) => Some(pattern(e, p, view)),
        _ => None,
    }
}

/// Whether the table for `p` finds it in `view`, the table a `static` of the check.
fn pattern(e: &Emitter, p: &str, view: &str) -> Result<String, Error> {
    let table = pattern_emit::build_ecma(p)
        .map_err(|why| Error::unsupported(format!("pattern {p:?}: {why}")))?;
    let name = e.table(p, table);
    Ok(format!("matches!({name}.is_match({view}), Ok(true))"))
}
