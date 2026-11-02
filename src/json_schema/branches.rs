//! Area: `anyOf`, `oneOf`, and `if` with `then` and `else`. Each subschema lowers onto the value's
//! type as a branch ([`onto`]), so its validity is one condition on the value. `anyOf` holds where
//! some branch does, `oneOf` where exactly one does. `if` holds where `then` does when `if` does,
//! and where `else` does when it doesn't. `then` or `else` without `if` apply nothing, and so
//! does an `if` with neither.
//!
//! A branch takes the value's base. A subschema of another kind holds for no value of it, so it's
//! a choice of nothing, admitting null where the subschema does. A number and an integer overlap:
//! an integer subschema of a number adds a whole-number check. `true` holds of every value and
//! `false` of none. A branch over a model checks the model's own fields: each property the
//! subschema gives and each name it requires is a [`Check::Field`].
//!
//! A `$ref` or `allOf` over a model is the definition's branch, or each subschema's, all holding.
//!
//! A branch over a model naming a key its fields lack reads the model's other keys
//! ([`super::others`]), when it holds any. Stub: a branch over a model holding a count,
//! `dependentRequired`, or `additionalProperties`; `items`, `prefixItems`, or
//! `additionalProperties` in an untyped branch over a list or map; a branch over a value of any
//! kind.

use serde_json::{Map, Value};

use super::combine::ty_meet;
use super::{union, Done, Key, Lower};

mod model;
use crate::ir::{Base, Check, Lit, Ty};
use crate::Error;

pub(super) fn keyword(k: &mut Key) -> Done {
    match k.key {
        "anyOf" => Some(choose(k, Check::AtLeastOne)),
        "oneOf" => Some(choose(k, Check::ExactlyOne)),
        "if" => Some(if_then_else(k)),
        // Held by `if`; alone, they apply nothing.
        "then" | "else" => Some(Ok(())),
        _ => None,
    }
}

fn choose(k: &mut Key, check: fn(Vec<Ty>) -> Check) -> Result<(), Error> {
    let (key, value) = (k.key, k.value);
    let Value::Array(xs) = value else {
        return Err(Error::invalid(format!("`{key}` of {value}")));
    };
    if xs.is_empty() {
        return Err(Error::invalid(format!("`{key}` of no subschema")));
    }
    let parent = parent(k.ty);
    let subs = xs
        .iter()
        .map(|x| onto(&mut *k.lower, key, &parent, x))
        .collect::<Result<Vec<_>, _>>()?;
    k.ty.checks.push(check(subs));
    Ok(())
}

fn if_then_else(k: &mut Key) -> Result<(), Error> {
    let (value, siblings) = (k.value, k.siblings);
    if !siblings.contains_key("then") && !siblings.contains_key("else") {
        return Ok(());
    }
    let parent = parent(k.ty);
    let when = onto(&mut *k.lower, "if", &parent, value)?;
    let then = arm(&mut *k.lower, "then", &parent, siblings.get("then"))?;
    let otherwise = arm(&mut *k.lower, "else", &parent, siblings.get("else"))?;
    k.ty.checks.push(Check::Conditional {
        when: Box::new(when),
        then,
        otherwise,
    });
    Ok(())
}

/// The value's type without its checks: what a branch lowers onto.
pub(super) fn parent(ty: &Ty) -> Ty {
    Ty {
        nullable: ty.nullable,
        ..Ty::of(ty.base.clone())
    }
}

/// An optional branch, lowered if present.
fn arm(
    lower: &mut Lower,
    key: &str,
    parent: &Ty,
    sub: Option<&Value>,
) -> Result<Option<Box<Ty>>, Error> {
    sub.map(|s| onto(lower, key, parent, s).map(Box::new))
        .transpose()
}

