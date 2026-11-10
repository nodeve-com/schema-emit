//! Front end: JSON Schema (2020-12) to the IR. A schema states its `type`, or is a `$ref`; each
//! other keyword belongs to one area's module, which lowers it to the IR or refuses it. A keyword
//! no area takes is unsupported, so nothing is silently unchecked.
//!
//! Beyond the standard: `format` names a number's width as OpenAPI's format registry does
//! (`int8` … `uint64`, `float`, `double`); `human-duration` is a duration's text; and the
//! keywords a caller names carry beside the check (`json_schema/notes.rs`).

mod boolean;
mod bounds;
mod branches;
mod choices;
mod combine;
mod defaults;
mod defs;
mod lists;
mod models;
mod negate;
mod notes;
mod others;
mod references;
mod root;
mod scalars;
mod text;
mod tuples;
mod unevaluated;
mod unions;

pub use defs::{lower_defs, lower_ty, Document};
pub use others::branch_names;
pub use root::lower;

use std::collections::HashMap;

use root::union;

use serde_json::{Map, Value};

use crate::ir::{Base, Model, Schema, Ty};
use crate::Error;

/// Lowering in progress: the models so far, and the definitions a `$ref` reads.
pub(crate) struct Lower {
    out: Schema,
    /// The resource each model lowered here sits in, by the model's index.
    homes: Vec<String>,
    defs: Map<String, Value>,
    /// The `$ref`s being followed, innermost last.
    following: Vec<String>,
    /// The document's resources and anchors, and the base URI of the schema being lowered.
    index: references::Index,
    base: String,
    /// The model of each followed `$ref` that something inside it refers back to.
    recursions: HashMap<String, usize>,
    /// Each `$ref` target lowered with nothing that checks beside it, so a definition is one
    /// model however often it is named.
    lowered: HashMap<String, Ty>,
    notes: Vec<String>,
}

/// A keyword being lowered onto the type its schema makes.
pub(crate) struct Key<'a> {
    pub key: &'a str,
    pub value: &'a Value,
    pub ty: &'a mut Ty,
    /// The schema's other keywords, for a keyword that reads its siblings' values.
    pub siblings: &'a Map<String, Value>,
    /// Lowers a subschema the keyword holds.
    pub lower: &'a mut Lower,
}

impl Key<'_> {
    /// The keyword refused: its value isn't one it takes.
    pub fn refuse(&self) -> Done {
        Some(Err(Error::invalid(format!(
            "`{}` of {}",
            self.key, self.value
        ))))
    }
}

/// What an area made of a keyword: `None` where it isn't the area's.
pub(crate) type Done = Option<Result<(), Error>>;

type Keyword = fn(&mut Key) -> Done;

/// Every area's, asked in turn.
const AREAS: [Keyword; 13] = [
    scalars::keyword,
    bounds::keyword,
    text::keyword,
    choices::keyword,
    others::keyword,
    models::keyword,
    lists::keyword,
    combine::keyword,
    negate::keyword,
    branches::keyword,
    unevaluated::keyword,
    defaults::keyword,
    notes::keyword,
];

impl Lower {
    fn new(out: Schema, schema: &Value, shared: &Map<String, Value>, notes: &[&str]) -> Lower {
        let mut defs = shared.clone();
        if let Some(Value::Object(own)) = schema.get("$defs") {
            defs.extend(own.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        Lower {
            out,
            homes: Vec::new(),
            defs,
            following: Vec::new(),
            index: references::index(schema),
            base: references::within(references::ROOT, schema),
            recursions: HashMap::new(),
            lowered: HashMap::new(),
            notes: notes.iter().map(|n| (*n).into()).collect(),
        }
    }

    /// Runs `f` inside the target of a `$ref`: followed, under the base URI around it.
    fn follow<T>(&mut self, t: &references::Target, f: impl FnOnce(&mut Lower) -> T) -> T {
        self.following.push(t.key.clone());
        let outer = std::mem::replace(&mut self.base, t.base.clone());
        let out = f(self);
        self.base = outer;
        self.following.pop();
        out
    }

    /// `schema` as a type named `name` where it makes a model.
    pub(crate) fn ty(&mut self, name: &str, schema: &Value) -> Result<Ty, Error> {
        let outer = self.base.clone();
        self.base = references::within(&outer, schema);
        let ty = self.ty_in(name, schema);
        self.base = outer;
        ty
    }

    /// [`Lower::ty`], the schema's `$id` already the base.
    fn ty_in(&mut self, name: &str, schema: &Value) -> Result<Ty, Error> {
        let s = match schema {
            Value::Object(s) => s,
            Value::Bool(admits) => return Ok(boolean::ty(*admits)),
            _ => return Err(Error::unsupported(format!("{name}: a schema of no object"))),
        };
        if let Some(target) = s.get("$dynamicRef") {
            return self.slot(name, target, s);
        }
        if let Some(target) = s.get("$ref") {
            return self.reference(name, target, s);
        }
        let (kind, nullable) = match s.get("type") {
            Some(Value::String(t)) => (t.as_str(), false),
            Some(Value::Array(ts)) => union(name, ts)?,
            Some(other) => return Err(Error::unsupported(format!("{name}: type {other}"))),
            None => return unions::untyped(self, name, s),
        };
        let base = match kind {
            "object" => models::base(self, name, s)?,
            "array" => {
                let s = unevaluated::fold_items(name, s)?;
                let mut ty = Ty::of(lists::base(self, name, &s)?);
                ty.nullable = nullable;
                return self.keywords(name, &s, ty);
            }
            _ => scalars::base(kind, s).map_err(|e| e.at(name))?,
        };
        let mut ty = Ty::of(base);
        ty.nullable = nullable;
        self.keywords(name, s, ty)
    }

    /// Each keyword of `s` lowered onto `ty`.
    pub(crate) fn keywords(
        &mut self,
        name: &str,
        s: &Map<String, Value>,
        mut ty: Ty,
    ) -> Result<Ty, Error> {
        for (key, value) in s {
            // A boolean schema's only keywords are its `$ref` and definitions, and `allOf`, which
            // meets a typed subschema into it. Any other keyword might apply to a kind it can't read.
            if matches!(ty.base, Base::Any) && !matches!(key.as_str(), "$defs" | "$ref" | "allOf") {
                return Err(
                    Error::unsupported(format!("keyword `{key}` on a boolean schema")).at(name),
                );
            }
            let mut k = Key {
                key,
                value,
                ty: &mut ty,
                siblings: s,
                lower: &mut *self,
            };
            let done = AREAS
                .iter()
                .find_map(|area| area(&mut k))
                .unwrap_or_else(|| Err(Error::unsupported(format!("keyword `{key}`"))));
            done.map_err(|e| e.at(name))?;
        }
        defaults::judge(&ty).map_err(|e| e.at(name))?;
        Ok(ty)
    }

    /// Adds a model, returning its index.
    pub(crate) fn model(&mut self, model: Model) -> usize {
        let index = self.out.models.len();
        self.homes.resize(index, String::new());
        self.homes.push(self.base.clone());
        self.out.models.push(model);
        index
    }

    /// A model's slot to fill once its fields lower, so a model's index precedes its fields'.
    pub(crate) fn reserve(&mut self, name: &str) -> usize {
        self.model(Model {
            name: name.into(),
            ..Model::default()
        })
    }

    pub(crate) fn fill(&mut self, index: usize, model: Model) {
        if let Some(slot) = self.out.models.get_mut(index) {
            *slot = model;
        }
    }
}
