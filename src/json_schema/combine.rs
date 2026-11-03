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
mod object;

pub(super) use meet::ty_meet;

use serde_json::{Map, Value};

use super::{lists, Done, Key, Lower};
use crate::ir::{Base, Check, Ty};
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
    let mut meet = Meet {
        name: "allOf",
        ty: k.ty,
        closed,
    };
    Some(k.lower.meet_all(&mut meet, &subs))
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
    let mut meet = Meet {
        name,
        ty: &mut ty,
        closed,
    };
    lower.meet_all(&mut meet, &rest)?;
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

/// A type other subschemas meet into: its name for errors, and whether the schema it came from
/// holds no key beyond its fields.
pub(super) struct Meet<'a> {
    pub name: &'a str,
    pub ty: &'a mut Ty,
    pub closed: bool,
}

impl Meet<'_> {
    /// Meets `false`: nothing is valid, which a list or an object can't say.
    fn deny(&mut self) -> Result<(), Error> {
        if self.ty.base.is_structured() {
            return Err(Error::unsupported(format!(
                "{}: `allOf` of `false` beside a list or an object",
                self.name
            )));
        }
        self.ty.checks.push(Check::OneOf(Vec::new()));
        Ok(())
    }
}

impl Lower {
    /// Meets each of `subs` into `meet`.
    pub(super) fn meet_all(&mut self, meet: &mut Meet, subs: &[&Value]) -> Result<(), Error> {
        for sub in subs {
            match sub {
                Value::Bool(true) => {}
                Value::Bool(false) => meet.deny()?,
                Value::Object(s) => self.meet_object(meet, sub, s)?,
                other => {
                    return Err(Error::invalid(format!(
                        "{}: `allOf` holds {other}",
                        meet.name
                    )))
                }
            }
        }
        Ok(())
    }

    /// One object subschema `sub`, whose keywords are `s`, met into `meet`.
    fn meet_object(
        &mut self,
        meet: &mut Meet,
        sub: &Value,
        s: &Map<String, Value>,
    ) -> Result<(), Error> {
        let sub_closed = is_closed(self, s);
        let object_sub = s.get("type").is_none_or(|t| t.as_str() == Some("object"));
        if matches!(meet.ty.base, Base::Model(_)) && object_sub && !s.contains_key("$ref") {
            self.merge_object(meet, s)?;
        } else if is_typed(sub) {
            let st = self.ty(meet.name, sub)?;
            self.into(meet, st, sub_closed)?;
        } else if let Some(st) = self.untyped_sub(meet, s)? {
            self.into(meet, st, sub_closed)?;
        }
        meet.closed |= sub_closed;
        Ok(())
    }

    /// A subschema with no `type` or `$ref` under a list parent. Where it states items, the list
    /// it makes (`Some`, to meet); otherwise its keywords apply to the parent's type (`None`).
    fn untyped_sub(
        &mut self,
        meet: &mut Meet,
        s: &Map<String, Value>,
    ) -> Result<Option<Ty>, Error> {
        let name = meet.name;
        let items = s.contains_key("items") || s.contains_key("prefixItems");
        if matches!(meet.ty.base, Base::Tuple { .. }) && items {
            return Err(Error::unsupported(format!(
                "{name}: `allOf` of items beside a tuple"
            )));
        }
        if matches!(meet.ty.base, Base::List(_)) && items {
            let base = lists::base(self, name, s)?;
            return Ok(Some(self.keywords(name, s, Ty::of(base))?));
        }
        *meet.ty = self.keywords(name, s, meet.ty.clone())?;
        Ok(None)
    }
}
