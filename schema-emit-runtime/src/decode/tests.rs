#![allow(clippy::unwrap_used)]

use std::string::ToString as _;

use super::*;

#[test]
fn a_failure_names_where_and_why() {
    let v = r#"{ "xs": [{ "k": 1 }, { "k": "no" }] }"#;
    let read = |x: &str| {
        let [k] = object(x, &["k"])?;
        field(k, "k", int)
    };
    let [xs] = object(v, &["xs"]).unwrap();
    let e = field(xs, "xs", |xs| list(xs, read)).unwrap_err().model("M");
    assert_eq!(e.to_string(), "M.xs[1].k: expected an integer");
    assert!(e.is_kind());
}

#[test]
fn a_map_key_and_a_closed_object_say_where() {
    let v = r#"{ "a": 1, "b": 2.5 }"#;
    let e = map(v, int).unwrap_err();
    assert_eq!(e.to_string(), ".b: expected an integer");
    let e = closed(v, &["a"]).unwrap_err();
    assert_eq!(e.to_string(), ".b: a key no field names");
    assert!(!e.is_kind());
}

#[test]
fn a_repeated_key_reads_its_last_value_and_escapes_read() {
    let v = r#"{ "a": 1, "b": "x\ty", "a": 2 }"#;
    assert_eq!(
        object(v, &["a", "b"]).unwrap(),
        [Some("2"), Some(r#""x\ty""#)]
    );
    assert_eq!(text(r#""x\ty""#), Ok("x\ty".into()));
}

#[test]
fn text_that_isnt_one_json_value_fails() {
    assert!(object("{ \"a\": [1, }", &["a"]).is_err_and(|e| !e.is_kind()));
    assert!(int("1 2").is_err_and(|e| !e.is_kind()));
    assert!(list("{}", int).is_err_and(|e| e.is_kind()));
    assert_eq!(nullable(" null ", int), Ok(None));
    let deep = |n: usize| std::format!("{}{}", "[".repeat(n), "]".repeat(n));
    assert!(root(&deep(129), "M", |_| Ok(())).is_err());
    assert!(root(&deep(128), "M", |_| Ok(())).is_ok());
    assert!(root(&"[\"[\"]".repeat(200), "M", |_| Ok(())).is_ok());
}

#[test]
fn integers_by_width_and_whole_floats() {
    assert_eq!(int("-3"), Ok(-3));
    assert_eq!(int("2.0"), Ok(2));
    assert_eq!(uint(&u64::MAX.to_string()), Ok(u64::MAX));
    assert!(int(&u64::MAX.to_string()).is_err_and(|e| !e.is_kind()));
    assert!(uint("-1").is_err_and(|e| !e.is_kind()));
    assert!(int("1e30").is_err_and(|e| !e.is_kind()));
    assert!(int("1.5").is_err_and(|e| e.is_kind()));
}

#[test]
fn text_formats() {
    assert!(timestamp(r#""2024-01-02T03:04:05Z""#).is_ok());
    assert!(timestamp(r#""yesterday""#).is_err_and(|e| e.is_kind()));
    let leap = timestamp(r#""2016-12-31T23:59:60Z""#).unwrap();
    assert_eq!(leap, timestamp(r#""2017-01-01T00:00:00Z""#).unwrap());
    assert!(timestamp(r#""2016-12-31T12:00:60Z""#).is_err_and(|e| e.is_kind()));
    assert_eq!(duration(r#""1.5s""#), Ok(TimeDelta::milliseconds(1500)));
    assert_eq!(duration(r#""-2s""#), Ok(TimeDelta::seconds(-2)));
    assert!(duration(r#""P1D""#).is_err_and(|e| e.is_kind()));
    assert!(duration(r#""9999999999999999y""#).is_err());
    assert_eq!(bytes(r#""aGk=""#), Ok(b"hi".to_vec()));
    assert!(bytes(r#""!""#).is_err_and(|e| e.is_kind()));
}

#[test]
fn a_tuple_reads_its_positions() {
    let v = r#"[1, "a"]"#;
    let [p0, p1, p2] = tuple(v).unwrap();
    assert_eq!(element(p0, 0, int), Ok(Some(1)));
    assert_eq!(element(p2, 2, int), Ok(None));
    assert_eq!(element(p1, 1, text), Ok(Some("a".into())));
    assert_eq!(tail(v, 1, text), Ok(std::vec!["a".into()]));
    assert!(fixed(v, 1).is_err());
    assert!(fixed(v, 2).is_ok());
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
