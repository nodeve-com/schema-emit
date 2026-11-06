#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::format;
use std::string::{String, ToString as _};
use std::vec;
use std::vec::Vec;

use serde_json::json;

use super::*;

/// A closed model of a required integer `a` and an optional integer `b`, as emitted code reads
/// an object.
#[derive(Debug, PartialEq)]
pub(super) struct Pair {
    pub(super) a: i64,
    pub(super) b: Option<i64>,
}

#[derive(Clone, Copy)]
struct PairKinds;

impl<'de> Kinds<'de> for PairKinds {
    type Value = Pair;
    const EXPECTED: &'static str = "an object";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<Pair, D::Error> {
        struct P;
        impl<'de> binary::Positions<'de> for P {
            type Value = Pair;
            const LEN: usize = 2;
            fn read<A: SeqAccess<'de>>(self, mut a: A) -> Result<Pair, A::Error> {
                let a_ = binary::next(&mut a, int::<i64>())?;
                let b = binary::next(&mut a, nullable(int::<i64>()))?;
                Ok(Pair { a: a_, b })
            }
        }
        binary::Tuple(P).deserialize(d)
    }

    fn map<A: MapAccess<'de>>(self, mut m: A) -> Result<Pair, A::Error> {
        let (mut a, mut b) = (None, None);
        while let Some(k) = m.next_key_seed(Names::new(&["a", "b"], true))? {
            match k {
                Slot::Field(0) => Parts::field(&mut a, ".a", &mut m, int::<i64>())?,
                Slot::Field(1) => {
                    Parts::optional(&mut b, ".b", &mut m, skip(int::<i64>()))?;
                }
                Slot::Field(_) => drop(m.next_value::<IgnoredAny>()?),
                Slot::Other(k) => return Err(Parts::unnamed(&k)),
            }
        }
        Parts::require::<_, A::Error>(&a, ".a")?;
        Parts::finish((|| Some(Pair { a: a?, b }))())
    }
}

impl<'de> Deserialize<'de> for Pair {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Pair, D::Error> {
        Shape(PairKinds).deserialize(d)
    }
}

/// `v` read whole by `seed`, at a root named `M`.
pub(super) fn read<'a, S: DeserializeSeed<'a> + Clone>(
    v: &'a str,
    seed: S,
) -> Result<S::Value, Unfit> {
    read_skips(v, seed).map(|(v, _)| v)
}

/// [`read`], with the skips.
pub(super) fn read_skips<'a, S: DeserializeSeed<'a> + Clone>(
    v: &'a str,
    seed: S,
) -> Result<(S::Value, Skips), Unfit> {
    json(v, |d| root(|| seed.clone().deserialize(d), "M"))
}

fn pair(v: &str) -> Result<Pair, Unfit> {
    read(v, of::<Pair>())
}

#[test]
fn a_failure_names_where_and_why() {
    let v = r#"[{ "a": 1 }, { "a": "no" }]"#;
    let e = read(v, list(of::<Pair>())).unwrap_err();
    assert_eq!(e.to_string(), "M[1].a: expected an integer");
    assert!(e.is_kind());
    let v = r#"{ "x": [{ "y": 1 }, { "y": 2.5 }] }"#;
    let e = read(v, map(list(map(int::<i64>())))).unwrap_err();
    assert_eq!(e.to_string(), "M.x[1].y: expected an integer");
}

