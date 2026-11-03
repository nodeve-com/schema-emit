//! Area: choices and enums. `OneOf` on a view: its literals as data, one array per check. A
//! front end drops a literal no value of the base can equal, so the array holds the base's kind.

use super::{guard, scalars, Walk};
use crate::ir::{Base, Check, Lit, Ty};
use crate::Error;

impl Walk<'_, '_> {
    /// A list, tuple, or model `value` (a `&` of it) against its choices and its other checks.
    /// It fails `range` where it equals no literal of some choice.
    pub(super) fn choices(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        let (choices, rest): (Vec<Check>, Vec<Check>) = ty
            .checks
            .iter()
            .cloned()
            .partition(|c| matches!(c, Check::OneOf(_)));
        let plain = Ty {
            nullable: false,
            ..ty.clone()
        };
        let mut conds = Vec::new();
        for choice in &choices {
            let Check::OneOf(lits) = choice else {
                continue;
            };
            let mut any = Vec::new();
            for lit in lits.iter().filter(|l| **l != Lit::Null) {
                any.push(self.eq(value, &plain, lit)?);
            }
            if any.is_empty() {
                return Ok(self.at.fail("Range"));
            }
            conds.push(format!("({})", any.join(" || ")));
        }
        let own = Ty {
            checks: rest,
            ..ty.clone()
        };
        Ok(self.present(value, &own)? + &guard(&self.at, &conds))
    }

    /// A boolean expression, true where `v`, a `&` of `ty`'s Rust (an `Option` where nullable),
    /// equals `lit`. Numbers compare by value, text by characters, a list item by item, a tuple
    /// and a model slot by slot, where a slot the literal lacks is absent.
    fn eq(&mut self, v: &str, ty: &Ty, lit: &Lit) -> Result<String, Error> {
        if ty.nullable {
            return self.eq_nullable(v, ty, lit);
        }
        let e = self.e;
        let bad = || Error::invalid(format!("a choice {lit:?} of {:?}", ty.base));
        Ok(match (&ty.base, lit) {
            (Base::Null, Lit::Null) => "true".into(),
            (Base::Bool | Base::Int(_) | Base::Float(_), _) => {
                let n = self.at.local("n");
                format!(
                    "{{ {} {n} == {} }}",
                    scalars::view(&ty.base, v, &n),
                    literal(&ty.base, lit).ok_or_else(bad)?
                )
            }
            (Base::Text, Lit::Text(s)) => {
                let t = self.at.local("s");
                format!(
                    "match {} {{ Ok({t}) => {t} == {s:?}, Err(_) => false }}",
                    e.access.text(v)
                )
            }
            (Base::List(item), Lit::List(xs)) => self.eq_items(v, item, xs)?,
            (Base::Tuple { .. } | Base::Model(_), _) => self.eq_slots(v, ty, lit)?,
            _ => return Err(bad()),
        })
    }

    /// [`Walk::eq`] of an `Option` `v`: null is none, and any other literal needs a value.
    fn eq_nullable(&mut self, v: &str, ty: &Ty, lit: &Lit) -> Result<String, Error> {
        if *lit == Lit::Null {
            return Ok(format!("{v}.is_none()"));
        }
        let y = self.at.local("y");
        let plain = Ty {
            nullable: false,
            ..ty.clone()
        };
        let inner = self.eq(&y, &plain, lit)?;
        Ok(format!(
            "match {v} {{ Some({y}) => ({inner}), None => false }}"
        ))
    }

    /// `v`, a `&` of a tuple or model, equal to `lit`, a list or an object: slot by slot.
    fn eq_slots(&mut self, v: &str, ty: &Ty, lit: &Lit) -> Result<String, Error> {
        let e = self.e;
        let mut slots = Vec::new();
        match (&ty.base, lit) {
            (Base::Tuple { prefix, .. }, Lit::List(xs)) => {
                for (i, p) in prefix.iter().enumerate() {
                    slots.push((e.access.element(v, i), p, xs.get(i)));
                }
            }
            (Base::Model(m), Lit::Object(kv)) => {
                for (i, f) in e.model(*m)?.fields.iter().enumerate() {
                    let found = kv.iter().find(|(k, _)| *k == f.name).map(|(_, l)| l);
                    slots.push((e.access.field(v, *m, i), &f.ty, found));
                }
            }
            _ => return Err(Error::invalid(format!("a choice {lit:?} of {:?}", ty.base))),
        }
        self.slots_eq(slots)
    }

    /// `v`, a `&` of a list of `item`, equal to the list literal `xs` item by item.
    fn eq_items(&mut self, v: &str, item: &Ty, xs: &[Lit]) -> Result<String, Error> {
        let (it, rust) = (self.at.local("l"), self.e.access.rust(self.e.schema, item)?);
        let mut conds = Vec::new();
        for lit in xs {
            let x = self.at.local("x");
            let body = self.eq(&x, item, lit)?;
            conds.push(format!(
                "(match {it}.next() {{ Some({x}) => {{ \
                 let {x}: &{rust} = ::core::borrow::Borrow::borrow(&{x}); {body} }} \
                 None => false }})"
            ));
        }
        conds.push(format!("{it}.next().is_none()"));
        Ok(format!(
            "match {} {{ Ok(mut {it}) => {}, Err(_) => false }}",
            self.e.access.items(v),
            conds.join(" && ")
        ))
    }

    /// Every slot, an `Option` read with the type it holds and the literal it must equal, equal:
    /// a slot with no literal is absent.
    fn slots_eq(&mut self, slots: Vec<(String, &Ty, Option<&Lit>)>) -> Result<String, Error> {
        let mut conds = Vec::new();
        for (read, ty, lit) in slots {
            conds.push(match lit {
                None => format!("({read}).is_none()"),
                Some(lit) => {
                    let x = self.at.local("f");
                    let body = self.eq(&x, ty, lit)?;
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
