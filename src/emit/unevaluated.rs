//! Area: unevaluated properties. A model's field the scope doesn't evaluate fails `range` when
//! present, at its own path, and so does each of the model's other keys the scope doesn't name.

use std::fmt::Write as _;

use super::Walk;
use crate::Error;

impl Walk<'_, '_> {
    /// Each field and other key of model `model`, read from `value`, that `evaluated` omits.
    pub(super) fn unevaluated(
        &self,
        value: &str,
        model: usize,
        evaluated: &[String],
    ) -> Result<String, Error> {
        let (m, e) = (self.e.model(model)?, self.e);
        let mut out = String::new();
        for (i, f) in m.fields.iter().enumerate() {
            if evaluated.contains(&f.name) {
                continue;
            }
            let _ = writeln!(
                out,
                "if ({}).is_some() {{ {} }}",
                e.access.field(value, model, i),
                self.at.under(&f.name).fail("Range")
            );
        }
        if m.others.is_some() {
            let list: Vec<String> = evaluated.iter().map(|n| format!("{n:?}")).collect();
            let _ = writeln!(
                out,
                "for (k, _) in {} {{ if ![{}].contains(&k) {{ {} }} }}",
                e.access.others(value, model)?,
                list.join(", "),
                self.at.under("*").fail("Range")
            );
        }
        Ok(out)
    }
}
