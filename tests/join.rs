//! A document's own definitions, and a second document joined among a first's.

#![allow(clippy::unwrap_used)]

use schema_emit::json_schema::{defs, join, lower};
use serde_json::json;

#[test]
fn defs_are_the_documents_own_by_name() {
    let document = json!({ "$defs": { "A": { "type": "string" }, "B": { "type": "integer" } } });
    let found = defs(&document);
    assert_eq!(found.keys().collect::<Vec<_>>(), ["A", "B"]);
    assert!(defs(&json!({ "type": "string" })).is_empty());
}

#[test]
fn join_puts_the_roots_and_definitions_among_the_first() {
    let first = json!({ "$defs": { "A": { "type": "string" } } });
    let second = json!({ "type": "integer", "$defs": { "B": { "type": "boolean" } } });
    let joined = join(&first, "Second", &second).unwrap();
    assert_eq!(
        joined,
        json!({ "$defs": {
            "A": { "type": "string" },
            "B": { "type": "boolean" },
            "Second": { "type": "integer" }
        } })
    );
}

#[test]
fn join_without_definitions_adds_them() {
    let joined = join(
        &json!({ "type": "object" }),
        "S",
        &json!({ "type": "string" }),
    )
    .unwrap();
    assert_eq!(
        joined,
        json!({ "type": "object", "$defs": { "S": { "type": "string" } } })
    );
}

#[test]
fn join_names_a_clash() {
    let first = json!({ "$defs": { "Label": { "type": "string" } } });
    let error = join(&first, "Label", &json!({})).unwrap_err();
    assert!(error.message.contains("Label"), "{}", error.message);
    let error = join(&first, "Other", &json!({ "$defs": { "Label": {} } })).unwrap_err();
    assert!(error.message.contains("Label"), "{}", error.message);
}

#[test]
fn defs_serve_as_shared() {
    let library = json!({ "$defs": { "Id": { "type": "string" } } });
    let schema = json!({ "type": "object", "properties": { "id": { "$ref": "#/$defs/Id" } } });
    let (lowered, root) = lower("Root", &schema, &defs(&library), &[]).unwrap();
    assert!(lowered.models.get(root).is_some());
}
