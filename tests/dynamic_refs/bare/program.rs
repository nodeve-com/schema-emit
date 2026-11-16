//! The emitted source of `bare::document` as programs: each filler decodes from JSON, writes
//! back, crosses postcard, and checks; a struct holds two of them; a field filled later starts
//! empty; and a second module fills a generic struct the first holds.

use schema_emit::emit::std_form::source;
use schema_emit::emit::Emitter;
use schema_emit::json_schema::lower_defs;
use serde_json::Map;

use crate::bare::{document, lowered, record, value_check, with_std};
use crate::external::{alone, module, theirs};
use crate::{model, run};

/// What every program starts with: `round!` reads `text` by `decode`, writes it by `writer` as
/// `back`, and crosses postcard, giving the value.
const HEAD: &str = r##"
use ::schema_emit_runtime as __s;
macro_rules! round {
    ($text:expr, $decode:path, $writer:path, $back:expr) => {{
        let (v, skips) = __s::decode::json($text, |d| $decode(d)).unwrap();
        assert!(skips.kept.is_empty());
        let with = __s::decode::ser::With(&v, $writer());
        assert_eq!(::serde_json::to_string(&with).unwrap(), $back);
        let bytes = ::postcard::to_allocvec(&with).unwrap();
        let mut de = ::postcard::Deserializer::from_bytes(&bytes);
        assert_eq!($decode(&mut de).unwrap().0, v);
        v
    }};
}
fn fails(f: &__s::Failures<8>) -> Vec<String> {
    f.iter().map(|x| format!("{x}: {}", x.reason.name())).collect()
}
"##;

