//! Area: text. `minLength` and `maxLength` in Unicode scalar values, `pattern`. On a value not
//! text they check nothing, as JSON Schema reads them.
//!
//! `pattern` is ECMA-262 by the standard; the emitter builds it through pattern-emit's ECMA guard.

use super::bounds::count;
use super::{Done, Key};
use crate::ir::{Base, Check, Count};

pub(super) fn keyword(k: &mut Key) -> Done {
    let check = match k.key {
        "minLength" | "maxLength" => {
            let Some(n) = count(k.value) else {
                return k.refuse();
            };
            let (min, max) = if k.key == "minLength" {
                (Some(n), None)
            } else {
                (None, Some(n))
            };
            Check::Length {
                min,
                max,
                of: Count::Chars,
            }
        }
        "pattern" => match k.value.as_str() {
            Some(p) => Check::Pattern(p.into()),
            None => return k.refuse(),
        },
        _ => return None,
    };
    if k.ty.base == Base::Text {
        k.ty.checks.push(check);
    }
    Some(Ok(()))
}
