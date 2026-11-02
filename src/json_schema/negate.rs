//! Area: negation. `not` holds where its subschema doesn't. On a value of the parent's kind, the
//! subschema lowers to a type, and the check is that the value fails some check of it. A
//! subschema of another kind holds for no such value, so `not` checks nothing there. A subschema
//! stating no type holds for every kind, so its keywords lower onto the parent's own.
//!
//! Stub: `not` beside an object, list, or map is unsupported, as is `not` of a boolean schema
//! there. An integer subschema beside a number adds its whole-number check.

use serde_json::Value;

use super::combine::ty_meet;
use super::{Done, Key};
use crate::ir::Json;
use crate::ir::{Base, Check, Ty};
use crate::Error;

pub(super) fn keyword(k: &mut Key) -> Done {
    if k.key != "not" {
        return None;
    }
    Some(negate(k))
}

fn negate(k: &mut Key) -> Result<(), Error> {
    let value: &Value = k.value;
    let parent = k.ty.base.clone();
    let sub = match value {
        Value::Object(s) if !s.contains_key("type") && !s.contains_key("$ref") => {
            if !scalar(&parent) {
                return Err(Error::unsupported(
                    "`not` of a schema with no type, beside an object, list, or map",
                ));
            }
            if s.contains_key("format") {
                return Err(Error::unsupported("`not` with `format` and no `type`"));
            }
            k.lower.keywords("not", s, Ty::of(parent.clone()))?
        }
        _ => k.lower.ty("not", value)?,
    };
    if let Some(inner) = on(&parent, sub)? {
        k.ty.checks.push(Check::Not(Box::new(inner)));
    }
    Ok(())
}

fn scalar(base: &Base) -> bool {
    matches!(base, Base::Bool | Base::Int(_) | Base::Float(_))
        || base.json_kind() == Some(Json::Text)
}

/// The subschema's type as it applies to a value of `parent`'s kind: `None` where it holds for
/// no value of that kind.
fn on(parent: &Base, sub: Ty) -> Result<Option<Ty>, Error> {
    if !scalar(parent) {
        return match (parent, &sub.base) {
            (Base::Any, _) | (_, Base::Any) => Err(Error::unsupported(
                "`not` of a boolean schema beside an object, list, or map",
            )),
            (Base::List(_) | Base::Tuple { .. }, Base::List(_) | Base::Tuple { .. })
            | (Base::Map(_), Base::Map(_))
            | (Base::Model(_), Base::Model(_)) => {
                Err(Error::unsupported("`not` of an object, list, or map"))
            }
            _ => Ok(None),
        };
    }
    if matches!((parent, &sub.base), (Base::Float(_), Base::Int(_))) {
        let mut ty = Ty::of(parent.clone());
        ty_meet("not", &mut ty, sub)?;
        return Ok(Some(ty));
    }
    // Every integer is a number, so a number subschema's checks apply to an integer's view.
    if matches!((parent, &sub.base), (Base::Int(_), Base::Float(_))) {
        return Ok(Some(Ty {
            base: parent.clone(),
            ..sub
        }));
    }
    let fits = matches!(
        (parent, &sub.base),
        (_, Base::Any)
            | (Base::Bool, Base::Bool)
            | (Base::Int(_), Base::Int(_))
            | (Base::Float(_), Base::Float(_))
    );
    Ok((fits || parent.same_text(&sub.base)).then_some(sub))
}
