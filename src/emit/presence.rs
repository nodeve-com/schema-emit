//! Area: optionality. A field the embedder reads as present checks its value; one absent fails
//! `absent` where required, and is silence where not.

use super::{At, Emitter};
use crate::ir::Field;
use crate::Error;

pub(super) fn field(
    e: &Emitter,
    at: &mut At,
    value: &str,
    model: usize,
    index: usize,
    f: &Field,
) -> Result<String, Error> {
    let x = at.local("f");
    let body = e.value(at, &x, &f.ty)?;
    let absent = if f.required {
        at.fail("Absent")
    } else {
        String::new()
    };
    Ok(format!(
        "match {} {{ Some({x}) => {{ {body}}} None => {{ {absent} }} }}\n",
        e.access.field(value, model, index)
    ))
}
