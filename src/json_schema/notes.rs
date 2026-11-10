//! Area: annotations. A keyword the caller names carries its text beside the check in
//! [`crate::ir::Notes`]; the emitter never reads it. This area is asked last, so naming a keyword
//! another area owns, such as `minimum`, never drops its check. Keywords that only annotate
//! (`title`, `description`, `$comment`, `$id`, `$anchor`, `examples`, `deprecated`, `readOnly`,
//! `writeOnly`, and the content keywords `contentMediaType`, `contentEncoding`,
//! `contentSchema`) check nothing, and carry when named. `$schema` checks nothing where it
//! names the standard's 2020-12 metaschema: another might drop a vocabulary, whose keywords then
//! check nothing, so it is unsupported. Any other keyword is unsupported.

use super::{Done, Key};
use crate::ir::Ty;
use crate::Error;

pub(super) fn keyword(k: &mut Key) -> Done {
    if k.key == "$schema" {
        return Some(metaschema(k));
    }
    if k.lower.notes.iter().any(|n| n == k.key) {
        let Some(text) = k.value.as_str() else {
            return k.refuse();
        };
        k.ty.notes.0.insert(k.key.into(), text.into());
        return Some(Ok(()));
    }
    if k.key == "$dynamicAnchor" {
        return Some(k.lower.declared(k.value));
    }
    match k.key {
        "title" | "description" | "$comment" | "examples" | "deprecated" | "readOnly"
        | "writeOnly" | "contentMediaType" | "contentEncoding" | "contentSchema" | "$id"
        | "$anchor" => Some(Ok(())),
        _ => None,
    }
}

/// The 2020-12 metaschema's address.
pub(super) const METASCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

/// Keywords that name or note a schema and check nothing: a schema holding only these admits
/// every value.
const ANNOTATIONS: [&str; 12] = [
    "$dynamicAnchor",
    "description",
    "title",
    "$comment",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "$defs",
    "$schema",
    "$id",
    "$anchor",
];

/// Whether every keyword of `s` but `skip` is an annotation.
pub(super) fn only_annotations(s: &serde_json::Map<String, serde_json::Value>, skip: &str) -> bool {
    s.keys()
        .all(|k| k == skip || ANNOTATIONS.contains(&k.as_str()))
}

/// `s`, holding `$ref` `target`, as any value where `target` is the metaschema: lowering a schema
/// checks it, so the metaschema adds nothing. `None` where it is another target.
pub(super) fn metaschema_ref(
    name: &str,
    target: &serde_json::Value,
    s: &serde_json::Map<String, serde_json::Value>,
) -> Option<Result<Ty, Error>> {
    (target.as_str()?.trim_end_matches('#') == METASCHEMA).then(|| {
        if only_annotations(s, "$ref") {
            return Ok(super::boolean::ty(true));
        }
        let note = "a keyword that checks beside the metaschema `$ref`";
        Err(Error::unsupported(format!("{name}: {note}")))
    })
}

fn metaschema(k: &Key) -> Result<(), Error> {
    match k.value.as_str() {
        Some(id) if id.trim_end_matches('#') == METASCHEMA => Ok(()),
        _ => Err(Error::unsupported(format!("`$schema` {}", k.value))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use serde_json::{json, Map};

    use crate::json_schema::lower;

    #[test]
    fn named_keywords_carry_through_a_reference() {
        let schema = json!({
            "$defs": { "T": { "type": "number", "alpha": "a1", "beta": "b2" } },
            "type": "object",
            "properties": { "t": { "$ref": "#/$defs/T", "maximum": 50 } }
        });
        let (s, m) = lower("Value", &schema, &Map::new(), &["alpha", "beta"]).unwrap();
        let t = &s.models[m].fields[0].ty;
        assert_eq!(t.notes.get("alpha"), Some("a1"));
        assert_eq!(t.notes.get("beta"), Some("b2"));
        assert_eq!(t.notes.get("gamma"), None);
        assert_eq!(t.checks.len(), 1);
    }

    #[test]
    fn an_unnamed_keyword_is_refused() {
        let schema = json!({ "type": "number", "unit": "Cel" });
        assert!(lower("V", &schema, &Map::new(), &[]).is_err());
        assert!(lower("V", &schema, &Map::new(), &["unit"]).is_ok());
    }

    #[test]
    fn a_named_keyword_must_be_text() {
        let schema = json!({ "type": "number", "alpha": 3 });
        assert!(lower("V", &schema, &Map::new(), &["alpha"]).is_err());
    }

    #[test]
    fn naming_a_checking_keyword_keeps_its_check() {
        let schema = json!({ "type": "number", "minimum": 3 });
        let (s, m) = lower("V", &schema, &Map::new(), &["minimum"]).unwrap();
        let t = &s.models[m].fields[0].ty;
        assert_eq!(t.checks.len(), 1);
        assert_eq!(t.notes.get("minimum"), None);
    }

    #[test]
    fn a_named_annotation_carries() {
        let schema = json!({ "type": "string", "title": "T" });
        let (s, m) = lower("V", &schema, &Map::new(), &["title"]).unwrap();
        assert_eq!(s.models[m].fields[0].ty.notes.get("title"), Some("T"));
    }

    #[test]
    fn allof_notes_that_disagree_are_refused() {
        let schema = json!({ "allOf": [
            { "type": "number", "alpha": "x" },
            { "type": "number", "alpha": "y" }
        ] });
        let err = lower("V", &schema, &Map::new(), &["alpha"]).unwrap_err();
        assert!(err.to_string().contains("alpha"), "{err}");
    }
}
