//! The Rust expressions that read a value of the std types, as the emitted checks use them. Each
//! takes `value`, a Rust expression for a `&` of the type read.

use super::{idents, StdForm};
use crate::ir::Ty;
use crate::Error;

impl StdForm<'_> {
    /// The Rust type of a value of `ty`: an `Option` of its base where it is nullable.
    pub(crate) fn rust(&self, ty: &Ty) -> String {
        self.ty(ty, None)
    }

    /// `Option<&T>`: field `field` of model `model`. A value model's one value is the model's own
    /// type, so it reads as `value`.
    pub(crate) fn field(&self, value: &str, model: usize, field: usize) -> String {
        if self.is_alias(model) {
            return format!("Some({value})");
        }
        let name = self
            .schema
            .models
            .get(self.schema.filling(model))
            .and_then(|m| {
                let f = m.fields.get(field)?;
                Some((idents::of(m).fields.get(field)?.clone(), f.held()))
            });
        match name {
            Some((name, true)) => format!("Some(&{value}.{name})"),
            Some((name, false)) => format!("{value}.{name}.as_ref()"),
            None => "None".into(),
        }
    }

    /// `Option<&T>`: position `index` of the tuple `value`.
    pub(crate) fn element(&self, value: &str, index: usize) -> String {
        format!("{value}.{index}.as_ref()")
    }

    /// `Result<&str, _>`: the text `value` reads as.
    pub(crate) fn text(&self, value: &str) -> String {
        format!("Ok::<&str, ()>({value}.as_str())")
    }

    /// `Result<impl Iterator, _>`: a list's items, each a `&T`.
    pub(crate) fn iter(&self, value: &str) -> String {
        format!("Ok::<_, ()>({value}.iter())")
    }

    /// An iterator of `(&str, &T)`: each key of model `model` no field names, with its value.
    pub(crate) fn others(&self, value: &str, model: usize) -> Result<String, Error> {
        let field = self.others_field(model)?;
        Ok(format!(
            "{value}.{field}.iter().map(|(k, x)| (k.as_str(), x))"
        ))
    }

    /// An iterator of `(&str, &T)`: each entry of the map `value`.
    pub(crate) fn entries(&self, value: &str) -> String {
        format!("{value}.iter().map(|(k, x)| (k.as_str(), x))")
    }

    /// `Option<&T>`: the value of the key no field names that `key`, a `&str`, is.
    pub(crate) fn other(&self, value: &str, model: usize, key: &str) -> Result<String, Error> {
        let field = self.others_field(model)?;
        Ok(format!(
            "{value}.{field}.iter().find(|(k, _)| k.as_str() == {key}).map(|(_, x)| x)"
        ))
    }
}
