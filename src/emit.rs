//! The IR as Rust: one function per model checking a value of it, each field's checks inline with
//! its path and bounds as literals, failures sent to a `schema_emit_runtime::Report`. A value
//! reads as the std form's ([`std_form::StdForm`]); every name the code writes is generated.
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
mod slots;
pub mod std_form;
mod tables;
mod text;
mod tuples;
mod unevaluated;
mod unions;
mod walk;

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;

use crate::ir::{Base, Check, Lit, Model, Schema, Ty};
use crate::Error;
use at::At;
use std_form::StdForm;
use walk::Walk;

/// The runtime crate, as the emitted code names it, under a name no model takes: the embedder
/// writes `use ::schema_emit_runtime as __s;` beside it.
pub const STD: &str = "__s";

/// A check's name, and each pattern in it with its table.
type Tables = (String, Vec<(String, Vec<u8>)>);

/// Emits checks over one schema.
pub struct Emitter<'a> {
    pub schema: &'a Schema,
    /// The types the checks read.
    pub std: &'a StdForm<'a>,
    /// The check being emitted.
    tables: RefCell<Tables>,
    /// The model whose function is being written, until its first value is.
    root: Cell<Option<usize>>,
    /// The functions of recursive models written so far, each with its name.
    extra: RefCell<Vec<(String, String)>>,
    /// Numbers the functions of recursive models.
    ids: Cell<usize>,
    /// What each slot holds in the model being walked, innermost last (`slots.rs`).
    fills: RefCell<Vec<(String, Ty)>>,
}

impl<'a> Emitter<'a> {
    /// Model `model`, or the model it fills: the one with its fields.
    fn model(&self, model: usize) -> Result<&'a Model, Error> {
        self.schema
            .models
            .get(self.schema.filling(model))
            .ok_or_else(|| Error::invalid(format!("no model {model}")))
    }

    /// Model `model` and its Rust type.
    fn parts(&self, model: usize) -> Result<(&'a Model, String), Error> {
        let m = self.model(model)?;
        let rust = self.std.rust(&Ty::of(Base::Model(model)));
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
