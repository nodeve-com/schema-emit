//! `x-strip-additional`: an open model drops the keys nothing reads.

use schema_emit::json_schema::lower;
use serde_json::{json, Map, Value};

/// An open object of one required field that strips its other keys.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["id"],
        "properties": { "id": { "type": "integer" }, "n": { "type": "integer", "format": "uint8" } },
        "x-strip-additional": true
    })
}

/// The extra keys leave no text, no bytes, and no written key, and a bad one still decodes.
pub const CHECK: &str = r##"
    let got = v(r#"{"id":1,"x":{"k":[1,{"a":null}]},"y":"é","n":2,"z":1}"#);
    assert_eq!(got, Root { id: 1, n: Some(2) });
    assert_eq!(out(&got), r#"{"id":1,"n":2}"#);
    assert_eq!(v(r#"{"x":1,"id":3}"#).id, 3);
    assert_eq!(e(r#"{"x":1}"#), "Root.id: a required field is absent");
    assert!(d(r#"{"id":1,"x":"#).is_err());
    let bytes = ::postcard::to_allocvec(&got).unwrap();
    assert_eq!(bytes, [2, 1, 2]);
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), got);
    let value: ::serde_json::Value = ::serde_json::from_str(r#"{"id":4,"q":[1]}"#).unwrap();
    assert_eq!(decode_root(&value).unwrap().0, Root { id: 4, n: None });
"##;

/// The error text lowering `schema` gives.
fn refused(schema: &Value) -> String {
    lower("Root", schema, &Map::new(), &[])
        .unwrap_err()
        .to_string()
}

#[test]
fn a_contradiction_or_a_non_boolean_is_refused_and_false_is_absent() {
    let with = |key: &str, value: Value| {
        let mut s = schema();
        s[key] = value;
        s
    };
    for closed in [
        with("additionalProperties", json!(false)),
        with("unevaluatedProperties", json!(false)),
    ] {
        assert!(refused(&closed).contains("x-strip-additional"));
    }
    assert!(refused(&with("x-strip-additional", json!("yes"))).contains("x-strip-additional"));
    assert!(
        refused(&json!({ "type": "string", "x-strip-additional": 1 }))
            .contains("x-strip-additional")
    );
    let (s, _) = lower(
        "Root",
        &with("x-strip-additional", json!(false)),
        &Map::new(),
        &[],
    )
    .unwrap();
    let kept = lower(
        "Root",
        &json!({ "type": "object", "properties": {} }),
        &Map::new(),
        &[],
    );
    assert_eq!(s.models[0].additional, kept.unwrap().0.models[0].additional);
}
