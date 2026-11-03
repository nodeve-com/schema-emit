//! Area: optionality. A field the embedder reads as present checks its value; one absent fails
//! `absent` where required, and is silence where not.

use super::Walk;
use crate::Error;

impl Walk<'_, '_> {
    /// Field `index` of model `model`, read from `value`, checked as its type says.
    pub(super) fn field(
        &mut self,
        value: &str,
        model: usize,
        index: usize,
    ) -> Result<String, Error> {
        let f = self
            .e
            .model(model)?
            .fields
            .get(index)
            .ok_or_else(|| Error::invalid(format!("model {model}: no field {index}")))?;
        let x = self.at.local("f");
        let body = self.value(&x, &f.ty)?;
        let absent = if f.required {
            self.at.fail("Absent")
        } else {
            String::new()
        };
        Ok(format!(
            "match {} {{ Some({x}) => {{ {body}}} None => {{ {absent} }} }}\n",
            self.e.access.field(value, model, index)
        ))
    }
}
