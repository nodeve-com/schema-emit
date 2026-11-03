//! An object subschema of an object parent under `allOf`. Its properties merge into the parent's
//! fields by name: a field only one side names keeps its type, and a field both name meets its
//! checks. Its other keywords then apply to the merged object.

use serde_json::{Map, Value};

use super::{is_closed, Meet};
use crate::ir::{Base, Field};
use crate::json_schema::{models, Lower};
use crate::Error;

/// The object subschema being merged.
struct Sub<'a> {
    keywords: &'a Map<String, Value>,
    required: Vec<String>,
    props: Map<String, Value>,
    closed: bool,
}

impl Sub<'_> {
    /// Refuses a merge the closed sides forbid: a closed parent that doesn't name a property, a
    /// closed subschema that doesn't name a field of the parent.
    fn check_closed(&self, name: &str, names: &[String], parent_closed: bool) -> Result<(), Error> {
        if parent_closed {
            if let Some(k) = self.props.keys().find(|k| !names.contains(k)) {
                return Err(Error::unsupported(format!(
                    "{name}: `allOf` adds `{k}`, which a closed object doesn't name"
                )));
            }
        }
        if self.closed {
            if let Some(n) = names.iter().find(|n| !self.props.contains_key(n.as_str())) {
                return Err(Error::unsupported(format!(
                    "{name}: a closed subschema doesn't name `{n}`, which another subschema gives"
                )));
            }
        }
        Ok(())
    }

    /// Refuses a `required` naming no field, and a count on an open object.
    fn check_required(&self, name: &str, names: &[String]) -> Result<(), Error> {
        let known = |r: &str| self.props.contains_key(r) || names.iter().any(|n| n == r);
        if let Some(r) = self.required.iter().find(|r| !known(r)) {
            return Err(Error::unsupported(format!(
                "{name}: `required` names `{r}`, which no field holds"
            )));
        }
        let counted = ["minProperties", "maxProperties", "dependentRequired"]
            .into_iter()
            .any(|k| self.keywords.contains_key(k));
        if !self.closed && counted {
            return Err(Error::unsupported(format!(
                "{name}: a count or `dependentRequired` on an open object in `allOf`"
            )));
        }
        Ok(())
    }
}

impl Lower {
    /// The object subschema `s` merged into `meet`, an object.
    pub(super) fn merge_object(
        &mut self,
        meet: &mut Meet,
        s: &Map<String, Value>,
    ) -> Result<(), Error> {
        let name = meet.name;
        let Base::Model(i) = meet.ty.base else {
            return Err(Error::invalid(format!(
                "{name}: `allOf` object on a non-object"
            )));
        };
        let (required, props) = models::field_lists(name, s)?;
        let sub = Sub {
            keywords: s,
            required,
            props,
            closed: is_closed(self, s),
        };
        let names = self.field_names(name, i)?;
        sub.check_closed(name, &names, meet.closed)?;
        sub.check_required(name, &names)?;
        for (k, p) in &sub.props {
            self.merge_prop(meet, k, p)?;
        }
        if let Some(m) = self.out.models.get_mut(i) {
            for f in &mut m.fields {
                f.required |= sub.required.contains(&f.name);
            }
        }
        *meet.ty = self.keywords(name, s, meet.ty.clone())?;
        Ok(())
    }

    /// The names of model `i`'s fields.
    fn field_names(&self, name: &str, i: usize) -> Result<Vec<String>, Error> {
        let model = self.out.models.get(i);
        let model = model.ok_or_else(|| Error::invalid(format!("{name}: no model {i}")))?;
        Ok(model.fields.iter().map(|f| f.name.clone()).collect())
    }

    /// Adds property `k`, of subschema `p`, as a new field of the object `meet` holds.
    fn add_prop(&mut self, meet: &Meet, k: &str, p: &Value) -> Result<(), Error> {
        let Base::Model(i) = meet.ty.base else {
            return Err(Error::invalid("`allOf` object on a non-object"));
        };
        let ty = self.ty(&format!("{}.{k}", meet.name), p)?;
        if let Some(m) = self.out.models.get_mut(i) {
            m.fields.push(Field {
                name: k.into(),
                ty,
                required: false,
            });
        }
        Ok(())
    }

    /// Property `k`, of subschema `p`, merged into the object `meet` holds: a new field, or
    /// the existing one's type met with `p`.
    fn merge_prop(&mut self, meet: &Meet, k: &str, p: &Value) -> Result<(), Error> {
        let Base::Model(i) = meet.ty.base else {
            return Err(Error::invalid("`allOf` object on a non-object"));
        };
        let field = format!("{}.{k}", meet.name);
        let fields = self.out.models.get(i).map(|m| m.fields.as_slice());
        let at = fields.and_then(|fs| fs.iter().position(|f| f.name == k));
        let Some(at) = at else {
            return self.add_prop(meet, k, p);
        };
        let held = fields.and_then(|fs| fs.get(at)).map(|f| f.ty.clone());
        let mut fty = held.ok_or_else(|| Error::invalid(format!("{field}: no field")))?;
        if matches!(fty.base, Base::Model(_)) {
            return Err(Error::unsupported(format!(
                "{field}: `allOf` gives an object property both subschemas name"
            )));
        }
        let mut inner = Meet {
            name: &field,
            ty: &mut fty,
            closed: false,
        };
        self.meet_all(&mut inner, &[p])?;
        let slot = self
            .out
            .models
            .get_mut(i)
            .and_then(|m| m.fields.get_mut(at));
        if let Some(f) = slot {
            f.ty = fty;
        }
        Ok(())
    }
}
