//! A `$ref` followed from inside its own target: the model standing for the target, filled once
//! the target lowers.

use serde_json::{Map, Value};

use super::Target;
use crate::ir::{Base, Field, Model, Ty};
use crate::json_schema::Lower;
use crate::Error;

/// Keywords that name or note a schema and check nothing beside a `$ref`.
const NOTES: [&str; 8] = [
    "$ref",
    "$defs",
    "$id",
    "$anchor",
    "$schema",
    "$comment",
    "title",
    "description",
];

/// Refuses a keyword beside a recursive `$ref` that checks something, which the wrapper can't hold.
pub(super) fn only_notes(name: &str, s: &Map<String, Value>) -> Result<(), Error> {
    match s.keys().find(|k| !NOTES.contains(&k.as_str())) {
        Some(k) => Err(Error::unsupported(format!(
            "{name}: `{k}` beside a recursive `$ref`"
        ))),
        None => Ok(()),
    }
}

impl Lower {
    /// The recursion's model for `t`, made once while `t` is being followed.
    pub(in crate::json_schema) fn recursion(
        &mut self,
        name: &str,
        t: &Target,
        s: &Map<String, Value>,
    ) -> Result<Ty, Error> {
        only_notes(name, s)?;
        let model = match self.recursions.get(&t.key) {
            Some(m) => *m,
            None => {
                let m = self.reserve(name);
                self.recursions.insert(t.key.clone(), m);
                m
            }
        };
        Ok(Ty::of(Base::Model(model)))
    }

    /// The recursion's model `model` filled with `ty`, the target it stands for, as the one field
    /// of a model named `name`; the type referring to it.
    pub(in crate::json_schema) fn wrap(&mut self, name: &str, ty: Ty, model: usize) -> Ty {
        let field = Field {
            name: String::new(),
            ty,
            required: true,
        };
        let wrapped = Model {
            name: name.into(),
            fields: vec![field],
            ..Model::default()
        };
        self.fill(model, wrapped);
        Ty::of(Base::Model(model))
    }
}
