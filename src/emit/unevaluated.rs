//! Area: unevaluated properties. A model's field the scope doesn't evaluate fails `range` when
//! present, at its own path, and so does each of the model's other keys the scope doesn't name.

use std::fmt::Write as _;

use super::{At, Emitter};
use crate::ir::Model;
use crate::Error;

pub(super) fn fields(
    e: &Emitter,
    at: &At,
    value: &str,
    m: &Model,
    model: usize,
    evaluated: &[String],
) -> Result<String, Error> {
    let mut out = String::new();
    for (i, f) in m.fields.iter().enumerate() {
        if evaluated.contains(&f.name) {
            continue;
        }
        let sub = at.under(&f.name);
        let _ = writeln!(
            out,
            "if ({}).is_some() {{ {} }}",
            e.access.field(value, model, i),
            sub.fail("Range")
        );
    }
    if m.others.is_some() {
        let list = evaluated
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>();
        let sub = at.under("*");
        let _ = writeln!(
            out,
            "for (k, _) in {} {{ if ![{}].contains(&k) {{ {} }} }}",
            e.access.others(value, model)?,
            list.join(", "),
            sub.fail("Range")
        );
    }
    Ok(out)
}
