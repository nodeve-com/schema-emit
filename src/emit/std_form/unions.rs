//! A union as a Rust enum: one variant per branch in order, holding the branch's type and named
//! from it: a model's name, else its Rust type's. The enum takes the `$defs` key that declares
//! it, unless a model's struct has that name. Else it is named from its branches' Rust types
//! joined by `Or`, so a union of the same branches is one enum wherever it sits.

use std::fmt::Write as _;

use super::StdForm;
use crate::ir::{Base, Ty, Union};

impl StdForm<'_> {
    /// The enum of `union`.
    pub(super) fn union_name(&self, union: &Union) -> String {
        let declared = union.name.as_deref().map(camel);
        let models = self.schema.models.len();
        if let Some(name) = declared.filter(|n| !(0..models).any(|i| self.model_name(i) == *n)) {
            return name;
        }
        let names: Vec<String> = union
            .variants
            .iter()
            .map(|v| match v.base {
                Base::Model(m) if !v.nullable => camel(&self.model_name(m)),
                _ => camel(&plain(&self.ty(v, None))),
            })
            .collect();
        names.join("Or")
    }

    /// The Rust type of a union a field of model `owner` holds: boxed where a variant holds
    /// `owner`, which would make the owner's size its own.
    pub(super) fn union_ty(&self, union: &Union, owner: Option<usize>) -> String {
        let name = self.union_name(union);
        match owner {
            Some(o) if self.boxes(union, o) => format!("::std::boxed::Box<{name}>"),
            _ => name,
        }
    }

    /// Whether a value of `union` held in model `owner` is boxed.
    pub(super) fn boxes(&self, union: &Union, owner: usize) -> bool {
        models(&union.variants).any(|m| self.holds(m, owner))
    }

    /// Variant `index`'s name.
    pub(super) fn variant_name(&self, union: &Union, index: usize) -> String {
        self.variant_names(&union.variants)
            .into_iter()
            .nth(index)
            .unwrap_or_default()
    }

    /// Each variant's name: its model's, else its type's, as words in upper camel case, made
    /// unique.
    fn variant_names(&self, variants: &[Ty]) -> Vec<String> {
        let mut used: Vec<String> = Vec::new();
        for v in variants {
            let name = match v.base {
                Base::Model(m) if !v.nullable => self.schema.models.get(m).map(|m| m.name.clone()),
                _ => None,
            };
            let mut id = camel(&name.unwrap_or_else(|| plain(&self.ty(v, None))));
            while used.contains(&id) {
                id.push('_');
            }
            used.push(id);
        }
        used
    }

    /// `union` as a Rust enum.
    fn union_item(&self, union: &Union) -> String {
        let mut out = String::new();
        let names = self.variant_names(&union.variants);
        for (name, v) in names.iter().zip(&union.variants) {
            let _ = write!(out, "{name}({}), ", self.ty(v, None));
        }
        format!(
            "{} #[allow(dead_code)] pub enum {} {{ {out}}}\n",
            self.derives(false),
            self.union_name(union)
        )
    }

    /// Each distinct union a model's fields hold, in the order found, by its enum's name.
    pub(super) fn distinct_unions(&self) -> Vec<(String, &Union)> {
        let mut found: Vec<&Union> = Vec::new();
        let local = self
            .schema
            .models
            .iter()
            .enumerate()
            .filter(|(i, _)| (self.external)(*i).is_none());
        for (_, m) in local {
            for ty in m.fields.iter().map(|f| &f.ty).chain(m.others.iter()) {
                unions_in(ty, &mut found);
            }
        }
        let mut out: Vec<(String, &Union)> = Vec::new();
        for union in found {
            let name = self.union_name(union);
            if !out.iter().any(|(n, _)| *n == name) {
                out.push((name, union));
            }
        }
        out
    }

    /// Each distinct union a model's fields hold, as an enum.
    pub(super) fn union_items(&self) -> String {
        let unions = self.distinct_unions();
        unions.iter().map(|(_, u)| self.union_item(u)).collect()
    }

    /// The enum value of `union` holding `lit`, a model's struct, as variant `index`.
    pub(super) fn variant_of(&self, union: &Union, index: usize, lit: &str) -> String {
        format!(
            "{}::{}({lit})",
            self.union_name(union),
            self.variant_name(union, index)
        )
    }

    /// `Option<&M>`: the variant `index` of `value`, a `&` of the enum or of a box of it.
    pub(super) fn read_variant(&self, value: &str, union: &Union, index: usize) -> String {
        let name = self.union_name(union);
        let variant = self.variant_name(union, index);
        format!(
            "{{ let e: &{name} = {value}; match e {{ {name}::{variant}(x) => Some(x), _ => None }} }}"
        )
    }
}

/// The models of `variants`, in order.
pub(super) fn models(variants: &[Ty]) -> impl Iterator<Item = usize> + '_ {
    variants.iter().filter_map(|v| match v.base {
        Base::Model(m) => Some(m),
        _ => None,
    })
}

/// The unions a value of `ty` holds, through lists, maps, and tuples.
fn unions_in<'a>(ty: &'a Ty, out: &mut Vec<&'a Union>) {
    match &ty.base {
        Base::Union(u) => {
            out.push(u);
            u.variants.iter().for_each(|t| unions_in(t, out));
        }
        Base::List(t) | Base::Map(t) => unions_in(t, out),
        Base::Tuple { prefix, rest } => {
            prefix.iter().for_each(|t| unions_in(t, out));
            rest.iter().for_each(|t| unions_in(t, out));
        }
        _ => {}
    }
}

/// `name` as words in upper camel case, an identifier's characters only.
pub fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(if start { c.to_ascii_uppercase() } else { c });
            start = false;
        } else {
            start = true;
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'V');
    }
    out
}

/// A Rust type's text without the paths that qualify the standard names, which a name reads as words.
fn plain(ty: &str) -> String {
    [
        "::std::string::",
        "::std::vec::",
        "::std::boxed::",
        "::core::option::",
    ]
    .iter()
    .fold(ty.to_string(), |t, path| t.replace(path, ""))
}
