//! The emitted source of `document` as a program: the generic struct and its fillers decode from
//! JSON, write back, cross postcard, and check, each slot by its own check.

use schema_emit::emit::std_form::source;

use crate::{lowered, model, run};

/// What the program asserts of the emitted items.
const MAIN: &str = r##"
fn fails(f: &__s::Failures<8>) -> Vec<String> {
    f.iter().map(|x| format!("{x}: {}", x.reason.name())).collect()
}

fn main() {
    let text = r#"{"id":"a","contents":{"title":"Dune"},"label":[1],"wrap":{"inner":{"title":"x"}},"crate":{"item":{"title":"y"},"weight":3}}"#;
    let (book, skips): (BookParcel, _) = __s::decode::json(text, |d| decode_parcel(d)).unwrap();
    assert!(skips.kept.is_empty());
    assert_eq!(book.contents.title, "Dune");
    assert_eq!(book.wrap.as_ref().unwrap().inner.as_ref().unwrap().title, "x");
    assert_eq!(book.crate_.as_ref().unwrap().item.title, "y");
    assert_eq!(::serde_json::to_string(&book).unwrap(), text);
    let bytes = ::postcard::to_allocvec(&book).unwrap();
    assert_eq!(::postcard::from_bytes::<BookParcel>(&bytes).unwrap(), book);
    let mut de = ::postcard::Deserializer::from_bytes(&bytes);
    assert_eq!(decode_parcel::<BookParcelContents, ParcelLabel, _>(&mut de).unwrap().0, book);

    let absent = __s::decode::json(r#"{"id":"a","contents":{}}"#, |d| {
        decode_parcel::<BookParcelContents, ParcelLabel, _>(d)
    });
    assert_eq!(absent.unwrap_err().to_string(), "Parcel.contents.title: a required field is absent");

    let (toy, _): (ToyParcel, _) =
        __s::decode::json(r#"{"id":"t","contents":{"pieces":0},"label":"red"}"#, |d| decode_parcel(d)).unwrap();
    assert_eq!(toy.label.as_ref().map(|l| l.value.as_str()), Some("red"));
    assert_eq!(toy.contents.pieces, 0);
    let bytes = ::postcard::to_allocvec(&toy).unwrap();
    assert_eq!(::postcard::from_bytes::<ToyParcel>(&bytes).unwrap(), toy);
    let (skipped, skips): (ToyParcel, _) =
        __s::decode::json(r#"{"id":"t","contents":{"pieces":2},"label":7}"#, |d| decode_parcel(d)).unwrap();
    assert_eq!(skipped.label, None);
    assert_eq!(skips.kept[0].to_string(), "Parcel.label: expected text");

    let (plain, _): (Parcel, _) =
        __s::decode::json(r#"{"id":"p","contents":{"anything":1}}"#, |d| decode_parcel(d)).unwrap();
    assert_eq!(plain.contents.others, r#""anything":1"#);
    assert_eq!(::serde_json::to_string(&plain).unwrap(), r#"{"id":"p","contents":{"anything":1}}"#);

    let mut f = __s::Failures::<8>::default();
    check_parcel(&toy, &mut f, check_toy, |_, _| {});
    assert_eq!(fails(&f), ["ToyParcel.contents.pieces: range"]);
    let long = r#"{"id":"a","contents":{"title":"Too long"},"crate":{"item":{"title":"ok"},"weight":101}}"#;
    let (long, _): (BookParcel, _) = __s::decode::json(long, |d| decode_parcel(d)).unwrap();
    let mut f = __s::Failures::<8>::default();
    check_parcel(&long, &mut f, check_book, |_, _| {});
    assert_eq!(fails(&f), ["BookParcel.contents.title: range", "Parcel.crate.weight: range"]);
    let mut f = __s::Failures::<8>::default();
    check_parcel(&book, &mut f, check_book, |_, _| {});
    assert!(f.is_empty());

    let (note, _): (SealedNote, _) =
        __s::decode::json(r#"{"kind":"k","extra":"e"}"#, |d| decode_sealed(d)).unwrap();
    assert_eq!(note.extra.as_ref().map(|e| e.value.as_str()), Some("e"));
    assert_eq!(::serde_json::to_string(&note).unwrap(), r#"{"kind":"k","extra":"e"}"#);
    let bytes = ::postcard::to_allocvec(&note).unwrap();
    assert_eq!(::postcard::from_bytes::<SealedNote>(&bytes).unwrap(), note);
    let (any, _): (Sealed, _) =
        __s::decode::json(r#"{"kind":"k","extra":{"n":[1,null]}}"#, |d| decode_sealed(d)).unwrap();
    assert_eq!(::serde_json::to_string(&any).unwrap(), r#"{"kind":"k","extra":{"n":[1,null]}}"#);
    println!("ok");
}
"##;

/// Builds and runs the program.
pub fn run() {
    let (schema, defs) = lowered();
    let named = |name: &str| schema.models.iter().position(|m| m.name == name).unwrap();
    let checks = [
        (model(&defs, "Parcel"), "check_parcel"),
        (named("BookParcel.contents"), "check_book"),
        (named("ToyParcel.contents"), "check_toy"),
    ];
    let items = source(&schema, &|_| None, &checks).unwrap();
    let main = format!(
        "#![allow(unused_imports, dead_code, clippy::all)]\nuse ::schema_emit_runtime as __s;\n{items}{MAIN}"
    );
    assert_eq!(run::execute("schema-emit-slots", &main).trim(), "ok");
}
