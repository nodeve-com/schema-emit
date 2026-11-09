//! Each `default` as a function returning it: `default_<name>()` of an alias model whose root
//! has one, and `<Model>::default_<field>()` of a struct's defaulted field. The decode fills an
//! absent or skipped field by calling it, as an embedder fills a value it has none of.

use std::fmt::Write as _;

use super::{idents, StdForm};
use crate::ir::{Base, Float, Lit, Model, Ty};

impl StdForm<'_> {
    /// The functions of each model's defaults.
    pub(super) fn default_fns(&self) -> String {
        let models = self.schema.models.iter().enumerate();
        (models.filter(|(i, _)| (self.external)(*i).is_none()))
            .map(|(i, m)| self.model_defaults(i, m))
            .collect()
    }

    /// The functions of model `i`'s defaults: its root's where it is an alias, else its fields'.
    fn model_defaults(&self, i: usize, m: &Model) -> String {
        let name = self.model_name(i);
        if let (true, Some(f)) = (self.is_alias(i), m.fields.first()) {
            return match &f.ty.default {
                Some(lit) => format!(
                    "#[allow(dead_code)] pub fn {}() -> {name} {{ {} }}\n",
                    self.default_fn(i),
                    literal(&f.ty, lit)
                ),
                None => String::new(),
            };
        }
        let mut fns = String::new();
        for (f, n) in m.fields.iter().zip(&idents::of(m).fields) {
            let Some(lit) = &f.ty.default else { continue };
            let _ = write!(
                fns,
                "pub fn {}() -> {} {{ {} }} ",
                field_fn(n),
                self.ty(&f.ty, Some(i)),
                literal(&f.ty, lit)
            );
        }
        match fns.is_empty() {
            true => fns,
            false => format!("#[allow(dead_code)] impl {name} {{ {fns}}}\n"),
        }
    }

    /// The name of alias model `i`'s default function, as the emitted code declares it:
    /// `default_` and the id its `decode_` takes. It exists where the model's root has a default.
    pub fn default_fn(&self, i: usize) -> String {
        format!("default_{}", self.decoder_id(i))
    }
}

/// The name of the default function of the field named `field` in Rust: `default_` and its
/// identifier, without a raw identifier's `r#`.
pub(super) fn field_fn(field: &str) -> String {
    format!("default_{}", field.trim_start_matches("r#"))
}

/// A default's Rust value, of `ty`'s std type.
fn literal(ty: &Ty, lit: &Lit) -> String {
    match (&ty.base, lit) {
        (Base::Float(Float::F32), Lit::Float(f)) => format!("{:?}f32", *f as f32),
        (_, Lit::Float(f)) => format!("{f:?}f64"),
        (_, Lit::Int(i)) => i.to_string(),
        (_, Lit::Bool(b)) => b.to_string(),
        (_, Lit::Text(s)) => format!("::std::string::String::from({s:?})"),
        _ => "::core::default::Default::default()".into(),
    }
}
