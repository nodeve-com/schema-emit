//! Decoding into the std types. Each model gets a serde `Deserialize` impl, written out here
//! rather than derived, and `decode_<name>`, the entry over a deserializer of any format: it
//! names the model at the root of a path that says where a value doesn't fit (`Model.field[2].key`).
//! The impl visits the object's entries as the deserializer reads them and builds no
//! `serde_json::Value`. A field's value is read through a seed (`decode/seeds.rs`); a union
//! decodes in `decode/union.rs`.
//!
//! A model reads each entry into its field's slot; a repeated key replaces it. A required field
//! that fails or is absent fails the model at once. An optional field that fails is absent and
//! a skip (`Skips`), which `decode_<name>` returns beside the value. A key no field names fails a
//! closed model, goes to the field of keys no field names if a check reads
//! them, and is kept as text otherwise if the model is open. A model that is one value reads that value; one that is an alias has no impl, only `decode_<name>`.
//!
//! In a format that isn't human-readable, the model's fields are read in order, as `binary.rs`
//! writes the reading.

use std::fmt::Write as _;

use super::{idents, StdForm};
use crate::ir::{Additional, Model, Ty};
use crate::Error;

mod binary;
mod seeds;
mod union;

/// The runtime's decode part, as emitted code names it.
pub(super) fn d(name: &str) -> String {
    format!("{}::decode::{name}", crate::emit::STD)
}

/// The code of a model's fields.
#[derive(Default)]
struct Fields {
    /// Each field's slot.
    slots: String,
    /// The arm reading each field's entry.
    arms: String,
    /// The check that each required field is there.
    requires: String,
    /// The struct's initializer of each field.
    init: String,
}

/// The code of a model's keys no field names.
#[derive(Default)]
struct Unnamed {
    /// Where such keys are kept.
    decl: String,
    /// The struct's initializer of the field holding them.
    init: String,
    /// The arm reading such a key's value.
    arm: String,
    /// Whether the key's text is kept.
    keep: bool,
}

