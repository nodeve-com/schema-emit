//! Area: lists. Each item checked under the list's path plus `[]`, its index filled at run; the
//! list's view is its item count, which `Items` bounds. A list the embedder can't read fails
//! `cast`. `Unique` walks the items pairwise, comparing each pair by JSON Schema's equality
//! ([`same`]), and fails the list once, as `duplicate`. `Contains` counts the items valid against
//! its scalar subschema ([`valid`]) during the same walk, and bounds the count with the list's
//! other conditions, so a failing list reports once, as `range`.
//!

use super::{guard, readable, scalars, At, Emitter};
use crate::ir::{Base, Check, Ty};
use crate::Error;

pub(super) fn list(
    e: &Emitter,
    at: &mut At,
    value: &str,
    ty: &Ty,
    item: &Ty,
) -> Result<String, Error> {
    let (count, index, each, items) = (at.local("c"), at.local("i"), at.local("x"), at.local("l"));
    let rust = e.access.rust(e.schema, item)?;
    let outer = at.path.clone();
    at.path.push_str("[]");
    at.indices.push(index.clone());
    let body = e.value(at, &each, item);
    at.indices.pop();
    at.path = outer;
    let body = body?;
    let mut checks = Vec::new();
    let mut contains = None;
    for check in &ty.checks {
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
    let mut conds = e.conds(&ty.base, &checks, &count)?;
    let (mut decl, mut step) = (String::new(), String::new());
    if let Some(check) = contains {
        let Check::Contains { ty: sub, .. } = check else {
            return Err(Error::invalid(format!("{check:?} as `contains`")));
        };
        let hits = at.local("h");
        let valid = valid(e, at, &each, sub)?;
        decl = format!("let mut {hits}: u64 = 0;\n");
        step = format!("if {valid} {{ {hits} = {hits}.saturating_add(1); }}\n");
        conds.extend(e.conds(&ty.base, std::slice::from_ref(check), &hits)?);
    }
    let dup = if ty.checks.contains(&Check::Unique) {
        distinct(e, at, value, item, &rust)?
    } else {
        String::new()
    };
    let walk = format!(
        "let mut {count}: u64 = 0;\n{decl}\
         for ({index}, {each}) in {items}.enumerate() {{ {count} = {count}.saturating_add(1); \
         let {each}: &{rust} = ::core::borrow::Borrow::borrow(&{each});\n{body}{step}}}\n{dup}{}",
        guard(at, &conds)
    );
    Ok(readable(at, &e.access.items(value), &items, &walk))
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

/// A boolean expression, true where the item `value`, a `&` of the list's item type, is valid
/// against `ty`, a scalar's type: its width and checks, as conditions on its view. An item that
/// won't read as text is not valid.
fn valid(e: &Emitter, at: &mut At, value: &str, ty: &Ty) -> Result<String, Error> {
    match &ty.base {
        Base::Bool | Base::Int(_) | Base::Float(_) => {
            let view = at.local("n");
            let bind = scalars::view(&ty.base, value, &view);
            let conds = e.conds(&ty.base, &ty.checks, &view)?;
            Ok(format!("{{ {bind} {} }}", all(&conds)))
        }
        Base::Text => {
            let view = at.local("t");
            let conds = e.conds(&ty.base, &ty.checks, &view)?;
            Ok(format!(
                "match {} {{ Ok({view}) => {}, Err(_) => false }}",
                e.access.text(value),
                all(&conds)
            ))
        }
        Base::Null => Err(Error::unsupported("`contains` of null")),
        Base::Timestamp | Base::Duration | Base::Bytes => Err(Error::unsupported(
            "`contains` of a timestamp, a duration, or bytes",
        )),
        Base::Any => Err(Error::unsupported("`contains` of any value")),
        Base::List(_) | Base::Tuple { .. } | Base::Map(_) | Base::Model(_) | Base::Union(_) => Err(
            Error::unsupported("`contains` of an object, a list, or a map"),
        ),
    }
}

/// The conditions, all of them: `true` for none.
fn all(conds: &[String]) -> String {
    if conds.is_empty() {
        "true".to_string()
    } else {
        format!("({})", conds.join(" && "))
    }
}

/// Fails the list once if two of its items are equal: each pair, index `j` after `i`, compared
/// once. Reads the items afresh, since a read yields a new iterator; a read that fails was
/// already reported as `cast`.
fn distinct(e: &Emitter, at: &mut At, value: &str, item: &Ty, rust: &str) -> Result<String, Error> {
    let (found, i, j, p, q) = (
        at.local("d"),
        at.local("i"),
        at.local("j"),
        at.local("p"),
        at.local("q"),
    );
    let eq = same(e, at, &p, &q, item)?;
    Ok(format!(
        "let mut {found} = false;\n\
         if let Ok(outer) = {} {{ for ({i}, {p}) in outer.enumerate() {{ \
         let {p}: &{rust} = ::core::borrow::Borrow::borrow(&{p}); \
         if let Ok(inner) = {} {{ for ({j}, {q}) in inner.enumerate() {{ \
         let {q}: &{rust} = ::core::borrow::Borrow::borrow(&{q}); \
         if {i} < {j} && ({eq}) {{ {found} = true; }} }} }} }} }}\n\
         if {found} {{ {} }}\n",
        e.access.items(value),
        e.access.items(value),
        at.fail("Duplicate")
    ))
}

/// A boolean expression, true where `a` and `b`, each a `&` of `ty`'s Rust type, are equal by
/// JSON Schema: numbers by value, so `1` and `1.0` alike; text by its characters; lists item by
/// item; objects field by field, a field absent in both equal. A map has no access method, so a
/// map under `uniqueItems` is unsupported.
pub(super) fn same(e: &Emitter, at: &mut At, a: &str, b: &str, ty: &Ty) -> Result<String, Error> {
    if ty.nullable {
        let (x, y) = (at.local("o"), at.local("o"));
        let present = same(
            e,
            at,
            &x,
            &y,
            &Ty {
                nullable: false,
                ..ty.clone()
            },
        )?;
        return Ok(format!(
            "match ({a}, {b}) {{ (Some({x}), Some({y})) => ({present}), (None, None) => true, \
             _ => false }}"
        ));
    }
    Ok(match &ty.base {
        Base::Null => "true".to_string(),
        // The embedder's own types, equal as the values they hold are.
        Base::Timestamp | Base::Duration | Base::Bytes => format!("{a} == {b}"),
        Base::Bool | Base::Int(_) | Base::Float(_) => {
            let (x, y) = (at.local("n"), at.local("n"));
            format!(
                "{{ {} {} {x} == {y} }}",
                scalars::view(&ty.base, a, &x),
                scalars::view(&ty.base, b, &y)
            )
        }
        Base::Text => {
            let (x, y) = (at.local("s"), at.local("s"));
            format!(
                "match ({}, {}) {{ (Ok({x}), Ok({y})) => {x} == {y}, _ => false }}",
                e.access.text(a),
                e.access.text(b)
            )
        }
        Base::List(item) => {
            let rust = e.access.rust(e.schema, item)?;
            let (xs, ys, p, q) = (at.local("l"), at.local("l"), at.local("p"), at.local("q"));
            let eq = same(e, at, &p, &q, item)?;
            format!(
                "match ({}, {}) {{ (Ok(mut {xs}), Ok(mut {ys})) => loop {{ \
                 match ({xs}.next(), {ys}.next()) {{ \
                 (None, None) => break true, \
                 (Some({p}), Some({q})) => {{ \
                 let {p}: &{rust} = ::core::borrow::Borrow::borrow(&{p}); \
                 let {q}: &{rust} = ::core::borrow::Borrow::borrow(&{q}); \
                 if !({eq}) {{ break false; }} }} \
                 _ => break false, }} }}, \
                 _ => false }}",
                e.access.items(a),
                e.access.items(b)
            )
        }
        Base::Model(m) => {
            let model = e
                .schema
                .models
                .get(*m)
                .ok_or_else(|| Error::invalid(format!("no model {m}")))?;
            let mut conds = Vec::new();
            for (n, f) in model.fields.iter().enumerate() {
                let (x, y) = (at.local("f"), at.local("f"));
                let eq = same(e, at, &x, &y, &f.ty)?;
                conds.push(format!(
                    "match ({}, {}) {{ (None, None) => true, (Some({x}), Some({y})) => ({eq}), \
                     _ => false }}",
                    e.access.field(a, *m, n),
                    e.access.field(b, *m, n)
                ));
            }
            if conds.is_empty() {
                "true".to_string()
            } else {
                format!("({})", conds.join(" && "))
            }
        }
        Base::Any | Base::Map(_) | Base::Tuple { .. } | Base::Union(_) => {
            return Err(Error::unsupported(
                "`uniqueItems` over a map, a tuple, or any value",
            ))
        }
    })
}
