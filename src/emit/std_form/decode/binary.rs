//! The binary reading of a model, a tuple, and a union: the code for a format that isn't
//! self-describing. A model reads its fields in order, an optional one as an option, then the
//! field of keys no field names if it has one: a list of key and value, else the text of the
//! unread members. A tuple reads an option per position, then the list of the rest. A union
//! reads its branch's index, then the branch's value. The layout is `decode::binary`'s.

use std::fmt::Write as _;

use super::d;
use crate::emit::std_form::{idents, StdForm};
use crate::ir::{Additional, Model, Union};

impl StdForm<'_> {
    /// The `binary` method of the `Kinds` impl reading model `i`.
    pub(super) fn model_positions(&self, i: usize, m: &Model) -> String {
        let names = idents::of(m);
        let (mut reads, mut init) = (String::new(), String::new());
        for (k, (f, n)) in m.fields.iter().zip(&names.fields).enumerate() {
            let seed = self.seed(&f.ty, Some(i));
            let seed = match f.held() {
                true => seed,
                false => optional(&seed),
            };
            let _ = write!(reads, "let f{k} = {}; ", next(&seed));
            let _ = write!(init, "{n}: f{k}, ");
        }
        let kept = match (&m.others, m.additional) {
            (Some(held), _) => Some(format!("{}({})", d("map"), self.seed(held, None))),
            (None, Additional::Keep) => Some(format!("{}()", d("text"))),
            (None, _) => None,
        };
        if let Some(seed) = &kept {
            let _ = write!(reads, "let o = {}; ", next(seed));
            let _ = write!(init, "{}: o, ", names.others);
        }
        let len = m.fields.len().saturating_add(usize::from(kept.is_some()));
        let name = self.model_name(i);
        self.positions(&name, len, &format!("{reads}Ok({name} {{ {init}}})"))
    }

    /// A `binary` method reading `len` positions with `body`, whose last expression is the
    /// `Result` of a `value`.
    pub(super) fn positions(&self, value: &str, len: usize, body: &str) -> String {
        format!(
            "fn binary<__D: {}<'de>>(self, d: __D) -> ::core::result::Result<{value}, __D::Error> {{ \
             struct __P; impl<'de> {}<'de> for __P {{ type Value = {value}; \
             const LEN: usize = {len}; \
             fn read<__A: {}<'de>>(self, mut a: __A) -> ::core::result::Result<{value}, __A::Error> {{ {body} }} }} \
             {}::deserialize({}(__P), d) }}",
            d("Deserializer"),
            d("binary::Positions"),
            d("SeqAccess"),
            d("DeserializeSeed"),
            d("binary::Tuple"),
        )
    }

    /// The statements reading `union`, named `name`, by its branch's index.
    pub(super) fn union_branches(&self, union: &Union, name: &str) -> String {
        let (mut arms, mut names) = (String::new(), String::new());
        for (i, variant) in union.variants.iter().enumerate() {
            let _ = write!(
                arms,
                "{i} => v.newtype_variant_seed({}).map({name}::{}), ",
                self.seed(variant, None),
                self.variant_name(union, i),
            );
            let _ = write!(names, "{:?}, ", self.variant_name(union, i));
        }
        format!(
            "struct __B; impl<'de> {}<'de> for __B {{ type Value = {name}; \
             const NAME: &'static str = {name:?}; \
             const NAMES: &'static [&'static str] = &[{names}]; \
             fn read<__Vt: {}<'de>>(self, i: u32, v: __Vt) -> ::core::result::Result<{name}, __Vt::Error> {{ \
             match i {{ {arms}_ => Err({}()) }} }} }} \
             return {}::deserialize({}(__B), d);",
            d("binary::Branches"),
            d("VariantAccess"),
            d("binary::no_branch"),
            d("DeserializeSeed"),
            d("binary::Choice"),
        )
    }
}

/// The expression reading the next position with `seed`.
pub(super) fn next(seed: &str) -> String {
    format!("{}(&mut a, {seed})?", d("binary::next"))
}

/// The seed of an optional position, an option of `ty`'s.
pub(super) fn optional(seed: &str) -> String {
    format!("{}({seed})", d("nullable"))
}
