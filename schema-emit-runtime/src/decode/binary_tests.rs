#![allow(clippy::unwrap_used)]

use std::string::ToString as _;
use std::vec;
use std::vec::Vec;

use chrono::{DateTime, TimeDelta};
use serde_json::json;

use super::ser::{Dynamic, With};
use super::tests::read;
use super::*;

/// `v` as postcard writes it.
fn enc<T: Serialize + ?Sized>(v: &T) -> Vec<u8> {
    postcard::to_allocvec(v).unwrap()
}

/// `bytes` read by `seed`, at a root named `M`.
fn bin<'a, S: DeserializeSeed<'a>>(bytes: &'a [u8], seed: S) -> Result<S::Value, Unfit> {
    let mut de = postcard::Deserializer::from_bytes(bytes);
    root(|| seed.deserialize(&mut de), "M").map(|(v, _)| v)
}

#[test]
fn scalars_and_lists_read_by_position() {
    assert_eq!(enc(&-3_i16), [5]);
    assert_eq!(bin(&[5], int::<i16>()), Ok(-3));
    assert_eq!(bin(&enc(&true), boolean()), Ok(true));
    assert_eq!(bin(&enc("hi"), text()), Ok("hi".to_string()));
    assert_eq!(bin(&enc(&1.5_f32), float::<f32>()), Ok(1.5));
    assert_eq!(
        bin(&enc(&[1_u16, 300][..]), list(int::<u16>())),
        Ok(vec![1, 300])
    );
    assert_eq!(bin(&enc(&Some(4_i64)), nullable(int::<i64>())), Ok(Some(4)));
    assert_eq!(bin(&enc(&None::<i64>), nullable(int::<i64>())), Ok(None));
}

#[test]
fn a_model_reads_its_fields_in_order_and_a_map_is_a_list_of_key_and_value() {
    let pair = bin(&[7, 1, 8], of::<super::tests::Pair>()).unwrap();
    assert_eq!((pair.a, pair.b), (-4, Some(4)));
    let entries = vec![("a".to_string(), 1_i64), ("b".to_string(), 2)];
    let bytes = enc(&entries);
    assert_eq!(bytes, [2, 1, b'a', 2, 1, b'b', 4]);
    assert_eq!(bin(&bytes, map(int::<i64>())), Ok(entries));
}

#[test]
fn time_and_bytes_are_numbers_and_raw_bytes() {
    let at = DateTime::from_timestamp(100, 7).unwrap();
    assert_eq!(enc(&(100_i64, 7_u32)), [200, 1, 7]);
    assert_eq!(bin(&[200, 1, 7], timestamp()), Ok(at));
    let span = TimeDelta::new(-3, 500).unwrap();
    assert_eq!(bin(&enc(&span), duration()), Ok(span));
    assert_eq!(bin(&[2, b'h', b'i'], bytes()), Ok(b"hi".to_vec()));
    let past = enc(&(1_000_000_000_000_000_i64, 0_u32));
    assert!(bin(&past, timestamp()).is_err());
}

#[test]
fn a_value_of_any_kind_is_the_text_of_its_json_in_binary() {
    let v = json!({ "k": [1, null] });
    let bytes = enc(&With(&v, Dynamic));
    assert_eq!(bin(&bytes, any()), Ok(v.clone()));
    assert_eq!(read(&v.to_string(), any()), Ok(v));
}

#[test]
fn a_failure_is_data() {
    let why = |v: &str, seed| read(v, seed).unwrap_err().why;
    assert!(matches!(why("300", int::<u8>()), Why::Range(_)));
    assert!(matches!(why(r#""x""#, int::<u8>()), Why::Kind(_)));
    assert_eq!(Unfit::absent().why, Why::Absent);
    assert_eq!(Unfit::unnamed().why, Why::Unnamed);
    let bad = serde_json::from_str::<i64>("x").unwrap_err();
    assert!(matches!(Unfit::from_json(&bad).why, Why::Other(_)));
    for why in [
        Why::Kind("a".into()),
        Why::Absent,
        Why::Range("b".into()),
        Why::Unnamed,
        Why::Other("c".into()),
    ] {
        let u = Unfit {
            path: "x".into(),
            why,
        };
        let _raised = u.clone().raise::<serde_json::Error>();
        assert_eq!(Unfit::raised(), Some(u));
    }
}
