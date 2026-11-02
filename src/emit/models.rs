//! Area: models, nested and as maps. A model's fields check in order, each under the path so far
//! plus `.<field>`; a nested model inlines, so every path is a literal. A model's `Properties`
//! checks count its fields present, after the fields have checked, and fail `range` past them.
//! A `Dependent` check fails `absent` at each field a present field requires and lacks.
//!
//! A map's entries read through [`Access::entries`], each value checked as the map's item type,
//! at the map's path plus `.*`. Stub: a count or choice on a map. A model that refers to itself
//! is called, not inlined (`recursion.rs`).

use std::fmt::Write as _;

use super::{guard, others, presence, At, Emitter};
use crate::ir::{Base, Check, Model, Ty};
use crate::Error;

/// Nesting deeper than this is a recursive model.
const DEPTH: usize = 32;

/// `value`, a `&` of model `model`, checked against each of `checks`, its fields first.
pub(super) fn model(
    e: &Emitter,
    at: &mut At,
    value: &str,
    model: usize,
    checks: &[Check],
) -> Result<String, Error> {
    if let Some(called) = super::recursion::call(e, at, value, model, checks) {
        return called;
    }
    inline(e, at, value, model, checks)
}

/// `value`, a `&` of model `model`, its fields checked in place, then each of `checks`.
pub(super) fn inline(
    e: &Emitter,
    at: &mut At,
    value: &str,
    model: usize,
    checks: &[Check],
) -> Result<String, Error> {
    let m = get(e, at, model)?;
    let mut out = String::new();
    for (i, f) in m.fields.iter().enumerate() {
        let outer = at.path.clone();
        if !f.name.is_empty() {
            at.path.push('.');
        }
        at.path.push_str(&f.name);
        let field = presence::field(e, at, value, model, i, f);
        at.path = outer;
        out.push_str(&field?);
    }
    out.push_str(&whole(e, at, value, m, model, checks)?);
    Ok(out)
}

/// Model `model`, unless the path here nests too deep for any model but a recursive one.
fn get<'a>(e: &'a Emitter, at: &At, model: usize) -> Result<&'a Model, Error> {
    let m = e
        .schema
        .models
        .get(model)
        .ok_or_else(|| Error::invalid(format!("no model {model}")))?;
    if at.path.matches('.').count() > DEPTH {
        return Err(Error::unsupported(format!("model {} nests itself", m.name)));
    }
    Ok(m)
}

/// A branch over model `model`: each field or other key it names ([`Check::Field`],
/// [`Check::Entry`]) checked as it says, then its other checks. A choice of a present model
/// holds nothing, as [`Emitter::present`] has it.
pub(super) fn view(
    e: &Emitter,
    at: &mut At,
    value: &str,
    model: usize,
    branch: &Ty,
) -> Result<String, Error> {
    let (m, checks) = (get(e, at, model)?, &branch.checks);
    if checks.iter().any(|c| matches!(c, Check::OneOf(_))) {
        return Ok(at.fail("Range"));
    }
    let mut out = String::new();
    let mut rest = Vec::new();
    for check in checks {
        let (read, name, ty, required, nullable) = match check {
            Check::Field {
                index,
                ty,
                required,
            } => {
                let f = m
                    .fields
                    .get(*index)
                    .ok_or_else(|| Error::invalid(format!("{}: no field {index}", m.name)))?;
                (
                    e.access.field(value, model, *index),
                    f.name.clone(),
                    ty,
                    *required,
                    f.ty.nullable,
                )
            }
            Check::Entry { name, ty, required } => {
                let nullable = m.others.as_ref().is_some_and(|o| o.nullable);
                let read = others::entry(e, value, m, model, name)?;
                (read, name.clone(), ty, *required, nullable)
            }
            _ => {
                rest.push(check.clone());
                continue;
            }
        };
        let outer = at.path.clone();
        if !name.is_empty() {
            at.path.push('.');
        }
        at.path.push_str(&name);
        let x = at.local("f");
        let body = e.branch_value(at, &x, ty, nullable);
        let absent = if required {
            at.fail("Absent")
        } else {
            String::new()
        };
        at.path = outer;
        let _ = writeln!(
            out,
            "match {read} {{ Some({x}) => {{ {}}} None => {{ {absent} }} }}",
            body?
        );
    }
    let (branches, rest): (Vec<Check>, Vec<Check>) = rest.into_iter().partition(Check::is_branch);
    out.push_str(&whole(e, at, value, m, model, &rest)?);
    out.push_str(&super::branches::structured(e, at, value, &branches)?);
    Ok(out)
}

