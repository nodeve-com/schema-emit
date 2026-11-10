//! A document's addresses: each resource (a schema with an `$id`), each `$anchor`, and each
//! `$dynamicAnchor` a resource's own `$defs` declare.

use std::collections::BTreeMap;

use iri_string::types::{IriAbsoluteStr, IriReferenceStr};
use serde_json::Value;

use super::ROOT;
use crate::Error;

/// What a document holds by address.
#[derive(Default)]
pub(in crate::json_schema) struct Index {
    /// Each resource and anchor: its schema, and the base URI around it.
    pub at: BTreeMap<String, (Value, String)>,
    /// Each `$dynamicAnchor` on one of a resource's own `$defs`, as `<resource>#<anchor>`: the
    /// `$defs` key declaring it.
    pub dynamic: BTreeMap<String, String>,
    /// The `$defs` key each resource sits under.
    pub names: BTreeMap<String, String>,
}

/// `reference` against `base`, as a string with its fragment kept.
pub(super) fn join(base: &str, reference: &str) -> Result<String, Error> {
    let bad =
        |e: &dyn std::fmt::Display| Error::invalid(format!("`{reference}` against `{base}`: {e}"));
    let r = IriReferenceStr::new(reference).map_err(|e| bad(&e))?;
    let b = IriAbsoluteStr::new(base).map_err(|e| bad(&e))?;
    Ok(r.resolve_against(b).to_string())
}

/// The base a schema's `$id` makes inside `outer`, or `outer` where it has none.
pub(in crate::json_schema) fn within(outer: &str, schema: &Value) -> String {
    let id = schema.get("$id").and_then(Value::as_str);
    let joined = id.and_then(|id| join(outer, id).ok());
    let joined = joined.unwrap_or_else(|| outer.into());
    joined.split('#').next().unwrap_or_default().into()
}

/// Every resource, `$anchor`, and declared `$dynamicAnchor` in `root`, and `root` itself.
pub(in crate::json_schema) fn index(root: &Value) -> Index {
    let mut out = Index::default();
    out.walk(root, ROOT, None);
    out.declared(root, &within(ROOT, root));
    out.at
        .entry(ROOT.into())
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

impl Index {
    /// `schema` and each subschema below it, `schema` under `$defs` key `def` where it is.
    fn walk(&mut self, schema: &Value, outer: &str, def: Option<&str>) {
        let Value::Object(s) = schema else {
            return;
        };
        let inner = within(outer, schema);
        if s.contains_key("$id") {
            self.at
                .entry(inner.clone())
                .or_insert_with(|| (schema.clone(), outer.into()));
            if let Some(def) = def {
                self.names.entry(inner.clone()).or_insert(def.into());
            }
            self.declared(schema, &inner);
        }
        if let Some(anchor) = s.get("$anchor").and_then(Value::as_str) {
            self.at
                .entry(format!("{inner}#{anchor}"))
                .or_insert_with(|| (schema.clone(), outer.into()));
        }
        for (key, value) in s {
            match (key.as_str(), value) {
                ("$defs", Value::Object(defs)) => {
                    (defs.iter()).for_each(|(k, sub)| self.walk(sub, &inner, Some(k)));
                }
                (key, Value::Object(m)) if MANY.contains(&key) => {
                    m.values().for_each(|sub| self.walk(sub, &inner, None));
                }
                (key, Value::Array(xs)) if MANY.contains(&key) => {
                    xs.iter().for_each(|sub| self.walk(sub, &inner, None));
                }
                (key, _) if ONE.contains(&key) => self.walk(value, &inner, None),
                _ => {}
            }
        }
    }

    /// Each `$dynamicAnchor` the `$defs` of `resource`, whose address is `id`, declare.
    fn declared(&mut self, resource: &Value, id: &str) {
        let Some(Value::Object(defs)) = resource.get("$defs") else {
            return;
        };
        for (key, def) in defs {
            let anchor = def.get("$dynamicAnchor").and_then(Value::as_str);
            if let Some(anchor) = anchor.filter(|_| def.get("$id").is_none()) {
                self.dynamic
                    .entry(format!("{id}#{anchor}"))
                    .or_insert(key.clone());
            }
        }
    }
}
