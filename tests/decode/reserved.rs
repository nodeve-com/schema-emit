//! Models named after the names the emitted code uses for its own helpers, the standard
//! library's, and the runtime's alias.

use serde_json::{json, Value};

/// The helper names the emitted code could have taken, as model names.
const NAMES: [&str; 18] = [
    "V", "D", "A", "S", "T", "P_", "B_", "Vt_", "R", "Place", "Self_", "Kinds", "Result", "Option",
    "Vec", "String", "Box", "Tree",
];

/// A model per name, each an object of one integer, beside a tuple, a union and a map.
pub fn schema() -> Value {
    let mut props = json!({
        "tup": { "type": "array", "prefixItems": [{ "type": "integer" }, { "type": "string" }], "items": false },
        "u": { "anyOf": [{ "type": "integer" }, { "type": "string" }] },
        "m": { "type": "object", "additionalProperties": { "type": "integer" } }
    });
    let mut defs = serde_json::Map::new();
    for name in NAMES {
        props[format!("f_{name}")] = json!({ "$ref": format!("#/$defs/{name}") });
        let mut def = json!({ "type": "object", "properties": {
            "x": { "type": "integer" },
            "n": { "type": "string" },
            "xs": { "type": "array", "items": { "type": "integer" } }
        } });
        if name == "Box" || name == "Tree" {
            def["properties"]["next"] = json!({ "$ref": format!("#/$defs/{name}") });
        }
        defs.insert(name.into(), def);
    }
    json!({ "type": "object", "properties": props, "$defs": defs })
}

/// A value of each reads, writes, and round trips through text and postcard.
pub const CHECK: &str = r##"
    let text = r#"{"tup":[1,"a"],"u":"s","m":{"k":2},
        "f_V":{"x":1,"n":"a","xs":[1]},"f_D":{"x":2},"f_A":{"x":3},"f_S":{"x":4},
        "f_T":{"x":5},"f_P_":{"x":6},"f_B_":{"x":7},"f_Vt_":{"x":8},"f_R":{"x":9},
        "f_Result":{"x":1,"n":"r","xs":[1,2]},"f_Option":{"x":1},
        "f_Vec":{"xs":[3]},"f_String":{"n":"s"},"f_Box":{"next":{"next":{"x":4}}},"f_Tree":{"x":1,"next":{"x":2}}}"#;
    let got = v(text);
    assert_eq!(v(&out(&got)), got);
    let bytes = ::postcard::to_allocvec(&got).unwrap();
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), got);
    assert!(s(r#"{"f_V":{"x":"no"}}"#)[0].starts_with("Root.f_V.x"));
"##;

/// Models named for the runtime's alias, and for it in another case: their decode functions
/// stay distinct.
pub fn alias() -> Value {
    let def = json!({ "type": "object", "properties": { "x": { "type": "integer" } } });
    json!({
        "type": "object",
        "properties": { "f_s": { "$ref": "#/$defs/s" }, "f_S": { "$ref": "#/$defs/S" } },
        "$defs": { "s": def, "S": def }
    })
}

pub const ALIAS: &str = r##"
    let got = v(r#"{"f_s":{"x":1},"f_S":{"x":2}}"#);
    assert_eq!(v(&out(&got)), got);
    assert!(s(r#"{"f_s":{"x":"no"}}"#)[0].starts_with("Root.f_s.x"));
    assert!(s(r#"{"f_S":{"x":"no"}}"#)[0].starts_with("Root.f_S.x"));
"##;
