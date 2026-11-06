//! Area: models, nested and as maps. A model's fields check in order, each under the path so far
//! plus `.<field>`; a nested model inlines, so every path is a literal. A model's `Properties`
//! checks count its fields present, after the fields have checked, and fail `range` past them.
//! A `Dependent` check fails `absent` at each field a present field requires and lacks.
//!
//! A map's entries read through `StdForm::entries`, each value checked as the map's item type,
//! at the map's path plus `.*`. Stub: a count or choice on a map. A model that refers to itself
//! is called, not inlined (`recursion.rs`).

use std::fmt::Write as _;

use super::{guard, others, Walk};
use crate::ir::{Base, Check, Model, Ty};
use crate::Error;

/// Nesting deeper than this is a recursive model.
const DEPTH: usize = 32;

/// A field or other key a branch names, with how its value reads.
struct Slot<'c> {
    read: String,
    name: String,
    ty: &'c Ty,
    required: bool,
    nullable: bool,
}

impl<'e> Walk<'_, 'e> {
    /// `value`, a `&` of model `model`, checked against each of `checks`, its fields first.
    pub(super) fn model(
        &mut self,
        value: &str,
        model: usize,
        checks: &[Check],
    ) -> Result<String, Error> {
        if let Some(called) = self.call(value, model, checks) {
            return called;
        }
        self.inline(value, model, checks)
    }

    /// `value`, a `&` of model `model`, its fields checked in place, then each of `checks`.
    pub(super) fn inline(
        &mut self,
        value: &str,
        model: usize,
        checks: &[Check],
    ) -> Result<String, Error> {
        let m = self.get(model)?;
        let mut out = String::new();
        for (i, f) in m.fields.iter().enumerate() {
            out.push_str(&self.within_field(&f.name, |w| w.field(value, model, i))?);
        }
        out.push_str(&self.whole(value, model, checks)?);
        Ok(out)
    }

    /// Model `model`, unless the path here nests too deep for any model but a recursive one.
    fn get(&self, model: usize) -> Result<&'e Model, Error> {
        let m = self.e.model(model)?;
        if self.at.path.matches('.').count() > DEPTH {
            return Err(Error::unsupported(format!("model {} nests itself", m.name)));
        }
        Ok(m)
    }

    /// A branch over model `model`: each field or other key it names ([`Check::Field`],
    /// [`Check::Entry`]) checked as it says, then its other checks. A choice of a present model
    /// holds nothing, as [`Walk::present`] has it.
    pub(super) fn view(&mut self, value: &str, model: usize, branch: &Ty) -> Result<String, Error> {
        self.get(model)?;
        if branch.checks.iter().any(|c| matches!(c, Check::OneOf(_))) {
            return Ok(self.at.fail("Range"));
        }
        let mut out = String::new();
        let mut rest = Vec::new();
        for check in &branch.checks {
            match self.slot(value, model, check)? {
                Some(slot) => out.push_str(&self.slot_check(&slot)?),
                None => rest.push(check.clone()),
            }
        }
        let (branches, rest): (Vec<Check>, Vec<Check>) =
            rest.into_iter().partition(Check::is_branch);
        out.push_str(&self.whole(value, model, &rest)?);
        out.push_str(&self.branches(value, &branches)?);
        Ok(out)
    }

    /// The slot `check` names in model `model`, or `None` where it names none.
    fn slot<'c>(
        &self,
        value: &str,
        model: usize,
        check: &'c Check,
    ) -> Result<Option<Slot<'c>>, Error> {
        let m = self.e.model(model)?;
        Ok(Some(match check {
            Check::Field {
                index,
                ty,
                required,
            } => {
                let f = m
                    .fields
                    .get(*index)
                    .ok_or_else(|| Error::invalid(format!("{}: no field {index}", m.name)))?;
                Slot {
                    read: self.e.std.field(value, model, *index),
                    name: f.name.clone(),
                    ty,
                    required: *required,
                    nullable: f.ty.nullable,
                }
            }
            Check::Entry { name, ty, required } => Slot {
                read: self.e.entry(value, model, name)?,
                name: name.clone(),
                ty,
                required: *required,
                nullable: m.others.as_ref().is_some_and(|o| o.nullable),
            },
            _ => return Ok(None),
        }))
    }

    /// The branch's check of one slot, under the slot's path.
    fn slot_check(&mut self, slot: &Slot) -> Result<String, Error> {
        let x = self.at.local("f");
        let (body, absent) = self.within_field(&slot.name, |w| {
            let body = w.branch_value(&x, slot.ty, slot.nullable);
            let absent = if slot.required {
                w.at.fail("Absent")
            } else {
                String::new()
            };
            (body, absent)
        });
        Ok(format!(
            "match {} {{ Some({x}) => {{ {}}} None => {{ {absent} }} }}\n",
            slot.read, body?
        ))
    }

    /// A model's checks as a whole: each dependency, then its count of keys present.
    pub(super) fn whole(
        &mut self,
        value: &str,
        model: usize,
        checks: &[Check],
    ) -> Result<String, Error> {
        let mut out = String::new();
        for check in checks {
            out.push_str(&self.dependent(value, model, check)?);
        }
        for check in checks {
            if let Check::Unevaluated(names) = check {
                out.push_str(&self.unevaluated(value, model, names)?);
            }
            out.push_str(&self.judge(value, model, check)?);
        }
        let counts: Vec<&Check> = checks
            .iter()
            .filter(|c| {
                !matches!(c, Check::Dependent { .. } | Check::Unevaluated(_)) && !others::judges(c)
            })
            .collect();
        out.push_str(&self.tally(value, model, &counts)?);
        Ok(out)
    }

    /// The count of model `model`'s keys present in `value`, against each count check.
    fn tally(&mut self, value: &str, model: usize, counts: &[&Check]) -> Result<String, Error> {
        if counts.is_empty() {
            return Ok(String::new());
        }
        let (e, m) = (self.e, self.e.model(model)?);
        let count = self.at.local("c");
        let mut tally = String::new();
        for i in 0..m.fields.len() {
            let _ = writeln!(
                tally,
                "if {}.is_some() {{ {count} = {count}.saturating_add(1); }}",
                e.std.field(value, model, i)
            );
        }
        if m.others.is_some() {
            let _ = writeln!(
                tally,
                "{count} = {count}.saturating_add({}.count() as u64);",
                e.std.others(value, model)?
            );
        }
        let conds = counts
            .iter()
            .map(|c| cond(c, &count))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!(
            "{{ let mut {count}: u64 = 0;\n{tally}{}}}\n",
            guard(&self.at, &conds)
        ))
    }

    /// Fails `absent` at each key `then` names when key `when` is present and that key isn't.
    /// A `when` no key holds is never present, so it checks nothing. Another check is silent.
    fn dependent(&self, value: &str, model: usize, check: &Check) -> Result<String, Error> {
        let Check::Dependent { when, then } = check else {
            return Ok(String::new());
        };
        let trigger = self.e.entry(value, model, when)?;
        let mut out = String::new();
        for name in then {
            let _ = writeln!(
                out,
                "if ({trigger}).is_some() && !({}).is_some() {{ {} }}",
                self.e.entry(value, model, name)?,
                self.at.under(name).fail("Absent")
            );
        }
        Ok(out)
    }

    /// A map's entries, each value checked as the map's item type, at the map's path plus `.*`.
    pub(super) fn map(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        let Base::Map(item) = &ty.base else {
            return Err(Error::invalid("a map check of another type"));
        };
        if let Some(c) = ty.checks.first() {
            return Err(Error::unsupported(format!("{c:?} on a map")));
        }
        let x = self.at.local("x");
        let body = self.within(".*", |w| w.value(&x, item));
        Ok(format!(
            "for (_, {x}) in {} {{ {}}}\n",
            self.e.std.entries(value),
            body?
        ))
    }
}

/// The condition a model's count of fields present, `view`, meets for one check.
fn cond(check: &Check, view: &str) -> Result<String, Error> {
    let Check::Properties { min, max } = check else {
        return Err(Error::unsupported(format!("{check:?} on a model")));
    };
    Ok(format!(
        "({}..={}).contains(&{view})",
        min.unwrap_or(0),
        max.unwrap_or(u64::MAX)
    ))
}
