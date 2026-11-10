//! A generic struct one module holds, filled from another: module `a` is emitted from the
//! model's document alone, module `b` from a document filling its slots, the model's own
//! structs marked as `a`'s. `b` writes only the slot types, their checks, and the aliases.

use schema_emit::emit::std_form::source;
use schema_emit::json_schema::lower_defs;
use serde_json::{json, Map};

use crate::{lowered, model, parcel, run};

/// What the program asserts across the two modules.
const MAIN: &str = r##"
fn main() {
    let text = r#"{"id":"a","contents":{"title":"Too long"},"label":[1],"crate":{"item":{"title":"y"},"weight":101}}"#;
    let (book, _): (b::BookParcel, _) = __s::decode::json(text, |d| a::decode_parcel(d)).unwrap();
    assert_eq!(book.contents.title, "Too long");
    assert_eq!(::serde_json::to_string(&book).unwrap(), text);
    let bytes = ::postcard::to_allocvec(&book).unwrap();
    assert_eq!(::postcard::from_bytes::<b::BookParcel>(&bytes).unwrap(), book);
    let mut f = __s::Failures::<8>::default();
    a::check_parcel(&book, &mut f, b::check_book, |_, _| {});
    let fails: Vec<String> = f.iter().map(|x| format!("{x}: {}", x.reason.name())).collect();
    assert_eq!(fails, ["BookParcel.contents.title: range", "Parcel.crate.weight: range"]);
    let (plain, _): (a::Parcel, _) =
        __s::decode::json(r#"{"id":"p","contents":{}}"#, |d| a::decode_parcel(d)).unwrap();
    assert_eq!(plain.id, "p");
    println!("ok");
}
"##;

/// A module named `name` holding `items`.
fn module(name: &str, items: &str) -> String {
    format!(
        "mod {name} {{ #![allow(unused_imports, dead_code, clippy::all)] \
         use ::schema_emit_runtime as __s;\n{items}}}\n"
    )
}

/// The source of module `a`, holding the model, and of module `b`, filling it.
fn sources() -> (String, String) {
    let alone = json!({ "$id": "https://example.test/root", "$defs": { "Parcel": parcel() } });
    let (held, defs) = lower_defs(&alone, &Map::new(), &[]).unwrap();
    let a = source(
        &held,
        &|_| None,
        &[(model(&defs, "Parcel"), "check_parcel")],
    )
    .unwrap();
    let (schema, _) = lowered();
    let theirs = |m: &schema_emit::ir::Model| {
        let held = held.models.iter().any(|h| h.name == m.name);
        held.then(|| "super::a".to_string())
    };
    let book = (schema.models.iter()).position(|m| m.name == "BookParcel.contents");
    let b = source(&schema, &theirs, &[(book.unwrap(), "check_book")]).unwrap();
    (a, b)
}

/// Builds and runs the program.
pub fn run() {
    let (a, b) = sources();
    for absent in [
        "struct Parcel",
        "for Parcel",
        "decode_parcel",
        "struct Crate",
        "check_parcel",
    ] {
        assert!(!b.contains(absent), "{absent}\n{b}");
    }
    assert!(
        b.contains(
            "pub type BookParcel = super::a::Parcel<BookParcelContents, super::a::ParcelLabel>;"
        ),
        "{b}"
    );
    let main = format!(
        "use ::schema_emit_runtime as __s;\n{}{}{MAIN}",
        module("a", &a),
        module("b", &b)
    );
    assert_eq!(
        run::execute("schema-emit-slots-external", &main).trim(),
        "ok"
    );
}