/// A model's checks as a whole: each dependency, then its count of keys present.
pub(super) fn whole(
    e: &Emitter,
    at: &mut At,
    value: &str,
    m: &Model,
    model: usize,
    checks: &[Check],
) -> Result<String, Error> {
    let mut out = String::new();
    for check in checks {
        if let Check::Dependent { when, then } = check {
            out.push_str(&dependent(e, at, value, m, model, when, then)?);
        }
    }
    for check in checks {
        if let Check::Unevaluated(names) = check {
            out.push_str(&super::unevaluated::fields(e, at, value, m, model, names)?);
        }
        out.push_str(&others::judge(e, at, value, m, model, check)?);
    }
    let counts: Vec<&Check> = checks
        .iter()
        .filter(|c| {
            !matches!(c, Check::Dependent { .. } | Check::Unevaluated(_)) && !others::judges(c)
        })
        .collect();
    if counts.is_empty() {
        return Ok(out);
    }
    let count = at.local("c");
    let mut tally = String::new();
    for (i, _) in m.fields.iter().enumerate() {
        let _ = writeln!(
            tally,
            "if {}.is_some() {{ {count} = {count}.saturating_add(1); }}",
            e.access.field(value, model, i)
        );
    }
    if m.others.is_some() {
        let _ = writeln!(
            tally,
            "{count} = {count}.saturating_add({}.count() as u64);",
            e.access.others(value, model)?
        );
    }
    let conds = counts
        .iter()
        .map(|c| cond(c, &count))
        .collect::<Result<Vec<_>, _>>()?;
    out.push_str(&format!(
        "{{ let mut {count}: u64 = 0;\n{tally}{}}}\n",
        guard(at, &conds)
    ));
    Ok(out)
}

/// Fails `absent` at each key `then` names when key `when` is present and that key isn't.
/// A `when` no key holds is never present, so it checks nothing.
fn dependent(
    e: &Emitter,
    at: &At,
    value: &str,
    m: &Model,
    model: usize,
    when: &str,
    then: &[String],
) -> Result<String, Error> {
    let trigger = others::entry(e, value, m, model, when)?;
    let mut out = String::new();
    for name in then {
        let sub = at.under(name);
        let _ = writeln!(
            out,
            "if ({trigger}).is_some() && !({}).is_some() {{ {} }}",
            others::entry(e, value, m, model, name)?,
            sub.fail("Absent")
        );
    }
    Ok(out)
}

/// The condition a model's count of fields present, `view`, meets for one check.
fn cond(check: &Check, view: &str) -> Result<String, Error> {
    let Check::Properties { min, max } = check else {
        return Err(Error::unsupported(format!("{check:?} on a model")));
    };
    Ok(format!(
        "({}..={}).contains(&{view})",
        min.unwrap_or(0),
        max.unwrap_or(u64::MAX)
    ))
}

/// A map's entries, each value checked as the map's item type, at the map's path plus `.*`.
pub(super) fn map(e: &Emitter, at: &mut At, value: &str, ty: &Ty) -> Result<String, Error> {
    let Base::Map(item) = &ty.base else {
        return Err(Error::invalid("a map check of another type"));
    };
    if let Some(c) = ty.checks.first() {
        return Err(Error::unsupported(format!("{c:?} on a map")));
    }
    let x = at.local("x");
    let outer = at.path.clone();
    at.path.push_str(".*");
    let body = e.value(at, &x, item);
    at.path = outer;
    Ok(format!(
        "for (_, {x}) in {} {{ {}}}\n",
        e.access.entries(value)?,
        body?
    ))
}
