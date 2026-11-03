//! Values as standard Rust, for use anywhere: a model a struct the embedder names, its fields
//! named from the schema. A required field is `T`, an optional one `Option<T>`; a
//! nullable type is an `Option` too. An integer is `i64` whatever its width but `uint64`'s `u64`,
//! a float `f64`; text `String`, bytes `Vec<u8>`, a timestamp `chrono::DateTime<chrono::Utc>`, a
//! duration `chrono::TimeDelta`; a list `Vec`; keys no field names a `BTreeMap<String, _>`. A
//! field whose type holds its own model is boxed, no other. A value of any kind is a
//! `serde_json::Value`, a null a `()`. A union is an enum, a variant per branch
//! (`std_form/unions.rs`). [`StdForm`] is the [`Access`] to it, and writes beside each struct the
//! function decoding a `serde_json::Value` into it (`std_form/decode.rs`).

use std::fmt::Write as _;

use super::Access;
use crate::ir::{Base, Int, Model, Schema, Ty, Union};
use crate::Error;

mod decode;
mod idents;
mod source;
mod unions;

pub use idents::fields;
pub use source::source;
pub use unions::camel;

/// The std form of `schema`'s models: model `i` is the struct `name(i)`.
pub struct StdForm<'a> {
    pub schema: &'a Schema,
    pub name: &'a dyn Fn(usize) -> String,
    /// The path of the module holding model `i`'s struct and `read_` function, where another
    /// crate writes them; none is written here.
    pub external: &'a dyn Fn(usize) -> Option<String>,
}

/// No model written elsewhere.
fn local(_: usize) -> Option<String> {
    None
}

impl Access for StdForm<'_> {
    fn rust(&self, _: &Schema, ty: &Ty) -> Result<String, Error> {
        Ok(self.ty(ty, None))
    }

    fn field(&self, value: &str, model: usize, field: usize) -> String {
        let name = self.schema.models.get(model).and_then(|m| {
            let f = m.fields.get(field)?;
            Some((idents::of(m).fields.get(field)?.clone(), f.required))
        });
        match name {
            Some((name, true)) => format!("Some(&{value}.{name})"),
            Some((name, false)) => format!("{value}.{name}.as_ref()"),
            None => "None".into(),
        }
    }

    fn others(&self, value: &str, model: usize) -> Result<String, Error> {
        let field = self.others_field(model)?;
        Ok(format!(
            "{value}.{field}.iter().map(|(k, x)| (k.as_str(), x))"
        ))
    }

    fn entries(&self, value: &str) -> Result<String, Error> {
        Ok(format!("{value}.iter().map(|(k, x)| (k.as_str(), x))"))
    }

    fn variant(&self, value: &str, union: &Union, index: usize) -> Result<String, Error> {
        Ok(self.read_variant(value, union, index))
    }

    fn other(&self, value: &str, model: usize, key: &str) -> Result<String, Error> {
        let field = self.others_field(model)?;
        Ok(format!("{value}.{field}.get({key})"))
    }
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
        }
    }

    /// This form, its models `external` names written in those modules.
    pub fn with_external(self, external: &'a dyn Fn(usize) -> Option<String>) -> Self {
        StdForm { external, ..self }
    }

    /// Model `m`'s struct, through its module where another crate writes it.
    fn model_path(&self, m: usize) -> String {
        match (self.external)(m) {
            Some(module) => format!("{module}::{}", (self.name)(m)),
            None => (self.name)(m),
        }
    }

    /// The Rust type of a value of `ty`, in model `owner` if it is a field's, which boxes a model
    /// that holds `owner`.
    fn ty(&self, ty: &Ty, owner: Option<usize>) -> String {
        let base = match &ty.base {
            Base::Null => "()".into(),
            Base::Any => "::serde_json::Value".into(),
            Base::Bool => "bool".into(),
            Base::Int(Int::U64) => "u64".into(),
            Base::Int(_) => "i64".into(),
            Base::Float(_) => "f64".into(),
            Base::Text => "String".into(),
            Base::Timestamp => "::chrono::DateTime<::chrono::Utc>".into(),
            Base::Duration => "::chrono::TimeDelta".into(),
            Base::Bytes => "Vec<u8>".into(),
            Base::List(item) => format!("Vec<{}>", self.ty(item, None)),
            Base::Tuple { prefix, rest } => self.tuple_ty(prefix, rest.as_deref(), owner),
            Base::Map(item) => format!(
                "::std::collections::BTreeMap<String, {}>",
                self.ty(item, None)
            ),
            Base::Union(u) => self.union_ty(u, owner),
            Base::Model(m) => match owner {
                Some(o) if self.holds(*m, o) => format!("Box<{}>", self.model_path(*m)),
                _ => self.model_path(*m),
            },
        };
        if ty.nullable {
            format!("Option<{base}>")
        } else {
            base
        }
    }

    /// The Rust tuple of a tuple type: an `Option` per position, then the list of the rest.
    fn tuple_ty(&self, prefix: &[Ty], rest: Option<&Ty>, owner: Option<usize>) -> String {
        let mut parts: Vec<String> = prefix
            .iter()
            .map(|p| format!("Option<{}>", self.ty(p, owner)))
            .collect();
        if let Some(r) = rest {
            parts.push(format!("Vec<{}>", self.ty(r, None)));
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

    /// Each model's struct, and the function decoding a value into it.
    pub fn items(&self) -> Result<String, Error> {
        let mut out = self.structs();
        out.push_str(&self.decoders()?);
        Ok(out)
    }

    /// Each model's struct, after the enums of the unions they hold.
    fn structs(&self) -> String {
        let mut out = self.union_items();
        for (i, m) in self.schema.models.iter().enumerate() {
            if (self.external)(i).is_some() {
                continue;
            }
            let _ = writeln!(
                out,
                "#[derive(Debug, Clone, PartialEq)] #[allow(dead_code)] pub struct {} {{ {}}}",
                (self.name)(i),
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
                format!("Option<{ty}>")
            };
            let _ = write!(fields, "pub {name}: {ty}, ");
        }
        if let Some(held) = &m.others {
            let held = self.ty(held, None);
            let _ = write!(
                fields,
                "pub {}: ::std::collections::BTreeMap<String, {held}>, ",
                names.others
            );
        }
        fields
    }
}

/// The models a value of `ty` holds in its own size: its own, or a tuple's.
fn inline(ty: &Ty) -> Vec<usize> {
    match &ty.base {
        Base::Model(m) => vec![*m],
        Base::Union(u) => unions::models(&u.variants).collect(),
        Base::Tuple { prefix, .. } => prefix.iter().flat_map(inline).collect(),
        _ => Vec::new(),
    }
}

/// A Rust tuple of `parts`, whose one part needs a comma to be a tuple.
fn tuple_of(parts: &[String]) -> String {
    match parts {
        [one] => format!("({one},)"),
        _ => format!("({})", parts.join(", ")),
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
