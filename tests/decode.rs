//! The emitted decode and encode, end to end: schemas lowered and emitted into modules of one
//! program that cargo builds once. The program's own assertions state each rule of the decode
//! (every failing field named, unknown keys, widths, the union rule, one impl for text and a
//! parsed value) and of the encode (what is written back, and round trips), and of the binary
//! form (`decode/binary.rs`).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "decode/binary.rs"]
mod binary;
#[path = "program/manifest.rs"]
mod manifest;
#[path = "decode/reserved.rs"]
mod reserved;
#[allow(dead_code)]
#[path = "suite/run.rs"]
mod run;
#[path = "decode/strip.rs"]
mod strip;
#[path = "decode/values.rs"]
mod values;

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::json_schema::lower;
use serde_json::{json, Map, Value};

/// Schema `schema` emitted as the module `name`, which the program's `check` reaches.
/// Models take the names `names` makes, or the schema's own (a field `f_X` holds model `X`)
/// where `exact`.
fn module(name: &str, schema: &Value, check: &str, exact: bool) -> String {
    let (s, _) = lower("Root", schema, &Map::new(), &[]).unwrap();
    let named = match exact {
        true => (s.models.iter())
            .map(|m| {
                m.name
                    .strip_prefix("Root.f_")
                    .unwrap_or(&m.name)
                    .to_string()
            })
            .collect(),
        false => names(&s),
    };
    let item = |i: usize| named.get(i).cloned().unwrap_or_default();
    let items = StdForm::new(&s, &item).items().unwrap();
    format!(
        "mod {name} {{ #![allow(unused_imports, dead_code, non_camel_case_types, clippy::all)] \
         use ::schema_emit_runtime as __s; {items}\n\
         fn d(t: &str) -> ::core::result::Result<Root, __s::decode::Unfit> {{ \
         __s::decode::json(t, |de| decode_root(de)).map(|r| r.0) }}\n\
         fn s(t: &str) -> ::std::vec::Vec<::std::string::String> {{ \
         __s::decode::json(t, |de| decode_root(de)).unwrap().1.kept.iter().map(|u| u.to_string()).collect() }}\n\
         fn e(t: &str) -> ::std::string::String {{ d(t).unwrap_err().to_string() }}\n\
         fn v(t: &str) -> Root {{ d(t).unwrap() }}\n\
         fn out(r: &Root) -> ::std::string::String {{ ::serde_json::to_string(r).unwrap() }}\n\
         pub fn check() {{ {check} }} }}\n"
    )
}

/// An object of an optional integer beside the kinds a field can be, open to other keys.
fn fields() -> Value {
    json!({
        "type": "object",
        "required": ["id"],
        "properties": {
            "id": { "type": "integer" },
            "note": { "type": "integer" },
            "tag": { "type": ["string", "null"] },
            "n8": { "type": "integer", "format": "int8" },
            "wide": { "type": "integer", "format": "uint128" },
            "at": { "type": "string", "format": "date-time" }
        },
        "additionalProperties": true
    })
}

