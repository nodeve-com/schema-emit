//! The std form's items as an embedder includes them: one struct or enum per definition, named by
//! it, and a model another crate writes read through that crate's module.

#![allow(clippy::unwrap_used)]

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::ir::Schema;
use schema_emit::json_schema::lower;
use serde_json::{json, Map};

/// `s`'s std form, each struct named by [`names`].
fn items(s: &Schema) -> String {
    let names = names(s);
    let name = |i: usize| names.get(i).cloned().unwrap_or_default();
    StdForm::new(s, &name).items().unwrap()
}

/// The std form of `schema` as the root `Root`.
fn rooted(schema: &serde_json::Value) -> String {
    items(&lower("Root", schema, &Map::new(), &[]).unwrap().0)
}

/// A root naming the definition `Part` twice.
fn schema() -> serde_json::Value {
    json!({
        "type": "object",
        "required": ["a"],
        "properties": {
            "a": { "$ref": "#/$defs/Part" },
            "b": { "type": "array", "items": { "$ref": "#/$defs/Part" } }
        },
        "$defs": {
            "Part": { "type": "object", "properties": { "n": { "type": "integer" } } }
        }
    })
}

#[test]
fn a_definition_named_twice_is_one_struct() {
    let items = rooted(&schema());
    assert_eq!(items.matches("pub struct Part ").count(), 1, "{items}");
    assert!(
        items.contains("pub b: ::core::option::Option<::std::vec::Vec<Part>>"),
        "{items}"
    );
}

#[test]
fn an_external_model_is_read_through_its_module() {
    let (s, _) = lower("Root", &schema(), &Map::new(), &[]).unwrap();
    let names = names(&s);
    let name = |i: usize| names.get(i).cloned().unwrap_or_default();
    let external = |i: usize| (name(i) == "Part").then(|| "::other".to_string());
    let items = StdForm::new(&s, &name)
        .with_external(&external)
        .items()
        .unwrap();
    assert!(!items.contains("pub struct Part "), "{items}");
    assert!(!items.contains("for Part "), "{items}");
    assert!(items.contains("pub a: ::other::Part"), "{items}");
    assert!(items.contains("of::<::other::Part>()"), "{items}");
}

#[test]
fn each_definition_lowers_once_by_name() {
    let (s, defs) = schema_emit::json_schema::lower_defs(&schema(), &Map::new(), &[]).unwrap();
    let got: Vec<&str> = defs.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(got, ["Part"]);
    assert_eq!(s.models.len(), 1);
}

#[test]
fn an_allof_over_a_definition_leaves_the_definition_as_it_is() {
    let schema = json!({
        "$defs": {
            "Fields": { "type": "object", "required": ["kind"], "properties": { "kind": { "type": "string" } } },
            "Plain": { "allOf": [{ "$ref": "#/$defs/Fields" }], "unevaluatedProperties": false },
            "Named": {
                "allOf": [{ "$ref": "#/$defs/Fields" }],
                "required": ["id"],
                "properties": { "id": { "type": "string" } },
                "unevaluatedProperties": false
            }
        }
    });
    let (s, _) = schema_emit::json_schema::lower_defs(&schema, &Map::new(), &[]).unwrap();
    let items = items(&s);
    assert!(
        items.contains("pub struct Fields { pub kind: ::std::string::String, pub others: ::std::string::String, }"),
        "{items}"
    );
    assert!(
        items.contains("pub struct Plain { pub kind: ::std::string::String, pub others: ::std::string::String, }"),
        "{items}"
    );
    assert!(
        items
            .contains("pub struct Named { pub kind: ::std::string::String, pub id: ::std::string::String, pub others: ::std::string::String, }"),
        "{items}"
    );
}

#[test]
fn a_union_takes_the_definition_naming_it() {
    let schema = json!({
        "type": "object",
        "required": ["a", "b", "c"],
        "properties": {
            "a": { "$ref": "#/$defs/Pick" },
            "b": { "anyOf": [{ "type": "string" }, { "type": "integer" }] },
            "c": { "$ref": "#/$defs/Part" }
        },
        "$defs": {
            "Pick": { "anyOf": [{ "$ref": "#/$defs/Part" }, { "type": "string" }] },
            "Part": { "anyOf": [{ "type": "boolean" }, { "type": "string" }] }
        }
    });
    let items = rooted(&schema);
    assert!(
        items.contains("pub enum Pick { Part(Part), String(::std::string::String), }"),
        "{items}"
    );
    assert!(
        items.contains("pub enum Part { Bool(bool), String(::std::string::String), }"),
        "{items}"
    );
    assert!(items.contains("pub enum StringOrI64 "), "{items}");
    assert!(
        items.contains("pub a: Pick, pub b: StringOrI64, pub c: Part,"),
        "{items}"
    );
    assert!(items.contains("for Pick "), "{items}");
}

#[test]
fn a_union_named_as_a_model_keeps_its_branches_name() {
    let schema = json!({
        "type": "object",
        "required": ["root"],
        "properties": { "root": { "$ref": "#/$defs/Root" } },
        "$defs": { "Root": { "anyOf": [{ "type": "boolean" }, { "type": "string" }] } }
    });
    let items = rooted(&schema);
    assert!(
        items.contains(
            "pub struct Root { pub root: BoolOrString, pub others: ::std::string::String, }"
        ),
        "{items}"
    );
}

#[test]
fn an_open_model_skips_the_keys_no_field_names() {
    let closes = |extra: &[(&str, serde_json::Value)]| {
        let mut schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        for (key, value) in extra {
            schema[*key] = value.clone();
        }
        rooted(&schema).contains("Parts::unnamed(")
    };
    assert!(!closes(&[]));
    assert!(closes(&[("additionalProperties", json!(false))]));
    assert!(closes(&[("unevaluatedProperties", json!(false))]));
}

#[test]
fn an_embedder_reads_the_names_the_emitted_code_uses() {
    let (s, _) = lower("Root", &schema(), &Map::new(), &[]).unwrap();
    let clash = |_: usize| "Foo_bar".to_string();
    let form = StdForm::new(&s, &clash);
    let items = form.items().unwrap();
    let count = s.models.len();
    assert!(count >= 2, "{count}");
    let mut seen = Vec::new();
    for i in 0..count {
        let (name, decoder) = (form.model_name(i), form.decoder(i));
        assert!(
            items.contains(&format!("pub struct {name} ")),
            "{name}: {items}"
        );
        assert!(
            items.contains(&format!("pub fn {decoder}<")),
            "{decoder}: {items}"
        );
        assert!(!seen.contains(&name) && !seen.contains(&decoder));
        seen.push(name);
        seen.push(decoder);
    }
    assert_eq!(form.model_name(1), "Foo_bar_");
    assert_eq!(form.decoder(0), "decode_foo_bar");
    assert_eq!(form.decoder(1), "decode_foo_bar_");
}
