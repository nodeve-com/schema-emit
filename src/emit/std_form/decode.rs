//! Functions decoding JSON text into the std types. A model's is `decode_<name>`, which refuses
//! text nested too deep and names the model at the root of a path that says where a value doesn't
//! fit; each also has a `read_<name>` of one value's text without that root, which the union and
//! the model around it call. Each reader is an expression over `schema_emit_runtime::decode`'s,
//! which put each step on the path of a failure inside them. A union decodes in `decode/union.rs`.
//!
//! A model takes its fields' text from the object first, then reads each in turn. A required field
//! absent, and a key no field names on a closed model, are refused. An open model that holds no
//! such keys skips them unread. Every other key goes to the field of keys no field names, each
//! value read as that field's type. A field whose type holds its own model is boxed, as the struct
//! has it.

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
            "#[allow(dead_code)] pub fn decode_{id}(v: &str) -> Result<{name}, {unfit}> {{ \
             {}(v, {:?}, read_{id}) }}\n",
            d("root"),
            m.name
        );
        let (head, init, tail) = self.model_body(i, m);
        let _ = writeln!(
            out,
            "#[allow(dead_code, unused_variables)] pub fn read_{id}(v: &str) \
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
            let init = self.value_fields(i, m, &names.fields);
            let empty = others("::std::collections::BTreeMap::new()".into());
            return (String::new(), init + &empty, String::new());
        }
        let keys: Vec<String> = m.fields.iter().map(|f| format!("{:?}", f.name)).collect();
        let keys = format!("&[{}]", keys.join(", "));
        let (mut init, mut slots) = (String::new(), Vec::new());
        for (k, (f, n)) in m.fields.iter().zip(&names.fields).enumerate() {
            let read = self.dec(&f.ty, Some(i), 1);
            let how = if f.required { "field" } else { "optional" };
            let _ = write!(init, "{n}: {}(f{k}, {:?}, |x0| {read})?, ", d(how), f.name);
            slots.push(format!("f{k}"));
        }
        let rest = m.others.as_ref().map(|held| {
            let read = self.dec(held, None, 1);
            format!("{}(v, {keys}, |x0| {read})?", d("rest"))
        });
        let tail = match rest {
            None if !m.open => format!("{}(v, {keys})?; ", d("closed")),
            _ => String::new(),
        };
        init.push_str(&others(rest.unwrap_or_default()));
        let head = format!("let [{}] = {}(v, {keys})?; ", slots.join(", "), d("object"));
        (head, init, tail)
    }

    /// The fields of a model that is one value, each read from that value.
    fn value_fields(&self, i: usize, m: &Model, names: &[String]) -> String {
        let mut init = String::new();
        for (f, n) in m.fields.iter().zip(names) {
            let read = format!("{}?", self.dec(&f.ty, Some(i), 0));
            let read = if f.required {
                read
            } else {
                format!("Some({read})")
            };
            let _ = write!(init, "{n}: {read}, ");
        }
        init
    }

    /// An expression of `Result<T, Unfit>`: the value at `depth` read as `ty`, a field of model
    /// `owner` where it is one. `depth` numbers the names a closure binds.
    pub(super) fn dec(&self, ty: &Ty, owner: Option<usize>, depth: usize) -> String {
        if !ty.nullable {
            return self.dec_base(ty, owner, depth);
        }
        let x = format!("x{depth}");
        let inner = self.dec_base(ty, owner, depth.saturating_add(1));
        format!("{}({}, |{x}| {inner})", d("nullable"), bound(depth))
    }

    /// [`StdForm::dec`] of a type that isn't nullable.
    fn dec_base(&self, ty: &Ty, owner: Option<usize>, depth: usize) -> String {
        let v = bound(depth);
        let read = |name: &str| format!("{}({v})", d(name));
        let boxed = |expr: String, boxes: bool| match boxes {
            true => format!("{expr}.map(Box::new)"),
            false => expr,
        };
        match &ty.base {
            Base::Null => read("null"),
            Base::Any => read("any"),
            Base::Bool => read("boolean"),
            Base::Int(Int::U64) => read("uint"),
            Base::Int(_) => read("int"),
            Base::Float(_) => read("float"),
            Base::Text => read("text"),
            Base::Timestamp => read("timestamp"),
            Base::Duration => read("duration"),
            Base::Bytes => read("bytes"),
            Base::List(item) | Base::Map(item) => self.dec_items(ty, item, depth),
            Base::Tuple { .. } => self.dec_tuple(ty, owner, depth),
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

    /// A list or map of `item` read in a closure: each item or entry value read as `item`.
    fn dec_items(&self, ty: &Ty, item: &Ty, depth: usize) -> String {
        let how = if matches!(ty.base, Base::List(_)) {
            "list"
        } else {
            "map"
        };
        let item = self.dec(item, None, depth.saturating_add(1));
        format!("{}({}, |x{depth}| {item})", d(how), bound(depth))
    }

    /// A tuple read in a closure that may refuse: its items from the array at `depth`, a position
    /// each, then the list of the items past them.
    fn dec_tuple(&self, ty: &Ty, owner: Option<usize>, depth: usize) -> String {
        let Base::Tuple { prefix, rest } = &ty.base else {
            return String::new();
        };
        let (v, x, next) = (bound(depth), format!("x{depth}"), depth.saturating_add(1));
        let (mut parts, mut slots) = (Vec::new(), Vec::new());
        for (i, p) in prefix.iter().enumerate() {
            let read = self.dec(p, owner, next);
            parts.push(format!(
                "{}(p{depth}_{i}, {i}, |{x}| {read})?",
                d("element")
            ));
            slots.push(format!("p{depth}_{i}"));
        }
        let n = prefix.len();
        let fixed = match rest {
            Some(item) => {
                let read = self.dec(item, None, next);
                parts.push(format!("{}({v}, {n}, |{x}| {read})?", d("tail")));
                String::new()
            }
            None => format!("{}({v}, {n})?; ", d("fixed")),
        };
        format!(
            "(|| -> Result<_, {}> {{ let [{}] = {}::<{n}>({v})?; {fixed}Ok({}) }})()",
            d("Unfit"),
            slots.join(", "),
            d("tuple"),
            tuple_of(&parts)
        )
    }
}

/// The name the value at `depth` is bound to: the function's own at the root, else the closure
/// above it binds it.
fn bound(depth: usize) -> String {
    match depth.checked_sub(1) {
        Some(above) => format!("x{above}"),
        None => "v".into(),
    }
}
