//! Area: combinators. `allOf`: a value is valid when valid against every subschema, so each
//! subschema lowers onto the one value's type and its checks accumulate. A subschema states the
//! parent's type or none; a different type, a field typed two ways, or integers of widths no one
//! width holds is refused. An object subschema merges its properties into the parent's fields by
//! name, a closed object (`additionalProperties: false`) naming every field the merge holds.
//!
//! `true` adds nothing; `false` makes nothing valid, as an empty choice does on a scalar. A parent
//! with no `type` takes its base from the first subschema that states one, its own keywords and
//! the other subschemas meeting it.
//!
//! Stub: a field both objects name as objects is refused; so is `false` beside a list or object,
//! and an open object's count or `dependentRequired`. A `$ref` to a definition that is itself a
//! `$ref` counts closed.

mod kinds;
mod meet;

pub(super) use meet::ty_meet;

use serde_json::{Map, Value};

use super::{lists, models, Done, Key, Lower};
use crate::ir::{Base, Check, Field, Ty};
use crate::Error;

pub(super) fn keyword(k: &mut Key) -> Done {
    if k.key != "allOf" {
        return None;
    }
    let subs = match subschemas(k.value) {
        Ok(subs) => subs,
        Err(e) => return Some(Err(e)),
    };
    let closed = is_closed(k.lower, k.siblings);
    Some(meet_all(k.lower, "allOf", k.ty, closed, &subs))
}

/// `s`, with no `type` and no `$ref`: its base is the first typed `allOf` subschema's, and the
/// other subschemas and `s`'s own keywords meet it. With none typed, the one kind its `enum`s and
/// `const`s share is its `type`.
pub(super) fn untyped(lower: &mut Lower, name: &str, s: &Map<String, Value>) -> Result<Ty, Error> {
    if super::notes::only_annotations(s, "") {
        return Ok(super::boolean::ty(true));
    }
    let mut rest = match s.get("allOf") {
        Some(all) => subschemas(all)?,
        None => Vec::new(),
    };
    let Some(pos) = rest.iter().position(|sub| is_typed(sub)) else {
        let Some(kind) = kinds::of(name, s, &rest)? else {
            return Err(Error::unsupported(format!(
                "{name}: no type, and no subschema of `allOf` states one"
            )));
        };
        let mut typed = s.clone();
        typed.insert("type".into(), kind.type_value());
        return lower.ty(name, &Value::Object(typed));
    };
    let head = rest.remove(pos);
    let mut own = s.clone();
    own.remove("allOf");
    let own = Value::Object(own);
    rest.push(&own);
    let mut ty = lower.ty(name, head)?;
    if head.get("$ref").is_some() {
        ty = own_model(lower, name, ty);
    }
    let closed = closed_value(lower, head);
    meet_all(lower, name, &mut ty, closed, &rest)?;
    Ok(ty)
}

/// `ty`, a `$ref`'s, its model a copy named `name` where it has one: the rest merge into it,
/// never into the model the `$ref` shares.
fn own_model(lower: &mut Lower, name: &str, mut ty: Ty) -> Ty {
    let Base::Model(m) = ty.base else {
        return ty;
    };
    if let Some(mut copy) = lower.out.models.get(m).cloned() {
        copy.name = name.into();
        ty.base = Base::Model(lower.model(copy));
    }
    ty
}

/// The subschemas `v` holds: a non-empty array.
fn subschemas(v: &Value) -> Result<Vec<&Value>, Error> {
    match v {
        Value::Array(xs) if !xs.is_empty() => Ok(xs.iter().collect()),
        _ => Err(Error::invalid(format!("`allOf` of {v}"))),
    }
}

/// Whether a subschema states its own type, so it lowers as a type of its own.
fn is_typed(sub: &Value) -> bool {
    matches!(sub, Value::Object(m) if m.contains_key("type") || m.contains_key("$ref"))
}

/// Whether `s` holds no key beyond its fields. A `$ref` counts its definition's closedness,
/// one level down.
fn is_closed(lower: &Lower, s: &Map<String, Value>) -> bool {
    let closes =
        |m: &Map<String, Value>| m.get("additionalProperties") == Some(&Value::Bool(false));
    if closes(s) {
        return true;
    }
    match s.get("$ref") {
        None => false,
        Some(target) => match target
            .as_str()
            .and_then(|t| t.strip_prefix("#/$defs/"))
            .and_then(|d| lower.defs.get(d))
        {
            Some(Value::Object(def)) => closes(def) || def.contains_key("$ref"),
            _ => true,
        },
    }
}

fn closed_value(lower: &Lower, v: &Value) -> bool {
    matches!(v, Value::Object(s) if is_closed(lower, s))
}

