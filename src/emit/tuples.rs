//! Area: tuples. Each position checks under the path plus `[i]`, when the value holds it: a
//! position `prefixItems` makes none for is absent, which fails nothing. A trailing list checks
//! its items as a list does, its tuple element read as the list. A tuple has no checks of its
//! own (a choice holds only null, which `present` checks), so any other check is unsupported.
//! `Unique` on a tuple with no trailing items compares each pair of positions that may be equal
//! ([`comparable`]), and fails the tuple once.

use super::Walk;
use crate::ir::{Base, Check, Ty};
use crate::Error;

impl Walk<'_, '_> {
    /// `value`, a `&` of a tuple, each position checked when the value holds it.
    pub(super) fn tuple(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        let Base::Tuple { prefix, rest } = &ty.base else {
            return Err(Error::invalid("a tuple check of another type"));
        };
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
            let y = self.at.local("y");
            let body = self.within(&format!("[{i}]"), |w| w.value(&y, p));
            out.push_str(&format!(
                "if let Some({y}) = {} {{ {}}}\n",
                self.e.access.element(value, i),
                body?
            ));
        }
        if let Some(item) = rest.as_deref() {
            let list = Ty::of(Base::List(Box::new(item.clone())));
            let items = format!("{value}.{}", prefix.len());
            out.push_str(&self.list(&items, &list, item)?);
        }
        if unique {
            out.push_str(&self.distinct_positions(value, prefix)?);
        }
        Ok(out)
    }

    /// Fails the tuple once if two of its present positions are equal. Each pair, `j` after
    /// `i`, compared once; a pair that may not be equal is not compared.
    fn distinct_positions(&mut self, value: &str, prefix: &[Ty]) -> Result<String, Error> {
        let found = self.at.local("d");
        let mut out = format!("let mut {found} = false;\n");
        for (i, a, j, b) in position_pairs(prefix) {
            if !comparable(a, b)? {
                continue;
            }
            let (p, q) = (self.at.local("p"), self.at.local("q"));
            let eq = self.same(&p, &q, a)?;
            out.push_str(&format!(
                "if let (Some({p}), Some({q})) = ({}, {}) {{ if {eq} {{ {found} = true; }} }}\n",
                self.e.access.element(value, i),
                self.e.access.element(value, j)
            ));
        }
        out.push_str(&format!("if {found} {{ {} }}\n", self.at.fail("Duplicate")));
        Ok(out)
    }
}

/// Each pair of positions, `j` after `i`, with their types.
fn position_pairs(prefix: &[Ty]) -> impl Iterator<Item = (usize, &Ty, usize, &Ty)> {
    prefix.iter().enumerate().flat_map(|(i, a)| {
        let later = prefix.iter().enumerate().skip(i.saturating_add(1));
        later.map(move |(j, b)| (i, a, j, b))
    })
}

/// Whether positions of types `a` and `b` may hold equal values: `false` where they never do, as
/// values of two JSON kinds never are. Two numbers of different bases (an integer and a float)
/// are one JSON kind but unequal views, so they are refused, as are nullable and any-valued
/// positions.
fn comparable(a: &Ty, b: &Ty) -> Result<bool, Error> {
    if a.nullable || b.nullable {
        return Err(Error::unsupported("`uniqueItems` of a nullable position"));
    }
    let (Some(ka), Some(kb)) = (a.base.json_kind(), b.base.json_kind()) else {
        return Err(Error::unsupported(
            "`uniqueItems` of a position that may hold any value",
        ));
    };
    if ka != kb {
        return Ok(false);
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
    Ok(true)
}
