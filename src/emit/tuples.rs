//! Area: tuples. Each position checks under the path plus `[i]`, when the value holds it: a
//! position `prefixItems` makes none for is absent, which fails nothing. A trailing list checks
//! its items as a list does, its tuple element read as the list. A tuple has no checks of its
//! own (a choice holds only null, which `present` checks), so any other check is unsupported.
//! `Unique` on a tuple with no trailing items compares each pair of positions that may be equal
//! ([`equal`]), and fails the tuple once.

use super::{lists, At, Emitter};
use crate::ir::{Base, Check, Ty};
use crate::Error;

pub(super) fn tuple(
    e: &Emitter,
    at: &mut At,
    value: &str,
    ty: &Ty,
    prefix: &[Ty],
    rest: Option<&Ty>,
) -> Result<String, Error> {
    let unique = ty.checks.contains(&Check::Unique);
    if let Some(check) = ty.checks.iter().find(|c| **c != Check::Unique) {
        return Err(Error::unsupported(format!("{check:?} on a tuple")));
    }
    if unique && rest.is_some() {
        return Err(Error::unsupported(
            "`uniqueItems` of a tuple with trailing items",
        ));
    }
    let mut out = String::new();
    for (i, p) in prefix.iter().enumerate() {
        let y = at.local("y");
        let outer = at.path.clone();
        at.path.push_str(&format!("[{i}]"));
        let body = e.value(at, &y, p);
        at.path = outer;
        out.push_str(&format!(
            "if let Some({y}) = {} {{ {}}}\n",
            e.access.element(value, i),
            body?
        ));
    }
    if let Some(item) = rest {
        let list = Ty::of(Base::List(Box::new(item.clone())));
        out.push_str(&lists::list(
            e,
            at,
            &format!("{value}.{}", prefix.len()),
            &list,
            item,
        )?);
    }
    if unique {
        out.push_str(&distinct(e, at, value, prefix)?);
    }
    Ok(out)
}

/// Fails the tuple once if two of its present positions are equal. Each pair, `j` after `i`,
/// compared once; a pair that may not be equal is not compared.
fn distinct(e: &Emitter, at: &mut At, value: &str, prefix: &[Ty]) -> Result<String, Error> {
    let found = at.local("d");
    let mut out = format!("let mut {found} = false;\n");
    for (i, a) in prefix.iter().enumerate() {
        for (j, b) in prefix.iter().enumerate() {
            if j <= i {
                continue;
            }
            let (p, q) = (at.local("p"), at.local("q"));
            let Some(eq) = equal(e, at, &p, &q, a, b)? else {
                continue;
            };
            out.push_str(&format!(
                "if let (Some({p}), Some({q})) = ({}, {}) {{ if {eq} {{ {found} = true; }} }}\n",
                e.access.element(value, i),
                e.access.element(value, j)
            ));
        }
    }
    out.push_str(&format!("if {found} {{ {} }}\n", at.fail("Duplicate")));
    Ok(out)
}

/// Whether positions `p` and `q`, of types `a` and `b`, may hold equal values: the condition
/// for it, or `None` where they never do, as values of two JSON kinds never are. Two numbers of
/// different bases (an integer and a float) are one JSON kind but unequal views, so they are
/// refused, as are nullable and any-valued positions.
fn equal(
    e: &Emitter,
    at: &mut At,
    p: &str,
    q: &str,
    a: &Ty,
    b: &Ty,
) -> Result<Option<String>, Error> {
    if a.nullable || b.nullable {
        return Err(Error::unsupported("`uniqueItems` of a nullable position"));
    }
    let (Some(ka), Some(kb)) = (a.base.json_kind(), b.base.json_kind()) else {
        return Err(Error::unsupported(
            "`uniqueItems` of a position that may hold any value",
        ));
    };
    if ka != kb {
        return Ok(None);
    }
    let same_base = matches!(
        (&a.base, &b.base),
        (Base::Null, Base::Null)
            | (Base::Bool, Base::Bool)
            | (Base::Int(_), Base::Int(_))
            | (Base::Float(_), Base::Float(_))
            | (Base::Text, Base::Text)
    ) || a.base == b.base;
    if !same_base {
        return Err(Error::unsupported(
            "`uniqueItems` of positions of two types that may hold equal values",
        ));
    }
    lists::same(e, at, p, q, a).map(Some)
}