/// Meets each of `subs` into `ty`. `closed` says whether the schema `ty` came from holds no key
/// beyond its fields.
fn meet_all(
    lower: &mut Lower,
    name: &str,
    ty: &mut Ty,
    mut closed: bool,
    subs: &[&Value],
) -> Result<(), Error> {
    for sub in subs {
        match sub {
            Value::Bool(true) => {}
            Value::Bool(false) => {
                if ty.base.is_structured() {
                    return Err(Error::unsupported(format!(
                        "{name}: `allOf` of `false` beside a list or an object"
                    )));
                }
                ty.checks.push(Check::OneOf(Vec::new()));
            }
            Value::Object(s) => {
                let sub_closed = is_closed(lower, s);
                let object_sub = s.get("type").is_none_or(|t| t.as_str() == Some("object"));
                if matches!(ty.base, Base::Model(_)) && object_sub && !s.contains_key("$ref") {
                    merge_object(lower, name, ty, s, closed, sub_closed)?;
                } else if is_typed(sub) {
                    let st = lower.ty(name, sub)?;
                    meet::into(lower, name, ty, st, closed, sub_closed)?;
                } else if let Some(st) = untyped_sub(lower, name, ty, s)? {
                    meet::into(lower, name, ty, st, closed, sub_closed)?;
                }
                closed |= sub_closed;
            }
            other => return Err(Error::invalid(format!("{name}: `allOf` holds {other}"))),
        }
    }
    Ok(())
}

/// A subschema with no `type` or `$ref` under a list parent. Where it states items, the list
/// it makes (`Some`, to meet); otherwise its keywords apply to the parent's type (`None`).
fn untyped_sub(
    lower: &mut Lower,
    name: &str,
    ty: &mut Ty,
    s: &Map<String, Value>,
) -> Result<Option<Ty>, Error> {
    let items = s.contains_key("items") || s.contains_key("prefixItems");
    if matches!(ty.base, Base::Tuple { .. }) && items {
        return Err(Error::unsupported(format!(
            "{name}: `allOf` of items beside a tuple"
        )));
    }
    if matches!(ty.base, Base::List(_)) && items {
        let base = lists::base(lower, name, s)?;
        return Ok(Some(lower.keywords(name, s, Ty::of(base))?));
    }
    *ty = lower.keywords(name, s, ty.clone())?;
    Ok(None)
}

/// An object subschema of an object parent. Its properties merge into the parent's fields by
/// name: a field only one side names keeps its type, and a field both name meets its checks.
/// Its other keywords then apply to the merged object.
fn merge_object(
    lower: &mut Lower,
    name: &str,
    ty: &mut Ty,
    s: &Map<String, Value>,
    closed: bool,
    sub_closed: bool,
) -> Result<(), Error> {
    let Base::Model(i) = ty.base else {
        return Err(Error::invalid(format!(
            "{name}: `allOf` object on a non-object"
        )));
    };
    let (required, props) = models::field_lists(name, s)?;
    let names: Vec<String> = lower
        .out
        .models
        .get(i)
        .ok_or_else(|| Error::invalid(format!("{name}: no model {i}")))?
        .fields
        .iter()
        .map(|f| f.name.clone())
        .collect();
    let has = |n: &str| names.iter().any(|f| f.as_str() == n);
    if closed {
        if let Some(k) = props.keys().find(|k| !has(k)) {
            return Err(Error::unsupported(format!(
                "{name}: `allOf` adds `{k}`, which a closed object doesn't name"
            )));
        }
    }
    if sub_closed {
        if let Some(n) = names.iter().find(|n| !props.contains_key(n.as_str())) {
            return Err(Error::unsupported(format!(
                "{name}: a closed subschema doesn't name `{n}`, which another subschema gives"
            )));
        }
    }
    if let Some(r) = required
        .iter()
        .find(|r| !props.contains_key(r.as_str()) && !has(r.as_str()))
    {
        return Err(Error::unsupported(format!(
            "{name}: `required` names `{r}`, which no field holds"
        )));
    }
    if !sub_closed
        && (s.contains_key("minProperties")
            || s.contains_key("maxProperties")
            || s.contains_key("dependentRequired"))
    {
        return Err(Error::unsupported(format!(
            "{name}: a count or `dependentRequired` on an open object in `allOf`"
        )));
    }
    for (k, p) in &props {
        let field = format!("{name}.{k}");
        match names.iter().position(|n| n == k) {
            Some(at) => {
                let mut fty = lower
                    .out
                    .models
                    .get(i)
                    .and_then(|m| m.fields.get(at))
                    .map(|f| f.ty.clone())
                    .ok_or_else(|| Error::invalid(format!("{field}: no field")))?;
                if matches!(fty.base, Base::Model(_)) {
                    return Err(Error::unsupported(format!(
                        "{field}: `allOf` gives an object property both subschemas name"
                    )));
                }
                meet_all(lower, &field, &mut fty, false, &[p])?;
                if let Some(f) = lower
                    .out
                    .models
                    .get_mut(i)
                    .and_then(|m| m.fields.get_mut(at))
                {
                    f.ty = fty;
                }
            }
            None => {
                let fty = lower.ty(&field, p)?;
                if let Some(m) = lower.out.models.get_mut(i) {
                    m.fields.push(Field {
                        name: k.clone(),
                        ty: fty,
                        required: false,
                    });
                }
            }
        }
    }
    if let Some(m) = lower.out.models.get_mut(i) {
        for f in &mut m.fields {
            f.required |= required.contains(&f.name);
        }
    }
    *ty = lower.keywords(name, s, ty.clone())?;
    Ok(())
}
