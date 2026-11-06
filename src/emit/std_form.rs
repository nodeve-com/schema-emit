#![doc = include_str!("../../docs/std-form.md")]

use std::fmt::Write as _;

use crate::ir::{Additional, Base, Float, Model, Schema, Ty};
use crate::Error;

mod decode;
mod derives;
mod encode;
mod idents;
mod paths;
mod read;
mod source;
mod types;
mod unions;

pub use idents::fields;
pub use source::source;
use types::{inline, rust_int, tuple_of};
pub use unions::camel;

/// The std form of `schema`'s models: model `i` is the struct `name(i)`.
pub struct StdForm<'a> {
    pub schema: &'a Schema,
    pub name: &'a dyn Fn(usize) -> String,
    /// The path of the module holding model `i`'s struct and its `Deserialize` and `Serialize` impls, where another
    /// crate writes them; none is written here.
    pub external: &'a dyn Fn(usize) -> Option<String>,
    /// The paths of derive macros each struct and enum also derives, beyond `Debug`, `Clone`, and
    /// `PartialEq`.
    pub derive: &'a [&'a str],
}

/// No model written elsewhere.
fn local(_: usize) -> Option<String> {
    None
}

/// A struct name for each of `schema`'s models: its name in upper camel case, made unique, for
/// an embedder with no names of its own to pass [`StdForm::new`].
pub fn names(schema: &Schema) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    for m in &schema.models {
        let base = unions::camel(&m.name);
        let mut id = base.clone();
        let mut n = 1_usize;
        while used.contains(&id) {
            n = n.saturating_add(1);
            id = format!("{base}{n}");
        }
        used.push(id);
    }
    used
}

impl<'a> StdForm<'a> {
    pub fn new(schema: &'a Schema, name: &'a dyn Fn(usize) -> String) -> Self {
        StdForm {
            schema,
            name,
            external: &local,
            derive: &[],
        }
    }

    /// This form, its models `external` names written in those modules.
    pub fn with_external(self, external: &'a dyn Fn(usize) -> Option<String>) -> Self {
        StdForm { external, ..self }
    }

