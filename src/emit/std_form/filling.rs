//! A model filling a type parameter with a type that is no struct, such as a number or a list.
//! The generic struct's own impls read a type parameter by its serde impl, which only a struct
//! written here has. So such a model reads and writes by code of its own: the generic struct's
//! fields, each dynamic reference the type filling it, behind `seed_<name>`, `decode_<name>`,
//! `writer_<name>`, and `encode_<name>`. A struct holding the model reads it by that seed and
//! writes it by that writer.

use super::{decode::d, idents, StdForm};
use crate::ir::{Base, Model, Ty};

/// Whether `ty` is a struct written here, which its own serde impl reads and writes.
fn plain(ty: &Ty) -> bool {
    matches!(ty.base, Base::Model(_)) && !ty.nullable
}

impl StdForm<'_> {
    /// Whether model `m` fills a type parameter with a type that is no struct, so it reads by
    /// `decode_<name>` and writes by `encode_<name>`, never by a serde impl.
    pub fn is_bare(&self, m: usize) -> bool {
        let of = self.schema.models.get(m).and_then(|m| m.of.as_ref());
        of.is_some_and(|of| !of.with.iter().all(plain))
    }

    /// `f`, writing the reading or the writing of model `filler` with its own types.
    fn filled<T>(&self, filler: usize, f: impl FnOnce() -> T) -> T {
        let outer = (
            self.within.replace(None),
            self.filling.replace(Some(filler)),
        );
        let out = f();
        self.within.set(outer.0);
        self.filling.set(outer.1);
        out
    }

    /// The type filling dynamic anchor `anchor` in the model being written with its own types.
    pub(super) fn filled_as(&self, anchor: &str) -> Option<Ty> {
        let filler = self.schema.models.get(self.filling.get()?)?;
        let of = filler.of.as_ref()?;
        let refs = &self.schema.models.get(of.model)?.dynamic_refs;
        let at = refs.iter().position(|r| r.name == anchor)?;
        of.with.get(at).cloned()
    }

    /// Whether model `m` has a type parameter the model being written with its own types fills
    /// with a type that is no struct.
    fn takes(&self, m: usize) -> bool {
        let refs = self.schema.models.get(m).map(|m| m.dynamic_refs.as_slice());
        let bare = |t: Ty| !plain(&t);
        (refs.unwrap_or_default().iter()).any(|r| self.filled_as(&r.name).is_some_and(bare))
    }

    /// The function `<kind>_<name>` of model `m`, through its module where another crate writes it.
    fn filler_fn(&self, kind: &str, m: usize) -> String {
        match (self.external)(m) {
            Some(module) => format!("{module}::{kind}_{}", idents::snake(&self.model_name(m))),
            None => format!("{kind}_{}", self.decoder_id(m)),
        }
    }

    /// The seed of a value of model `m`.
    pub(super) fn model_seed(&self, m: usize) -> String {
        match self.schema.models.get(m) {
            _ if self.is_bare(m) => format!("{}()", self.filler_fn("seed", m)),
            Some(model) if self.takes(m) => self.object_seed(m, model),
            _ => format!("{}::<{}>()", d("of"), self.model_path(m)),
        }
    }

    /// The writer of a value of model `m`.
    pub(super) fn model_writer(&self, m: usize) -> String {
        match self.schema.models.get(m) {
            _ if self.is_bare(m) => format!("{}()", self.filler_fn("writer", m)),
            Some(model) if self.takes(m) => self.object_writer(m, model),
            _ => format!("{}::Plain", d("ser")),
        }
    }

    /// The writer of model `i`, an object: a type writing its fields.
    fn object_writer(&self, i: usize, m: &Model) -> String {
        let (path, ser, serializer) = (self.model_path(i), d("Serialize"), d("Serializer"));
        format!(
            "{{ #[derive(Clone, Copy)] struct __W; impl {}::Write<{path}> for __W {{ #[allow(unused_imports)] \
             fn write<__S: {serializer}>(&self, v: &{path}, s: __S) -> ::core::result::Result<__S::Ok, __S::Error> {{ \
             use {ser} as _; use {serializer} as _; use {} as _; use {} as _; {} }} }} __W }}",
            d("ser"),
            d("SerializeMap"),
            d("SerializeTuple"),
            self.encode_body(i, m, "v"),
        )
    }

    /// `seed_<name>` and `decode_<name>` of model `i`, which fills model `of` with a type that is
    /// no struct.
    pub(super) fn filler_decoder(&self, i: usize, m: &Model) -> String {
        let Some((of, model)) = self.filled_model(m) else {
            return String::new();
        };
        let (name, id, seed) = (self.model_name(i), self.decoder_id(i), d("DeserializeSeed"));
        format!(
            "#[allow(dead_code)] pub fn seed_{id}() -> impl for<'de> {seed}<'de, Value = {name}> + Copy {{ {} }}\n\
             #[allow(dead_code)] pub fn decode_{id}<'de, __D: {}<'de>>(d: __D) \
             -> ::core::result::Result<({name}, {}), {}> {{ {}(|| {seed}::deserialize(seed_{id}(), d), {:?}) }}\n",
            self.filled(i, || self.object_seed(of, model)),
            d("Deserializer"),
            d("Skips"),
            d("Unfit"),
            d("root"),
            m.name,
        )
    }

    /// `writer_<name>` and `encode_<name>` of model `i`, which fills model `of` with a type that
    /// is no struct.
    pub(super) fn filler_encoder(&self, i: usize, m: &Model) -> String {
        let Some((of, model)) = self.filled_model(m) else {
            return String::new();
        };
        let (name, id, write) = (self.model_name(i), self.decoder_id(i), d("ser::Write"));
        format!(
            "#[allow(dead_code)] pub fn writer_{id}() -> impl {write}<{name}> + Copy {{ {} }}\n\
             #[allow(dead_code)] pub fn encode_{id}<__S: {}>(v: &{name}, s: __S) \
             -> ::core::result::Result<__S::Ok, __S::Error> {{ {write}::write(&writer_{id}(), v, s) }}\n",
            self.filled(i, || self.object_writer(of, model)),
            d("Serializer"),
        )
    }

    /// The model `m` fills, and its index.
    fn filled_model(&self, m: &Model) -> Option<(usize, &Model)> {
        let of = m.of.as_ref()?.model;
        Some((of, self.schema.models.get(of)?))
    }
}
