//! Area: ranges and bounds. `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`,
//! `multipleOf`. On a value not a number they check nothing, as JSON Schema reads them.

use serde_json::Value;

use super::{Done, Key};
use crate::ir::{Base, Check, Num};

pub(super) fn keyword(k: &mut Key) -> Done {
    let make: fn(Num) -> Check = match k.key {
        "minimum" => |at| Check::Min {
            at,
            exclusive: false,
        },
        "maximum" => |at| Check::Max {
            at,
            exclusive: false,
        },
        "exclusiveMinimum" => |at| Check::Min {
            at,
            exclusive: true,
        },
        "exclusiveMaximum" => |at| Check::Max {
            at,
            exclusive: true,
        },
        "multipleOf" => Check::MultipleOf,
        _ => return None,
    };
    let Some(n) = num(k.value) else {
        return k.refuse();
    };
    if matches!(k.ty.base, Base::Int(_) | Base::Float(_)) {
        k.ty.checks.push(make(n));
    }
    Some(Ok(()))
}

/// A count a schema states: a non-negative integer, written whole or as a decimal with no
/// fraction (`2.0`).
pub(super) fn count(value: &Value) -> Option<u64> {
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    let f = value.as_f64()?;
    // `u64::MAX` as f64 is 2^64, so the bound admits every u64 an f64 can name.
    ((0.0..18_446_744_073_709_551_616.0).contains(&f) && f.fract() == 0.0).then_some(f as u64)
}

/// A JSON number as the IR's.
pub(super) fn num(value: &Value) -> Option<Num> {
    let n = value.as_number()?;
    Some(match (n.as_i64(), n.as_u64()) {
        (Some(i), _) => Num::Int(i.into()),
        (_, Some(u)) => Num::Int(u.into()),
        _ => Num::Float(n.as_f64()?),
    })
}