impl StdForm<'_> {
    /// Each union's and each model's decode code.
    pub(super) fn decoders(&self) -> ::core::result::Result<String, Error> {
        let mut out = String::new();
        for (_, union) in self.distinct_unions() {
            out.push_str(&self.union_decoder(union)?);
        }
        for (i, m) in self.schema.models.iter().enumerate() {
            if (self.external)(i).is_none() {
                out.push_str(&self.model_decoder(i, m));
            }
        }
        Ok(out)
    }

    /// The name of model `i`'s decode function, without `decode_`: its name in snake case, with
    /// `_` added while an earlier model's is the same.
    pub(super) fn decoder_id(&self, i: usize) -> String {
        let mut used: Vec<String> = Vec::new();
        for j in 0..=i {
            let mut id = idents::snake(&self.model_name(j));
            while used.contains(&id) {
                id.push('_');
            }
            used.push(id);
        }
        used.pop().unwrap_or_default()
    }

    /// The name of model `i`'s decode function, as the emitted code declares it: `decode_` and
    /// its id. A model written elsewhere has its function there, not here.
    pub fn decoder(&self, i: usize) -> String {
        format!("decode_{}", self.decoder_id(i))
    }

    /// Model `i`'s `decode_` function and `Deserialize` impl, or its `decode_` function alone where it
    /// is an alias, which no impl can be written for.
    fn model_decoder(&self, i: usize, m: &Model) -> String {
        let (name, id) = (self.model_name(i), self.decoder_id(i));
        let (de, deser) = (d("Deserialize"), d("Deserializer"));
        if let (true, Some(f)) = (self.is_alias(i), m.fields.first()) {
            let seed = format!(
                "{}::deserialize({}, d)",
                d("DeserializeSeed"),
                self.seed(&f.ty, None)
            );
            return format!(
                "#[allow(dead_code)] pub fn decode_{id}<'de, __D: {deser}<'de>>(d: __D) \
                 -> ::core::result::Result<({name}, {}), {}> {{ {}(|| {seed}, {:?}) }}\n",
                d("Skips"),
                d("Unfit"),
                d("root"),
                m.name,
            );
        }
        let body = match m.is_value() {
            true => self.value_body(i, m),
            false => self.object_body(i, m),
        };
        format!(
            "#[allow(dead_code)] pub fn decode_{id}<'de, __D: {deser}<'de>>(d: __D) \
             -> ::core::result::Result<({name}, {}), {}> {{ {}(|| <{name} as {de}<'de>>::deserialize(d), {:?}) }}\n\
             impl<'de> {de}<'de> for {name} {{ \
             fn deserialize<__D: {deser}<'de>>(d: __D) -> ::core::result::Result<{name}, __D::Error> {{ {body} }} }}\n",
            d("Skips"),
            d("Unfit"),
            d("root"),
            m.name,
        )
    }

    /// The body of the impl of a model that is one value, read from the value whole.
    fn value_body(&self, i: usize, m: &Model) -> String {
        let names = idents::of(m);
        let mut init = String::new();
        for (f, n) in m.fields.iter().zip(&names.fields) {
            let seed = self.seed(&f.ty, Some(i));
            let read = format!("{}::deserialize({seed}, d)?", d("DeserializeSeed"));
            let read = if f.held() {
                read
            } else {
                format!(
                    "{}::deserialize({}({seed}), d)?",
                    d("DeserializeSeed"),
                    d("skip")
                )
            };
            let _ = write!(init, "{n}: {read}, ");
        }
        if m.others.is_some() {
            let _ = write!(init, "{}: ::std::vec::Vec::new(), ", names.others);
        } else if m.additional == Additional::Keep {
            let _ = write!(init, "{}: ::std::string::String::new(), ", names.others);
        }
        format!("Ok({} {{ {init}}})", self.model_name(i))
    }

    /// The body of the impl of a model of an object: a visitor reading its entries.
    fn object_body(&self, i: usize, m: &Model) -> String {
        let name = self.model_name(i);
        let (f, u) = (self.fields_code(i, m), self.unnamed(m));
        let keys: Vec<String> = m.fields.iter().map(|f| format!("{:?}", f.name)).collect();
        let (parts, slot, ignored) = (d("Parts"), d("Slot"), d("IgnoredAny"));
        let read = format!(
            "{}{}\
             while let Some(k) = m.next_key_seed({}::new(&[{}], {}))? {{ match k {{ {}\
             {slot}::Field(_) => {{ m.next_value::<{ignored}>()?; }} {} }} }} \
             {}{parts}::finish((|| Some({name} {{ {}{}}}))())",
            f.slots,
            u.decl,
            d("Names"),
            keys.join(", "),
            u.keep,
            f.arms,
            u.arm,
            f.requires,
            f.init,
            u.init,
        );
        format!(
            "#[derive(Clone, Copy)] struct __V; impl<'de> {}<'de> for __V {{ type Value = {name}; \
             const EXPECTED: &'static str = \"an object\"; \
             {} \
             #[allow(unused_mut, unused_variables)] \
             fn map<__A: {}<'de>>(self, mut m: __A) -> ::core::result::Result<{name}, __A::Error> {{ {read} }} }} \
             {}::deserialize({}(__V), d)",
            d("Kinds"),
            self.model_positions(i, m),
            d("MapAccess"),
            d("DeserializeSeed"),
            d("Shape"),
        )
    }

    /// Model `i`'s field code.
    fn fields_code(&self, i: usize, m: &Model) -> Fields {
        let names = idents::of(m);
        let (slot, parts) = (d("Slot"), d("Parts"));
        let mut out = Fields::default();
        for (k, (f, n)) in m.fields.iter().zip(&names.fields).enumerate() {
            let seed = self.seed(&f.ty, Some(i));
            let _ = write!(out.slots, "let mut f{k} = None; ");
            let step = format!(".{}", f.name);
            let arm = match f.required {
                true => {
                    format!("{parts}::field(&mut f{k}, {step:?}, &mut m, {seed})?")
                }
                false => format!(
                    "{parts}::optional(&mut f{k}, {step:?}, &mut m, {}({seed}))?",
                    d("skip")
                ),
            };
            let _ = write!(out.arms, "{slot}::Field({k}) => {arm}, ");
            if f.required {
                let _ = write!(
                    out.requires,
                    "{parts}::require::<_, __A::Error>(&f{k}, {step:?})?; "
                );
                let _ = write!(out.init, "{n}: f{k}?, ");
            } else if f.ty.default.is_some() {
                let _ = write!(
                    out.init,
                    "{n}: f{k}.unwrap_or_else({}::{}), ",
                    self.model_name(i),
                    super::defaults::field_fn(n)
                );
            } else {
                let _ = write!(out.init, "{n}: f{k}, ");
            }
        }
        out
    }

    /// The code of model `m`'s keys no field names, kept as typed values of `held`.
    fn unnamed_typed(&self, m: &Model, held: &Ty) -> Unnamed {
        let read = format!("|| m.next_value_seed({})", self.seed(held, None));
        Unnamed {
            decl: "let mut o = ::std::vec::Vec::new(); ".into(),
            init: format!("{}: o, ", idents::of(m).others),
            arm: format!(
                "{}::Other(k) => {}::other(&mut o, k, {read})?, ",
                d("Slot"),
                d("Parts")
            ),
            keep: true,
        }
    }

    /// What model `m` does with a key no field names.
    fn unnamed(&self, m: &Model) -> Unnamed {
        let slot = d("Slot");
        match &m.others {
            Some(held) => self.unnamed_typed(m, held),
            None if m.additional == Additional::Keep => Unnamed {
                decl: format!("let mut o = {}::default(); ", d("Unread")),
                init: format!("{}: o.text(), ", idents::of(m).others),
                arm: format!("{slot}::Other(k) => o.push(&k, &mut m)?, "),
                keep: true,
            },
            None if m.additional == Additional::Strip => Unnamed {
                arm: format!(
                    "{slot}::Other(_) => {{ m.next_value::<{}>()?; }} ",
                    d("IgnoredAny")
                ),
                ..Unnamed::default()
            },
            None => Unnamed {
                arm: format!(
                    "{slot}::Other(k) => return ::core::result::Result::Err({}::unnamed(&k)), ",
                    d("Parts")
                ),
                keep: true,
                ..Unnamed::default()
            },
        }
    }
}