/// What the program asserts of each filler and of the struct holding two.
const MAIN: &str = r##"
fn main() {
    let level: Record<f64> = round!(r#"{"name":"a","value":2}"#, decode_level, writer_level, r#"{"name":"a","value":2.0}"#);
    assert_eq!(level.value, 2.0);
    let numbers = round!(r#"{"name":"n","value":[1.5,2]}"#, decode_numbers, writer_numbers, r#"{"name":"n","value":[1.5,2.0]}"#);
    assert_eq!(numbers.value, vec![1.5, 2.0]);
    let counts = round!(r#"{"name":"c","value":{"x":1.0,"y":2}}"#, decode_counts, writer_counts, r#"{"name":"c","value":{"x":1,"y":2}}"#);
    assert_eq!(counts.value, vec![("x".to_string(), 1_i64), ("y".to_string(), 2)]);
    let whole = round!(r#"{"name":"w","value":3.0}"#, decode_whole, writer_whole, r#"{"name":"w","value":3}"#);
    assert_eq!(whole.value, 3_i64);
    let blob = round!(r#"{"name":"b","value":"aGk="}"#, decode_blob, writer_blob, r#"{"name":"b","value":"aGk="}"#);
    assert_eq!(blob.value, b"hi".to_vec());
    let span = round!(r#"{"name":"s","value":"1m 30s"}"#, decode_span, writer_span, r#"{"name":"s","value":"90s"}"#);
    assert_eq!(span.value, ::chrono::TimeDelta::seconds(90));

    let mut out = Vec::new();
    encode_whole(&whole, &mut ::serde_json::Serializer::new(&mut out)).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), r#"{"name":"w","value":3}"#);
    let unfit = __s::decode::json(r#"{"name":"w","value":3.5}"#, |d| decode_whole(d)).unwrap_err();
    assert_eq!(unfit.to_string(), "whole.value: expected an integer");
    let absent = __s::decode::json(r#"{"name":"a"}"#, |d| decode_level(d)).unwrap_err();
    assert_eq!(absent.to_string(), "level.value: a required field is absent");
    let extra = __s::decode::json(r#"{"name":"a","value":1,"more":1}"#, |d| decode_level(d));
    assert!(extra.is_err());

    let (low, _) = __s::decode::json(r#"{"name":"a","value":-1}"#, |d| decode_level(d)).unwrap();
    let mut f = __s::Failures::<8>::default();
    check_record(&low, &mut f, check_level_value);
    assert_eq!(fails(&f), ["level.value: range"]);
    let mut f = __s::Failures::<8>::default();
    check_record(&level, &mut f, check_level_value);
    assert!(f.is_empty());

    let text = r#"{"first":{"name":"a","value":-2.0},"all":[{"name":"c","value":{"k":1}}]}"#;
    let (shelf, skips): (Shelf, _) = __s::decode::json(text, |d| decode_shelf(d)).unwrap();
    assert!(skips.kept.is_empty());
    assert_eq!(shelf.first.value, -2.0);
    assert_eq!(shelf.all.as_ref().unwrap()[0].value, vec![("k".to_string(), 1_i64)]);
    assert_eq!(::serde_json::to_string(&shelf).unwrap(), text);
    let bytes = ::postcard::to_allocvec(&shelf).unwrap();
    assert_eq!(::postcard::from_bytes::<Shelf>(&bytes).unwrap(), shelf);
    let mut f = __s::Failures::<8>::default();
    check_shelf(&shelf, &mut f);
    assert_eq!(fails(&f), ["Shelf.first.value: range"]);
    println!("ok");
}
"##;

/// What the program asserts of a filled field the program fills later.
const EMPTY: &str = r##"
fn main() {
    let none: Level = round!(r#"{"name":"a"}"#, decode_level, writer_level, r#"{"name":"a"}"#);
    assert_eq!(none.value, None);
    let some = round!(r#"{"name":"a","value":2}"#, decode_level, writer_level, r#"{"name":"a","value":2.0}"#);
    assert_eq!(some.value, Some(2.0_f64));
    println!("ok");
}
"##;

/// What the program asserts across the two modules.
const ACROSS: &str = r##"
fn main() {
    let counts: a::Record<Vec<(String, i64)>> = round!(r#"{"name":"c","value":{"x":1.0}}"#, b::decode_counts, b::writer_counts, r#"{"name":"c","value":{"x":1}}"#);
    assert_eq!(counts.value, vec![("x".to_string(), 1_i64)]);
    let (low, _) = __s::decode::json(r#"{"name":"a","value":-1}"#, |d| b::decode_level(d)).unwrap();
    let mut f = __s::Failures::<8>::default();
    a::check_record(&low, &mut f, b::check_level_value);
    assert_eq!(fails(&f), ["level.value: range"]);
    let (plain, _): (a::Record, _) =
        __s::decode::json(r#"{"name":"p","value":[1]}"#, |d| a::decode_record(d)).unwrap();
    assert_eq!(::serde_json::to_string(&plain).unwrap(), r#"{"name":"p","value":[1]}"#);
    println!("ok");
}
"##;

/// Runs the program of `items` and `main`, named `name`.
fn execute(name: &str, items: &str, main: &str) {
    let source = format!("#![allow(unused_imports, dead_code, clippy::all)]{HEAD}{items}{main}");
    assert_eq!(run::execute(name, &source).trim(), "ok");
}

#[test]
fn each_filler_reads_writes_and_checks_and_a_struct_holds_two() {
    let s = lowered();
    let checks = [
        (model(&s, "Record"), "check_record"),
        (model(&s, "Shelf"), "check_shelf"),
    ];
    let mut items = source(&s, &|_| None, &checks).unwrap();
    items.push_str(&value_check(
        &s,
        "level",
        "level.value",
        "check_level_value",
    ));
    execute("schema-emit-bare-fillers", &items, MAIN);
}

#[test]
fn a_filled_field_filled_later_reads_absent_and_writes_nothing() {
    let s = lowered();
    let level = model(&s, "level");
    let items = with_std(&s, |std| {
        let std = std.with_empty(level, &["value"]).unwrap();
        let check = Emitter::new(&std).check(model(&s, "Record"), "check_record");
        std.items().unwrap() + &check.unwrap()
    });
    execute("schema-emit-bare-fillers-empty", &items, EMPTY);
}

#[test]
fn a_generic_struct_another_module_holds_is_filled_with_a_type_that_is_no_object() {
    let (held, a) = alone("Record", record(), "check_record");
    let mut both = document();
    both["$defs"].as_object_mut().unwrap().remove("Shelf");
    let s = lower_defs(&both, &Map::new(), &[]).unwrap();
    let theirs = |m: &schema_emit::ir::Model| theirs(&held, m);
    let mut b = source(&s, &theirs, &[]).unwrap();
    assert!(
        b.contains("pub type Level = super::a::Record<f64>;") && !b.contains("struct Record"),
        "{b}"
    );
    b.push_str(&value_check(
        &s,
        "level",
        "level.value",
        "check_level_value",
    ));
    let items = module("a", &a) + &module("b", &b);
    execute("schema-emit-bare-fillers-across", &items, ACROSS);
}
