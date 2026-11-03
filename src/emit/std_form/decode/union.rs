//! A union's `Deserialize` impl: its value read once as text, then each branch tried on it in
//! order. The first that decodes and whose check passes is the value. Where none passes its
//! check, the first that decodes is, so its check reports why. Where none decodes, the failure is
//! the branch's the value fit furthest. The check is the union's own, over its enum
//! (`Emitter::check_union`), so a branch of any type has one. In a format that isn't
//! human-readable the value is the branch its index names (`binary.rs`).

use super::d;
use crate::emit::{Emitter, STD};
use crate::ir::{Json, Ty, Union};
use crate::Error;

use super::super::{idents, StdForm};

impl StdForm<'_> {
    /// The `Deserialize` impl of `union`'s enum, and the check it calls.
    pub(super) fn union_decoder(&self, union: &Union) -> ::core::result::Result<String, Error> {
        let name = self.union_name(union);
        let ok = format!("ok_{}", idents::snake(&name));
        let mut out = Emitter::new(self.schema, self).check_union(union, &ok)?;
        let mut tries = String::new();
        for (i, variant) in union.variants.iter().enumerate() {
            tries.push_str(&format!(
                "pick.branch({}, {}, |x| {{ let c = {}; \
                 let mut h = {STD}::Holds::default(); {ok}(&c, &mut h); (c, h.held()) }}); ",
                kinds(variant),
                self.seed(variant, None),
                self.variant_of(union, i, "x"),
            ));
        }
        let (de, deser) = (d("Deserialize"), d("Deserializer"));
        out.push_str(&format!(
            "impl<'de> {de}<'de> for {name} {{ #[allow(unused_mut)] \
             fn deserialize<__D: {deser}<'de>>(d: __D) -> ::core::result::Result<{name}, __D::Error> {{ \
             if !d.is_human_readable() {{ {branches} }} \
             let raw = <{} as {de}<'de>>::deserialize(d)?; \
             let mut pick = {}::new(raw.get()); {tries}pick.finish() }} }}\n",
            d("Raw"),
            d("Pick"),
            branches = self.union_branches(union, &name),
        ));
        Ok(out)
    }
}

/// The mask of the JSON kinds a branch takes.
fn kinds(ty: &Ty) -> String {
    let kind = |name: &str| format!("{}::{name}", d("kind"));
    let base = match ty.base.json_kind() {
        Some(Json::Null) => kind("NULL"),
        Some(Json::Bool) => kind("BOOL"),
        Some(Json::Number) => kind("NUMBER"),
        Some(Json::Text) => kind("TEXT"),
        Some(Json::Array) => kind("ARRAY"),
        Some(Json::Object) => kind("OBJECT"),
        None => return kind("ANY"),
    };
    match ty.nullable {
        true => format!("{base} | {}", kind("NULL")),
        false => base,
    }
}
