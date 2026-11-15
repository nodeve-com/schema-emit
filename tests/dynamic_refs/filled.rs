//! A schema with every dynamic reference filled, at any depth; each `$defs` entry by name; and a
//! field filled later named through what fills a dynamic reference.

use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::ir::{Base, Model, Schema};

use crate::{lowered, model};

/// The model field `name` of `m` holds.
fn inner(m: &Model, name: &str) -> usize {
    let f = m.fields.iter().find(|f| f.name == name).unwrap();
    match f.ty.base {
        Base::Model(i) => i,
        _ => panic!("{name} holds no model"),
    }
}

/// The name of the model field `name` of model `at` of `s` holds.
fn holds(s: &Schema, at: usize, name: &str) -> String {
    s.models[inner(&s.models[at], name)].name.clone()
}

#[test]
fn a_filled_schema_holds_each_dynamic_reference_as_its_filling_at_any_depth() {
    let s = lowered();
    let f = s.filled();
    assert!(!format!("{:?}", f.models).contains("DynamicRef"));
    assert!(f.models.iter().all(|m| m.dynamic_refs.is_empty()));
    let (book, toy, parcel) = (
        model(&s, "BookParcel"),
        model(&s, "ToyParcel"),
        model(&s, "Parcel"),
    );
    assert_eq!(holds(&f, book, "contents"), "BookParcel.contents");
    assert_eq!(holds(&f, parcel, "contents"), "Parcel.contents");
    let crate_of = |at: usize| inner(&f.models[at], "crate");
    let (book_crate, toy_crate) = (crate_of(book), crate_of(toy));
    assert!(book_crate >= s.models.len() && toy_crate >= s.models.len());
    assert_ne!(book_crate, toy_crate);
    assert_eq!(f.filling(book_crate), crate_of(parcel));
    assert_eq!(f.models[book_crate].name, "Crate");
    assert_eq!(holds(&f, book_crate, "item"), "BookParcel.contents");
    assert_eq!(holds(&f, toy_crate, "item"), "ToyParcel.contents");
    let wrap = inner(&f.models[toy], "wrap");
    assert_eq!(holds(&f, wrap, "inner"), "ToyParcel.contents");
    assert_eq!(f.filling(book), parcel);
}

#[test]
fn a_schema_names_each_definition_with_its_type() {
    let s = lowered();
    let named: Vec<&str> = s.named.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        named,
        ["Parcel", "BookParcel", "ToyParcel", "Sealed", "SealedNote"]
    );
    assert_eq!(s.models[model(&s, "SealedNote")].name, "SealedNote");
}

#[test]
fn a_path_runs_through_what_fills_a_dynamic_reference() {
    let s = lowered();
    let named = names(&s);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let (toy, parcel) = (model(&s, "ToyParcel"), model(&s, "Parcel"));
    let paths = ["id", "contents.pieces", "crate.item.pieces"];
    let std = StdForm::new(&s, &name).with_empty(toy, &paths).unwrap();
    assert!(std.can_be_empty(toy, 0) && std.can_be_empty(parcel, 0));
    let types = std.types();
    for part in [
        "pub struct ToyParcelContents { pub pieces: ::core::option::Option<i64>, pub others",
        "pub struct BookParcelContents { pub title: ::std::string::String, pub others",
        "pub id: ::core::option::Option<::std::string::String>, pub contents: Contents,",
    ] {
        assert!(types.contains(part), "{part}\n{types}");
    }
    let err = StdForm::new(&s, &name).with_empty(toy, &["contents.title"]);
    assert!(err.is_err());
}
