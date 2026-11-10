//! What is no slot, and so doesn't lower: each refusal, with its reason.

use schema_emit::json_schema::{lower, lower_defs};
use serde_json::{json, Map, Value};

use crate::{anchor, document};

/// Why `document`, changed by `change`, doesn't lower.
fn refused(change: impl FnOnce(&mut Value)) -> String {
    let mut document = document();
    change(&mut document);
    let err = lower_defs(&document, &Map::new(), &[]).unwrap_err();
    assert!(err.unsupported, "{err}");
    err.message
}

#[test]
fn a_dynamic_reference_that_is_no_slot_is_refused() {
    let parcel = |d: &mut Value, at: &str, to: Value| d["$defs"]["Parcel"]["properties"][at] = to;
    let beside = refused(|d| {
        parcel(
            d,
            "label",
            json!({ "$dynamicRef": "#label", "type": "string" }),
        )
    });
    assert!(beside.contains("beside another keyword"), "{beside}");
    let other = refused(|d| parcel(d, "label", json!({ "$dynamicRef": "toy-parcel#label" })));
    assert!(other.contains("names another document"), "{other}");
    let none = refused(|d| parcel(d, "label", json!({ "$dynamicRef": "#absent" })));
    assert!(none.contains("names no `$dynamicAnchor`"), "{none}");
    let stray = refused(|d| parcel(d, "id", json!({ "type": "string", "$dynamicAnchor": "id" })));
    assert!(
        stray.contains("outside its resource's own `$defs`"),
        "{stray}"
    );
}

#[test]
fn a_slot_no_type_parameter_can_stand_for_is_refused() {
    let slot = json!({ "$dynamicRef": "#contents" });
    let parcel = |d: &mut Value, to: Value| d["$defs"]["Parcel"]["properties"]["label"] = to;
    let union = refused(|d| parcel(d, json!({ "anyOf": [slot, { "type": "null" }] })));
    assert!(
        union.contains("a slot outside an object's property"),
        "{union}"
    );
    let own = refused(|d| {
        d["$defs"]["Parcel"]["$defs"]["label"] =
            anchor("label", json!({ "type": "array", "items": slot }));
    });
    assert!(own.contains("own schema reaches a slot"), "{own}");
    let itself = refused(|d| d["$defs"]["Parcel"]["properties"]["next"] = json!({ "$ref": "#" }));
    assert!(itself.contains("a slot outside"), "{itself}");
    let root = json!({ "$dynamicRef": "#x", "$defs": { "x": anchor("x", json!({})) } });
    let err = lower("Root", &root, &Map::new(), &[]).unwrap_err();
    assert!(
        err.unsupported && err.message.contains("a slot outside"),
        "{err}"
    );
}

#[test]
fn a_slot_filled_twice_or_away_from_a_root_is_refused() {
    let again = refused(|d| {
        d["$defs"]["Again"] = json!({ "$id": "again", "$ref": "book-parcel", "$defs": {
            "contents": anchor("contents", json!({ "type": "string" }))
        } });
    });
    assert!(
        again.contains("is filled before this resource's"),
        "{again}"
    );
    let away = refused(|d| {
        d["$defs"]["Away"] = json!({ "$id": "away", "type": "object",
            "properties": { "p": { "$ref": "parcel" } },
            "$defs": { "contents": anchor("contents", json!({ "type": "string" })) }
        });
    });
    assert!(away.contains("away from the root"), "{away}");
    let beside = refused(|d| d["$defs"]["BookParcel"]["propertyNames"] = json!({ "maxLength": 9 }));
    assert!(beside.contains("beside a keyword that checks"), "{beside}");
}
