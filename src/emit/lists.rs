//! Area: lists. Each item checked under the list's path plus `[]`, its index filled at run; the
//! list's view is its item count, which `Items` bounds. A list the embedder can't read fails
//! `cast`. `Unique` walks the items pairwise, comparing each pair by JSON Schema's equality
//! ([`same`]), and fails the list once, as `duplicate`. `Contains` counts the items valid against
//! its scalar subschema ([`valid`]) during the same walk, and bounds the count with the list's
//! other conditions, so a failing list reports once, as `range`.
//!

use super::{guard, readable, scalars, Walk};
use crate::ir::{Base, Check, Ty};
use crate::Error;

impl Walk<'_, '_> {
    /// `value`, a `&` of a list of `item`, each item checked, then the list's own checks.
    pub(super) fn list(&mut self, value: &str, ty: &Ty, item: &Ty) -> Result<String, Error> {
        let e = self.e;
        let at = &mut self.at;
        let (count, index, each, items) =
            (at.local("c"), at.local("i"), at.local("x"), at.local("l"));
        let rust = e.std.rust(item);
        let body = self.within("[]", |w| {
            w.at.indices.push(index.clone());
            let body = w.value(&each, item);
            w.at.indices.pop();
            body
        })?;
        let (checks, contains) = split_checks(&ty.checks)?;
        let mut conds = e.conds(&ty.base, &checks, &count)?;
        let (mut decl, mut step) = (String::new(), String::new());
        if let Some(check) = contains {
            let hits = self.at.local("h");
            (decl, step) = self.contained(check, &each, &hits)?;
            conds.extend(e.conds(&ty.base, std::slice::from_ref(check), &hits)?);
        }
        let dup = if ty.checks.contains(&Check::Unique) {
            self.distinct(value, item, &rust)?
        } else {
            String::new()
        };
        let walk = format!(
            "let mut {count}: u64 = 0;\n{decl}\
             for ({index}, {each}) in {items}.enumerate() {{ {count} = {count}.saturating_add(1); \
             let {each}: &{rust} = ::core::borrow::Borrow::borrow(&{each});\n{body}{step}}}\n{dup}{}",
            guard(&self.at, &conds)
        );
        Ok(readable(&self.at, &e.std.iter(value), &items, &walk))
    }

    /// The counter declaration and the per-item step of `check`, a `contains`: `hits` counts the
    /// items `each` valid against its subschema.
    fn contained(
        &mut self,
        check: &Check,
        each: &str,
        hits: &str,
    ) -> Result<(String, String), Error> {
        let Check::Contains { ty: sub, .. } = check else {
            return Err(Error::invalid(format!("{check:?} as `contains`")));
        };
        let valid = self.valid(each, sub)?;
        Ok((
            format!("let mut {hits}: u64 = 0;\n"),
            format!("if {valid} {{ {hits} = {hits}.saturating_add(1); }}\n"),
        ))
    }
}

/// The checks the list's walk bounds by its count, and its one `contains`.
fn split_checks(all: &[Check]) -> Result<(Vec<Check>, Option<&Check>), Error> {
    let mut checks = Vec::new();
    let mut contains = None;
    for check in all {
        match check {
            Check::Unique => {}
            Check::Contains { .. } => {
                if contains.replace(check).is_some() {
                    return Err(Error::invalid("two `contains` on one list"));
                }
            }
            _ => checks.push(check.clone()),
        }
    }
    Ok((checks, contains))
}

pub(super) fn cond(check: &Check, base: &Base, view: &str) -> Option<Result<String, Error>> {
    let (min, max) = match check {
        Check::Items { min, max } => (*min, *max),
        Check::Contains { min, max, .. } => (Some(*min), *max),
        _ => return None,
    };
    if !matches!(base, Base::List(_)) {
        return Some(Err(Error::invalid(format!("{check:?} on {base:?}"))));
    }
    let mut conds = vec!["true".to_string()];
    if let Some(min) = min.filter(|m| *m > 0) {
        conds.push(format!("{view} >= {min}"));
    }
    if let Some(max) = max {
        conds.push(format!("{view} <= {max}"));
    }
    Some(Ok(format!("({})", conds.join(" && "))))
}

/// The conditions, all of them: `true` for none.
fn all(conds: &[String]) -> String {
    if conds.is_empty() {
        "true".to_string()
    } else {
        format!("({})", conds.join(" && "))
    }
}

impl Walk<'_, '_> {
    /// A boolean expression, true where the item `value`, a `&` of the list's item type, is valid
    /// against `ty`, a scalar's type: its width and checks, as conditions on its view. An item
    /// that won't read as text is not valid.
    fn valid(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        let e = self.e;
        match &ty.base {
            Base::Bool | Base::Int(_) | Base::Float(_) => {
                let view = self.at.local("n");
                let bind = scalars::view(&ty.base, value, &view);
                let conds = e.conds(&ty.base, &ty.checks, &view)?;
                Ok(format!("{{ {bind} {} }}", all(&conds)))
            }
            Base::Text => {
                let view = self.at.local("t");
                let conds = e.conds(&ty.base, &ty.checks, &view)?;
                Ok(format!(
                    "match {} {{ Ok({view}) => {}, Err(_) => false }}",
                    e.std.text(value),
                    all(&conds)
                ))
            }
            Base::Null => Err(Error::unsupported("`contains` of null")),
            Base::Timestamp | Base::Duration | Base::Bytes => Err(Error::unsupported(
                "`contains` of a timestamp, a duration, or bytes",
            )),
            Base::Any | Base::Slot(_) => {
                Err(Error::unsupported("`contains` of any value or a slot"))
            }
            Base::List(_) | Base::Tuple { .. } | Base::Map(_) | Base::Model(_) | Base::Union(_) => {
                Err(Error::unsupported(
                    "`contains` of an object, a list, or a map",
                ))
            }
        }
    }

