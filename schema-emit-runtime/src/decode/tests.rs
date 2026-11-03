#![allow(clippy::unwrap_used)]

use std::string::ToString as _;

use super::*;

/// `v` read whole by `f`, at a root of no name.
fn read<'a, T>(
    v: &'a str,
    f: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<T, Unfit> {
    root(v, "", f)
}

/// A closed model of a required integer `a` and an optional integer `b`, as emitted code reads it.
fn pair(r: &mut Reader<'_>) -> Result<(i64, Option<i64>), Unfit> {
    let (mut a, mut b, mut rest) = (None, None, Rest::default());
    object(r, |k, r| match k {
        "a" => put(&mut a, r, int),
        "b" => put(&mut b, r, int),
        _ => unnamed(&mut rest, k, r),
    })?;
    let out = (field(a, "a")?, optional(b, "b")?);
    rest.done()?;
    Ok(out)
}

#[test]
fn a_failure_names_where_and_why() {
    let v = r#"{ "a": [{ "a": 1 }, { "a": "no" }] }"#;
    let e = root(v, "M", |r| map(r, |r| list(r, pair))).unwrap_err();
    assert_eq!(e.to_string(), "M.a[1].a: expected an integer");
    assert!(e.is_kind());
}

#[test]
fn a_map_key_and_a_closed_object_say_where() {
    let v = r#"{ "a": 1, "b": 2.5 }"#;
    let e = read(v, |r| map(r, int)).unwrap_err();
    assert_eq!(e.to_string(), ".b: expected an integer");
    let e = read(r#"{ "a": 1, "c": 2, "d": 3 }"#, pair).unwrap_err();
    assert_eq!(e.to_string(), ".c: a key no field names");
    assert!(!e.is_kind());
}

#[test]
fn fields_fail_in_schema_order_then_closed_keys() {
    let e = read(r#"{ "c": 0, "b": "x", "a": "y" }"#, pair).unwrap_err();
    assert_eq!(e.to_string(), ".a: expected an integer");
    let e = read(r#"{ "c": 0, "b": "x" }"#, pair).unwrap_err();
    assert_eq!(e.to_string(), ".a: a required field is absent");
    let e = read(r#"{ "c": 0, "b": "x", "a": 1 }"#, pair).unwrap_err();
    assert_eq!(e.to_string(), ".b: expected an integer");
}

#[test]
fn a_repeated_key_reads_its_last_value_and_escapes_read() {
    let v = r#"{ "a": "no", "b": 1, "\u0061": 2 }"#;
    assert_eq!(read(v, pair), Ok((2, Some(1))));
    assert_eq!(read(r#""x\ty""#, text), Ok("x\ty".into()));
}

#[test]
fn text_that_isnt_one_json_value_fails() {
    let not_json = |e: Unfit| e.to_string() == "M: text that isn't one JSON value";
    assert!(root(r#"{ "a": "no", "b": [1, }"#, "M", pair).is_err_and(not_json));
    assert!(root(r#"[{ "a": "no" }, ]"#, "M", |r| list(r, pair)).is_err_and(not_json));
    assert!(root("1 2", "M", int).is_err_and(not_json));
    assert!(read("{}", |r| list(r, int)).is_err_and(|e| e.is_kind()));
    assert_eq!(read(" null ", |r| nullable(r, int)), Ok(None));
    let deep = |n: usize| std::format!("{}{}", "[".repeat(n), "]".repeat(n));
    assert!(read(&deep(129), skip).is_err());
    assert!(read(&std::format!("[1, {}]", deep(128)), |r| list(r, int)).is_err());
    assert!(read(&deep(128), skip).is_ok());
    assert_eq!(read(&deep(3), any), Ok(serde_json::json!([[[]]])));
}

#[test]
fn integers_by_width_and_whole_floats() {
    assert_eq!(read("-3", int), Ok(-3));
    assert_eq!(read("2.0", int), Ok(2));
    assert_eq!(read(&u64::MAX.to_string(), uint), Ok(u64::MAX));
    assert!(read(&u64::MAX.to_string(), int).is_err_and(|e| !e.is_kind()));
    assert!(read("-1", uint).is_err_and(|e| !e.is_kind()));
    assert!(read("1e30", int).is_err_and(|e| !e.is_kind()));
    assert!(read("1.5", int).is_err_and(|e| e.is_kind()));
}

#[test]
fn text_formats() {
    let at = |v| read(v, timestamp);
    assert!(at(r#""2024-01-02T03:04:05Z""#).is_ok());
    assert!(at(r#""yesterday""#).is_err_and(|e| e.is_kind()));
    let leap = at(r#""2016-12-31T23:59:60Z""#).unwrap();
    assert_eq!(leap, at(r#""2017-01-01T00:00:00Z""#).unwrap());
    assert!(at(r#""2016-12-31T12:00:60Z""#).is_err_and(|e| e.is_kind()));
    let span = |v| read(v, duration);
    assert_eq!(span(r#""1.5s""#), Ok(TimeDelta::milliseconds(1500)));
    assert_eq!(span(r#""-2s""#), Ok(TimeDelta::seconds(-2)));
    assert!(span(r#""P1D""#).is_err_and(|e| e.is_kind()));
    assert!(span(r#""9999999999999999y""#).is_err());
    assert_eq!(read(r#""aGk=""#, bytes), Ok(b"hi".to_vec()));
    assert!(read(r#""!""#, bytes).is_err_and(|e| e.is_kind()));
}

#[test]
fn a_tuple_reads_its_positions() {
    let v = r#"[1, "a"]"#;
    let three = |r: &mut Reader<'_>| {
        let mut a = array(r)?;
        let p = (element(&mut a, int)?, element(&mut a, text)?);
        Ok((p, element(&mut a, int)?))
    };
    assert_eq!(read(v, three), Ok(((Some(1), Some("a".into())), None)));
    let rest = |r: &mut Reader<'_>| {
        let mut a = array(r)?;
        Ok((element(&mut a, int)?, tail(a, text)?))
    };
    assert_eq!(read(v, rest), Ok((Some(1), std::vec!["a".into()])));
    let one = |r: &mut Reader<'_>| {
        let mut a = array(r)?;
        element(&mut a, int).and_then(|p| fixed(a).map(|()| p))
    };
    assert_eq!(
        read(v, one).unwrap_err().to_string(),
        "[1]: expected no more items than the tuple holds"
    );
    assert_eq!(read("[1]", one), Ok(Some(1)));
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
