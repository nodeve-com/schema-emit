#![allow(clippy::unwrap_used)]

use std::format;
use std::string::{String, ToString as _};
use std::vec::Vec;

use serde::de::{DeserializeSeed, Deserializer, Visitor};
use serde_json::{json, Value};

use super::tests::{read_skips, Pair};
use super::*;

/// The places of `skips`, as text.
fn places(skips: &Skips) -> Vec<String> {
    skips.kept.iter().map(|u| u.to_string()).collect()
}

const BAD_B: &str = "expected an integer";

#[test]
fn a_bad_optional_value_is_absent_and_a_skip_names_its_place() {
    let (pair, skips) = read_skips(r#"{ "a": 1, "b": "x" }"#, of::<Pair>()).unwrap();
    assert_eq!(pair, Pair { a: 1, b: None });
    assert_eq!(places(&skips), [format!("M.b: {BAD_B}")]);
    assert_eq!(skips.dropped, 0);
    let (_, skips) = read_skips(
        r#"[{ "a": 1 }, { "a": 2 }, { "b": [1], "a": 3 }]"#,
        list(of::<Pair>()),
    )
    .unwrap();
    assert_eq!(places(&skips), [format!("M[2].b: {BAD_B}")]);
    let (_, skips) =
        read_skips(r#"{ "k": [{ "a": 1, "b": {} }] }"#, map(list(of::<Pair>()))).unwrap();
    assert_eq!(places(&skips), [format!("M.k[0].b: {BAD_B}")]);
}

#[test]
fn a_bad_required_value_fails_and_a_good_run_skips_nothing() {
    let e = read_skips(r#"{ "a": "x", "b": "y" }"#, of::<Pair>()).unwrap_err();
    assert_eq!(e.to_string(), format!("M.a: {BAD_B}"));
    let (_, skips) = read_skips(r#"{ "a": 1, "b": 2 }"#, of::<Pair>()).unwrap();
    assert!(skips.is_empty());
}

#[test]
fn the_first_skips_are_kept_and_the_rest_counted() {
    let items: Vec<String> = (0..20)
        .map(|_| r#"{ "a": 1, "b": "x" }"#.to_string())
        .collect();
    let (_, skips) = read_skips(&format!("[{}]", items.join(",")), list(of::<Pair>())).unwrap();
    assert_eq!(skips.kept.len(), 15);
    assert_eq!(skips.dropped, 5);
    assert_eq!(skips.total(), 20);
    assert_eq!(skips.kept[14].to_string(), format!("M[14].b: {BAD_B}"));
}

/// A deserializer of a tree: it answers a newtype struct with itself.
struct Tree<'a>(&'a Value);

impl<'de> Deserializer<'de> for Tree<'de> {
    type Error = serde_json::Error;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_any(v)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        v: V,
    ) -> Result<V::Value, Self::Error> {
        v.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

#[test]
fn text_a_value_and_a_tree_deserializer_give_the_same_result() {
    let text = r#"{ "a": 1, "b": "x" }"#;
    let want = read_skips(text, of::<Pair>()).unwrap();
    let value: Value = serde_json::from_str(text).unwrap();
    let from_value = root(|| Pair::deserialize(&value), "M").unwrap();
    assert_eq!(from_value, want);
    let from_tree = root(|| skip(of::<Pair>()).deserialize(Tree(&value)), "M").unwrap();
    assert_eq!(from_tree.0, Some(want.0));
    assert_eq!(from_tree.1, want.1);
    let bad = json!("x");
    let (none, skips) = root(|| skip(int::<i64>()).deserialize(Tree(&bad)), "M").unwrap();
    assert_eq!(none, None);
    assert_eq!(places(&skips), [format!("M: {BAD_B}")]);
}

#[test]
fn a_value_read_without_a_decode_notes_nothing() {
    let value = json!({ "a": 1, "b": "x" });
    assert_eq!(Pair::deserialize(&value).unwrap(), Pair { a: 1, b: None });
    let good = json!({ "a": 1 });
    let (_, skips) = root(|| Pair::deserialize(&good), "M").unwrap();
    assert!(skips.is_empty());
}

#[derive(Debug, PartialEq)]
enum Either {
    Strict(Pair),
    Loose(Value),
}

impl<'de> Deserialize<'de> for Either {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Either, D::Error> {
        let raw = Raw::deserialize(d)?;
        let mut pick = Pick::new(raw.get());
        pick.branch(kind::OBJECT, of::<Pair>(), |p| {
            let holds = p.a < 10;
            (Either::Strict(p), holds)
        });
        pick.branch(kind::ANY, any(), |v| (Either::Loose(v), true));
        pick.finish()
    }
}

#[test]
fn only_the_chosen_union_branch_keeps_its_skips() {
    let (v, skips) = read_skips(r#"{ "a": 5, "b": "x" }"#, of::<Either>()).unwrap();
    assert!(matches!(v, Either::Strict(_)));
    assert_eq!(places(&skips), [format!("M.b: {BAD_B}")]);
    let (v, skips) = read_skips(r#"{ "a": 50, "b": "x" }"#, of::<Either>()).unwrap();
    assert!(matches!(v, Either::Loose(_)));
    assert!(skips.is_empty());
}

#[test]
fn a_skip_inside_a_skipped_read_value_is_still_a_skip_of_its_own() {
    let seed = || skip(list(of::<Pair>()));
    let (got, skips) = read_skips(r#"[{ "a": 1, "b": "x" }, { "a": 2 }]"#, seed()).unwrap();
    assert_eq!(got.unwrap().len(), 2);
    assert_eq!(places(&skips), [format!("M[0].b: {BAD_B}")]);
    let (got, skips) = read_skips(r#"[{ "a": "x", "b": "x" }]"#, seed()).unwrap();
    assert_eq!(got, None);
    assert_eq!(places(&skips), [format!("M[0].a: {BAD_B}")]);
    let seed = skip(list(of::<Either>()));
    let (got, skips) =
        read_skips(r#"[{ "a": 50, "b": "x" }, { "a": 5, "b": "y" }]"#, seed).unwrap();
    assert_eq!(got.unwrap().len(), 2);
    assert_eq!(places(&skips), [format!("M[1].b: {BAD_B}")]);
}

#[test]
fn skips_display_each_place_and_the_count_after_the_cap() {
    let items: Vec<String> = (0..16)
        .map(|_| r#"{ "a": 1, "b": "x" }"#.to_string())
        .collect();
    let (_, skips) = read_skips(&format!("[{}]", items.join(",")), list(of::<Pair>())).unwrap();
    let text = skips.to_string();
    assert!(text.starts_with(&format!("M[0].b: {BAD_B}; M[1].b")));
    assert!(text.ends_with(" and 1 more"));
}