/// `sub`, a subschema of a value of `parent`'s type, as a branch: `parent`'s base with `sub`'s
/// checks, nullable where both admit null.
pub(super) fn onto(lower: &mut Lower, key: &str, parent: &Ty, sub: &Value) -> Result<Ty, Error> {
    let base = &parent.base;
    if *base == Base::Any {
        return Err(Error::unsupported(format!(
            "`{key}` over a value of any kind"
        )));
    }
    let s = match sub {
        Value::Bool(true) => return Ok(parent.clone()),
        Value::Bool(false) => return Ok(none(base, false)),
        Value::Object(s) => s,
        other => return Err(Error::invalid(format!("`{key}` holds {other}"))),
    };
    if let Base::Model(m) = base {
        if let Some(nullable) = object(key, s)? {
            let nullable = parent.nullable && nullable;
            if let Some(target) = s.get("$ref") {
                return via_ref(lower, key, parent, target, s);
            }
            if let Some(all) = s.get("allOf") {
                return all_of(lower, key, parent, s, all);
            }
            return model::branch(lower, key, *m, nullable, s);
        }
    }
    if !(s.contains_key("type") || s.contains_key("$ref")) {
        if let Some(k) = ["items", "prefixItems", "additionalProperties"]
            .into_iter()
            .find(|k| base.is_structured() && s.contains_key(*k))
        {
            return Err(Error::unsupported(format!(
                "`{k}` in an untyped `{key}` over a list or map"
            )));
        }
        return lower.keywords(key, s, parent.clone());
    }
    let st = lower.ty(key, sub)?;
    if !overlaps(base, &st.base) {
        return Ok(none(base, st.nullable || st.base == Base::Null));
    }
    let mut ty = parent.clone();
    ty_meet(key, &mut ty, st)?;
    Ok(ty)
}

/// `s`, holding `$ref` `target`, over a model: the definition's branch, then `s`'s other keywords
/// as a branch of their own, both holding.
fn via_ref(
    lower: &mut Lower,
    key: &str,
    parent: &Ty,
    target: &Value,
    s: &Map<String, Value>,
) -> Result<Ty, Error> {
    let t = super::references::definition(lower, key, target)?;
    let mut ty = lower.follow(&t, |l| onto(l, key, parent, &t.schema))?;
    let mut rest = s.clone();
    rest.remove("$ref");
    let st = onto(lower, key, parent, &Value::Object(rest))?;
    and(key, &mut ty, st)?;
    Ok(ty)
}

/// `s`, holding `allOf` `all`, over a model: its other keywords' branch and each subschema's,
/// all of which hold.
fn all_of(
    lower: &mut Lower,
    key: &str,
    parent: &Ty,
    s: &Map<String, Value>,
    all: &Value,
) -> Result<Ty, Error> {
    let Value::Array(subs) = all else {
        return Err(Error::invalid(format!("`allOf` of {all}")));
    };
    if subs.is_empty() {
        return Err(Error::invalid("`allOf` of no subschema"));
    }
    let mut rest = s.clone();
    rest.remove("allOf");
    let mut ty = onto(lower, key, parent, &Value::Object(rest))?;
    for sub in subs {
        let st = onto(lower, key, parent, sub)?;
        and(key, &mut ty, st)?;
    }
    Ok(ty)
}

/// Adds branch `st`'s checks to `ty`, both branches over one model: each holds, so both
/// admit null only where each does.
fn and(key: &str, ty: &mut Ty, st: Ty) -> Result<(), Error> {
    if ty.base != st.base {
        return Err(Error::unsupported(format!(
            "`{key}` gives an object two kinds"
        )));
    }
    ty.nullable &= st.nullable;
    ty.checks.extend(st.checks);
    Ok(())
}

/// A branch holding no value of `base`, and null where `null` says.
fn none(base: &Base, null: bool) -> Ty {
    let lits = if null { vec![Lit::Null] } else { Vec::new() };
    Ty {
        nullable: null,
        checks: vec![Check::OneOf(lits)],
        ..Ty::of(base.clone())
    }
}

/// Whether `s` describes an object: `Some` of whether it admits null where it states no other
/// kind, `None` where it states another.
fn object(key: &str, s: &Map<String, Value>) -> Result<Option<bool>, Error> {
    let (kind, nullable) = match s.get("type") {
        None => return Ok(Some(true)),
        Some(Value::String(t)) => (t.as_str(), false),
        Some(Value::Array(ts)) => union(key, ts)?,
        Some(other) => return Err(Error::unsupported(format!("`{key}`: type {other}"))),
    };
    Ok((kind == "object").then_some(nullable))
}

/// Whether a value of kind `a` can be one of kind `b`: the same kind, or a number and an integer.
fn overlaps(a: &Base, b: &Base) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
        || matches!(
            (a, b),
            (Base::Int(_), Base::Float(_)) | (Base::Float(_), Base::Int(_))
        )
}
