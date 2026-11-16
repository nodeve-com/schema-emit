//! A dynamic reference filled with a type that is no object: the type itself is the generic
//! struct's parameter, in the schema and in the emitted Rust, read and written by schema-emit's
//! own code for that type. `bare/program.rs` builds the emitted source and runs it.

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::emit::Emitter;
use schema_emit::ir::{Base, Check, Float, Schema};
use schema_emit::json_schema::{lower, lower_defs};
use serde_json::{json, Map, Value};

use crate::{anchor, model};

/// A closed model of a name and a dynamic reference of any value.
pub fn record() -> Value {
    json!({
        "$id": "record", "type": "object", "required": ["name", "value"],
        "properties": { "name": { "type": "string" }, "value": { "$dynamicRef": "#value" } },
        "additionalProperties": false,
        "$defs": { "Value": anchor("value", json!({})) }
    })
}

/// Resource `id`, filling `record` with `value`.
fn filler(id: &str, value: Value) -> Value {
    json!({ "$id": id, "$ref": "record", "$defs": { "value": anchor("value", value) } })
}

/// `record`, a schema filling it with each kind of type that is no object, and a model holding
/// two of them.
pub fn document() -> Value {
    json!({ "$id": "https://example.test/root", "$defs": {
        "Record": record(),
        "level": filler("level", json!({ "type": "number", "minimum": 0 })),
        "numbers": filler("numbers", json!({ "type": "array", "items": { "type": "number", "format": "double" } })),
        "counts": filler("counts", json!({ "type": "object", "additionalProperties": { "type": "integer" } })),
        "whole": filler("whole", json!({ "type": "integer" })),
        "blob": filler("blob", json!({ "type": "string", "contentEncoding": "base64" })),
        "span": filler("span", json!({ "type": "string", "format": "human-duration" })),
        "Shelf": { "$id": "shelf", "type": "object", "required": ["first"], "properties": {
            "first": { "$ref": "level" },
            "all": { "type": "array", "items": { "$ref": "counts" } }
        } }
    } })
}

/// `document` lowered.
pub fn lowered() -> Schema {
    lower_defs(&document(), &Map::new(), &[]).unwrap()
}

/// The std form of `s` as `f` reads it, each struct named by [`names`].
pub fn with_std<T>(s: &Schema, f: impl FnOnce(StdForm) -> T) -> T {
    let named = names(s);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    f(StdForm::new(s, &name))
}

#[test]
fn a_filler_holds_the_type_itself_and_a_filled_schema_gives_the_field_that_type() {
    let s = lowered();
    let (record, level) = (model(&s, "Record"), model(&s, "level"));
    let of = s.models[level].of.clone().unwrap();
    assert_eq!(of.model, record);
    assert_eq!(of.with[0].base, Base::Float(Float::F64));
    assert!(matches!(of.with[0].checks[..], [Check::Min { .. }]));
    let numbers = &s.models[model(&s, "numbers")].of.as_ref().unwrap().with[0];
    assert!(matches!(&numbers.base, Base::List(item) if item.base == Base::Float(Float::F64)));
    assert!(s.models.iter().all(|m| !m.name.starts_with("level.")));
    let filled = s.filled();
    let value = &filled.models[level].fields[1];
    assert_eq!((value.name.as_str(), &value.ty), ("value", &of.with[0]));
    assert_eq!(filled.filling(level), record);
    with_std(&s, |std| {
        assert!(std.is_bare(level) && !std.is_bare(record));
        assert!(!std.can_be_empty(filled.filling(level), 1));
    });
}

#[test]
fn a_filler_is_the_generic_struct_of_the_type_itself_with_its_own_reading_and_writing() {
    let s = lowered();
    let items = with_std(&s, |std| std.items().unwrap());
    for part in [
        "pub struct Record<Value = RecordValue> { pub name: ::std::string::String, pub value: Value, }",
        "pub type Level = Record<f64>;",
        "pub type Numbers = Record<::std::vec::Vec<f64>>;",
        "pub type Counts = Record<::std::vec::Vec<(::std::string::String, i64)>>;",
        "pub type Whole = Record<i64>;",
        "pub type Blob = Record<::std::vec::Vec<u8>>;",
        "pub type Span = Record<::chrono::TimeDelta>;",
        "pub fn seed_level() -> impl for<'de> __s::decode::DeserializeSeed<'de, Value = Level> + Copy",
        "pub fn decode_level<'de, __D: __s::decode::Deserializer<'de>>(d: __D)",
        "pub fn writer_level() -> impl __s::decode::ser::Write<Level> + Copy",
        "pub fn encode_level<__S: __s::decode::Serializer>(v: &Level, s: __S)",
        "__s::decode::Parts::field(&mut f1, \".value\", &mut m, __s::decode::map(__s::decode::int::<i64>()))",
        "m.serialize_entry(\"value\", &__s::decode::ser::With(&v.value, __s::decode::ser::Base64))",
        "pub struct Shelf { pub first: Level, pub all: ::core::option::Option<::std::vec::Vec<Counts>>,",
        "__s::decode::Parts::field(&mut f0, \".first\", &mut m, seed_level())",
        "__s::decode::ser::Seq(writer_counts())",
    ] {
        assert!(items.contains(part), "{part}\n{items}");
    }
    assert!(!items.contains("LevelValue"), "{items}");
}

#[test]
fn a_filled_field_a_program_fills_later_is_an_option_of_the_type_itself() {
    let s = lowered();
    let level = model(&s, "level");
    with_std(&s, |std| {
        let std = std.with_empty(level, &["value"]).unwrap();
        assert!(std.can_be_empty(s.filled().filling(level), 1));
        let items = std.items().unwrap();
        let held = "pub value: ::core::option::Option<Value>, }";
        assert!(items.contains(held), "{items}");
    });
}

#[test]
fn a_root_filling_a_dynamic_reference_with_a_type_that_is_no_object_is_that_struct() {
    let mut root = filler("level", json!({ "type": "number", "minimum": 0 }));
    root["$defs"]["Record"] = record();
    let (s, at) = lower("level", &root, &Map::new(), &[]).unwrap();
    let of = s.models[at].of.clone().unwrap();
    assert_eq!(of.with[0].base, Base::Float(Float::F64));
    let items = with_std(&s, |std| std.items().unwrap());
    assert!(items.contains("pub type Level = Record<f64>;"), "{items}");
    assert!(items.contains("pub fn decode_level<"), "{items}");
}

#[test]
fn a_default_on_a_schema_filling_a_dynamic_reference_is_refused() {
    let mut document = document();
    document["$defs"]["level"]["$defs"]["value"]["default"] = json!(1);
    let err = lower_defs(&document, &Map::new(), &[]).unwrap_err();
    assert!(
        err.unsupported && err.message.contains("`default`"),
        "{err}"
    );
}

/// The check `name` of what model `filler` of `s` fills its dynamic reference with, its
/// failures' paths from `path`.
pub fn value_check(s: &Schema, filler: &str, path: &str, name: &str) -> String {
    let with = &s.models[model(s, filler)].of.as_ref().unwrap().with[0];
    with_std(s, |std| Emitter::new(&std).check_value(with, path, name)).unwrap()
}
