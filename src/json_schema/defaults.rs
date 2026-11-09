//! Area: defaults. `default` on a boolean, a number, or plain text is the value a field of the
//! type takes where its key is absent ([`crate::ir::Ty::default`]). 2020-12 reads it as an
//! annotation; schema-emit fills it in. Once a schema's keywords are lowered, [`judge`] tests the
//! default against the type's checks, and refuses one that fails or one it can't judge.

use serde_json::Value;

use super::{Done, Key};
use crate::ir::{Base, Check, Count, Float, Lit, Num, Ty};
use crate::Error;

pub(super) fn keyword(k: &mut Key) -> Done {
    if k.key != "default" {
        return None;
    }
    Some(lit(k, k.value).map(|l| k.ty.default = Some(l)))
}

/// `value` as a default of `k`'s type.
fn lit(k: &Key, value: &Value) -> Result<Lit, Error> {
    let ty = &*k.ty;
    let unheld = |what: &str| Err(Error::unsupported(format!("a default of {what}")));
    if ty.nullable {
        return unheld("a nullable type");
    }
    if !matches!(
        ty.base,
        Base::Bool | Base::Int(_) | Base::Float(_) | Base::Text
    ) {
        return unheld(&format!("{:?}", ty.base));
    }
    let fits = |l: &Lit| match (&ty.base, l) {
        (Base::Int(w), Lit::Int(i)) => {
            let (lo, hi) = w.bounds();
            (lo..=hi).contains(i)
        }
        (Base::Float(Float::F32), Lit::Float(f)) => f.abs() <= f64::from(f32::MAX),
        _ => true,
    };
    match super::choices::member(&k.lower.out, ty, value)? {
        Some(l) if fits(&l) => Ok(l),
        _ => Err(Error::unsupported(format!(
            "`default` {value}, which its type refuses"
        ))),
    }
}

/// Refuses `ty`'s default where one of its checks fails it, or is one it can't judge it by.
pub(super) fn judge(ty: &Ty) -> Result<(), Error> {
    let Some(lit) = &ty.default else {
        return Ok(());
    };
    for check in &ty.checks {
        if !passes(check, lit)? {
            return Err(Error::unsupported(format!(
                "`default` {lit:?}, which its type's checks fail"
            )));
        }
    }
    Ok(())
}

/// Whether `lit` passes `check`; an error where `check` can't judge it.
fn passes(check: &Check, lit: &Lit) -> Result<bool, Error> {
    Ok(match (check, lit) {
        (Check::OneOf(lits), _) => lits.contains(lit),
        (Check::Min { at, exclusive }, _) => {
            compare(lit, *at).is_some_and(|o| o.is_gt() || (!exclusive && o.is_eq()))
        }
        (Check::Max { at, exclusive }, _) => {
            compare(lit, *at).is_some_and(|o| o.is_lt() || (!exclusive && o.is_eq()))
        }
        (Check::MultipleOf(by), _) => multiple(lit, *by),
        (Check::Whole, Lit::Float(f)) => f.fract() == 0.0,
        (Check::Whole, Lit::Int(_)) => true,
        (Check::Length { min, max, of }, Lit::Text(s)) => {
            let n = match of {
                Count::Chars => s.chars().count(),
                Count::Bytes => s.len(),
            } as u64;
            min.is_none_or(|m| n >= m) && max.is_none_or(|m| n <= m)
        }
        (Check::Pattern(p), Lit::Text(s)) => crate::matching::matches(p, s)?,
        _ => {
            return Err(Error::unsupported(format!(
                "a default beside {check:?}, which can't judge it"
            )))
        }
    })
}

/// A number literal against a bound: exact between integers, else as floats.
fn compare(lit: &Lit, at: Num) -> Option<std::cmp::Ordering> {
    match (lit, at) {
        (Lit::Int(i), Num::Int(b)) => Some(i.cmp(&b)),
        (Lit::Int(i), Num::Float(b)) => (*i as f64).partial_cmp(&b),
        (Lit::Float(f), Num::Int(b)) => f.partial_cmp(&(b as f64)),
        (Lit::Float(f), Num::Float(b)) => f.partial_cmp(&b),
        _ => None,
    }
}

/// Whether a number literal is a whole multiple of `by`.
fn multiple(lit: &Lit, by: Num) -> bool {
    match (lit, by) {
        (Lit::Int(i), Num::Int(b)) => i.checked_rem(b) == Some(0),
        (Lit::Int(i), Num::Float(b)) => ((*i as f64) / b).fract() == 0.0,
        (Lit::Float(f), Num::Int(b)) => (f / b as f64).fract() == 0.0,
        (Lit::Float(f), Num::Float(b)) => (f / b).fract() == 0.0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use serde_json::{json, Map};

    use crate::ir::Lit;
    use crate::json_schema::lower;

    fn default_of(property: serde_json::Value) -> Result<Option<Lit>, crate::Error> {
        let schema = json!({ "type": "object", "properties": { "p": property } });
        let (s, m) = lower("V", &schema, &Map::new(), &[])?;
        Ok(s.models[m].fields[0].ty.default.clone())
    }

    #[test]
    fn a_scalar_default_lowers() {
        let d = |p| default_of(p).unwrap();
        assert_eq!(
            d(json!({ "type": "boolean", "default": false })),
            Some(Lit::Bool(false))
        );
        assert_eq!(
            d(json!({ "type": "integer", "default": 3.0 })),
            Some(Lit::Int(3))
        );
        assert_eq!(
            d(json!({ "type": "number", "default": 2 })),
            Some(Lit::Float(2.0))
        );
        assert_eq!(
            d(json!({ "type": "string", "default": "a", "enum": ["a", "b"] })),
            Some(Lit::Text("a".into()))
        );
        assert_eq!(d(json!({ "type": "boolean" })), None);
    }

    #[test]
    fn a_default_beside_a_ref_lowers() {
        let schema = json!({
            "$defs": { "Flag": { "type": "boolean" } },
            "type": "object",
            "properties": { "p": { "$ref": "#/$defs/Flag", "default": true } }
        });
        let (s, m) = lower("V", &schema, &Map::new(), &[]).unwrap();
        assert_eq!(s.models[m].fields[0].ty.default, Some(Lit::Bool(true)));
    }

    #[test]
    fn a_default_its_type_refuses_is_refused() {
        for p in [
            json!({ "type": "integer", "default": 1.5 }),
            json!({ "type": "integer", "format": "int8", "default": 300 }),
            json!({ "type": "boolean", "default": "no" }),
            json!({ "type": "string", "minLength": 4, "default": "bad" }),
            json!({ "type": "number", "maximum": 1, "default": 2 }),
            json!({ "type": "string", "pattern": "^a", "default": "b" }),
            json!({ "type": "string", "enum": ["a"], "default": "b" }),
            json!({ "type": ["string", "null"], "default": "a" }),
            json!({ "type": "array", "items": { "type": "string" }, "default": [] }),
        ] {
            assert!(default_of(p.clone()).is_err(), "{p}");
        }
    }

    #[test]
    fn allof_defaults_that_disagree_are_refused() {
        let p = json!({ "allOf": [
            { "type": "boolean", "default": true },
            { "type": "boolean", "default": false }
        ] });
        assert!(default_of(p).is_err());
    }
}
