//! Area: references within one document. A `$ref` is a URI reference resolved against the base URI
//! of the schema holding it: the nearest `$id` above, else a root of no address. Its target is the
//! schema a resource's `$id` names, the `$anchor` its fragment names, or the JSON Pointer its
//! fragment holds (`~0`, `~1`, and percent-escapes decoded). A target outside the document is
//! unsupported: nothing here fetches one.
//!
//! A target already being followed is a recursion. The model it makes is wrapped in a model of
//! one field named empty ([`Model::is_value`]), so the type refers to itself through a model and
//! the emitter checks it by a call ([`crate::emit`]).

use percent_encoding::percent_decode_str;
use serde_json::{Map, Value};

use super::notes;
use super::others::counted;
use super::Lower;
use crate::ir::{Base, Ty};
use crate::Error;
use recursion::{noted, only_notes};

mod index;
mod recursion;
mod slots;

use index::join;
pub(super) use index::{index, within, Index};

/// The base of a document with no `$id`.
pub(super) const ROOT: &str = "json-schema:///";

/// A `$ref`'s target.
pub(super) struct Target {
    /// Its address, the same for every `$ref` spelling that reaches it.
    pub key: String,
    pub name: String,
    /// The `$defs` key it is, where the fragment names one.
    pub def: Option<String>,
    pub schema: Value,
    /// The base URI around the schema, before its own `$id`.
    pub base: String,
}

/// The schema at JSON Pointer `pointer` below `schema`, whose base is `inner`, with the base
/// around it.
fn pointer(schema: &Value, outer: &str, inner: &str, pointer: &str) -> Option<(Value, String)> {
    let (mut node, mut outer, mut inner) = (schema, outer.to_string(), inner.to_string());
    for token in pointer.split('/').skip(1) {
        let token = token.replace("~1", "/").replace("~0", "~");
        node = match node {
            Value::Object(o) => o.get(&token)?,
            Value::Array(xs) => xs.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
        outer = inner.clone();
        inner = within(&inner, node);
    }
    Some((node.clone(), outer))
}

/// The schema `target` names, from the base URI around `lower`'s schema in progress.
pub(super) fn resolve(lower: &Lower, name: &str, target: &Value) -> Result<Target, Error> {
    let text = target
        .as_str()
        .ok_or_else(|| Error::invalid(format!("{name}: `$ref` {target}")))?;
    let full = join(&lower.base, text).map_err(|e| e.at(name))?;
    let (doc, raw) = full.split_once('#').unwrap_or((&full, ""));
    let frag = percent_decode_str(raw)
        .decode_utf8()
        .map_err(|_| Error::invalid(format!("{name}: `$ref` {target}")))?;
    let (schema, base) = locate(lower, doc, &frag).ok_or_else(|| {
        if lower.index.at.contains_key(doc) {
            Error::invalid(format!("{name}: no schema at `{text}`"))
        } else {
            Error::unsupported(format!("{name}: `$ref` {target}"))
        }
    })?;
    let last = frag.rsplit('/').next().unwrap_or_default();
    let def = frag.strip_prefix("/$defs/").filter(|d| !d.contains('/'));
    // A resource has one address however a `$ref` spells it: its `$id`.
    let own = schema.get("$id").map(|_| within(&base, &schema));
    let named = own.as_ref().and_then(|id| lower.index.names.get(id));
    Ok(Target {
        key: own.map_or_else(|| format!("{doc}#{raw}"), |id| format!("{id}#")),
        name: match (last.is_empty(), named) {
            (false, _) => last.into(),
            (true, Some(key)) => key.clone(),
            (true, None) => "ref".into(),
        },
        def: def.map(|d| d.replace("~1", "/").replace("~0", "~")),
        schema,
        base,
    })
}

/// The schema at fragment `frag` of document `doc`, with the base URI around it: a pointer into
/// the document, an anchor, or a definition shared beside it.
fn locate(lower: &Lower, doc: &str, frag: &str) -> Option<(Value, String)> {
    let found = if frag.is_empty() || frag.starts_with('/') {
        lower
            .index
            .at
            .get(doc)
            .and_then(|(schema, outer)| pointer(schema, outer, doc, frag))
    } else {
        lower.index.at.get(&format!("{doc}#{frag}")).cloned()
    };
    found.or_else(|| {
        let def = frag.strip_prefix("/$defs/")?;
        Some((lower.defs.get(def)?.clone(), lower.base.clone()))
    })
}

/// The target of a `$ref` that is not already being followed.
pub(super) fn definition(lower: &Lower, name: &str, target: &Value) -> Result<Target, Error> {
    let t = resolve(lower, name, target)?;
    if lower.following.contains(&t.key) {
        return Err(Error::unsupported(format!(
            "{name}: `{}` refers to itself",
            t.name
        )));
    }
    Ok(t)
}

/// Whether a count of properties on a value of `def` is the count the model checks: a closed
/// object, or a type that isn't an object. A `$ref` or a type list might be an open object.
fn counts_fields(def: &Value) -> bool {
    match def.get("type") {
        Some(Value::String(t)) if t.as_str() == "object" => {
            def.get("additionalProperties") == Some(&Value::Bool(false))
        }
        Some(Value::String(_)) => true,
        _ => false,
    }
}

/// Refuses a keyword beside a `$ref` that the definition's type owns, and a count of properties
/// on an open object.
fn refuse_beside(name: &str, s: &Map<String, Value>, t: &Target) -> Result<(), Error> {
    let owned = [
        "type",
        "properties",
        "required",
        "items",
        "dependentRequired",
    ];
    if let Some(k) = owned.iter().find(|k| s.contains_key(**k)) {
        return Err(Error::unsupported(format!("{name}: `{k}` beside `$ref`")));
    }
    if counted(s) && !counts_fields(&t.schema) {
        return Err(Error::unsupported(format!(
            "{name}: a count of properties on `{}`, an open object",
            t.name
        )));
    }
    Ok(())
}

impl Lower {
    /// `s`, holding `$ref` `target`: the schema it names, then `s`'s other keywords on it.
    pub(super) fn reference(
        &mut self,
        name: &str,
        target: &Value,
        s: &Map<String, Value>,
    ) -> Result<Ty, Error> {
        if let Some(any) = notes::metaschema_ref(name, target, s) {
            return any;
        }
        let t = resolve(self, name, target)?;
        if self.following.contains(&t.key) {
            return self.recursion(name, &t, s);
        }
        refuse_beside(name, s, &t)?;
        let plain = only_notes(name, s).is_ok();
        let ty = match self.lowered.get(&t.key).filter(|_| plain) {
            Some(ty) => ty.clone(),
            None => self.target(name, &t, s)?,
        };
        let ty = self.cross(name, &t, (s, ty))?;
        self.keywords(name, s, ty)
    }

    /// The type of `t`, lowered as the `$ref` in `s` reaches it.
    fn target(&mut self, name: &str, t: &Target, s: &Map<String, Value>) -> Result<Ty, Error> {
        let ty = self.follow(t, |l| l.ty(&t.name, &t.schema));
        let wrapper = self.recursions.remove(&t.key);
        let mut ty = ty?;
        // A union a definition declares takes the definition's name, unless one it refers to
        // named it.
        if let (Base::Union(u), Some(def)) = (&mut ty.base, &t.def) {
            u.name.get_or_insert_with(|| def.clone());
        }
        if let Some(model) = wrapper {
            only_notes(name, s)?;
            ty = self.wrap(name, ty, model);
        }
        if noted(s).is_none() || wrapper.is_some() {
            self.lowered.insert(t.key.clone(), ty.clone());
        }
        Ok(ty)
    }
}
