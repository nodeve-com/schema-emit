#![allow(clippy::unwrap_used)]

use serde_json::json;
use std::string::ToString as _;

use super::*;

#[test]
fn a_failure_names_where_and_why() {
    let v = json!({ "xs": [{ "k": 1 }, { "k": "no" }] });
    let o = object(&v).unwrap();
    let read = |x: &Value| field(object(x)?, "k", int);
    let e = field(o, "xs", |xs| list(xs, read)).unwrap_err().model("M");
    assert_eq!(e.to_string(), "M.xs[1].k: expected an integer");
    assert!(e.is_kind());
}

#[test]
fn a_map_key_and_a_closed_object_say_where() {
    let v = json!({ "a": 1, "b": 2.5 });
    let e = map(&v, int).unwrap_err();
    assert_eq!(e.to_string(), ".b: expected an integer");
    let e = closed(object(&v).unwrap(), &["a"]).unwrap_err();
    assert_eq!(e.to_string(), ".b: a key no field names");
    assert!(!e.is_kind());
}

#[test]
fn integers_by_width_and_whole_floats() {
    assert_eq!(int(&json!(-3)), Ok(-3));
    assert_eq!(int(&json!(2.0)), Ok(2));
    assert_eq!(uint(&json!(u64::MAX)), Ok(u64::MAX));
    assert!(int(&json!(u64::MAX)).is_err_and(|e| !e.is_kind()));
    assert!(uint(&json!(-1)).is_err_and(|e| !e.is_kind()));
    assert!(int(&json!(1e30)).is_err_and(|e| !e.is_kind()));
    assert!(int(&json!(1.5)).is_err_and(|e| e.is_kind()));
}

#[test]
fn text_formats() {
    assert!(timestamp(&json!("2024-01-02T03:04:05Z")).is_ok());
    assert!(timestamp(&json!("yesterday")).is_err_and(|e| e.is_kind()));
    let leap = timestamp(&json!("2016-12-31T23:59:60Z")).unwrap();
    assert_eq!(leap, timestamp(&json!("2017-01-01T00:00:00Z")).unwrap());
    assert!(timestamp(&json!("2016-12-31T12:00:60Z")).is_err_and(|e| e.is_kind()));
    assert_eq!(duration(&json!("1.5s")), Ok(TimeDelta::milliseconds(1500)));
    assert_eq!(duration(&json!("-2s")), Ok(TimeDelta::seconds(-2)));
    assert!(duration(&json!("P1D")).is_err_and(|e| e.is_kind()));
    assert!(duration(&json!("9999999999999999y")).is_err());
    assert_eq!(bytes(&json!("aGk=")), Ok(b"hi".to_vec()));
    assert!(bytes(&json!("!")).is_err_and(|e| e.is_kind()));
}

#[test]
fn a_tuple_reads_its_positions() {
    let xs = [json!(1), json!("a")];
    assert_eq!(element(&xs, 0, int), Ok(Some(1)));
    assert_eq!(element(&xs, 2, int), Ok(None));
    assert_eq!(tail(&xs, 1, text), Ok(std::vec!["a".into()]));
    assert!(fixed(&xs, 1).is_err());
    assert!(fixed(&xs, 2).is_ok());
}

#[test]
fn the_failure_that_says_more_wins() {
    let kind = Unfit::kind("text");
    let other = Unfit::other("why");
    assert_eq!(Unfit::either(None, kind.clone()), kind);
    assert_eq!(Unfit::either(Some(kind.clone()), other.clone()), other);
    assert_eq!(Unfit::either(Some(other.clone()), kind), other);
}

#[test]
fn a_union_fails_as_the_branch_that_fit_further() {
    let lacks = Unfit::other(ABSENT).field("stream");
    let inside = Unfit::kind("text").field("expect");
    let root = Unfit::kind("an object");
    assert_eq!(Unfit::either(Some(lacks.clone()), inside.clone()), inside);
    assert_eq!(Unfit::either(Some(inside.clone()), lacks.clone()), inside);
    assert_eq!(Unfit::either(Some(root.clone()), lacks.clone()), lacks);
    assert_eq!(Unfit::either(Some(lacks.clone()), root), lacks);
}
