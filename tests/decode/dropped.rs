//! The option dropping undeclared keys from the binary forms: an open model with text and a
//! typed model of them, through postcard and JSON.

use serde_json::{json, Value};

/// Two open models: one keeps its other keys as text, one as typed values.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["id"],
        "properties": {
            "id": { "type": "integer", "format": "uint8" },
            "typed": {
                "type": "object",
                "properties": { "n": { "type": "integer", "format": "uint8" } },
                "additionalProperties": { "type": "integer", "format": "uint8" }
            }
        }
    })
}

/// The binary form carries each other key, last; the JSON form writes them inline.
pub const KEPT: &str = r##"
    let x = v(r#"{"id":1,"extra":true,"typed":{"n":2,"k":3}}"#);
    let bytes = ::postcard::to_allocvec(&x).unwrap();
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), x);
    assert!(bytes.windows(5).any(|w| w == b"extra"), "{bytes:?}");
    assert_eq!(out(&x), r#"{"id":1,"typed":{"n":2,"k":3},"extra":true}"#);
"##;

/// The binary form leaves each other key out and reads them back empty; the JSON form is as
/// without the option.
pub const DROPPED: &str = r##"
    let x = v(r#"{"id":1,"extra":true,"typed":{"n":2,"k":3}}"#);
    assert_eq!(out(&x), r#"{"id":1,"typed":{"n":2,"k":3},"extra":true}"#);
    let bytes = ::postcard::to_allocvec(&x).unwrap();
    assert!(!bytes.windows(5).any(|w| w == b"extra"), "{bytes:?}");
    let back = ::postcard::from_bytes::<Root>(&bytes).unwrap();
    assert_eq!(out(&back), r#"{"id":1,"typed":{"n":2}}"#);
    assert_eq!(bytes, [1, 1, 1, 2]);
"##;
