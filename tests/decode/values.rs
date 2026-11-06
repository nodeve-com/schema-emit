//! A root that is no object: its decode and encode take the value's own Rust type.

use serde_json::{json, Value};

/// A list of one-byte integers.
pub fn list() -> Value {
    json!({ "type": "array", "items": { "type": "integer", "format": "int8" } })
}

/// Reads, writes back through `encode_root`, and fails with the root's name.
pub const LIST: &str = r##"
    let got: ::std::vec::Vec<i8> = v("[1,-2]");
    assert_eq!(got, [1, -2]);
    let written = encode_root(&got, ::serde_json::value::Serializer).unwrap();
    assert_eq!(written, ::serde_json::json!([1, -2]));
    assert_eq!(e("[300]"), "Root[0]: an integer outside i8");
    assert_eq!(e("{}"), "Root: expected an array");
"##;

/// A map of any values.
pub fn map() -> Value {
    json!({ "type": "object", "additionalProperties": {} })
}

pub const MAP: &str = r##"
    let got: ::std::vec::Vec<(::std::string::String, ::any_value::Value)> = v(r#"{"a":[1,null]}"#);
    assert_eq!(got.len(), 1);
    let written = encode_root(&got, ::serde_json::value::Serializer).unwrap();
    assert_eq!(written, ::serde_json::json!({"a": [1, null]}));
"##;
