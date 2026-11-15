//! A schema with every dynamic reference filled: each model holding the fields of the struct it is,
//! each dynamic reference the type filling it at any depth.

use super::{Base, Check, Filled, Model, Schema, Ty};

/// A walk filling a schema's dynamic references.
struct Filling<'a> {
    of: &'a Schema,
    out: Schema,
    /// Each model of `out`, by index: the model whose struct it is, and what that struct's dynamic
    /// references hold there, in its dynamic references' order.
    made: Vec<(usize, Vec<Ty>)>,
}

impl Schema {
    /// This schema with every model filling nothing: each dynamic reference, at any depth, the type
    /// filling it. A model keeps its index and holds the fields of the struct it is, so a filler holds
    /// the fields of the model it fills, and keeps its `of`. A model with dynamic references a field
    /// holds under other types than its own comes after the schema's models, its `of` naming its
    /// struct, so [`Schema::filling`] of any model gives the model whose struct holds it.
    pub fn filled(&self) -> Schema {
        let made = self.models.iter().enumerate().map(|(i, m)| match &m.of {
            Some(of) => (of.model, of.with.clone()),
            None => (i, m.dynamic_refs.iter().map(|d| d.ty.clone()).collect()),
        });
        let mut f = Filling {
            of: self,
            out: self.clone(),
            made: made.collect(),
        };
        let mut k = 0;
        while let Some((model, with)) = f.made.get(k).cloned() {
            let built = f.model(k, model, &with);
            if let Some(slot) = f.out.models.get_mut(k) {
                *slot = built;
            }
            k = k.saturating_add(1);
        }
        f.out
    }
}

impl Filling<'_> {
    /// Model `k` of the result: the struct of model `model`, its dynamic references holding `with`.
    fn model(&mut self, k: usize, model: usize, with: &[Ty]) -> Model {
        let Some(own) = self.of.models.get(model) else {
            return Model::default();
        };
        let names = own.dynamic_refs.iter().map(|d| d.name.clone());
        let scope: Vec<(String, Ty)> = names.zip(with.iter().cloned()).collect();
        let mut out = own.clone();
        for f in &mut out.fields {
            self.ty(&mut f.ty, &scope);
        }
        if let Some(others) = &mut out.others {
            self.ty(others, &scope);
        }
        for c in &mut out.checks {
            self.check(c, &scope);
        }
        out.dynamic_refs = Vec::new();
        out.of = match self.of.models.get(k) {
            Some(m) => {
                out.name.clone_from(&m.name);
                m.of.clone()
            }
            None => Some(Filled {
                model,
                with: with.to_vec(),
            }),
        };
        out
    }

    /// `ty` with each dynamic reference the type `scope` gives it, and each model the one of the result
    /// holding it under `scope`.
    fn ty(&mut self, ty: &mut Ty, scope: &[(String, Ty)]) {
        match &mut ty.base {
            Base::DynamicRef(n) => {
                if let Some((_, fill)) = scope.iter().find(|(s, _)| s == n) {
                    let mut fill = fill.clone();
                    self.ty(&mut fill, &[]);
                    *ty = fill;
                    return;
                }
            }
            Base::Model(m) => *m = self.instance(*m, scope),
            Base::List(t) | Base::Map(t) => self.ty(t, scope),
            Base::Tuple { prefix, rest } => {
                prefix.iter_mut().for_each(|t| self.ty(t, scope));
                if let Some(t) = rest {
                    self.ty(t, scope);
                }
            }
            Base::Union(u) => u.variants.iter_mut().for_each(|t| self.ty(t, scope)),
            _ => {}
        }
        for c in &mut ty.checks {
            self.check(c, scope);
        }
    }

    /// `check` with each type it holds filled as [`Filling::ty`] fills it.
    fn check(&mut self, check: &mut Check, scope: &[(String, Ty)]) {
        check.tys_mut().into_iter().for_each(|t| self.ty(t, scope));
    }

    /// The model of the result that model `m` is under `scope`: a filler or a model without dynamic
    /// references is itself; one with them holds what `scope` gives each, else its own type.
    fn instance(&mut self, m: usize, scope: &[(String, Ty)]) -> usize {
        let Some(model) = self.of.models.get(m).filter(|g| g.of.is_none()) else {
            return m;
        };
        let with: Vec<Ty> = (model.dynamic_refs.iter())
            .map(|d| {
                let filled = scope.iter().find(|(n, _)| *n == d.name);
                filled.map_or_else(|| d.ty.clone(), |(_, t)| t.clone())
            })
            .collect();
        if let Some(k) = (self.made.iter()).position(|(g, w)| *g == m && *w == with) {
            return k;
        }
        self.made.push((m, with));
        self.out.models.push(Model::default());
        self.made.len().saturating_sub(1)
    }
}
