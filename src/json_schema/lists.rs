//! Area: lists. An `array` of `items`, one schema for every item; `minItems`, `maxItems`,
//! `uniqueItems`, and `contains` with `minContains` and `maxContains`.
//!
//! An object item may hold keys no field names, and equality can't see those, so `uniqueItems`
//! refuses items that may hold one: an object not closed (`additionalProperties: false`), or
//! with such a field.
//!
//! `contains` lowers its subschema, which must be a scalar of the items' kind: a valid item is
//! then a condition on its view. An object, list, or map subschema, or one of another kind than
//! `items`, is unsupported rather than walked or guessed. `minContains` and `maxContains` count
//! beside `contains`, and mean nothing without it.
//!
//! An array with `prefixItems` lowers as a list or a tuple ([`super::tuples`]). Without it, an
//! array with no `items` is unsupported.

use serde_json::{Map, Value};

use super::bounds::count;
use super::{Done, Key, Lower};
use crate::ir::{Base, Check, Ty};
use crate::Error;

pub(super) fn base(lower: &mut Lower, name: &str, s: &Map<String, Value>) -> Result<Base, Error> {
    if s.contains_key("prefixItems") {
        return super::tuples::base(lower, name, s);
    }
    let items = s
        .get("items")
        .ok_or_else(|| Error::unsupported(format!("{name}: an array without `items`")))?;
    if s.get("uniqueItems") == Some(&Value::Bool(true)) && super::models::open(items) {
        return Err(Error::unsupported(format!(
            "{name}: `uniqueItems` over an object that may hold keys no field names"
        )));
    }
    Ok(Base::List(Box::new(lower.ty(&format!("{name}[]"), items)?)))
}

/// `contains` on a list: at least `minContains` (default 1) and at most `maxContains` items
/// valid against it. Other types take no `contains`, so it's nothing there.
fn contains(k: &mut Key) -> Done {
    let Base::List(item) = &k.ty.base else {
        return Some(Ok(()));
    };
    let nullable = item.nullable;
    let item = item.base.clone();
    let sub = match contains_schema(k, &item) {
        Ok(sub) => sub,
        Err(e) => return Some(Err(e)),
    };
    if let Err(e) = check_contains(&sub, nullable, &item) {
        return Some(Err(e));
    }
    let (min, max) = match contains_bounds(k.siblings) {
        Ok(bounds) => bounds,
        Err(e) => return Some(Err(e)),
    };
    if min == 0 && max.is_none() {
        return Some(Ok(()));
    }
    k.ty.checks.push(Check::Contains {
        ty: Box::new(sub),
        min,
        max,
    });
    Some(Ok(()))
}

/// Refuses a `contains` subschema `sub` that isn't a plain scalar of the items' kind: `nullable`
/// says whether the items are nullable, `item` is their base.
fn check_contains(sub: &Ty, nullable: bool, item: &Base) -> Result<(), Error> {
    if nullable || sub.nullable {
        return Err(Error::unsupported("`contains` of a nullable item"));
    }
    if sub.base.is_structured() {
        return Err(Error::unsupported(
            "`contains` of an object, a list, or a map",
        ));
    }
    if !same_kind(&sub.base, item) {
        return Err(Error::unsupported(
            "`contains` of another type than `items`",
        ));
    }
    Ok(())
}

/// `minContains` (default 1) and `maxContains` of the list's siblings.
fn contains_bounds(siblings: &Map<String, Value>) -> Result<(u64, Option<u64>), Error> {
    let bound = |key: &str| {
        siblings
            .get(key)
            .map(|v| count(v).ok_or_else(|| Error::invalid(format!("`{key}` of {v}"))))
            .transpose()
    };
    Ok((bound("minContains")?.unwrap_or(1), bound("maxContains")?))
}

/// The subschema of `contains`, lowered. A subschema with no `type` applies to items of the
/// items' own type, so it takes that type; `true` and `false` are every item and none. A width
/// (`format`) names a type, so it can't come without one.
fn contains_schema(k: &mut Key, item: &Base) -> Result<Ty, Error> {
    match k.value {
        Value::Bool(true) => Ok(Ty::of(item.clone())),
        Value::Bool(false) => {
            let mut none = Ty::of(item.clone());
            none.checks.push(Check::OneOf(Vec::new()));
            Ok(none)
        }
        Value::Object(s) if !s.contains_key("type") && !s.contains_key("$ref") => {
            if s.contains_key("format") {
                return Err(Error::unsupported("`contains` with `format` and no `type`"));
            }
            k.lower.keywords("contains", s, Ty::of(item.clone()))
        }
        _ => k.lower.ty("contains", k.value),
    }
}

/// Whether two bases are scalars of one kind: a bool, an integer (of any width), a float (of
/// any width), or text.
fn same_kind(a: &Base, b: &Base) -> bool {
    matches!(
        (a, b),
        (Base::Bool, Base::Bool) | (Base::Int(_), Base::Int(_)) | (Base::Float(_), Base::Float(_))
    ) || a.same_text(b)
}

/// A tuple's keywords: `prefixItems` and `items` are its shape. Its counts and `contains` are
/// unsupported, not guessed. `uniqueItems` of a tuple with no trailing items is a check on its
/// positions, which the emitter compares pairwise.
fn tuple_keyword(k: &mut Key) -> Done {
    match k.key {
        "prefixItems" | "items" | "minContains" | "maxContains" => Some(Ok(())),
        "uniqueItems" if *k.value == Value::Bool(false) => Some(Ok(())),
        "uniqueItems"
            if *k.value == Value::Bool(true)
                && matches!(k.ty.base, Base::Tuple { rest: None, .. }) =>
        {
            k.ty.checks.push(Check::Unique);
            Some(Ok(()))
        }
        "uniqueItems" => Some(Err(Error::unsupported(
            "`uniqueItems` of a tuple with trailing items",
        ))),
        "minItems" | "maxItems" | "contains" => {
            Some(Err(Error::unsupported(format!("`{}` of a tuple", k.key))))
        }
        _ => None,
    }
}

pub(super) fn keyword(k: &mut Key) -> Done {
    if matches!(k.ty.base, Base::Tuple { .. }) {
        return tuple_keyword(k);
    }
    let list = matches!(k.ty.base, Base::List(_));
    let (min, max) = match k.key {
        // On another type, `prefixItems` applies to nothing, as JSON Schema says.
        "items" | "prefixItems" => return Some(Ok(())),
        "contains" => return contains(k),
        // `contains` reads these beside it.
        "minContains" | "maxContains" => return Some(Ok(())),
        "uniqueItems" => {
            return match k.value {
                Value::Bool(unique) => {
                    if list && *unique {
                        k.ty.checks.push(Check::Unique);
                    }
                    Some(Ok(()))
                }
                _ => k.refuse(),
            };
        }
        "minItems" => (count(k.value), None),
        "maxItems" => (None, count(k.value)),
        _ => return None,
    };
    if min.is_none() && max.is_none() {
        return k.refuse();
    }
    if list {
        k.ty.checks.push(Check::Items { min, max });
    }
    Some(Ok(()))
}