    /// This form, its structs and enums also deriving `derive`.
    pub fn with_derive(self, derive: &'a [&'a str]) -> Self {
        StdForm { derive, ..self }
    }

    /// The Rust type of a value of `ty`, in model `owner` if it is a field's, which boxes a model
    /// that holds `owner`.
    fn ty(&self, ty: &Ty, owner: Option<usize>) -> String {
        let base = match &ty.base {
            Base::Null => "::any_value::Null".into(),
            Base::Any => "::any_value::Value".into(),
            Base::Bool => "bool".into(),
            Base::Int(w) => rust_int(*w).into(),
            Base::Float(Float::F32) => "f32".into(),
            Base::Float(Float::F64) => "f64".into(),
            Base::Text => "::std::string::String".into(),
            Base::Timestamp => "::chrono::DateTime<::chrono::Utc>".into(),
            Base::Duration => "::chrono::TimeDelta".into(),
            Base::Bytes => "::std::vec::Vec<u8>".into(),
            Base::List(item) => format!("::std::vec::Vec<{}>", self.ty(item, None)),
            Base::Tuple { prefix, rest } => self.tuple_ty(prefix, rest.as_deref(), owner),
            Base::Map(item) => format!(
                "::std::vec::Vec<(::std::string::String, {})>",
                self.ty(item, None)
            ),
            Base::Union(u) => self.union_ty(u, owner),
            Base::Model(m) => match owner {
                Some(o) if self.holds(*m, o) => {
                    format!("::std::boxed::Box<{}>", self.model_path(*m))
                }
                _ => self.model_path(*m),
            },
        };
        if ty.nullable {
            format!("::core::option::Option<{base}>")
        } else {
            base
        }
    }

    /// The Rust tuple of a tuple type: an `Option` per position, then the list of the rest.
    fn tuple_ty(&self, prefix: &[Ty], rest: Option<&Ty>, owner: Option<usize>) -> String {
        let mut parts: Vec<String> = prefix
            .iter()
            .map(|p| format!("::core::option::Option<{}>", self.ty(p, owner)))
            .collect();
        if let Some(r) = rest {
            parts.push(format!("::std::vec::Vec<{}>", self.ty(r, None)));
        }
        tuple_of(&parts)
    }

    /// Whether a value of model `from` holds a value of model `to` in its own size: `from` is
    /// `to`, or has a field of a model that does, directly or in a tuple.
    fn holds(&self, from: usize, to: usize) -> bool {
        let mut seen = vec![from];
        let mut todo = vec![from];
        while let Some(m) = todo.pop() {
            if m == to {
                return true;
            }
            todo.extend(self.unseen_models(m, &mut seen));
        }
        false
    }

    /// The models a value of model `m` holds in its own size, through its fields, that `seen`
    /// lacks; `seen` gains them.
    fn unseen_models(&self, m: usize, seen: &mut Vec<usize>) -> Vec<usize> {
        let fields = self.schema.models.get(m).map(|m| m.fields.as_slice());
        let mut fresh = Vec::new();
        for n in fields
            .unwrap_or_default()
            .iter()
            .flat_map(|f| inline(&f.ty))
        {
            if !seen.contains(&n) {
                seen.push(n);
                fresh.push(n);
            }
        }
        fresh
    }

    /// The name of model `model`'s field of keys no field names.
    fn others_field(&self, model: usize) -> Result<String, Error> {
        let m = self.schema.models.get(model);
        Ok(idents::of(m.ok_or_else(|| Error::invalid(format!("no model {model}")))?).others)
    }

    /// Each model's struct, and the code decoding a value into it and encoding it.
    pub fn items(&self) -> Result<String, Error> {
        let mut out = self.structs();
        out.push_str(&self.decoders()?);
        out.push_str(&self.encoders());
        Ok(out)
    }

    /// Each model's struct, after the enums of the unions they hold, without the functions
    /// decoding them.
    pub fn types(&self) -> String {
        self.structs()
    }

    /// Each model's struct, or alias where it is a value's own type, after the enums of the unions
    /// they hold.
    fn structs(&self) -> String {
        let mut out = self.union_items();
        for (i, m) in self.schema.models.iter().enumerate() {
            if (self.external)(i).is_some() {
                continue;
            }
            if let (true, Some(f)) = (self.is_alias(i), m.fields.first()) {
                let _ = writeln!(
                    out,
                    "#[allow(dead_code)] pub type {} = {};",
                    self.model_name(i),
                    self.ty(&f.ty, None)
                );
                continue;
            }
            let _ = writeln!(
                out,
                "{} #[allow(dead_code)] pub struct {} {{ {}}}",
                self.derives(self.defaults(i, &mut Vec::new())),
                self.model_name(i),
                self.struct_fields(i, m)
            );
        }
        out
    }

    /// Model `i`'s fields, then its field of keys no field names, as the struct declares them.
    fn struct_fields(&self, i: usize, m: &Model) -> String {
        let names = idents::of(m);
        let mut fields = String::new();
        for (f, name) in m.fields.iter().zip(&names.fields) {
            let ty = self.ty(&f.ty, Some(i));
            let ty = if f.required {
                ty
            } else {
                format!("::core::option::Option<{ty}>")
            };
            let _ = write!(fields, "pub {name}: {ty}, ");
        }
        if let Some(held) = &m.others {
            let held = self.ty(held, None);
            let _ = write!(
                fields,
                "pub {}: ::std::vec::Vec<(::std::string::String, {held})>, ",
                names.others
            );
        } else if m.additional == Additional::Keep {
            let _ = write!(fields, "pub {}: ::std::string::String, ", names.others);
        }
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::fields;

    #[test]
    fn a_name_rust_refuses_or_two_that_collapse_stay_fields() {
        let names = fields(["type", "self", "fooBar", "foo_bar"]);
        assert_eq!(names, ["r#type", "self_", "foo_bar", "foo_bar_"]);
    }
}
