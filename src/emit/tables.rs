//! A check's pattern tables: each a `static` beside it, with a `_load` function that checks them
//! all, for the embedder to call once ahead of any value.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;

use super::std_form::StdForm;
use super::{unions, Emitter, Walk, STD};
use crate::ir::{Ty, Union};
use crate::Error;

impl<'a> Emitter<'a> {
    /// An emitter of checks over the models of `std`, reading the types it writes.
    pub fn new(std: &'a StdForm<'a>) -> Self {
        Emitter {
            schema: std.schema,
            std,
            tables: RefCell::default(),
            root: Cell::default(),
            extra: RefCell::default(),
            ids: Cell::default(),
            fills: RefCell::default(),
        }
    }

    /// The patterns of the last check emitted, each with its table's size in bytes.
    pub fn tables(&self) -> Vec<(String, usize)> {
        let sized = |(source, table): &(String, Vec<u8>)| (source.clone(), table.len());
        self.tables.borrow().1.iter().map(sized).collect()
    }

    /// The static holding pattern `p`'s `table`, once per table.
    pub(super) fn table(&self, p: &str, table: Vec<u8>) -> String {
        let mut state = self.tables.borrow_mut();
        let n = match state.1.iter().position(|(_, t)| *t == table) {
            Some(n) => n,
            None => {
                state.1.push((p.into(), table));
                state.1.len().saturating_sub(1)
            }
        };
        format!("{}_P{n}", state.0)
    }

    /// `pub fn <name>`, checking a `&` of model `model`'s type and reporting to `r`. Each pattern
    /// is a `static <name>_P<n>`; with any, `pub fn <name>_load` checks every table. A model with
    /// slots takes a function per slot after `r`, in the slots' order, checking a value of it. A
    /// model filling slots has no check of its own: the model it fills has, taking each slot's.
    pub fn check(&self, model: usize, name: &str) -> Result<String, Error> {
        let filling = self.schema.filling(model);
        if filling != model {
            return Err(Error::invalid(format!(
                "{name}: model {model} fills model {filling}: check that model, passing each slot's check"
            )));
        }
        (self.std).inside(model, || {
            self.checked(name, || self.model_check(model, name))
        })
    }

    /// `pub fn <name>`, checking a `&` of `union` as [`Emitter::check`] does a model: the variant
    /// the value holds, by that variant's check. Decoding a union calls it to tell a branch whose
    /// check passes from one that merely fits.
    pub fn check_union(&self, union: &Union, name: &str) -> Result<String, Error> {
        self.checked(name, || unions::function(self, union, name))
    }

    /// `pub fn <name>`, checking a `&` of a value of `ty` that isn't null, as [`Emitter::check`]
    /// does a model: one property's value, apart from the model holding it. Its failures' paths
    /// start at `path` where a model's start at its name.
    pub fn check_value(&self, ty: &Ty, path: &str, name: &str) -> Result<String, Error> {
        self.checked(name, || self.function(name, path, ty))
    }

    /// `pub fn <name>`, checking a `&` of a value of `ty`, its failures' paths from `path`.
    pub(super) fn function(&self, name: &str, path: &str, ty: &Ty) -> Result<String, Error> {
        let rust = self.std.rust(&Ty::of(ty.base.clone()));
        let body = Walk::new(self, path).present("v", ty)?;
        let (slots, checks) = self.std.check_slots();
        Ok(format!(
            "#[allow(unused_variables, unused_mut, unused_parens, unreachable_patterns, clippy::all)]\n\
             pub fn {name}<{slots}__R: {STD}::Report>(v: &{rust}, r: &mut __R{checks}) {{\n{body}}}\n"
        ))
    }

    /// The function `body` writes, with each pattern table it holds and their `_load`.
    fn checked(
        &self,
        name: &str,
        body: impl FnOnce() -> Result<String, Error>,
    ) -> Result<String, Error> {
        *self.tables.borrow_mut() = (name.into(), Vec::new());
        self.extra.borrow_mut().clear();
        self.ids.set(0);
        self.root.set(None);
        let mut code = body()?;
        self.extra
            .borrow()
            .iter()
            .for_each(|(_, function)| code.push_str(function));
        let state = self.tables.borrow();
        let mut out = String::new();
        for (n, (_, table)) in state.1.iter().enumerate() {
            let _ = writeln!(
                out,
                "static {name}_P{n}: {STD}::Pattern = {STD}::Pattern::new({});",
                pattern_emit::byte_string(table)
            );
        }
        out.push_str(&code);
        if !state.1.is_empty() {
            let loads: Vec<String> = (0..state.1.len())
                .map(|n| format!("{name}_P{n}.load()?;"))
                .collect();
            let _ = writeln!(
                out,
                "pub fn {name}_load() -> ::core::result::Result<(), {STD}::Unusable> {{ {} Ok(()) }}",
                loads.join(" ")
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_check_names_its_failures_from_the_path_given() {
        let document = serde_json::json!({
            "type": "object",
            "properties": {"total": {"type": "integer", "maximum": 10}},
        });
        let shared = serde_json::Map::new();
        let (schema, root) = crate::json_schema::lower("Record", &document, &shared, &[]).unwrap();
        let name = |_: usize| "Record".to_string();
        let std = StdForm::new(&schema, &name);
        let total = &schema.models[root].fields[0];
        let check = Emitter::new(&std).check_value(&total.ty, "sum", "check_sum");
        let check = check.unwrap();
        assert!(check.contains("pub fn check_sum<"), "{check}");
        assert!(
            check.contains("\"sum\"") && !check.contains("Record"),
            "{check}"
        );
    }
}
