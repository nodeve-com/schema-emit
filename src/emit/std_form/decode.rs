//! Functions decoding a `serde_json::Value` into the std types. A model's is `decode_<name>`,
//! which names the model at the root of a path that says where a value doesn't fit; each also has
//! a `read_<name>` of the value without that root, which the union and the model around it call.
//! Each reader is an expression over `schema_emit_runtime::decode`'s, which put each step on the
//! path of a failure inside them. A union decodes in `decode/union.rs`.
//!
//! A required field absent, and a key no field names on a model that holds no such keys, are
//! refused. Every other key goes to the field of keys no field names, each value read as that
//! field's type. A field whose type holds its own model is boxed, as the struct has it.

use std::fmt::Write as _;

use super::{idents, tuple_of, StdForm};
use crate::ir::{Base, Int, Model, Ty};
use crate::Error;

mod union;

/// The runtime's decoders, as emitted code names them.
fn d(name: &str) -> String {
    format!("{}::decode::{name}", crate::emit::STD)
}

impl StdForm<'_> {
    /// Each union's and each model's decode functions.
    pub(super) fn decoders(&self) -> Result<String, Error> {
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

    /// The name of model `i`'s decode function, without `read_` or `decode_`.
    fn decoder_id(&self, i: usize) -> String {
        idents::snake(&(self.name)(i))
    }

    /// Model `i`'s `read_` and `decode_` functions.
    fn model_decoder(&self, i: usize, m: &Model) -> String {
        let (name, id) = ((self.name)(i), self.decoder_id(i));
        let unfit = d("Unfit");
        let mut out = format!(
            "#[allow(dead_code)] pub fn decode_{id}(v: &::serde_json::Value) \
             -> Result<{name}, {unfit}> {{ read_{id}(v).map_err(|e| e.model({:?})) }}\n",
            m.name
        );
        let (head, init, tail) = self.model_body(i, m);
        let _ = writeln!(
            out,
            "#[allow(dead_code, unused_variables)] pub fn read_{id}(v: &::serde_json::Value) \
             -> Result<{name}, {unfit}> {{ {head}let out = {name} {{ {init}}}; {tail}Ok(out) }}"
        );
        out
    }

    /// What goes before a model's struct, its fields, and what goes after.
    fn model_body(&self, i: usize, m: &Model) -> (String, String, String) {
        let names = idents::of(m);
        let others = |entries: String| match &m.others {
            Some(_) => format!("{}: {entries}, ", names.others),
            None => String::new(),
        };
        if m.is_value() {
            let init = m
                .fields
                .iter()
                .zip(&names.fields)
                .fold(String::new(), |mut s, (f, n)| {
                    let read = format!("{}?", self.dec(&f.ty, Some(i), "v", 0));
                    let read = if f.required {
                        read
                    } else {
                        format!("Some({read})")
                    };
                    let _ = write!(s, "{n}: {read}, ");
                    s
                });
            let empty = others("::std::collections::BTreeMap::new()".into());
            return (String::new(), init + &empty, String::new());
        }
        let keys: Vec<String> = m.fields.iter().map(|f| format!("{:?}", f.name)).collect();
        let keys = format!("&[{}]", keys.join(", "));
        let mut init = String::new();
        for (f, n) in m.fields.iter().zip(&names.fields) {
            let read = self.dec(&f.ty, Some(i), "x0", 1);
            let how = if f.required { "field" } else { "optional" };
            let _ = write!(init, "{n}: {}(o, {:?}, |x0| {read})?, ", d(how), f.name);
        }
        let rest = m.others.as_ref().map(|held| {
            let read = self.dec(held, None, "x0", 1);
            format!("{}(o, {keys}, |x0| {read})?", d("rest"))
        });
        let tail = match rest {
            Some(_) => String::new(),
            None => format!("{}(o, {keys})?; ", d("closed")),
        };
        init.push_str(&others(rest.unwrap_or_default()));
        (format!("let o = {}(v)?; ", d("object")), init, tail)
    }

    /// An expression of `Result<T, Unfit>`: `v`, a `&Value`, read as `ty`, a field of model
    /// `owner` where it is one. `depth` numbers the names a closure binds.
    pub(super) fn dec(&self, ty: &Ty, owner: Option<usize>, v: &str, depth: usize) -> String {
        if !ty.nullable {
            return self.dec_base(ty, owner, v, depth);
        }
        let x = format!("x{depth}");
        let inner = self.dec_base(ty, owner, &x, depth.saturating_add(1));
        format!("{}({v}, |{x}| {inner})", d("nullable"))
    }

    /// [`StdForm::dec`] of a type that isn't nullable.
    fn dec_base(&self, ty: &Ty, owner: Option<usize>, v: &str, depth: usize) -> String {
        let read = |name: &str| format!("{}({v})", d(name));
        let next = depth.saturating_add(1);
        let boxed = |expr: String, boxes: bool| match boxes {
            true => format!("{expr}.map(Box::new)"),
            false => expr,
        };
        match &ty.base {
            Base::Null => read("null"),
            Base::Any => format!("Ok::<_, {}>({v}.clone())", d("Unfit")),
            Base::Bool => read("boolean"),
            Base::Int(Int::U64) => read("uint"),
            Base::Int(_) => read("int"),
            Base::Float(_) => read("float"),
            Base::Text => read("text"),
            Base::Timestamp => read("timestamp"),
            Base::Duration => read("duration"),
            Base::Bytes => read("bytes"),
            Base::List(item) | Base::Map(item) => {
                let how = if matches!(ty.base, Base::List(_)) {
                    "list"
                } else {
                    "map"
                };
                let item = self.dec(item, None, &format!("x{depth}"), next);
                format!("{}({v}, |x{depth}| {item})", d(how))
            }
            Base::Tuple { prefix, rest } => {
                self.dec_tuple(prefix, rest.as_deref(), owner, v, depth)
            }
            Base::Union(u) => {
                let call = format!("read_{}({v})", idents::snake(&self.union_name(u)));
                boxed(call, owner.is_some_and(|o| self.boxes(u, o)))
            }
            Base::Model(m) => {
                let module = (self.external)(*m).map(|p| p + "::").unwrap_or_default();
                let call = format!("{module}read_{}({v})", self.decoder_id(*m));
                boxed(call, owner.is_some_and(|o| self.holds(*m, o)))
            }
        }
    }

    /// A tuple read in a closure that may refuse: its items from the array `v`, a position each,
    /// then the list of the items past them.
    fn dec_tuple(
        &self,
        prefix: &[Ty],
        rest: Option<&Ty>,
        owner: Option<usize>,
        v: &str,
        depth: usize,
    ) -> String {
        let (xs, x, next) = (
            format!("xs{depth}"),
            format!("x{depth}"),
            depth.saturating_add(1),
        );
        let mut parts: Vec<String> = Vec::new();
        for (i, p) in prefix.iter().enumerate() {
            let read = self.dec(p, owner, &x, next);
            parts.push(format!("{}({xs}, {i}, |{x}| {read})?", d("element")));
        }
        let mut fixed = String::new();
        match rest {
            Some(item) => {
                let read = self.dec(item, None, &x, next);
                parts.push(format!(
                    "{}({xs}, {}, |{x}| {read})?",
                    d("tail"),
                    prefix.len()
                ));
            }
            None => fixed = format!("{}({xs}, {})?; ", d("fixed"), prefix.len()),
        }
        format!(
            "(|| -> Result<_, {}> {{ let {xs} = {}({v})?.as_slice(); {fixed}Ok({}) }})()",
            d("Unfit"),
            d("array"),
            tuple_of(&parts)
        )
    }
}
