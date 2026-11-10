//! Slots. A `$dynamicRef: "#<name>"` alone in its schema is a slot of the model holding it, where
//! one of its resource's own `$defs` declares `$dynamicAnchor: "<name>"`: the slot's own type. A
//! model holding a model of its resource with slots has those slots too.
//!
//! A `$ref` into another resource fills the slots its own resource declares anchors for, making
//! a model that is the target with those types ([`Filled`]). It fills from its resource's root
//! only, so the anchor in force is the one JSON Schema's dynamic scope gives. A slot filled or
//! left as its own type stays so: a `$ref` from a resource declaring its anchor is refused. The
//! filled model's type keeps the checks the target's carries.

use serde_json::{json, Map, Value};

use super::recursion::noted;
use super::{resolve, within, Target};
use crate::ir::{Base, Model, Ty};
use crate::json_schema::{notes, Lower};
use crate::Error;

/// The slot names a type reaches: those open in its resource, and those already filled or left
/// as their own type.
#[derive(Default)]
struct Reach {
    open: Vec<String>,
    sealed: Vec<String>,
    seen: Vec<(usize, bool)>,
}

impl Lower {
    /// Whether the resource being lowered declares anchor `slot`.
    fn declares(&self, slot: &str) -> bool {
        let key = format!("{}#{slot}", self.base);
        self.index.dynamic.contains_key(&key)
    }

    /// Keyword `$dynamicAnchor` of `value`: one of its resource's own `$defs` declares it.
    pub(in crate::json_schema) fn declared(&self, value: &Value) -> Result<(), Error> {
        match value.as_str() {
            Some(anchor) if self.declares(anchor) => Ok(()),
            _ => Err(Error::unsupported(format!(
                "`$dynamicAnchor` {value} outside its resource's own `$defs`"
            ))),
        }
    }

    /// `s`, holding `$dynamicRef` `target`: the slot it names.
    pub(in crate::json_schema) fn slot(
        &mut self,
        name: &str,
        target: &Value,
        s: &Map<String, Value>,
    ) -> Result<Ty, Error> {
        let refuse =
            |why: &str| Error::unsupported(format!("{name}: `$dynamicRef` {target} {why}"));
        if !notes::only_annotations(s, "$dynamicRef") {
            return Err(refuse("beside another keyword"));
        }
        let text = target.as_str().unwrap_or_default();
        let Some(slot) = text.strip_prefix('#').filter(|n| !n.starts_with('/')) else {
            return Err(refuse("names another document"));
        };
        if !self.declares(slot) {
            return Err(refuse("names no `$dynamicAnchor` its resource declares"));
        }
        let home = self.base.clone();
        self.anchor(name, &home, slot)?;
        Ok(Ty::of(Base::Slot(slot.into())))
    }

    /// The type of the schema declaring anchor `slot` in resource `home`.
    fn anchor(&mut self, name: &str, home: &str, slot: &str) -> Result<Ty, Error> {
        let Some(def) = self.index.dynamic.get(&format!("{home}#{slot}")) else {
            return Err(Error::unsupported(format!(
                "{name}: slot `{slot}`, which its resource declares no `$dynamicAnchor` for"
            )));
        };
        let def = def.clone();
        let pointer = format!("#/$defs/{}", def.replace('~', "~0").replace('/', "~1"));
        let outer = std::mem::replace(&mut self.base, home.into());
        let found = resolve(self, name, &json!(pointer));
        self.base = outer;
        let mut t = found?;
        if let Some(ty) = self.lowered.get(&t.key) {
            return Ok(ty.clone());
        }
        if self.following.contains(&t.key) {
            return Err(Error::unsupported(format!(
                "{name}: slot `{slot}` holds itself"
            )));
        }
        t.name = match self.index.names.get(home) {
            Some(resource) => format!("{resource}.{def}"),
            None => def,
        };
        let ty = self.follow(&t, |l| l.ty(&t.name, &t.schema))?;
        let reach = self.reach(&ty, home);
        if self.recursions.contains_key(&t.key)
            || !(reach.open.is_empty() && reach.sealed.is_empty())
        {
            return Err(Error::unsupported(format!(
                "{name}: slot `{slot}`'s own schema reaches a slot or itself"
            )));
        }
        let ty = self.own_type(&t.name, ty);
        self.lowered.insert(t.key, ty.clone());
        Ok(ty)
    }

