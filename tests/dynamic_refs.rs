//! Dynamic references: a model leaving parts open lowers to one model with dynamic references and a model per schema
//! filling them, emits as one generic struct and an alias per filler, and what isn't a dynamic reference is
//! refused (`dynamic_refs/refusals.rs`). `dynamic_refs/program.rs` builds the emitted source and runs it.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "dynamic_refs/closed.rs"]
mod closed;
#[path = "dynamic_refs/external.rs"]
mod external;
#[path = "dynamic_refs/filled.rs"]
mod filled;
#[path = "program/manifest.rs"]
mod manifest;
#[path = "dynamic_refs/program.rs"]
mod program;
#[path = "dynamic_refs/refusals.rs"]
mod refusals;
#[allow(dead_code)]
#[path = "suite/run.rs"]
mod run;

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::emit::Emitter;
use schema_emit::ir::{Base, Schema, Ty};
use schema_emit::json_schema::lower_defs;
use serde_json::{json, Map, Value};

/// A filler of `parcel`: resource `id` declaring each of `anchors`.
fn filler(id: &str, anchors: Value) -> Value {
    json!({ "$id": id, "$ref": "parcel", "$defs": anchors })
}

/// `schema`, declaring anchor `name`.
pub fn anchor(name: &str, mut schema: Value) -> Value {
    schema["$dynamicAnchor"] = json!(name);
    schema
}

/// A model with a required dynamic reference, an optional one of any value, one in a nested object, and one
/// in another definition of its resource.
pub fn parcel() -> Value {
    json!({
        "$id": "parcel",
        "type": "object",
        "required": ["id", "contents"],
        "properties": {
            "id": { "type": "string" },
            "contents": { "$dynamicRef": "#contents" },
            "label": { "$dynamicRef": "#label" },
            "wrap": { "type": "object", "properties": { "inner": { "$dynamicRef": "#contents" } } },
            "crate": { "$ref": "#/$defs/Crate" }
        },
        "$defs": {
            "contents": anchor("contents", json!({ "type": "object" })),
            "label": anchor("label", json!({})),
            "Crate": {
                "type": "object",
                "required": ["item"],
                "properties": {
                    "item": { "$dynamicRef": "#contents" },
                    "weight": { "type": "integer", "maximum": 100 }
                }
            }
        }
    })
}

/// A closed model built with `allOf` over an open one, holding a dynamic reference of any value.
fn sealed() -> Value {
    json!({
        "$id": "sealed",
        "allOf": [{ "$ref": "#/$defs/Open" }],
        "properties": { "extra": { "$dynamicRef": "#extra" } },
        "unevaluatedProperties": false,
        "$defs": {
            "Open": { "type": "object", "required": ["kind"], "properties": { "kind": { "type": "string" } } },
            "extra": anchor("extra", json!({}))
        }
    })
}

/// An object requiring `field`, of `schema`.
fn holding(field: &str, schema: Value) -> Value {
    json!({ "type": "object", "required": [field], "properties": { field: schema } })
}

/// `parcel` and two schemas filling it, and `sealed` and one filling it.
pub fn document() -> Value {
    let title = holding("title", json!({ "type": "string", "maxLength": 5 }));
    let pieces = holding("pieces", json!({ "type": "integer", "minimum": 1 }));
    json!({
        "$id": "https://example.test/root",
        "$defs": {
            "Parcel": parcel(),
            "BookParcel": filler("book-parcel", json!({ "contents": anchor("contents", title) })),
            "ToyParcel": filler("toy-parcel", json!({
                "contents": anchor("contents", pieces),
                "label": anchor("label", json!({ "type": "string" }))
            })),
            "Sealed": sealed(),
            "SealedNote": { "$id": "sealed-note", "$ref": "sealed", "$defs": {
                "extra": anchor("extra", json!({ "type": "string" }))
            } }
        }
    })
}

/// `document` lowered, each definition's type in [`Schema::named`].
pub fn lowered() -> Schema {
    lower_defs(&document(), &Map::new(), &[]).unwrap()
}

/// The index of the model definition `name` of `s` lowers to.
pub fn model(s: &Schema, name: &str) -> usize {
    let ty = &s.named.iter().find(|(n, _)| n == name).unwrap().1;
    match ty.base {
        Base::Model(m) => m,
        _ => panic!("{name} is no model"),
    }
}

/// `s`'s std form, each struct named by [`names`].
fn items(s: &Schema) -> String {
    let named = names(s);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    StdForm::new(s, &name).items().unwrap()
}

/// The check `ok` of `s`'s model `model`.
fn check(s: &Schema, model: usize) -> Result<String, schema_emit::Error> {
    let named = names(s);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    Emitter::new(&StdForm::new(s, &name)).check(model, "ok")
}

