//! A schema with no `type` takes the one kind its `enum`s and `const`s share. The suite types
//! such a schema from each instance, so these lower the schema as written.

use schema_emit::ir::Base;
use schema_emit::ir::Schema;
use schema_emit::json_schema::{lower_ty, Document};
use serde_json::{json, Map, Value};

fn document<'a>(schema: &'a Value, shared: &'a Map<String, Value>) -> Document<'a> {
    Document {
        schema,
        shared,
        notes: &[],
    }
}

fn lowered(schema: &Value) -> Result<(Base, bool), String> {
    lower_ty("V", &document(schema, &Map::new()), Schema::default())
        .map(|(_, ty)| (ty.base, ty.nullable))
        .map_err(|e| e.to_string())
}

#[test]
fn a_choice_of_one_kind_takes_it() {
    assert_eq!(
        lowered(&json!({"enum": ["a", "b"]})),
        Ok((Base::Text, false))
    );
    assert_eq!(lowered(&json!({"const": true})), Ok((Base::Bool, false)));
    assert_eq!(
        lowered(&json!({"enum": ["a", null]})),
        Ok((Base::Text, true))
    );
    assert_eq!(lowered(&json!({"const": null})), Ok((Base::Null, false)));
    assert!(matches!(
        lowered(&json!({"enum": [1, 2]})),
        Ok((Base::Int(_), false))
    ));
    assert!(matches!(
        lowered(&json!({"enum": [1, 2.5]})),
        Ok((Base::Float(_), false))
    ));
}

#[test]
fn a_choice_of_more_than_one_kind_is_refused() {
    assert!(lowered(&json!({"enum": ["a", 1]})).is_err());
    assert!(lowered(&json!({"enum": [true, "a"]})).is_err());
}

#[test]
fn an_untyped_allof_over_a_typed_object_with_properties_is_an_object() {
    let s = json!({
        "$defs": {"Base": {"type": "object", "required": ["kind"],
            "properties": {"kind": {"enum": ["x", "y"]}}}},
        "allOf": [{"$ref": "#/$defs/Base"}],
        "required": ["id"],
        "properties": {"id": {"type": "string"}},
        "unevaluatedProperties": false
    });
    let (schema, ty) = lower_ty("V", &document(&s, &Map::new()), Schema::default()).unwrap();
    let Base::Model(i) = ty.base else {
        panic!("not a model")
    };
    let names: Vec<&str> = schema.models[i]
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert!(
        names.contains(&"kind") && names.contains(&"id"),
        "{names:?}"
    );
}

#[test]
fn allof_subschemas_naming_one_kind_take_it() {
    let s = json!({"allOf": [{"enum": ["a", "b"]}, {"enum": ["b", "c"]}]});
    assert_eq!(lowered(&s), Ok((Base::Text, false)));
    let s = json!({"allOf": [{"minLength": 1}], "const": "a"});
    assert_eq!(lowered(&s), Ok((Base::Text, false)));
    assert!(lowered(&json!({"allOf": [{"enum": ["a"]}, {"const": 1}]})).is_err());
}