    /// `ty` as a type a slot holds: an object's model, else a model of the one value, named
    /// `name`, so each has the decode, encode, and check a model with the slot calls.
    fn own_type(&mut self, name: &str, ty: Ty) -> Ty {
        if matches!(ty.base, Base::Model(_)) && !ty.nullable && ty.checks.is_empty() {
            return ty;
        }
        Ty::of(Base::Model(self.model(Model::value(name, ty))))
    }

    /// The slot names `ty`, a type of resource `home`, reaches.
    fn reach(&self, ty: &Ty, home: &str) -> Reach {
        let mut out = Reach::default();
        self.reach_in(ty, Some(home), &mut out);
        out.open.sort();
        out.open.dedup();
        out
    }

    /// [`Lower::reach`] into `out`, `home` the resource whose slots are open, where any is.
    fn reach_in(&self, ty: &Ty, home: Option<&str>, out: &mut Reach) {
        match &ty.base {
            Base::Slot(n) if home.is_some() => out.open.push(n.clone()),
            Base::Slot(n) => out.sealed.push(n.clone()),
            Base::Model(m) if !out.seen.contains(&(*m, home.is_some())) => {
                out.seen.push((*m, home.is_some()));
                let inner = home.filter(|h| self.homes.get(*m).is_some_and(|own| own == h));
                self.reach_model(*m, inner, out);
            }
            _ => {}
        }
        let held = ty.checks.iter().flat_map(|c| c.tys());
        (ty.parts().into_iter().chain(held)).for_each(|t| self.reach_in(t, home, out));
    }

    /// [`Lower::reach_in`] of everything model `m` holds.
    fn reach_model(&self, m: usize, home: Option<&str>, out: &mut Reach) {
        let Some(model) = self.out.models.get(m) else {
            return;
        };
        if let Some(of) = &model.of {
            of.with.iter().for_each(|t| self.reach_in(t, None, out));
            self.reach_in(&Ty::of(Base::Model(of.model)), None, out);
        }
        let fields = model.fields.iter().map(|f| &f.ty);
        let held = model.checks.iter().flat_map(|c| c.tys());
        (fields.chain(&model.others).chain(held)).for_each(|t| self.reach_in(t, home, out));
    }

    /// `ty`, the target `t` of the `$ref` in `s`, as the schema holding the `$ref` takes it: the
    /// same in `t`'s own resource, else its slots filled where this resource declares them.
    pub(super) fn cross(
        &mut self,
        name: &str,
        t: &Target,
        (s, ty): (&Map<String, Value>, Ty),
    ) -> Result<Ty, Error> {
        let home = within(&t.base, &t.schema);
        if home == self.base {
            return Ok(ty);
        }
        let reach = self.reach(&ty, &home);
        let model = match self.filler(&reach, (s, &ty)) {
            Ok(Some(model)) => model,
            Ok(None) => return Ok(ty),
            Err((n, why)) => {
                let target = &t.name;
                let message = format!("{name}: slot `{n}` of `{target}` is filled {why}");
                return Err(Error::unsupported(message));
            }
        };
        let mut with = Vec::new();
        for n in &reach.open {
            let from = if self.declares(n) {
                self.base.clone()
            } else {
                home.clone()
            };
            with.push(self.anchor(name, &from, n)?);
        }
        let base = Base::Model(self.filled(name, model, with));
        Ok(Ty { base, ..ty })
    }

    /// The model the `$ref` in `s` fills, `ty` its target's type reaching `reach`: `None` where
    /// this resource declares none of its slots, and the slot and why where it can't fill one.
    fn filler(
        &self,
        reach: &Reach,
        (s, ty): (&Map<String, Value>, &Ty),
    ) -> Result<Option<usize>, (String, &'static str)> {
        if let Some(n) = reach.sealed.iter().find(|n| self.declares(n)) {
            let why = "before this resource's `$dynamicAnchor` reaches it";
            return Err((n.clone(), why));
        }
        let Some(first) = reach.open.iter().find(|n| self.declares(n)) else {
            return Ok(None);
        };
        let root = self.index.at.get(&self.base).and_then(|r| r.0.as_object());
        if root != Some(s) {
            return Err((first.clone(), "away from the root of the schema filling it"));
        }
        match (&ty.base, ty.nullable, noted(s)) {
            (Base::Model(model), false, None) => Ok(Some(*model)),
            _ => Err((first.clone(), "beside a keyword that checks")),
        }
    }
}

mod settle;
