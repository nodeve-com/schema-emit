//! The IR as Rust: one function per model checking a value of it, each field's checks inline with
//! its path and bounds as literals, failures sent to a `schema_emit_runtime::Report`. How a value
//! reads is the embedder's ([`Access`]); every name the code writes is generated.
//!
//! A value binds to a view of its base, which each check reads: an integer `i128`, a float
//! `f64`, a bool, text `&str`, a list its item count `u64`. A check is a condition on the view;
//! the value fails `range` where any is false.

mod at;
mod boolean;
mod bounds;
mod branches;
mod choices;
mod lists;
mod models;
mod negate;
mod others;
mod presence;
mod recursion;
mod scalars;
pub mod std_form;
mod tables;
mod text;
mod tuples;
mod unevaluated;
mod unions;
mod walk;

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;

use crate::ir::{Base, Check, Lit, Model, Schema, Ty, Union};
use crate::Error;
use at::At;
use walk::Walk;

/// The runtime crate, as the emitted code names it, under a name no model takes: the embedder
/// writes `use ::schema_emit_runtime as __s;` beside it.
pub const STD: &str = "__s";

/// How the embedder's values read, each method a Rust expression.
pub trait Access {
    /// The Rust type of a value of `ty`: an `Option` of its base where it is nullable.
    fn rust(&self, schema: &Schema, ty: &Ty) -> Result<String, Error>;
    /// `Option<&T>`: field `field` of model `model`, read from `value`, a `&` of the model's type.
    fn field(&self, value: &str, model: usize, field: usize) -> String;
    /// `Option<&T>`: position `index` of the tuple `value`, a `&` of the tuple's type.
    fn element(&self, value: &str, index: usize) -> String {
        format!("{value}.{index}.as_ref()")
    }
    /// `Result<&str, _>`: the text `value`, a `&` of text's type, reads as.
    fn text(&self, value: &str) -> String {
        format!("Ok::<&str, ()>({value}.as_str())")
    }
    /// `Result<impl Iterator, _>`: a list's items, each a `T` or a `&T`.
    fn items(&self, value: &str) -> String {
        format!("Ok::<_, ()>({value}.iter())")
    }
    /// An iterator of `(&str, &T)`: each key of model `model` no field names, with its value,
    /// read from `value`. An embedder with no such keys has none to give.
    fn others(&self, _value: &str, _model: usize) -> Result<String, Error> {
        Err(Error::unsupported("keys no field names"))
    }
    /// An iterator of `(&str, &T)`: each entry of the map `value`, a `&` of the map's type.
    fn entries(&self, _value: &str) -> Result<String, Error> {
        Err(Error::unsupported("maps"))
    }
    /// `Option<&T>`: variant `index` of `value`, a `&` of the type of `union`; `None` where the
    /// value holds another.
    fn variant(&self, _value: &str, _union: &Union, _index: usize) -> Result<String, Error> {
        Err(Error::unsupported("unions"))
    }
    /// `Option<&T>`: the value of the key no field names that `key`, a `&str`, is.
    fn other(&self, _value: &str, _model: usize, _key: &str) -> Result<String, Error> {
        Err(Error::unsupported("keys no field names"))
    }
}

/// A check's name, and each pattern in it with its table.
type Tables = (String, Vec<(String, Vec<u8>)>);

/// Emits checks over one schema.
pub struct Emitter<'a> {
    pub schema: &'a Schema,
    pub access: &'a dyn Access,
    /// The check being emitted.
    tables: RefCell<Tables>,
    /// The model whose function is being written, until its first value is.
    root: Cell<Option<usize>>,
    /// The functions of recursive models written so far, each with its name.
    extra: RefCell<Vec<(String, String)>>,
    /// Numbers the functions of recursive models.
    ids: Cell<usize>,
}

impl<'a> Emitter<'a> {
    /// Model `model`.
    fn model(&self, model: usize) -> Result<&'a Model, Error> {
        self.schema
            .models
            .get(model)
            .ok_or_else(|| Error::invalid(format!("no model {model}")))
    }

    /// Model `model` and its Rust type.
    fn parts(&self, model: usize) -> Result<(&'a Model, String), Error> {
        let m = self.model(model)?;
        let rust = self.access.rust(self.schema, &Ty::of(Base::Model(model)))?;
        Ok((m, rust))
    }

    /// A model's check, as one function, or as the call of its recursive function.
    fn model_check(&self, model: usize, name: &str) -> Result<String, Error> {
        if self.cyclic(model) {
            let (m, rust) = self.parts(model)?;
            if let Some(call) = Walk::new(self, &m.name).marked("v", model)? {
                return Ok(format!(
                    "pub fn {name}<__R: {STD}::Report>(v: &{rust}, r: &mut __R) {{\n{call}}}\n"
                ));
            }
        }
        self.plain_check(model, name)
    }

    pub(super) fn plain_check(&self, model: usize, name: &str) -> Result<String, Error> {
        let m = self.model(model)?;
        self.root.set(Some(model));
        let root = Ty {
            checks: m.checks.clone(),
            ..Ty::of(Base::Model(model))
        };
        self.function(name, &m.name, &root)
    }

    /// Every condition a value with view `view` must meet: its width's, then each check's.
    fn conds(&self, base: &Base, checks: &[Check], view: &str) -> Result<Vec<String>, Error> {
        let mut out: Vec<String> = scalars::width(base, view).into_iter().collect();
        for check in checks {
            let cond = bounds::cond(check, base, view)
                .or_else(|| text::cond(self, check, base, view))
                .or_else(|| choices::cond(check, base, view))
                .or_else(|| lists::cond(check, base, view))
                .or_else(|| negate::cond(self, check, view))
                .or_else(|| branches::cond(self, check, view))
                .unwrap_or_else(|| Err(Error::unsupported(format!("{check:?} on {base:?}"))))?;
            out.push(cond);
        }
        Ok(out)
    }
}

/// `body` with `bind` bound to what `read`, a `Result`, gives; a value that won't read fails
/// `cast`.
fn readable(at: &At, read: &str, bind: &str, body: &str) -> String {
    format!(
        "match {read} {{ Ok({bind}) => {{ {body}}} Err(_) => {{ {} }} }}\n",
        at.fail("Cast")
    )
}

/// Whether null meets each of `checks`. A choice holds where its literals include null, and a
/// branch check by the branches that admit null. Every other check reads a value that isn't
/// null, so null passes it.
fn holds_of_null(checks: &[Check]) -> bool {
    checks.iter().all(|c| match c {
        Check::OneOf(lits) => lits.contains(&Lit::Null),
        Check::ExactlyOne(subs) => subs.iter().filter(|s| admits_null(s)).count() == 1,
        Check::AtLeastOne(subs) => subs.iter().any(admits_null),
        Check::Conditional {
            when,
            then,
            otherwise,
        } => match admits_null(when) {
            true => then.as_deref().is_none_or(admits_null),
            false => otherwise.as_deref().is_none_or(admits_null),
        },
        _ => true,
    })
}

/// Whether branch `ty` holds of null.
fn admits_null(ty: &Ty) -> bool {
    (ty.nullable || ty.base == Base::Null) && holds_of_null(&ty.checks)
}

/// No condition where `holds`, else one that fails.
fn admits(holds: bool) -> Vec<String> {
    if holds {
        Vec::new()
    } else {
        vec!["false".into()]
    }
}

/// Fails `range` unless every one of `conds` holds.
fn guard(at: &At, conds: &[String]) -> String {
    if conds.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "if !({}) {{ {} }}",
        conds.join(" && "),
        at.fail("Range")
    );
    out
}
