//! The binary form, through postcard: schemas and the assertions on what they encode to.

use serde_json::{json, Value};

/// A required timestamp beside an optional byte.
pub fn stamped() -> Value {
    json!({
        "type": "object",
        "required": ["at"],
        "properties": {
            "at": { "type": "string", "format": "date-time" },
            "n": { "type": "integer", "format": "uint8" }
        },
        "additionalProperties": false
    })
}

/// A timestamp is whole seconds then nanoseconds, an optional field an option, no text at all.
pub const STAMPED: &str = r##"
    let x = v(r#"{"at":"1970-01-01T00:01:40.000000007Z","n":5}"#);
    let bytes = ::postcard::to_allocvec(&x).unwrap();
    assert_eq!(bytes, [200, 1, 7, 1, 5]);
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), x);
    let bare = v(r#"{"at":"1970-01-01T00:01:40.000000007Z"}"#);
    assert_eq!(::postcard::to_allocvec(&bare).unwrap(), [200, 1, 7, 0]);
    assert_eq!(::postcard::from_bytes::<Root>(&[200, 1, 7, 0]).unwrap(), bare);
    assert!(::postcard::from_bytes::<Root>(&[200, 1]).is_err());
    let mut de = ::postcard::Deserializer::from_bytes(&[200, 1, 7, 1, 5]);
    assert_eq!(decode_root(&mut de).unwrap().0, x);
"##;

/// Every kind of field, beside an open model's unread keys.
pub fn every() -> Value {
    json!({
        "type": "object",
        "required": ["i", "t"],
        "properties": {
            "i": { "type": "integer", "format": "int32" },
            "f": { "type": "number" },
            "ok": { "type": "boolean" },
            "t": { "type": "string" },
            "nul": { "type": ["string", "null"] },
            "b": { "type": "string", "contentEncoding": "base64" },
            "d": { "type": "string", "format": "human-duration" },
            "at": { "type": "string", "format": "date-time" },
            "xs": { "type": "array", "items": { "type": "integer" } },
            "m": { "type": "object", "additionalProperties": { "type": "integer" } },
            "tup": { "type": "array", "prefixItems": [{ "type": "integer" }, { "type": "string" }], "items": { "type": "boolean" } },
            "u": { "anyOf": [{ "type": "integer" }, { "type": "string" }] },
            "p": { "$ref": "#/$defs/P" },
            "any": {}
        },
        "additionalProperties": true,
        "$defs": { "P": { "type": "object", "required": ["z"], "properties": { "z": { "type": "integer" } } } }
    })
}

/// A value of every kind reads back from its bytes as it was, and as the same JSON.
pub const EVERY: &str = r##"
    let text = r#"{"i":-7,"f":2.5,"ok":true,"t":"hi","nul":null,"b":"aGk=","d":"1.5s","at":"2024-05-06T07:08:09.5Z","xs":[1,2,3],"m":{"a":1,"b":2},"tup":[4,"s",true,false],"u":"str","p":{"z":9},"any":{"k":[1,null]},"extra":{"q":1}}"#;
    let x = v(text);
    let bytes = ::postcard::to_allocvec(&x).unwrap();
    let back = ::postcard::from_bytes::<Root>(&bytes).unwrap();
    assert_eq!(back, x);
    assert_eq!(out(&back), out(&x));
    let written = out(&x);
    assert!(written.contains(r#""tup":[4,"s",true,false]"#), "{written}");
    assert_eq!(v(&written), x);
    for (given, text) in [("1.001s", "1.001s"), ("0.1s", "0.1s"), ("-2s", "-2s"), ("1s", "1s"), ("-0.5s", "-0.5s"), ("90m", "5400s")] {
        let d = v(&format!(r#"{{"i":1,"t":"","d":"{given}"}}"#));
        let written = out(&d);
        assert!(written.contains(&format!(r#""d":"{text}""#)), "{written}");
        assert_eq!(v(&written), d);
        let bytes = ::postcard::to_allocvec(&d).unwrap();
        assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), d);
    }
    assert!(!bytes.windows(4).any(|w| w == b"2024" || w == b"1.5s"));
    for t in [r#"{"i":1,"t":"","u":3}"#, r#"{"i":1,"t":"","tup":[4]}"#, r#"{"i":1,"t":"","nul":"x","m":{}}"#] {
        let x = v(t);
        let bytes = ::postcard::to_allocvec(&x).unwrap();
        assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), x);
    }
    assert!(::postcard::from_bytes::<Root>(&bytes[..bytes.len() - 3]).is_err());
"##;