const FIELDS: &str = r##"
    assert_eq!(v(r#"{"id":1,"note":"x"}"#).note, None);
    assert_eq!(s(r#"{"id":1,"note":"x"}"#), ["Root.note: expected an integer"]);
    assert_eq!(e(r#"{"id":"x","note":"y","tag":5}"#), "Root.id: expected an integer");
    assert_eq!(e(r#"{"note":1}"#), "Root.id: a required field is absent");
    assert_eq!(e(r#"{"id":"x"}"#), "Root.id: expected an integer");
    let why = |t: &str| d(t).unwrap_err().why;
    assert!(matches!(why(r#"{"note":1}"#), __s::decode::Why::Absent));
    assert!(matches!(why(r#"{"id":"x"}"#), __s::decode::Why::Kind(_)));
    assert!(matches!(why(r#"{"id":1e30}"#), __s::decode::Why::Range(_)));
    assert_eq!(e("[1]"), "Root: expected an object");
    assert_eq!(v(r#"{"id":1.0}"#).id, 1);
    assert!(d(r#"{"id":1} {"#).is_err());
    assert_eq!(v(r#"{"id":1,"tag":null}"#).tag, Some(None));
    assert_eq!(v(r#"{"id":1}"#).tag, None);
    assert_eq!(v(r#"{"id":1,"tag":5}"#).tag, None);
    assert_eq!(s(r#"{"id":1,"tag":5}"#), ["Root.tag: expected text"]);
    assert_eq!(v(r#"{"id":1,"tag":"a"}"#).tag, Some(Some("a".into())));
    assert!(s(r#"{"id":1,"n8":300}"#)[0].starts_with("Root.n8: "));
    assert_eq!(v(r#"{"id":1,"n8":-128}"#).n8, Some(-128_i8));
    let wide = u128::MAX.to_string();
    assert_eq!(v(&format!(r#"{{"id":1,"wide":{wide}}}"#)).wide, Some(u128::MAX));
    assert!(s(r#"{"id":1,"wide":-1}"#)[0].starts_with("Root.wide: "));
    let leap = v(r#"{"id":1,"at":"2016-12-31T23:59:60Z"}"#);
    assert_eq!(out(&leap), r#"{"id":1,"at":"2017-01-01T00:00:00Z"}"#);
    assert_eq!(Root::default().id, 0);

    let text = r#"{"id":1,"x":{"k":[1,2]},"y":null,"id2":"é"}"#;
    let kept = v(text);
    assert_eq!(kept.others, r#""x":{"k":[1,2]},"y":null,"id2":"é""#);
    assert_eq!(out(&kept), r#"{"id":1,"x":{"k":[1,2]},"y":null,"id2":"é"}"#);
    let bytes = ::postcard::to_allocvec(&kept).unwrap();
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), kept);
    let value: ::serde_json::Value = ::serde_json::from_str(text).unwrap();
    let from_value = decode_root(&value).unwrap().0;
    assert_eq!(from_value.id, 1);
    assert_eq!(out(&from_value), r#"{"id":1,"x":{"k":[1,2]},"y":null,"id2":"é"}"#);
    let all = v(r#"{"id":1,"note":2,"tag":null}"#);
    assert_eq!(out(&all), r#"{"id":1,"note":2,"tag":null}"#);
"##;

const CLOSED: &str = r##"
    assert_eq!(e(r#"{"a":1,"z":[1]}"#), "Root.z: a key no field names");
    assert!(matches!(d(r#"{"z":1}"#).unwrap_err().why, __s::decode::Why::Unnamed));
    assert_eq!(e(r#"{"a":"x","z":1}"#), "Root.z: a key no field names");
    assert_eq!(e(r#"{"z":1,"a":"x"}"#), "Root.z: a key no field names");
    assert_eq!(s(r#"{"a":"x"}"#), ["Root.a: expected an integer"]);
    assert_eq!(out(&v(r#"{"a":1}"#)), r#"{"a":1}"#);
"##;

const TYPED: &str = r##"
    let got = v(r#"{"a":1,"x_b":2,"x_c":3}"#);
    assert_eq!(got.others.len(), 2);
    assert_eq!(out(&got), r#"{"a":1,"x_b":2,"x_c":3}"#);
    assert_eq!(got.others, [("x_b".to_string(), 2), ("x_c".to_string(), 3)]);
    assert_eq!(v(r#"{"x_b":2,"x_b":5}"#).others, [("x_b".to_string(), 5)]);
    let bytes = ::postcard::to_allocvec(&got).unwrap();
    assert_eq!(::postcard::from_bytes::<Root>(&bytes).unwrap(), got);
    assert_eq!(v(r#"{"x_b":2}"#).a, None);
    assert!(d(r#"{"x_b":"no"}"#).is_err());
"##;

const SHAPES: &str = r##"
    assert_eq!(e(r#"{"u":[1]}"#), "Root.u: expected a number");
    let text = r#"{"u":"x","d":"1.5s","b":"aGk=","l":[{"z":1}],"t":[1,"a"]}"#;
    let got = v(text);
    let written = out(&got);
    assert!(written.contains(r#""b":"aGk=""#), "{written}");
    assert!(written.contains(r#""d":"1.5s""#), "{written}");
    assert!(written.contains(r#""u":"x""#), "{written}");
    assert_eq!(v(&written), got);
    assert_eq!(v(r#"{"u":7}"#).u, v(&out(&v(r#"{"u":7}"#))).u);
    assert_eq!(v(r#"{"u":1,"l":[{"z":1},{"z":"x"}]}"#).l, None);
    assert_eq!(s(r#"{"u":1,"l":[{"z":1},{"z":"x"}]}"#), ["Root.l[1].z: expected an integer"]);
    assert_eq!(s(r#"{"u":1,"l":{"z":1}}"#), ["Root.l: expected an array"]);
    assert_eq!(v(r#"{"u":1,"t":[1]}"#).t, Some((Some(1), None)));
    assert_eq!(v(r#"{"u":1,"t":[1,"a",3]}"#).t, None);
    assert!(s(r#"{"u":1,"t":[1,"a",3]}"#)[0].starts_with("Root.t[2]"));
    assert_eq!(s(r#"{"u":1,"d":5,"b":[]}"#).len(), 2);
    assert_eq!(v(r#"{"u":1,"o":{"z":"x"}}"#).o, None);
    assert_eq!(s(r#"{"u":1,"o":{"z":"x"}}"#), ["Root.o.z: expected an integer"]);
    assert_eq!(s(r#"{"u":1,"o":5}"#), ["Root.o: expected an object"]);
"##;

/// A closed object of one field.
fn closed() -> Value {
    json!({
        "type": "object",
        "properties": { "a": { "type": "integer" } },
        "additionalProperties": false
    })
}

/// A closed object whose other keys a pattern types.
fn typed() -> Value {
    let mut schema = closed();
    schema["patternProperties"] = json!({ "^x_": { "type": "integer" } });
    schema
}

/// A union, a tuple, a duration, bytes, and a list of models.
fn shapes() -> Value {
    json!({
        "type": "object",
        "required": ["u"],
        "properties": {
            "u": { "anyOf": [{ "type": "integer" }, { "type": "string" }] },
            "t": { "type": "array", "prefixItems": [{ "type": "integer" }, { "type": "string" }], "items": false },
            "d": { "type": "string", "format": "human-duration" },
            "b": { "type": "string", "contentEncoding": "base64" },
            "l": { "type": "array", "items": { "$ref": "#/$defs/P" } },
            "o": { "$ref": "#/$defs/P" }
        },
        "$defs": { "P": { "type": "object", "required": ["z"], "properties": { "z": { "type": "integer" } } } }
    })
}

#[test]
fn the_decode_and_the_encode_follow_their_rules() {
    let mut main = String::new();
    let modules = [
        ("fields", fields(), FIELDS),
        ("closed", closed(), CLOSED),
        ("typed", typed(), TYPED),
        ("shapes", shapes(), SHAPES),
        ("stamped", binary::stamped(), binary::STAMPED),
        ("every", binary::every(), binary::EVERY),
        ("strip", strip::schema(), strip::CHECK),
        ("reserved", reserved::schema(), reserved::CHECK),
        ("alias", reserved::alias(), reserved::ALIAS),
        ("list", values::list(), values::LIST),
        ("map", values::map(), values::MAP),
    ];
    for (name, schema, check) in &modules {
        main.push_str(&module(
            name,
            schema,
            check,
            name.starts_with("reserved") || *name == "alias",
        ));
    }
    main.push_str("fn main() {\n");
    for (name, _, _) in &modules {
        main.push_str(&format!("{name}::check();\n"));
    }
    main.push_str("println!(\"ok\");\n}\n");
    assert_eq!(run::execute("schema-emit-decode", &main).trim(), "ok");
}
