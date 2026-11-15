//! Fields a program fills later: required, yet held as `Option<T>`, decoded and checked absent
//! without a failure. One with a default stays `T`. A path that names no field fails.

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::ir::Schema;
use schema_emit::json_schema::lower;
use serde_json::{json, Map, Value};

/// A closed object requiring four fields: a bounded integer, a defaulted one, and a model
/// requiring a short text.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "total", "n", "place"],
        "properties": {
            "id": { "type": "integer" },
            "total": { "type": "integer", "minimum": 0 },
            "n": { "type": "integer", "format": "int8", "default": 3 },
            "place": { "$ref": "#/$defs/Place" }
        },
        "additionalProperties": false,
        "$defs": {
            "Place": {
                "type": "object",
                "required": ["key"],
                "properties": { "key": { "type": "string", "maxLength": 3 } },
                "additionalProperties": false
            }
        }
    })
}

/// The fields the program fills later: one of the root's, one with a default, and one through
/// a `$ref`.
pub const PATHS: &[&str] = &["total", "n", "place.key"];

/// Absent fields filled later decode and check without a failure; present ones decode, fail,
/// and are written as any field is.
pub const CHECK: &str = r##"
    let empty = v(r#"{"id":1,"place":{}}"#);
    assert_eq!(empty, Root { id: 1, total: None, n: 3, place: Place { key: None } });
    assert!(fails(&empty).is_empty());
    assert_eq!(out(&empty), r#"{"id":1,"n":3,"place":{}}"#);
    let full = v(r#"{"id":1,"total":-1,"n":4,"place":{"key":"long"}}"#);
    assert_eq!(full, Root { id: 1, total: Some(-1), n: 4, place: Place { key: Some("long".into()) } });
    assert_eq!(fails(&full), ["Root.total: range", "Root.place.key: range"]);
    assert_eq!(v(&out(&full)), full);
    assert_eq!(e(r#"{"id":1,"total":"x","place":{}}"#), "Root.total: expected an integer");
    assert_eq!(e(r#"{"id":1,"place":{"key":2}}"#), "Root.place.key: expected text");
    assert_eq!(e(r#"{"total":1,"place":{}}"#), "Root.id: a required field is absent");
    for value in [&empty, &full] {
        let bytes = ::postcard::to_allocvec(value).unwrap();
        assert_eq!(&::postcard::from_bytes::<Root>(&bytes).unwrap(), value);
    }
"##;

/// [`schema`] lowered, and its root's index.
fn lowered() -> (Schema, usize) {
    lower("Root", &schema(), &Map::new(), &[]).unwrap()
}

#[test]
fn a_field_filled_later_can_be_empty_and_stays_required() {
    let (s, root) = lowered();
    let named = names(&s);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let std = StdForm::new(&s, &name).with_empty(root, PATHS).unwrap();
    let fields = &s.models[root].fields;
    let empty: Vec<(&str, bool)> = (fields.iter().enumerate())
        .map(|(k, f)| (f.name.as_str(), std.can_be_empty(root, k)))
        .collect();
    assert_eq!(
        empty,
        [
            ("id", false),
            ("total", true),
            ("n", false),
            ("place", false)
        ]
    );
    assert!(fields.iter().all(|f| f.required));
    assert!(!std.can_be_empty(root, fields.len()));
    let types = std.types();
    assert!(
        types.contains(
            "pub struct Place { pub key: ::core::option::Option<::std::string::String>, }"
        ),
        "{types}"
    );
}

#[test]
fn a_path_naming_no_field_fails_naming_the_path() {
    let (s, root) = lowered();
    let name = |_: usize| String::from("M");
    for path in [
        "nope",
        "place.nope",
        "id.key",
        "place.key.more",
        "",
        "place.",
    ] {
        let err = StdForm::new(&s, &name).with_empty(root, &[path]).err();
        let message = err.map(|e| e.message).unwrap_or_default();
        assert!(message.contains(&format!("`{path}`")), "{path}: {message}");
    }
}
