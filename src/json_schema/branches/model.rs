//! A branch over a model: each field the subschema names checked by its subschema, then its other
//! keywords on the model. A key no field names is a [`Check::Entry`] of the model's other keys.

use serde_json::{Map, Value};

use super::{parent, Lower};
use crate::ir::{Base, Check, Ty};
use crate::json_schema::models;
use crate::Error;

/// A key a branch names: where it sits for errors, and what its subschema asks of it.
struct Want<'a> {
    place: String,
    name: &'a str,
    /// The subschema checking the key's value; none where the branch only requires the key.
    schema: Option<&'a Value>,
    required: bool,
}

impl Lower {
    /// A branch over `over`, a model: its nullability is the branch's.
    pub(super) fn branch(
        &mut self,
        key: &str,
        over: &Ty,
        s: &Map<String, Value>,
    ) -> Result<Ty, Error> {
        let Base::Model(m) = over.base else {
            return Err(Error::invalid(format!("`{key}` branches a non-model")));
        };
        refuse_counts(key, s)?;
        let checks = self.member_checks(key, m, s)?;
        let names_others = checks.iter().any(|c| matches!(c, Check::Entry { .. }));
        if names_others && s.contains_key("unevaluatedProperties") {
            return Err(Error::unsupported(format!(
                "`unevaluatedProperties` in a `{key}` naming keys no field names"
            )));
        }
        let mut rest = s.clone();
        for k in ["type", "properties", "required"] {
            rest.remove(k);
        }
        let ty = Ty {
            nullable: over.nullable,
            checks,
            ..Ty::of(Base::Model(m))
        };
        self.keywords(key, &rest, ty)
    }

    /// The check of each key `s` gives a subschema or requires, properties first.
    fn member_checks(
        &mut self,
        key: &str,
        m: usize,
        s: &Map<String, Value>,
    ) -> Result<Vec<Check>, Error> {
        let (required, props) = models::field_lists(key, s)?;
        let mut wants: Vec<Want> = props
            .iter()
            .map(|(name, p)| Want {
                place: format!("{key}.{name}"),
                name,
                schema: Some(p),
                required: required.contains(name),
            })
            .collect();
        let only_required = required.iter().filter(|r| !props.contains_key(r.as_str()));
        wants.extend(only_required.map(|name| Want {
            place: key.into(),
            name,
            schema: None,
            required: true,
        }));
        wants.iter().map(|w| self.member(m, w)).collect()
    }

    /// The check of key `w` of model `m`: its field's, else its other keys'.
    fn member(&mut self, m: usize, w: &Want) -> Result<Check, Error> {
        let model = self
            .out
            .models
            .get(m)
            .ok_or_else(|| Error::invalid(format!("{}: no model {m}", w.place)))?;
        let others = model.others.clone();
        match model.fields.iter().position(|f| f.name == w.name) {
            Some(index) => {
                let fty = model.fields.get(index).map(|f| parent(&f.ty));
                let fty = fty.ok_or_else(|| Error::invalid(format!("{}: no field", w.place)))?;
                self.field(w, index, &fty)
            }
            None => self.entry(w, others.as_ref()),
        }
    }

    /// Field `index`, of type `fty`, checked as the subschema of `w`.
    fn field(&mut self, w: &Want, index: usize, fty: &Ty) -> Result<Check, Error> {
        let p = w.schema.unwrap_or(&Value::Bool(true));
        let ty = self.onto(&w.place, fty, p)?;
        Ok(Check::Field {
            index,
            ty: Box::new(ty),
            required: w.required,
        })
    }

    /// The value under key `w`, which no field names, checked as its subschema, if any. The
    /// model must hold other keys, and a subschema judging their values needs them typed.
    fn entry(&mut self, w: &Want, others: Option<&Ty>) -> Result<Check, Error> {
        let others = others.ok_or_else(|| {
            Error::unsupported(format!(
                "{}: names `{}`, which the object's fields lack",
                w.place, w.name
            ))
        })?;
        let base = parent(others);
        let trivial = match w.schema {
            None | Some(Value::Bool(true)) => true,
            Some(Value::Object(o)) => o.is_empty(),
            _ => false,
        };
        let ty = match w.schema {
            Some(p) if !trivial => self.onto(&w.place, &base, p)?,
            _ => base,
        };
        Ok(Check::Entry {
            name: w.name.into(),
            ty: Box::new(ty),
            required: w.required,
        })
    }
}

/// Refuses the keywords a branch over an object can't judge.
fn refuse_counts(key: &str, s: &Map<String, Value>) -> Result<(), Error> {
    let found = [
        "minProperties",
        "maxProperties",
        "dependentRequired",
        "additionalProperties",
    ]
    .into_iter()
    .find(|k| s.contains_key(*k));
    match found {
        Some(k) => Err(Error::unsupported(format!(
            "`{k}` in a `{key}` over an object"
        ))),
        None => Ok(()),
    }
}
