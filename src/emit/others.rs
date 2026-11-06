//! Area: keys no field names. A model holding [`Model::others`] is read through
//! `StdForm::others` and `StdForm::other`: every other key and its value, or the value under
//! one key. A key is a field's name, else an other key; a name neither holds is never present.
//!
//! A key's value fails at the model's path plus `.*`, since a path is a literal. `Present` fails
//! `absent` at the key's own path. A rule on a key's name, `Matching` and `Unmatched` by pattern
//! and `Names` by its text checks, reads the key as a `&str`.

use std::fmt::Write as _;

use super::{guard, Emitter, Walk};
use crate::ir::{Base, Check, Ty};
use crate::Error;

impl Emitter<'_> {
    /// `Option<&T>`: the value of key `name` of model `model`, read from `value`.
    pub(super) fn entry(&self, value: &str, model: usize, name: &str) -> Result<String, Error> {
        let m = self.model(model)?;
        if let Some(i) = m.fields.iter().position(|f| f.name == name) {
            return Ok(self.std.field(value, model, i));
        }
        match m.others {
            Some(_) => self.std.other(value, model, &format!("{name:?}")),
            None => Ok("None::<&()>".into()),
        }
    }
}

impl Walk<'_, '_> {
    /// The failures of one check on a model's keys, or nothing where it isn't one.
    pub(super) fn judge(
        &mut self,
        value: &str,
        model: usize,
        check: &Check,
    ) -> Result<String, Error> {
        let e = self.e;
        match check {
            Check::Present(names) => self.keys_present(value, model, names),
            Check::Matching { pattern, ty } => {
                let entries = e.std.others(value, model)?;
                self.each(&entries, |k| pattern_cond(e, pattern, k), ty)
            }
            Check::Unmatched { patterns, ty } => {
                let entries = e.std.others(value, model)?;
                self.each(&entries, |k| unmatched_cond(e, patterns, k), ty)
            }
            Check::Names(ty) => self.key_names(value, model, ty),
            _ => Ok(String::new()),
        }
    }

    /// Each other key, from the iterator `entries`, where `found` holds of the key's name, its
    /// value checked as `ty`.
    fn each(
        &mut self,
        entries: &str,
        found: impl FnOnce(&str) -> Result<String, Error>,
        ty: &Ty,
    ) -> Result<String, Error> {
        let k = self.at.local("k");
        let found = found(&k)?;
        let x = self.at.local("x");
        let body = self.within(".*", |w| w.value(&x, ty))?;
        Ok(format!(
            "for ({k}, {x}) in {entries} {{ if {found} {{ {body}}} }}\n"
        ))
    }

    /// Each key named is present.
    fn keys_present(&self, value: &str, model: usize, names: &[String]) -> Result<String, Error> {
        let mut out = String::new();
        for name in names {
            let _ = writeln!(
                out,
                "if !({}).is_some() {{ {} }}",
                self.e.entry(value, model, name)?,
                self.at.under(name).fail("Absent")
            );
        }
        Ok(out)
    }

    /// Each key present, a field's or another, fails `range` unless its name meets `ty`.
    fn key_names(&mut self, value: &str, model: usize, ty: &Ty) -> Result<String, Error> {
        let (e, m) = (self.e, self.e.model(model)?);
        let mut out = String::new();
        let k = self.at.local("k");
        let conds = e.conds(&Base::Text, &ty.checks, &k)?;
        for (i, f) in m.fields.iter().enumerate() {
            let _ = writeln!(
                out,
                "if ({}).is_some() {{ let {k}: &str = {:?};\n{}}}",
                e.std.field(value, model, i),
                f.name,
                guard(&self.at, &conds)
            );
        }
        if m.others.is_some() {
            let _ = writeln!(
                out,
                "for ({k}, _) in {} {{\n{}}}",
                e.std.others(value, model)?,
                guard(&self.at, &conds)
            );
        }
        Ok(out)
    }
}

/// Whether `check` is one [`Walk::judge`] writes.
pub(super) fn judges(check: &Check) -> bool {
    matches!(
        check,
        Check::Present(_) | Check::Matching { .. } | Check::Unmatched { .. } | Check::Names(_)
    )
}

/// Whether the pattern finds itself in the key `k`, a `&str`.
fn pattern_cond(e: &Emitter, pattern: &str, k: &str) -> Result<String, Error> {
    super::text::cond(e, &Check::Pattern(pattern.into()), &Base::Text, k)
        .unwrap_or_else(|| Err(Error::invalid("a pattern check")))
}

/// Whether the key `k` matches none of `patterns`.
fn unmatched_cond(e: &Emitter, patterns: &[String], k: &str) -> Result<String, Error> {
    let found = patterns
        .iter()
        .map(|p| pattern_cond(e, p, k))
        .collect::<Result<Vec<_>, _>>()?;
    let any = if found.is_empty() {
        "false".into()
    } else {
        found.join(" || ")
    };
    Ok(format!("!({any})"))
}
