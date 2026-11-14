//! A model with dynamic references as a generic struct: a type parameter per dynamic anchor, named
//! from the anchor in upper camel case, `_` added while a struct holds the name. Inside the model's
//! own items a dynamic reference is its type parameter, and a model with dynamic references takes the
//! type parameters of the same names. Anywhere else such a model holds each anchor's own type, the
//! type parameter's default.

use std::fmt::Write as _;

use super::{idents, unions, StdForm};
use crate::ir::{Base, DynamicRef, Model};

impl StdForm<'_> {
    /// `f`, writing an item of model `model`: its type parameters are in scope.
    pub(crate) fn inside<T>(&self, model: usize, f: impl FnOnce() -> T) -> T {
        let outer = self.within.replace(Some(model));
        let out = f();
        self.within.set(outer);
        out
    }

    /// The type parameters in scope.
    pub(crate) fn scope(&self) -> &[DynamicRef] {
        let model = self.within.get().and_then(|m| self.schema.models.get(m));
        model.map(|m| m.dynamic_refs.as_slice()).unwrap_or_default()
    }

    /// The type parameter of dynamic anchor `anchor`.
    pub(super) fn param(&self, anchor: &str) -> String {
        let mut id = unions::camel(anchor);
        while (0..self.schema.models.len()).any(|m| self.model_name(m) == id) {
            id.push('_');
        }
        id
    }

    /// The type arguments model `m` takes where it is named: none where it shares no type parameter with
    /// the scope, else each type parameter's parameter, or its own type where the scope lacks it.
    pub(super) fn args(&self, m: usize) -> String {
        let dynamic_refs = self.schema.models.get(m).map(|m| m.dynamic_refs.as_slice());
        let dynamic_refs = dynamic_refs.unwrap_or_default();
        let scope = self.scope();
        let shared = |s: &DynamicRef| scope.iter().any(|o| o.name == s.name);
        if !dynamic_refs.iter().any(shared) {
            return String::new();
        }
        let args = dynamic_refs.iter().map(|s| match shared(s) {
            true => self.param(&s.name),
            false => self.ty(&s.ty, None),
        });
        format!("<{}>", args.collect::<Vec<_>>().join(", "))
    }

    /// The type parameters `m`'s struct declares, each defaulting to its type parameter's own type.
    pub(super) fn declared(&self, m: &Model) -> String {
        if m.dynamic_refs.is_empty() {
            return String::new();
        }
        let params = m.dynamic_refs.iter().map(|s| {
            let own = self.within.replace(None);
            let ty = self.ty(&s.ty, None);
            self.within.set(own);
            format!("{} = {ty}", self.param(&s.name))
        });
        format!("<{}>", params.collect::<Vec<_>>().join(", "))
    }

    /// The type parameters in scope as an impl or a function declares them, each bound by
    /// `bound` where it is not empty, between `open` and `close`; nothing where the scope has
    /// none.
    pub(super) fn generics(&self, bound: &str, (open, close): (&str, &str)) -> String {
        let colon = if bound.is_empty() { "" } else { ": " };
        let params =
            (self.scope().iter()).map(|s| format!("{}{colon}{bound}", self.param(&s.name)));
        let params: Vec<String> = params.collect();
        match params.is_empty() {
            true => String::new(),
            false => format!("{open}{}{close}", params.join(", ")),
        }
    }

    /// Whether model `m` is the type a type parameter holds, which has impls of its own, as no alias has.
    pub(super) fn fills_param(&self, m: usize) -> bool {
        let held = |x: &Model| {
            let own = x.dynamic_refs.iter().map(|s| &s.ty);
            let with = x.of.iter().flat_map(|of| &of.with);
            own.chain(with).any(|t| t.base == Base::Model(m))
        };
        self.schema.models.iter().any(held)
    }

    /// A type holding no value, named `name`, for an impl inside a function: its declaration,
    /// its type, and its value. It carries the type parameters in scope, which an item inside a
    /// function can't otherwise name.
    pub(super) fn unit(&self, name: &str) -> (String, String, String) {
        let params: Vec<String> = (self.scope().iter().map(|s| self.param(&s.name))).collect();
        if params.is_empty() {
            let decl = format!("#[derive(Clone, Copy)] struct {name};");
            return (decl, name.into(), name.into());
        }
        let list = params.join(", ");
        let ty = format!("{name}<{list}>");
        let decl = format!(
            "struct {name}<{list}>(::core::marker::PhantomData<fn() -> ({list},)>); \
             impl<{list}> Clone for {ty} {{ fn clone(&self) -> Self {{ *self }} }} \
             impl<{list}> Copy for {ty} {{}}"
        );
        (decl, ty, format!("{name}(::core::marker::PhantomData)"))
    }

    /// The function a check calls for a value of type parameter `anchor`: a parameter of the check.
    pub(crate) fn param_check(&self, anchor: &str) -> String {
        format!("param_{}", idents::snake(anchor))
    }

    /// What a check of the model in scope declares for its type parameters: its type parameters, and a
    /// parameter per type parameter taking the function checking a value of it.
    pub(crate) fn check_params(&self) -> (String, String) {
        let mut fns = String::new();
        for s in self.scope() {
            let _ = write!(
                fns,
                ", {}: impl Fn(&{}, &mut __R)",
                self.param_check(&s.name),
                self.param(&s.name)
            );
        }
        (self.generics("", ("", ", ")), fns)
    }
}