#[test]
fn a_model_holds_its_dynamic_refs_and_a_filler_the_types_filling_them() {
    let s = lowered();
    let parcel = model(&s, "Parcel");
    let refs = &s.models[parcel].dynamic_refs;
    assert_eq!(
        refs.iter().map(|r| &*r.name).collect::<Vec<_>>(),
        ["contents", "label"]
    );
    assert_eq!(
        s.models[parcel].fields[1].ty.base,
        Base::DynamicRef("contents".into())
    );
    let named = |ty: &Ty| match ty.base {
        Base::Model(m) => s.models[m].name.clone(),
        _ => format!("{:?}", ty.base),
    };
    let own: Vec<String> = refs.iter().map(|r| named(&r.ty)).collect();
    assert_eq!(own, ["Parcel.contents", "Parcel.label"]);
    let filled = |name: &str| {
        let of = s.models[model(&s, name)].of.clone().unwrap();
        assert_eq!(of.model, parcel);
        of.with.iter().map(named).collect::<Vec<_>>()
    };
    assert_eq!(
        filled("BookParcel"),
        ["BookParcel.contents", "Parcel.label"]
    );
    assert_eq!(
        filled("ToyParcel"),
        ["ToyParcel.contents", "ToyParcel.label"]
    );
    let filled = s.filled();
    let book = &filled.models[model(&s, "BookParcel")];
    assert_eq!(book.name, "BookParcel");
    assert_eq!(named(&book.fields[1].ty), "BookParcel.contents");
    assert!(book.dynamic_refs.is_empty() && filled.filling(model(&s, "BookParcel")) == parcel);
}

#[test]
fn a_model_with_dynamic_refs_is_one_generic_struct_and_a_filler_an_alias() {
    let s = lowered();
    let items = items(&s);
    for part in [
        "pub struct ParcelLabel { pub value: ::any_value::Value, }",
        "pub struct ToyParcelLabel { pub value: ::std::string::String, }",
        "pub struct Parcel<Contents = ParcelContents, Label = ParcelLabel> { pub id: ::std::string::String, pub contents: Contents, pub label: ::core::option::Option<Label>, pub wrap: ::core::option::Option<ParcelWrap<Contents>>, pub crate_: ::core::option::Option<Crate<Contents>>,",
        "pub struct ParcelWrap<Contents = ParcelContents> { pub inner: ::core::option::Option<Contents>,",
        "pub struct Crate<Contents = ParcelContents> { pub item: Contents,",
        "pub type BookParcel = Parcel<BookParcelContents, ParcelLabel>;",
        "pub type ToyParcel = Parcel<ToyParcelContents, ToyParcelLabel>;",
        "pub struct Sealed<Extra = SealedExtra> { pub kind: ::std::string::String, pub extra: ::core::option::Option<Extra>, pub others: ::std::string::String, }",
        "pub type SealedNote = Sealed<SealedNoteExtra>;",
        "pub fn decode_parcel<'de, Contents: for<'__a> __s::decode::Deserialize<'__a>, Label: for<'__a> __s::decode::Deserialize<'__a>, __D:",
    ] {
        assert!(items.contains(part), "{part}\n{items}");
    }
    for once in [
        "pub fn decode_parcel<",
        "pub struct Parcel<",
        "Serialize for Parcel<",
    ] {
        assert_eq!(items.matches(once).count(), 1, "{once}");
    }
    assert!(
        !items.contains("decode_book_parcel<") && !items.contains("for BookParcel "),
        "{items}"
    );
}

#[test]
fn a_generic_struct_written_elsewhere_is_filled_through_its_module() {
    let s = lowered();
    let named = names(&s);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let parcel = model(&s, "Parcel");
    let external = |i: usize| (i == parcel).then(|| "::other".to_string());
    let std = StdForm::new(&s, &name).with_external(&external);
    let items = std.items().unwrap();
    assert!(
        !items.contains("pub struct Parcel<") && !items.contains("for Parcel<"),
        "{items}"
    );
    assert!(
        items.contains("pub type BookParcel = ::other::Parcel<BookParcelContents, ParcelLabel>;"),
        "{items}"
    );
}

#[test]
fn a_check_takes_each_dynamic_refs_own_and_a_filler_has_none() {
    let s = lowered();
    let check = check(&s, model(&s, "Parcel")).unwrap();
    assert!(
        check.contains("pub fn ok<Contents, Label, __R: __s::Report>(v: &Parcel<Contents, Label>, r: &mut __R, param_contents: impl Fn(&Contents, &mut __R), param_label: impl Fn(&Label, &mut __R))"),
        "{check}"
    );
    assert_eq!(check.matches("param_contents(").count(), 3, "{check}");
    let err = crate::check(&s, model(&s, "BookParcel")).unwrap_err();
    assert!(!err.unsupported && err.message.contains("fills"), "{err}");
}

#[test]
fn a_model_with_dynamic_refs_held_from_another_resource_holds_each_dynamic_refs_own_type() {
    let mut document = document();
    document["$defs"]["Shelf"] = json!({ "$id": "shelf", "type": "object",
        "properties": { "first": { "$ref": "parcel" }, "all": { "type": "array", "items": { "$ref": "book-parcel" } } }
    });
    let s = lower_defs(&document, &Map::new(), &[]).unwrap();
    assert!(s.models[model(&s, "Shelf")].dynamic_refs.is_empty());
    let check = check(&s, model(&s, "Shelf")).unwrap();
    assert!(
        check.contains("\"Shelf.all[].contents.title\"") && !check.contains("param_"),
        "{check}"
    );
    let items = items(&s);
    assert!(
        items.contains("pub struct Shelf { pub first: ::core::option::Option<Parcel>, pub all: ::core::option::Option<::std::vec::Vec<BookParcel>>,"),
        "{items}"
    );
}

#[test]
fn the_emitted_generic_struct_builds_and_reads_writes_and_checks() {
    program::run();
}

#[test]
fn a_generic_struct_another_module_holds_is_filled_and_checked_across_modules() {
    external::run();
}
