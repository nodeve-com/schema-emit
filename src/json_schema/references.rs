//! Area: references within one document. A `$ref` is a URI reference resolved against the base URI
//! of the schema holding it: the nearest `$id` above, else a root of no address. Its target is the
//! schema a resource's `$id` names, the `$anchor` its fragment names, or the JSON Pointer its
//! fragment holds (`~0`, `~1`, and percent-escapes decoded). A target outside the document is
//! unsupported: nothing here fetches one.
//!
//! A target already being followed is a recursion. The model it makes is wrapped in a model of
//! one field named empty ([`Model::is_value`]), so the type refers to itself through a model and
//! the emitter checks it by a call ([`crate::emit`]).

use std::collections::BTreeMap;

use iri_string::types::{IriAbsoluteStr, IriReferenceStr};
use percent_encoding::percent_decode_str;
use serde_json::{Map, Value};

use super::notes;
use super::others::counted;
use super::Lower;
use crate::ir::{Base, Field, Model, Ty};
use crate::Error;
use recursion::{only_notes, recursion};

mod recursion;

/// The base of a document with no `$id`.
pub(super) const ROOT: &str = "json-schema:///";

/// Each resource and anchor of a document by address: its schema, and the base URI around it.
pub(super) type Index = BTreeMap<String, (Value, String)>;

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

/// `reference` against `base`, as a string with its fragment kept.
fn join(base: &str, reference: &str) -> Result<String, Error> {
    let bad =
        |e: &dyn std::fmt::Display| Error::invalid(format!("`{reference}` against `{base}`: {e}"));
    let r = IriReferenceStr::new(reference).map_err(|e| bad(&e))?;
    let b = IriAbsoluteStr::new(base).map_err(|e| bad(&e))?;
    Ok(r.resolve_against(b).to_string())
}

/// The base a schema's `$id` makes inside `outer`, or `outer` where it has none.
pub(super) fn within(outer: &str, schema: &Value) -> String {
    let id = schema.get("$id").and_then(Value::as_str);
    let joined = id.and_then(|id| join(outer, id).ok());
    let joined = joined.unwrap_or_else(|| outer.into());
    joined.split('#').next().unwrap_or_default().into()
}

/// Every resource (a schema with an `$id`) and `$anchor` in `root`, and `root` itself.
pub(super) fn index(root: &Value) -> Index {
    let mut out = Index::new();
    walk(root, ROOT, &mut out);
    out.entry(ROOT.into())
        .or_insert_with(|| (root.clone(), ROOT.into()));
    out
}

/// The keywords whose value is one subschema, and those whose value is a map or a list of them.
const ONE: [&str; 11] = [
    "items",
    "additionalProperties",
    "contains",
    "not",
    "if",
    "then",
    "else",
    "propertyNames",
    "unevaluatedItems",
    "unevaluatedProperties",
    "contentSchema",
];
const MANY: [&str; 8] = [
    "$defs",
    "properties",
    "patternProperties",
    "dependentSchemas",
    "prefixItems",
    "allOf",
    "anyOf",
    "oneOf",
];

fn walk(schema: &Value, outer: &str, out: &mut Index) {
    let Value::Object(s) = schema else {
        return;
    };
    let inner = within(outer, schema);
    if s.contains_key("$id") {
        out.entry(inner.clone())
            .or_insert_with(|| (schema.clone(), outer.into()));
    }
    if let Some(anchor) = s.get("$anchor").and_then(Value::as_str) {
        out.entry(format!("{inner}#{anchor}"))
            .or_insert_with(|| (schema.clone(), outer.into()));
    }
    for (key, value) in s {
        let key = key.as_str();
        if ONE.contains(&key) {
            walk(value, &inner, out);
        } else if MANY.contains(&key) {
            let subs: Vec<&Value> = match value {
                Value::Object(m) => m.values().collect(),
                Value::Array(xs) => xs.iter().collect(),
                _ => Vec::new(),
            };
            subs.into_iter().for_each(|sub| walk(sub, &inner, out));
        }
    }
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
    let outside = || Error::unsupported(format!("{name}: `$ref` {target}"));
    let full = join(&lower.base, text).map_err(|e| e.at(name))?;
    let (doc, raw) = full.split_once('#').unwrap_or((&full, ""));
    let frag = percent_decode_str(raw)
        .decode_utf8()
        .map_err(|_| Error::invalid(format!("{name}: `$ref` {target}")))?;
    let key = format!("{doc}#{raw}");
    let found = if frag.is_empty() || frag.starts_with('/') {
        lower
            .index
            .get(doc)
            .and_then(|(schema, outer)| pointer(schema, outer, doc, &frag))
    } else {
        lower.index.get(&format!("{doc}#{frag}")).cloned()
    };
    // A definition shared beside the document.
    let shared = || {
        let def = frag.strip_prefix("/$defs/")?;
        Some((lower.defs.get(def)?.clone(), lower.base.clone()))
    };
    let (schema, base) =
        found
            .or_else(shared)
            .ok_or_else(|| match lower.index.contains_key(doc) {
                true => Error::invalid(format!("{name}: no schema at `{text}`")),
                false => outside(),
            })?;
    let last = frag.rsplit('/').next().unwrap_or_default();
    let def = frag.strip_prefix("/$defs/").filter(|d| !d.contains('/'));
    Ok(Target {
        key,
        name: if last.is_empty() {
            "ref".into()
        } else {
            last.into()
        },
        def: def.map(|d| d.replace("~1", "/").replace("~0", "~")),
        schema,
        base,
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

/// `s`, holding `$ref` `target`: the schema it names, then `s`'s other keywords on it.
pub(super) fn reference(
    lower: &mut Lower,
    name: &str,
    target: &Value,
    s: &Map<String, Value>,
) -> Result<Ty, Error> {
    if let Some(any) = notes::metaschema_ref(name, target, s) {
        return any;
    }
    let t = resolve(lower, name, target)?;
    if lower.following.contains(&t.key) {
        return recursion(lower, name, &t, s);
    }
    if let Some(k) = [
        "type",
        "properties",
        "required",
        "items",
        "dependentRequired",
    ]
    .iter()
    .find(|k| s.contains_key(**k))
    {
        return Err(Error::unsupported(format!("{name}: `{k}` beside `$ref`")));
    }
    if counted(s) && !counts_fields(&t.schema) {
        return Err(Error::unsupported(format!(
            "{name}: a count of properties on `{}`, an open object",
            t.name
        )));
    }
    let plain = only_notes(name, s).is_ok();
    if let Some(ty) = lower.lowered.get(&t.key).filter(|_| plain) {
        return Ok(ty.clone());
    }
    let ty = lower.follow(&t, |l| l.ty(&t.name, &t.schema));
    let wrapper = lower.recursions.remove(&t.key);
    let mut ty = ty?;
    // A union a definition declares takes the definition's name, unless one it refers to named it.
    if let (Base::Union(u), Some(def)) = (&mut ty.base, &t.def) {
        u.name.get_or_insert_with(|| def.clone());
    }
    let Some(model) = wrapper else {
        let ty = lower.keywords(name, s, ty)?;
        if plain {
            lower.lowered.insert(t.key, ty.clone());
        }
        return Ok(ty);
    };
    only_notes(name, s)?;
    let field = Field {
        name: String::new(),
        ty,
        required: true,
    };
    let wrapped = Model {
        name: name.into(),
        fields: vec![field],
        ..Model::default()
    };
    lower.fill(model, wrapped);
    let ty = Ty::of(Base::Model(model));
    lower.lowered.insert(t.key, ty.clone());
    Ok(ty)
}
