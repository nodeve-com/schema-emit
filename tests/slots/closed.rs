//! A closed model over `allOf` definitions of its own resource lowers alike wherever the resource
//! sits.

use schema_emit::ir::Schema;
use schema_emit::json_schema::lower_defs;
use serde_json::{json, Map, Value};

use super::{anchor, check, filler, items, model};

/// The resource `parcel`, whose `Current` closes a record over `allOf` definitions of its own.
fn tracked() -> Value {
    let fields = json!({ "type": "object", "required": ["source"], "properties": {
        "source": { "type": "string" }, "data": { "type": "string" },
        "headers": { "type": "object", "additionalProperties": { "type": "string" } }
    } });
    let over = |base: &str, required: Value, properties: Value| json!({ "allOf": [{ "$ref": format!("#/$defs/{base}") }], "required": required, "properties": properties });
    let mut entry = over(
        "Val",
        json!(["count"]),
        json!({ "count": { "type": "integer" } }),
    );
    entry["unevaluatedProperties"] = json!(false);
    let mut current = over(
        "Fields",
        json!(["address", "history"]),
        json!({
            "address": { "$dynamicRef": "#address" },
            "value": { "$dynamicRef": "#value" },
            "history": { "type": "array", "items": { "$ref": "#/$defs/Entry" } }
        }),
    );
    current["unevaluatedProperties"] = json!(false);
    json!({
        "$id": "parcel",
        "type": "object",
        "properties": { "wrap": { "$ref": "#/$defs/Current" } },
        "$defs": {
            "Fields": fields,
            "Val": over("Fields", json!(["address"]), json!({ "address": { "type": "string" } })),
            "Entry": entry,
            "Current": current,
            "address": anchor("address", json!({ "type": "object" })),
            "value": { "$dynamicAnchor": "value" }
        }
    })
}

#[test]
fn closed_definitions_over_allof_lower_alike_as_the_root_and_as_an_embedded_resource() {
    let root = lower_defs(&tracked(), &Map::new(), &[]).unwrap().0;
    let book = filler(
        "book-parcel",
        json!({ "address": anchor("address", json!({
        "type": "object", "required": ["topic"], "properties": { "topic": { "type": "string" } }
    })) }),
    );
    let document = json!({ "$id": "https://example.test/root",
        "$defs": { "Parcel": tracked(), "BookParcel": book } });
    let (s, defs) = lower_defs(&document, &Map::new(), &[]).unwrap();
    let items = items(&s);
    assert!(items.contains("pub type BookParcel"), "{items}");
    let find = |s: &Schema, name: &str| s.models.iter().position(|m| m.name == name).unwrap();
    for name in ["Current", "Entry"] {
        let embedded = check(&s, find(&s, name)).unwrap();
        // The root names its slot types apart from its definitions `address` and `value`.
        let alone = check(&root, find(&root, name)).unwrap();
        assert_eq!(
            embedded,
            alone
                .replace("Address_", "Address")
                .replace("Value_", "Value")
        );
    }
    let wrap = check(&s, model(&defs, "Parcel")).unwrap();
    assert!(wrap.contains("Parcel.wrap"), "{wrap}");
}
