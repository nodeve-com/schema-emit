//! Area: choices and enums. `OneOf` on a view: its literals as data, one array per check. A
//! front end drops a literal no value of the base can equal, so the array holds the base's kind.

use super::{guard, scalars, At, Emitter};
use crate::ir::{Base, Check, Lit, Ty};
use crate::Error;

/// A list, tuple, or model `value` (a `&` of it) against its choices and its other checks. It
/// fails `range` where it equals no literal of some choice.
pub(super) fn structured(e: &Emitter, at: &mut At, value: &str, ty: &Ty) -> Result<String, Error> {
    let (choices, rest): (Vec<Check>, Vec<Check>) = ty
        .checks
        .iter()
        .cloned()
        .partition(|c| matches!(c, Check::OneOf(_)));
    let mut conds = Vec::new();
    for choice in &choices {
        let Check::OneOf(lits) = choice else {
            continue;
        };
        let mut any = Vec::new();
        for lit in lits.iter().filter(|l| **l != Lit::Null) {
            any.push(eq(
                e,
                at,
                value,
                &Ty {
                    nullable: false,
                    ..ty.clone()
                },
                lit,
            )?);
        }
        if any.is_empty() {
            return Ok(at.fail("Range"));
        }
        conds.push(format!("({})", any.join(" || ")));
    }
    let own = Ty {
        checks: rest,
        ..ty.clone()
    };
    Ok(e.present(at, value, &own)? + &guard(at, &conds))
}

/// A boolean expression, true where `v`, a `&` of `ty`'s Rust (an `Option` where nullable),
/// equals `lit`. Numbers compare by value, text by characters, a list item by item, a tuple and
/// a model slot by slot, where a slot the literal lacks is absent.
fn eq(e: &Emitter, at: &mut At, v: &str, ty: &Ty, lit: &Lit) -> Result<String, Error> {
    if ty.nullable {
        if *lit == Lit::Null {
            return Ok(format!("{v}.is_none()"));
        }
        let y = at.local("y");
        let inner = eq(
            e,
            at,
            &y,
            &Ty {
                nullable: false,
                ..ty.clone()
            },
            lit,
        )?;
        return Ok(format!(
            "match {v} {{ Some({y}) => ({inner}), None => false }}"
        ));
    }
    let bad = || Error::invalid(format!("a choice {lit:?} of {:?}", ty.base));
    Ok(match (&ty.base, lit) {
        (Base::Null, Lit::Null) => "true".into(),
        (Base::Bool | Base::Int(_) | Base::Float(_), _) => {
            let n = at.local("n");
            format!(
                "{{ {} {n} == {} }}",
                scalars::view(&ty.base, v, &n),
                literal(&ty.base, lit).ok_or_else(bad)?
            )
        }
        (Base::Text, Lit::Text(s)) => {
            let t = at.local("s");
            format!(
                "match {} {{ Ok({t}) => {t} == {s:?}, Err(_) => false }}",
                e.access.text(v)
            )
        }
        (Base::List(item), Lit::List(xs)) => {
            let (it, rust) = (at.local("l"), e.access.rust(e.schema, item)?);
            let mut conds = Vec::new();
            for lit in xs {
                let x = at.local("x");
                let body = eq(e, at, &x, item, lit)?;
                conds.push(format!(
                    "(match {it}.next() {{ Some({x}) => {{ \
                     let {x}: &{rust} = ::core::borrow::Borrow::borrow(&{x}); {body} }} \
                     None => false }})"
                ));
            }
            conds.push(format!("{it}.next().is_none()"));
            format!(
                "match {} {{ Ok(mut {it}) => {}, Err(_) => false }}",
                e.access.items(v),
                conds.join(" && ")
            )
        }
        (Base::Tuple { prefix, .. }, Lit::List(xs)) => {
            let mut slots = Vec::new();
            for (i, p) in prefix.iter().enumerate() {
                slots.push((e.access.element(v, i), p, xs.get(i)));
            }
            slots_eq(e, at, slots)?
        }
        (Base::Model(m), Lit::Object(kv)) => {
            let model = e
                .schema
                .models
                .get(*m)
                .ok_or_else(|| Error::invalid(format!("no model {m}")))?;
            let mut slots = Vec::new();
            for (i, f) in model.fields.iter().enumerate() {
                let found = kv.iter().find(|(k, _)| *k == f.name).map(|(_, l)| l);
                slots.push((e.access.field(v, *m, i), &f.ty, found));
            }
            slots_eq(e, at, slots)?
        }
        _ => return Err(bad()),
    })
}

/// Every slot, an `Option` read with the type it holds and the literal it must equal, equal: a
/// slot with no literal is absent.
fn slots_eq(
    e: &Emitter,
    at: &mut At,
    slots: Vec<(String, &Ty, Option<&Lit>)>,
) -> Result<String, Error> {
    let mut conds = Vec::new();
    for (read, ty, lit) in slots {
        conds.push(match lit {
            None => format!("({read}).is_none()"),
            Some(lit) => {
                let x = at.local("f");
                let body = eq(e, at, &x, ty, lit)?;
                format!("(match {read} {{ Some({x}) => ({body}), None => false }})")
            }
        });
    }
    Ok(if conds.is_empty() {
        "true".into()
    } else {
        conds.join(" && ")
    })
}

/// A scalar literal as a Rust expression of `base`'s view.
fn literal(base: &Base, lit: &Lit) -> Option<String> {
    Some(match (base, lit) {
        (Base::Bool, Lit::Bool(b)) => b.to_string(),
        (Base::Int(_), Lit::Int(i)) => format!("{i}i128"),
        (Base::Float(_), Lit::Float(f)) => format!("f64::from_bits({:#x})", f.to_bits()),
        (Base::Float(_), Lit::Int(i)) => format!("f64::from_bits({:#x})", (*i as f64).to_bits()),
        (Base::Text, Lit::Text(s)) => format!("{s:?}"),
        _ => return None,
    })
}

pub(super) fn cond(check: &Check, base: &Base, view: &str) -> Option<Result<String, Error>> {
    let Check::OneOf(lits) = check else {
        return None;
    };
    if base.is_structured() {
        return Some(Err(Error::unsupported(format!("a choice of {base:?}"))));
    }
    let mut items = Vec::new();
    for lit in lits {
        // Null is judged by the null check, never among a present value's literals.
        if *lit == Lit::Null {
            continue;
        }
        items.push(match literal(base, lit) {
            Some(item) => item,
            None => return Some(Err(Error::invalid(format!("a choice {lit:?} of {base:?}")))),
        });
    }
    if items.is_empty() {
        return Some(Ok("false".into()));
    }
    Some(Ok(format!("[{}].contains(&{view})", items.join(", "))))
}