#[test]
fn the_first_failing_part_fails_the_decode_and_is_named() {
    let e = pair(r#"{ "a": "no", "b": "x", "c": 1 }"#).unwrap_err();
    assert_eq!(e.to_string(), "M.a: expected an integer");
    let e = pair(r#"{ "c": 1, "a": "no" }"#).unwrap_err();
    assert_eq!(e.to_string(), "M.c: a key no field names");
    let e = pair(r#"{ "b": 1 }"#).unwrap_err();
    assert_eq!(e.to_string(), "M.a: a required field is absent");
    let e = read(r#"[{ "b": 1 }, { "a": 1 }]"#, list(of::<Pair>())).unwrap_err();
    assert_eq!(e.to_string(), "M[0].a: a required field is absent");
}

#[test]
fn a_failure_ends_the_decode_where_it_is_met() {
    let keys: Vec<String> = (0..30).map(|i| format!(r#""k{i}": "{i}""#)).collect();
    let e = pair(&format!("{{ {} }}", keys.join(", "))).unwrap_err();
    assert_eq!(e.to_string(), "M.k0: a key no field names");
    let e = read(r#"[{ "c": 1, "a": "no" }, { "a": 1 }]"#, list(of::<Pair>())).unwrap_err();
    assert_eq!(e.to_string(), "M[0].c: a key no field names");
    let e = pair(r#"{ "a": "no", "b": [[[ not json"#).unwrap_err();
    assert_eq!(e.to_string(), "M.a: expected an integer");
}

#[test]
fn a_closed_model_fails_on_a_key_no_field_names() {
    let e = pair(r#"{ "a": 1, "c": [1, 2], "d": 3 }"#).unwrap_err();
    assert_eq!(e.to_string(), "M.c: a key no field names");
    assert!(!e.is_kind());
    assert!(pair("[1]").is_err_and(|e| e.is_kind()));
    assert!(pair("3").is_err_and(|e| e.is_kind()));
}

#[test]
fn a_repeated_key_reads_its_last_value_and_escapes_read() {
    let v = r#"{ "a": 1, "b": 1, "a": 2 }"#;
    assert_eq!(pair(v), Ok(Pair { a: 2, b: Some(1) }));
    assert_eq!(read(r#""x\ty""#, text()), Ok("x\ty".into()));
}

#[test]
fn a_field_that_is_null_differs_from_one_that_is_absent() {
    let seed = nullable(int::<i64>());
    assert_eq!(read(" null ", seed), Ok(None));
    assert_eq!(read("4", seed), Ok(Some(4)));
    assert!(read(r#""x""#, seed).is_err_and(|e| e.is_kind()));
}

#[test]
fn text_that_isnt_one_json_value_fails() {
    for bad in [r#"{ "a": 1, "b": [1, }"#, "[1, ]", r#"{"a":1} 2"#, "", "{"] {
        let e = read(bad, of::<Pair>()).unwrap_err();
        assert!(!e.is_kind() && !e.path.starts_with("M.a"), "{bad}: {e}");
    }
    assert!(read("1 2", int::<i64>()).is_err());
}

#[test]
fn depth_is_serde_jsons() {
    let deep = |n: usize| std::format!("{}{}", "[".repeat(n), "]".repeat(n));
    assert!(read(&deep(127), any()).is_ok());
    assert!(read(&deep(128), any()).is_err());
    assert!(read(&std::format!("[1, {}]", deep(127)), list(any())).is_err());
    assert!(read(&deep(128), of::<Pair>()).is_err());
    assert_eq!(
        read(&deep(3), any()),
        Ok(any_value::Value::List(vec![any_value::Value::List(vec![
            any_value::Value::List(vec![])
        ])]))
    );
}

#[test]
fn integers_keep_their_width_and_whole_floats_fill_them() {
    assert_eq!(read("-3", int::<i8>()), Ok(-3));
    assert_eq!(read("2.0", int::<u16>()), Ok(2));
    assert_eq!(read(&u64::MAX.to_string(), int::<u64>()), Ok(u64::MAX));
    assert!(read("300", int::<u8>()).is_err_and(|e| !e.is_kind()));
    assert!(read(&u64::MAX.to_string(), int::<i64>()).is_err_and(|e| !e.is_kind()));
    assert!(read("-1", int::<u64>()).is_err_and(|e| !e.is_kind()));
    assert!(read("1e30", int::<i64>()).is_err_and(|e| !e.is_kind()));
    assert!(read("1.5", int::<i64>()).is_err_and(|e| e.is_kind()));
    assert!(read("true", int::<i64>()).is_err_and(|e| e.is_kind()));
    assert_eq!(read(&i128::MIN.to_string(), int::<i128>()), Ok(i128::MIN));
    assert_eq!(read(&u128::MAX.to_string(), int::<u128>()), Ok(u128::MAX));
    assert!(read(&u128::MAX.to_string(), int::<i128>()).is_err());
}

#[test]
fn floats_keep_their_width() {
    assert_eq!(read("3", float::<f64>()), Ok(3.0));
    assert_eq!(read("0.5", float::<f32>()), Ok(0.5_f32));
    assert!(read(r#""x""#, float::<f64>()).is_err_and(|e| e.is_kind()));
}

#[test]
fn text_formats() {
    let at = |v| read(v, timestamp());
    assert!(at(r#""2024-01-02T03:04:05Z""#).is_ok());
    assert!(at(r#""yesterday""#).is_err_and(|e| e.is_kind()));
    let leap = at(r#""2016-12-31T23:59:60Z""#).unwrap();
    assert_eq!(leap, at(r#""2017-01-01T00:00:00Z""#).unwrap());
    assert!(at(r#""2016-12-31T12:00:60Z""#).is_err_and(|e| e.is_kind()));
    let off = at(r#""2024-01-02T03:04:05+01:00""#).unwrap();
    assert_eq!(off, at(r#""2024-01-02T02:04:05Z""#).unwrap());
    let span = |v| read(v, duration());
    assert_eq!(span(r#""1.5s""#), Ok(chrono::TimeDelta::milliseconds(1500)));
    assert_eq!(span(r#""-2s""#), Ok(chrono::TimeDelta::seconds(-2)));
    assert!(span(r#""P1D""#).is_err_and(|e| e.is_kind()));
    assert!(span(r#""9999999999999999y""#).is_err());
    assert_eq!(read(r#""aGk=""#, bytes()), Ok(b"hi".to_vec()));
    assert!(read(r#""!""#, bytes()).is_err_and(|e| e.is_kind()));
}

#[derive(Debug, PartialEq)]
enum Cell {
    Int(i64),
    Text(String),
}

impl<'de> Deserialize<'de> for Cell {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Cell, D::Error> {
        let raw = Raw::deserialize(d)?;
        let mut pick = Pick::new(raw.get());
        let small = |n: i64| (Cell::Int(n), n <= 100);
        pick.branch(kind::NUMBER, int::<i64>(), small);
        pick.branch(kind::TEXT, text(), |t| (Cell::Text(t), true));
        pick.finish()
    }
}

#[test]
fn a_union_takes_the_first_branch_that_passes_its_check_else_the_first_that_decodes() {
    let cell = |v| read(v, of::<Cell>());
    assert_eq!(cell("7"), Ok(Cell::Int(7)));
    assert_eq!(cell("7.0"), Ok(Cell::Int(7)));
    assert_eq!(cell(r#""x""#), Ok(Cell::Text("x".into())));
    assert_eq!(cell("700"), Ok(Cell::Int(700)));
    assert_eq!(cell("[1]").unwrap_err().to_string(), "M: expected a number");
}

#[test]
fn a_union_fails_as_the_branch_that_fit_further() {
    let lacks = Unfit::absent().field("stream");
    let inside = Unfit::kind("text").field("expect");
    let root = Unfit::kind("an object");
    assert_eq!(Unfit::either(Some(lacks.clone()), inside.clone()), inside);
    assert_eq!(Unfit::either(Some(inside.clone()), lacks.clone()), inside);
    assert_eq!(Unfit::either(Some(root.clone()), lacks.clone()), lacks);
    assert_eq!(Unfit::either(Some(lacks.clone()), root), lacks);
    assert_eq!(Unfit::either(None, lacks.clone()), lacks);
}

#[test]
fn one_impl_reads_text_a_parsed_value_and_yaml() {
    let text = r#"{ "a": 5, "b": 6 }"#;
    let want = Ok(Pair { a: 5, b: Some(6) });
    assert_eq!(pair(text), want);
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(root(|| Pair::deserialize(&value), "M").map(|r| r.0), want);
    let yaml = "a: 5\nb: 6\n";
    let from_yaml = root(|| serde_saphyr::from_str::<Pair>(yaml), "M");
    assert_eq!(from_yaml.map(|r| r.0), want);
    let bad = root(|| serde_saphyr::from_str::<Pair>("a: no\n"), "M");
    assert_eq!(bad.unwrap_err().to_string(), "M.a: expected an integer");
    let cells: Result<BTreeMap<String, Cell>, _> = serde_saphyr::from_str("k: 3\nj: text\n");
    assert_eq!(cells.unwrap().len(), 2);
    let from_value = || Cell::deserialize(&json!({ "k": 1 }));
    assert!(root(from_value, "M").is_err_and(|e| e.is_kind()));
    assert_eq!(Cell::deserialize(&json!(4)).unwrap(), Cell::Int(4));
}

#[test]
fn a_failure_crosses_a_deserializer_whole() {
    let e = Unfit::kind("text").field("k").index(2).field("m");
    let _raised = e.clone().raise::<serde_json::Error>();
    assert_eq!(Unfit::raised(), Some(e));
    assert_eq!(Unfit::raised(), None);
    assert!(!Unfit::from_json(&serde_json::from_str::<i64>("x").unwrap_err()).is_kind());
}
