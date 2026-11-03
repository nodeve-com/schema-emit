//! The seed of a type: the expression that reads a value of it from a deserializer, composed from
//! the runtime's seeds. A tuple is a seed of its own, a block holding a type that reads its array.

use super::binary::{next, optional};
use std::fmt::Write as _;

use super::d;
use crate::emit::std_form::{rust_int, tuple_of, StdForm};
use crate::ir::{Base, Float, Ty};

/// Where a tuple's seed reads from.
#[derive(Clone, Copy)]
enum Place {
    /// A prefix position.
    Item,
    /// The items past the prefix.
    Rest,
}

impl StdForm<'_> {
    /// The seed of `ty`, a field of model `owner` where it is one, which boxes a model that holds
    /// `owner`.
    pub(super) fn seed(&self, ty: &Ty, owner: Option<usize>) -> String {
        let base = self.seed_base(ty, owner);
        match ty.nullable {
            true => format!("{}({base})", d("nullable")),
            false => base,
        }
    }

    /// [`StdForm::seed`] of a type that isn't nullable.
    fn seed_base(&self, ty: &Ty, owner: Option<usize>) -> String {
        let call = |name: &str| format!("{}()", d(name));
        match &ty.base {
            Base::Null => call("null"),
            Base::Any => call("any"),
            Base::Bool => call("boolean"),
            Base::Int(w) => format!("{}::<{}>()", d("int"), rust_int(*w)),
            Base::Float(Float::F32) => format!("{}::<f32>()", d("float")),
            Base::Float(Float::F64) => format!("{}::<f64>()", d("float")),
            Base::Text => call("text"),
            Base::Timestamp => call("timestamp"),
            Base::Duration => call("duration"),
            Base::Bytes => call("bytes"),
            Base::List(item) => format!("{}({})", d("list"), self.seed(item, None)),
            Base::Map(item) => format!("{}({})", d("map"), self.seed(item, None)),
            Base::Tuple { .. } => self.tuple_seed(ty, owner),
            Base::Union(u) => {
                let of = format!("{}::<{}>()", d("of"), self.union_name(u));
                self.boxed(of, owner.is_some_and(|o| self.boxes(u, o)))
            }
            Base::Model(m) => {
                let of = format!("{}::<{}>()", d("of"), self.model_path(*m));
                self.boxed(of, owner.is_some_and(|o| self.holds(*m, o)))
            }
        }
    }

    /// `seed`, boxed where `boxes`.
    fn boxed(&self, seed: String, boxes: bool) -> String {
        match boxes {
            true => format!("{}({seed})", d("boxed")),
            false => seed,
        }
    }

    /// The seed of a tuple: a type reading its array, a position at a time, then the list of the
    /// items past them. An array of more items than a fixed tuple holds fails.
    fn tuple_seed(&self, ty: &Ty, owner: Option<usize>) -> String {
        let Base::Tuple { prefix, rest } = &ty.base else {
            return String::new();
        };
        let (mut reads, parts) = self.tuple_reads(ty, owner, &|seed, at| match at {
            Place::Item => format!("p.item(&mut a, {seed})?"),
            Place::Rest => format!("p.tail(&mut a, {seed})?"),
        });
        if rest.is_none() {
            reads.push_str("p.fixed(&mut a)?; ");
        }
        let value = self.tuple_ty(prefix, rest.as_deref(), owner);
        let (binary_reads, built) = self.tuple_reads(ty, owner, &|seed, at| match at {
            Place::Item => next(&optional(seed)),
            Place::Rest => next(&format!("{}({seed})", d("list"))),
        });
        let body = format!("{binary_reads}Ok({})", tuple_of(&built));
        let len = prefix.len().saturating_add(usize::from(rest.is_some()));
        format!(
            "{{ #[derive(Clone, Copy)] struct __T; impl<'de> {}<'de> for __T {{ type Value = {value}; \
             const EXPECTED: &'static str = \"an array\"; {} \
             fn seq<__A: {}<'de>>(self, mut a: __A) -> ::core::result::Result<Self::Value, __A::Error> {{ \
             let mut p = {}::default(); {reads}::core::result::Result::Ok({}) }} }} {}(__T) }}",
            d("Kinds"),
            self.positions(&value, len, &body),
            d("SeqAccess"),
            d("Parts"),
            tuple_of(&parts),
            d("Shape"),
        )
    }
}

impl StdForm<'_> {
    /// A tuple's reads, `let t0 = …;` for each position and `let tr = …;` for the rest, and the
    /// names they bind. `read` makes the expression reading a position from its seed.
    fn tuple_reads(
        &self,
        ty: &Ty,
        owner: Option<usize>,
        read: &dyn Fn(&str, Place) -> String,
    ) -> (String, Vec<String>) {
        let Base::Tuple { prefix, rest } = &ty.base else {
            return Default::default();
        };
        let (mut reads, mut parts) = (String::new(), Vec::new());
        for (k, p) in prefix.iter().enumerate() {
            let _ = write!(
                reads,
                "let t{k} = {}; ",
                read(&self.seed(p, owner), Place::Item)
            );
            parts.push(format!("t{k}"));
        }
        if let Some(r) = rest {
            let _ = write!(
                reads,
                "let tr = {}; ",
                read(&self.seed(r, None), Place::Rest)
            );
            parts.push("tr".into());
        }
        (reads, parts)
    }
}