    /// Fails the list once if two of its items are equal: each pair, index `j` after `i`,
    /// compared once. Reads the items afresh, since a read yields a new iterator; a read that
    /// fails was already reported as `cast`.
    fn distinct(&mut self, value: &str, item: &Ty, rust: &str) -> Result<String, Error> {
        let at = &mut self.at;
        let (found, i, j, p, q) = (
            at.local("d"),
            at.local("i"),
            at.local("j"),
            at.local("p"),
            at.local("q"),
        );
        let eq = self.same(&p, &q, item)?;
        let items = self.e.std.iter(value);
        Ok(format!(
            "let mut {found} = false;\n\
             if let Ok(outer) = {items} {{ for ({i}, {p}) in outer.enumerate() {{ \
             let {p}: &{rust} = ::core::borrow::Borrow::borrow(&{p}); \
             if let Ok(inner) = {items} {{ for ({j}, {q}) in inner.enumerate() {{ \
             let {q}: &{rust} = ::core::borrow::Borrow::borrow(&{q}); \
             if {i} < {j} && ({eq}) {{ {found} = true; }} }} }} }} }}\n\
             if {found} {{ {} }}\n",
            self.at.fail("Duplicate")
        ))
    }

    /// A boolean expression, true where `a` and `b`, each a `&` of `ty`'s Rust type, are equal by
    /// JSON Schema: numbers by value, so `1` and `1.0` alike; text by its characters; lists item
    /// by item; objects field by field, a field absent in both equal. A map has no access
    /// method, so a map under `uniqueItems` is unsupported.
    pub(super) fn same(&mut self, a: &str, b: &str, ty: &Ty) -> Result<String, Error> {
        if ty.nullable {
            return self.same_nullable(a, b, ty);
        }
        let e = self.e;
        Ok(match &ty.base {
            Base::Null => "true".to_string(),
            // The embedder's own types, equal as the values they hold are.
            Base::Timestamp | Base::Duration | Base::Bytes => format!("{a} == {b}"),
            Base::Bool | Base::Int(_) | Base::Float(_) => {
                let (x, y) = (self.at.local("n"), self.at.local("n"));
                format!(
                    "{{ {} {} {x} == {y} }}",
                    scalars::view(&ty.base, a, &x),
                    scalars::view(&ty.base, b, &y)
                )
            }
            Base::Text => {
                let (x, y) = (self.at.local("s"), self.at.local("s"));
                format!(
                    "match ({}, {}) {{ (Ok({x}), Ok({y})) => {x} == {y}, _ => false }}",
                    e.std.text(a),
                    e.std.text(b)
                )
            }
            Base::List(item) => self.same_items(a, b, item)?,
            Base::Model(m) => self.same_fields(a, b, *m)?,
            Base::Any | Base::Map(_) | Base::Tuple { .. } | Base::Union(_) | Base::Slot(_) => {
                return Err(Error::unsupported(
                    "`uniqueItems` over a map, a tuple, a slot, or any value",
                ))
            }
        })
    }

    /// `a` and `b`, each an `Option` of `ty`'s Rust, both absent or both present and equal.
    fn same_nullable(&mut self, a: &str, b: &str, ty: &Ty) -> Result<String, Error> {
        let (x, y) = (self.at.local("o"), self.at.local("o"));
        let inner = Ty {
            nullable: false,
            ..ty.clone()
        };
        let present = self.same(&x, &y, &inner)?;
        Ok(format!(
            "match ({a}, {b}) {{ (Some({x}), Some({y})) => ({present}), (None, None) => true, \
             _ => false }}"
        ))
    }

    /// `a` and `b`, each a `&` of a list of `item`, equal item by item.
    fn same_items(&mut self, a: &str, b: &str, item: &Ty) -> Result<String, Error> {
        let rust = self.e.std.rust(item);
        let at = &mut self.at;
        let (xs, ys, p, q) = (at.local("l"), at.local("l"), at.local("p"), at.local("q"));
        let eq = self.same(&p, &q, item)?;
        Ok(format!(
            "match ({}, {}) {{ (Ok(mut {xs}), Ok(mut {ys})) => loop {{ \
             match ({xs}.next(), {ys}.next()) {{ \
             (None, None) => break true, \
             (Some({p}), Some({q})) => {{ \
             let {p}: &{rust} = ::core::borrow::Borrow::borrow(&{p}); \
             let {q}: &{rust} = ::core::borrow::Borrow::borrow(&{q}); \
             if !({eq}) {{ break false; }} }} \
             _ => break false, }} }}, \
             _ => false }}",
            self.e.std.iter(a),
            self.e.std.iter(b)
        ))
    }

    /// `a` and `b`, each a `&` of model `model`, equal field by field.
    fn same_fields(&mut self, a: &str, b: &str, model: usize) -> Result<String, Error> {
        let m = self.e.model(model)?;
        let mut conds = Vec::new();
        for (n, f) in m.fields.iter().enumerate() {
            let (x, y) = (self.at.local("f"), self.at.local("f"));
            let eq = self.same(&x, &y, &f.ty)?;
            conds.push(format!(
                "match ({}, {}) {{ (None, None) => true, (Some({x}), Some({y})) => ({eq}), \
                 _ => false }}",
                self.e.std.field(a, model, n),
                self.e.std.field(b, model, n)
            ));
        }
        if conds.is_empty() {
            return Ok("true".to_string());
        }
        Ok(format!("({})", conds.join(" && ")))
    }
}
