//! Encoding the std types. Each model gets a serde `Serialize` impl, written out here rather than
//! derived, except an alias, which gets the function `encode_<name>` taking a serializer: an object of its fields in schema order, an absent optional field skipped, a nullable
//! value that is `None` as null, the keys a check reads as the typed values they are, and the
//! keys nothing reads written back as they arrived. A union is its branch's value, untagged. A
//! field is written through a writer composed from its type (`schema_emit_runtime::decode::ser`),
//! so a timestamp, a duration, and bytes keep their text forms.
//!
//! A format that isn't human-readable gets the positional form `decode::binary` reads: a model's
//! fields in order, an optional one an option, then its keys no field names; a union its
//! branch's index and value; a tuple an option per position, then the list of the rest.

use std::fmt::Write as _;

use super::{decode::d, idents, tuple_of, StdForm};
use crate::ir::{Additional, Base, Model, Ty};

impl StdForm<'_> {
    /// Each union's and each model's `Serialize` impl.
    pub(super) fn encoders(&self) -> String {
        let mut out = String::new();
        for (name, union) in self.distinct_unions() {
            let mut arms = String::new();
            for (i, v) in union.variants.iter().enumerate() {
                let variant = self.variant_name(union, i);
                let _ = write!(
                    arms,
                    "{name}::{variant}(x) => if s.is_human_readable() {{ {} }} else {{ \
                     s.serialize_newtype_variant({name:?}, {i}, {variant:?}, &{}::With(x, {})) }}, ",
                    self.written("x", v),
                    d("ser"),
                    self.writer(v),
                );
            }
            out.push_str(&self.encoder(&name, &format!("match self {{ {arms}}}")));
        }
        for (i, m) in self.schema.models.iter().enumerate() {
            if (self.external)(i).is_some() {
                continue;
            }
            match (self.is_alias(i), m.fields.first()) {
                (true, Some(f)) => out.push_str(&self.alias_encoder(i, &f.ty)),
                _ => out.push_str(&self.encoder(&self.model_name(i), &self.encode_body(m))),
            }
        }
        out
    }

    /// The `Serialize` impl of `name` with `body`.
    fn encoder(&self, name: &str, body: &str) -> String {
        let (ser, serializer) = (d("Serialize"), d("Serializer"));
        format!(
            "impl {ser} for {name} {{ #[allow(unused_imports)] \
             fn serialize<__S: {serializer}>(&self, s: __S) -> ::core::result::Result<__S::Ok, __S::Error> {{ \
             use {ser} as _; use {serializer} as _; use {} as _; use {} as _; {body} }} }}\n",
            d("SerializeMap"),
            d("SerializeTuple"),
        )
    }

    /// `encode_<name>` of alias model `i`, whose value of type `ty` has no `Serialize` impl to hold
    /// the code.
    fn alias_encoder(&self, i: usize, ty: &Ty) -> String {
        let (ser, serializer) = (d("Serialize"), d("Serializer"));
        format!(
            "#[allow(dead_code, unused_imports)] pub fn encode_{}<__S: {serializer}>(v: &{}, s: __S) \
             -> ::core::result::Result<__S::Ok, __S::Error> {{ use {ser} as _; {} }}\n",
            self.decoder_id(i),
            self.model_name(i),
            self.written("v", ty),
        )
    }

    /// The body of the `Serialize` impl of model `m`.
    fn encode_body(&self, m: &Model) -> String {
        let names = idents::of(m);
        if m.is_value() {
            return match (m.fields.first(), names.fields.first()) {
                (Some(f), Some(n)) if f.required => self.written(&format!("&self.{n}"), &f.ty),
                (Some(f), Some(n)) => format!(
                    "match &self.{n} {{ Some(v) => {}, None => s.serialize_none() }}",
                    self.written("v", &f.ty)
                ),
                _ => "s.serialize_none()".into(),
            };
        }
        let mut out = format!(
            "if !s.is_human_readable() {{ {} }} let mut m = s.serialize_map(None)?; ",
            self.encode_positions(m)
        );
        for (f, n) in m.fields.iter().zip(&names.fields) {
            let entry = |value: &str| {
                let w = self.writer(&f.ty);
                format!(
                    "m.serialize_entry({:?}, &{}::With({value}, {w}))?;",
                    f.name,
                    d("ser")
                )
            };
            match f.required {
                true => out.push_str(&entry(&format!("&self.{n}"))),
                false => {
                    let _ = write!(out, "if let Some(v) = &self.{n} {{ {} }} ", entry("v"));
                }
            }
        }
        out.push_str(&self.encode_others(m));
        out + "m.end()"
    }

    /// The statements writing model `m` positionally.
    fn encode_positions(&self, m: &Model) -> String {
        let names = idents::of(m);
        let ser = d("ser");
        let others = match (&m.others, m.additional) {
            (Some(held), _) => Some(format!("{ser}::Entries({})", self.writer(held))),
            (None, Additional::Keep) => Some(format!("{ser}::Plain")),
            (None, _) => None,
        };
        let mut out = format!(
            "let mut t = s.serialize_tuple({})?; ",
            m.fields.len().saturating_add(usize::from(others.is_some()))
        );
        for (f, n) in m.fields.iter().zip(&names.fields) {
            let w = self.writer(&f.ty);
            let w = if f.required {
                w
            } else {
                format!("{ser}::Maybe({w})")
            };
            let _ = write!(out, "t.serialize_element(&{ser}::With(&self.{n}, {w}))?; ");
        }
        if let Some(w) = others {
            let _ = write!(
                out,
                "t.serialize_element(&{ser}::With(&self.{}, {w}))?; ",
                names.others
            );
        }
        out + "return t.end();"
    }

    /// The code writing the keys of model `m` that no field names.
    fn encode_others(&self, m: &Model) -> String {
        let others = idents::of(m).others;
        match (&m.others, m.additional) {
            (Some(held), _) => {
                let entry = format!("&{}::With(v, {})", d("ser"), self.writer(held));
                format!("for (k, v) in &self.{others} {{ m.serialize_entry(k, {entry})?; }} ")
            }
            (None, Additional::Keep) => {
                format!("{}(&mut m, &self.{others})?; ", d("members"))
            }
            (None, _) => String::new(),
        }
    }

    /// An expression serializing the value `x` of type `ty` to `s`.
    fn written(&self, x: &str, ty: &Ty) -> String {
        format!("{}::With({x}, {}).serialize(s)", d("ser"), self.writer(ty))
    }

    /// The writer of a value of `ty`.
    fn writer(&self, ty: &Ty) -> String {
        let ser = d("ser");
        let base = match &ty.base {
            Base::Timestamp => format!("{ser}::Stamp"),
            Base::Duration => format!("{ser}::Span"),
            Base::Bytes => format!("{ser}::Base64"),
            Base::List(item) => format!("{ser}::Seq({})", self.writer(item)),
            Base::Map(item) => format!("{ser}::Entries({})", self.writer(item)),
            Base::Any => format!("{ser}::Dynamic"),
            Base::Tuple { prefix, rest } => {
                let mut parts: Vec<String> = prefix
                    .iter()
                    .map(|p| format!("{ser}::Maybe({})", self.writer(p)))
                    .collect();
                parts.extend(
                    rest.iter()
                        .map(|r| format!("{ser}::Seq({})", self.writer(r))),
                );
                format!("{ser}::Positional({})", tuple_of(&parts))
            }
            _ => format!("{ser}::Plain"),
        };
        match ty.nullable {
            true => format!("{ser}::Maybe({base})"),
            false => base,
        }
    }
}
