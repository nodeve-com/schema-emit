//! Area: unions. A schema with no `type` whose `anyOf` holds two or more branches lowers to a
//! [`Base::Union`] of their types, models or any other: a value is the first branch it fits, and
//! the check is that branch's. Another schema of no type is refused as `combine` does.
//!
//! Stub: `oneOf`, which a typed value can't judge, since it holds one branch and the others
//! can't read it; any keyword beside `anyOf` that checks.

use serde_json::{Map, Value};

use super::{combine, notes, Lower};
use crate::ir::{Base, Ty, Union};
use crate::Error;

/// `s`, with no `type` and no `$ref`, as a union where it holds `anyOf` and no other checking
/// keyword.
pub(super) fn untyped(lower: &mut Lower, name: &str, s: &Map<String, Value>) -> Result<Ty, Error> {
    let only = |key: &str| notes::only_annotations(s, key);
    match (s.get("anyOf"), s.get("oneOf")) {
        (Some(Value::Array(xs)), None) if xs.len() > 1 && only("anyOf") => {
            let mut variants = Vec::new();
            for (i, branch) in xs.iter().enumerate() {
                variants.push(lower.ty(&format!("{name}.{i}"), branch)?);
            }
            Ok(Ty::of(Base::Union(Union {
                name: None,
                variants,
            })))
        }
        (None, Some(Value::Array(xs))) if xs.len() > 1 && only("oneOf") => {
            Err(Error::unsupported(format!(
                "{name}: `oneOf` over branches with no type: a typed value holds one branch, \
                 and no other can judge it"
            )))
        }
        _ => combine::untyped(lower, name, s),
    }
}
