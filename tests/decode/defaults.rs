//! `default`: a field takes it where its key is absent or its value is skipped, and each
//! default is a function returning it.

use serde_json::{json, Value};

/// A closed object of one required field beside a defaulted one of each kind a default holds.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["id"],
        "properties": {
            "id": { "type": "integer" },
            "on": { "type": "boolean", "default": true },
            "n": { "type": "integer", "format": "int8", "minimum": -5, "default": -3 },
            "f": { "type": "number", "format": "float", "default": 1.5 },
            "w": { "type": "number", "default": 2 },
            "t": { "type": "string", "enum": ["a", "say \"hi\" é"], "default": "say \"hi\" é" }
        },
        "additionalProperties": false
    })
}

/// Absent and skipped values take the default, given values stay, and every field is written.
pub const CHECK: &str = r##"
    let filled = v(r#"{"id":1}"#);
    assert_eq!(filled, Root { id: 1, on: true, n: -3, f: 1.5, w: 2.0, t: "say \"hi\" é".into() });
    let given = v(r#"{"id":1,"on":false,"n":4,"f":0.25,"w":-1,"t":"a"}"#);
    assert_eq!(given, Root { id: 1, on: false, n: 4, f: 0.25, w: -1.0, t: "a".into() });
    assert_eq!(v(r#"{"id":1,"n":"x"}"#).n, -3);
    assert_eq!(s(r#"{"id":1,"n":"x"}"#), ["Root.n: expected an integer"]);
    assert_eq!((Root::default_n(), Root::default_t()), (-3, "say \"hi\" é".to_string()));
    assert_eq!(d(&out(&filled)).unwrap(), filled);
    assert!(out(&filled).contains(r#""on":true"#));
    let bytes = ::postcard::to_allocvec(&given).unwrap();
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), given);
"##;

/// A root number with a default.
pub fn root() -> Value {
    json!({ "type": "number", "format": "float", "default": 2.5 })
}

/// The root's default is `default_root()`.
pub const ROOT: &str = r##"
    assert_eq!(default_root(), 2.5f32);
    assert_eq!(v("1"), 1.0);
"##;
