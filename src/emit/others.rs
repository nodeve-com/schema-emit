//! Area: keys no field names. A model holding [`Model::others`] is read through
//! [`Access::others`] and [`Access::other`]: every other key and its value, or the value under
//! one key. A key is a field's name, else an other key; a name neither holds is never present.
//!
//! A key's value fails at the model's path plus `.*`, since a path is a literal. `Present` fails
//! `absent` at the key's own path. A rule on a key's name, `Matching` and `Unmatched` by pattern
//! and `Names` by its text checks, reads the key as a `&str`.
//!
//! [`Access::others`]: super::Access::others
//! [`Access::other`]: super::Access::other

use std::fmt::Write as _;

use super::{guard, At, Emitter};
use crate::ir::{Base, Check, Model, Ty};
use crate::Error;

/// `Option<&T>`: the value of key `name` of model `model`, read from `value`.
pub(super) fn entry(
    e: &Emitter,
    value: &str,
    m: &Model,
    model: usize,
    name: &str,
) -> Result<String, Error> {
    if let Some(i) = m.fields.iter().position(|f| f.name == name) {
        return Ok(e.access.field(value, model, i));
    }
    match m.others {
        Some(_) => e.access.other(value, model, &format!("{name:?}")),
        None => Ok("None::<&()>".into()),
    }
}

/// The failures of one check on a model's keys, or nothing where it isn't one.
pub(super) fn judge(
    e: &Emitter,
    at: &mut At,
    value: &str,
    m: &Model,
    model: usize,
    check: &Check,
) -> Result<String, Error> {
    match check {
        Check::Present(names) => present(e, at, value, m, model, names),
        Check::Matching { pattern, ty } => {
            let k = at.local("k");
            let found = pattern_cond(e, pattern, &k)?;
            each(e, at, value, model, &k, &found, ty)
        }
        Check::Unmatched { patterns, ty } => {
            let k = at.local("k");
            let found = patterns
                .iter()
                .map(|p| pattern_cond(e, p, &k))
                .collect::<Result<Vec<_>, _>>()?;
            let any = if found.is_empty() {
                "false".into()
            } else {
                found.join(" || ")
            };
            each(e, at, value, model, &k, &format!("!({any})"), ty)
        }
        Check::Names(ty) => names(e, at, value, m, model, ty),
        _ => Ok(String::new()),
    }
}

/// Whether `check` is one [`judge`] writes.
pub(super) fn judges(check: &Check) -> bool {
    matches!(
        check,
        Check::Present(_) | Check::Matching { .. } | Check::Unmatched { .. } | Check::Names(_)
    )
}

/// Whether the pattern finds itself in the key `k`, a `&str`.
fn pattern_cond(e: &Emitter, pattern: &str, k: &str) -> Result<String, Error> {
    super::text::cond(e, &Check::Pattern(pattern.into()), &Base::Text, k)
        .unwrap_or_else(|| Err(Error::invalid("a pattern check")))
}

/// Each other key where `found` holds, its value checked as `ty`.
fn each(
    e: &Emitter,
    at: &mut At,
    value: &str,
    model: usize,
    k: &str,
    found: &str,
    ty: &Ty,
) -> Result<String, Error> {
    let x = at.local("x");
    let outer = at.path.clone();
    at.path.push_str(".*");
    let body = e.value(at, &x, ty);
    at.path = outer;
    Ok(format!(
        "for ({k}, {x}) in {} {{ if {found} {{ {}}} }}\n",
        e.access.others(value, model)?,
        body?
    ))
}

/// Each key named is present.
fn present(
    e: &Emitter,
    at: &At,
    value: &str,
    m: &Model,
    model: usize,
    names: &[String],
) -> Result<String, Error> {
    let mut out = String::new();
    for name in names {
        let sub = at.under(name);
        let _ = writeln!(
            out,
            "if !({}).is_some() {{ {} }}",
            entry(e, value, m, model, name)?,
            sub.fail("Absent")
        );
    }
    Ok(out)
}

/// Each key present, a field's or another, fails `range` unless its name meets `ty`.
fn names(
    e: &Emitter,
    at: &mut At,
    value: &str,
    m: &Model,
    model: usize,
    ty: &Ty,
) -> Result<String, Error> {
    let mut out = String::new();
    let k = at.local("k");
    let conds = e.conds(&Base::Text, &ty.checks, &k)?;
    for (i, f) in m.fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "if ({}).is_some() {{ let {k}: &str = {:?};\n{}}}",
            e.access.field(value, model, i),
            f.name,
            guard(at, &conds)
        );
    }
    if m.others.is_some() {
        let _ = writeln!(
            out,
            "for ({k}, _) in {} {{\n{}}}",
            e.access.others(value, model)?,
            guard(at, &conds)
        );
    }
    Ok(out)
}
