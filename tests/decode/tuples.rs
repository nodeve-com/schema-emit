//! A tuple read and written apart from the model holding it: `decode_value` and `encode_value`
//! of each field's type, beside the Rust type `rust` names.

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::json_schema::lower;
use serde_json::{json, Map, Value};

/// A tuple of an integer then text, its other items as `items` says.
fn mixed(items: Option<Value>) -> Value {
    let mut tuple =
        json!({ "type": "array", "prefixItems": [{ "type": "integer" }, { "type": "string" }] });
    if let Some(items) = items {
        tuple["items"] = items;
    }
    tuple
}

/// A tuple of each kind: mixed and closed, with typed items, with free items, of one kind, as a
/// list's item, and as a map's value.
pub fn schema() -> Value {
    let same = json!({ "type": "array", "prefixItems": [{ "type": "integer" }, { "type": "integer" }], "items": false });
    json!({
        "type": "object",
        "properties": {
            "closed": mixed(Some(json!(false))),
            "typed": mixed(Some(json!({ "type": "boolean" }))),
            "free": mixed(None),
            "same": same,
            "listed": { "type": "array", "items": mixed(Some(json!(false))) },
            "mapped": { "type": "object", "additionalProperties": mixed(Some(json!({ "type": "boolean" }))) }
        }
    })
}

const OPT: &str = "::core::option::Option";
const TEXT: &str = "::std::string::String";
const VEC: &str = "::std::vec::Vec";

/// Each field, its Rust type, and a value of it as JSON text.
fn cases() -> [(&'static str, String, &'static str); 6] {
    let pair = format!("{OPT}<i64>, {OPT}<{TEXT}>");
    let typed = format!("({pair}, {VEC}<bool>)");
    [
        ("closed", format!("({pair})"), r#"[1,"a"]"#),
        ("typed", typed.clone(), r#"[1,"a",true,false]"#),
        (
            "free",
            format!("({pair}, {VEC}<::any_value::Value>)"),
            r#"[1,"a",null,{"k":[2]}]"#,
        ),
        ("same", format!("({OPT}<i64>, {OPT}<i64>)"), "[1,2]"),
        ("listed", format!("{VEC}<({pair})>"), r#"[[1,"a"],[2,"b"]]"#),
        (
            "mapped",
            format!("{VEC}<({TEXT}, {typed})>"),
            r#"{"k":[1,"a",true],"l":[2,"b"]}"#,
        ),
    ]
}

/// The program's assertions: each field's type is the one `rust` names, and its own writer writes
/// the JSON its own reader read, the same JSON the model's writer writes for the field.
pub fn check() -> String {
    let (s, root) = lower("Root", &schema(), &Map::new(), &[]).unwrap();
    let named = names(&s);
    let item = |i: usize| named.get(i).cloned().unwrap_or_default();
    let std = StdForm::new(&s, &item);
    let mut out = String::new();
    for (field, rust, text) in cases() {
        let fields = &s.models[root].fields;
        let ty = &fields.iter().find(|f| f.name == field).unwrap().ty;
        assert_eq!(std.rust(ty), rust, "{field}");
        let (read, write) = (format!("read_{field}"), format!("write_{field}"));
        out.push_str(&std.decode_value(ty, field, &read).unwrap());
        out.push_str(&std.encode_value(ty, &write).unwrap());
        out.push_str(&format!(
            "let x: {rust} = __s::decode::json(r#\"{text}\"#, |de| {read}(de)).unwrap().0;\n\
             let mut buf = ::std::vec::Vec::new();\n\
             {write}(&x, &mut ::serde_json::Serializer::new(&mut buf)).unwrap();\n\
             assert_eq!(::std::str::from_utf8(&buf).unwrap(), r#\"{text}\"#);\n\
             let whole = r#\"{{\"{field}\":{text}}}\"#;\n\
             assert_eq!(v(whole).{field}, Some(x));\n\
             assert_eq!(out(&v(whole)), whole);\n"
        ));
    }
    out.push_str(
        "let bad = __s::decode::json(\"[1,2]\", |de| read_closed(de)).unwrap_err();\n\
         assert_eq!(bad.to_string(), \"closed[1]: expected text\");\n",
    );
    out
}
