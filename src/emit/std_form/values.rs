//! A value read and written apart from the model holding it: one function decoding a value of a
//! type, one encoding it, each as a field of that type reads and writes. An alias model's
//! `decode_<name>` and `encode_<name>` are these, of its root's type.

use super::{decode::d, StdForm};
use crate::ir::{Base, Ty};
use crate::Error;

/// Whether a value of `ty` holds a dynamic reference outside any model.
fn open(ty: &Ty) -> bool {
    match &ty.base {
        Base::DynamicRef(_) => true,
        Base::List(item) | Base::Map(item) => open(item),
        Base::Tuple { prefix, rest } => {
            prefix.iter().any(open) || rest.as_deref().is_some_and(open)
        }
        Base::Union(u) => u.variants.iter().any(open),
        _ => false,
    }
}

impl StdForm<'_> {
    /// `pub fn <name>`, decoding a value of `ty` from any serde deserializer as a model's
    /// `decode_<name>` does: one property's value, apart from the model holding it. It returns
    /// [`StdForm::rust`]'s type beside its skips, and its failures' paths start at `path`. It
    /// refuses a type holding a dynamic reference, whose type parameter no function outside its
    /// model declares.
    pub fn decode_value(&self, ty: &Ty, path: &str, name: &str) -> Result<String, Error> {
        self.closed(ty, name)?;
        Ok(self.value_decoder(name, &self.rust(ty), ty, path))
    }

    /// `pub fn <name>`, taking a `&` of [`StdForm::rust`]'s type of `ty` and a serde serializer,
    /// and writing the value as a model's `Serialize` writes a field of that type. It refuses
    /// what [`StdForm::decode_value`] refuses.
    pub fn encode_value(&self, ty: &Ty, name: &str) -> Result<String, Error> {
        self.closed(ty, name)?;
        Ok(self.value_encoder(name, &self.rust(ty), ty))
    }

    /// Refuses a function `name` of a type holding a dynamic reference.
    fn closed(&self, ty: &Ty, name: &str) -> Result<(), Error> {
        match open(ty) {
            true => Err(Error::invalid(format!(
                "{name}: the type holds a dynamic reference, which only its model's items read and write"
            ))),
            false => Ok(()),
        }
    }

    /// `pub fn <name>`, decoding a value of `ty`, the Rust type `rust`, its failures' paths from
    /// `path`.
    pub(super) fn value_decoder(&self, name: &str, rust: &str, ty: &Ty, path: &str) -> String {
        format!(
            "#[allow(dead_code)] pub fn {name}<'de, __D: {}<'de>>(d: __D) \
             -> ::core::result::Result<({rust}, {}), {}> {{ {}(|| {}::deserialize({}, d), {path:?}) }}\n",
            d("Deserializer"),
            d("Skips"),
            d("Unfit"),
            d("root"),
            d("DeserializeSeed"),
            self.seed(ty, None),
        )
    }

    /// `pub fn <name>`, encoding a value of `ty`, the Rust type `rust`.
    pub(super) fn value_encoder(&self, name: &str, rust: &str, ty: &Ty) -> String {
        let (ser, serializer) = (d("Serialize"), d("Serializer"));
        format!(
            "#[allow(dead_code, unused_imports)] pub fn {name}<__S: {serializer}>(v: &{rust}, s: __S) \
             -> ::core::result::Result<__S::Ok, __S::Error> {{ use {ser} as _; {} }}\n",
            self.written("v", ty),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_holding_a_dynamic_reference_gets_no_function_of_its_own() {
        let schema = crate::ir::Schema::default();
        let name = |_: usize| String::new();
        let std = StdForm::new(&schema, &name);
        let open = Ty::of(Base::List(Box::new(Ty::of(Base::DynamicRef(
            "item".into(),
        )))));
        assert!(std.encode_value(&open, "write").is_err());
        assert!(std.decode_value(&open, "at", "read").is_err());
        let closed = Ty::of(Base::List(Box::new(Ty::of(Base::Bool))));
        let write = std.encode_value(&closed, "write").unwrap();
        assert!(write.contains("pub fn write<"), "{write}");
        let read = std.decode_value(&closed, "at", "read").unwrap();
        assert!(
            read.contains("pub fn read<") && read.contains("\"at\""),
            "{read}"
        );
    }
}
