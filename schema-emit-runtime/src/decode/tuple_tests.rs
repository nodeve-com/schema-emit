#![allow(clippy::unwrap_used)]

use std::string::{String, ToString as _};
use std::vec;
use std::vec::Vec;

use super::tests::read;
use super::*;

/// A tuple of an integer, text, then the list of the rest, as emitted code reads an array.
#[derive(Clone, Copy)]
struct Tuple;

impl<'de> Kinds<'de> for Tuple {
    type Value = (Option<i64>, Option<String>, Vec<bool>);
    const EXPECTED: &'static str = "an array";

    fn binary<D: Deserializer<'de>>(self, _: D) -> Result<Self::Value, D::Error> {
        Err(Self::wrong())
    }

    fn seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut p = Parts::default();
        let first = p.item(&mut a, int::<i64>())?;
        let second = p.item(&mut a, text())?;
        let rest = p.tail(&mut a, boolean())?;
        Ok((first, second, rest))
    }
}

#[test]
fn a_tuple_reads_its_positions_and_the_rest() {
    let got = read(r#"[1, "a", true, false]"#, Shape(Tuple)).unwrap();
    assert_eq!(got, (Some(1), Some("a".into()), vec![true, false]));
    assert_eq!(read("[1]", Shape(Tuple)), Ok((Some(1), None, vec![])));
    let e = read(r#"[1, "a", true, 3]"#, Shape(Tuple)).unwrap_err();
    assert_eq!(e.to_string(), "M[3]: expected a boolean");
    let two = |v| read(v, Shape(Fixed));
    assert_eq!(two("[1]"), Ok(Some(1)));
    assert_eq!(
        two("[1, 2]").unwrap_err().to_string(),
        "M[1]: expected no more items than the tuple holds"
    );
}

#[derive(Clone, Copy)]
struct Fixed;

impl<'de> Kinds<'de> for Fixed {
    type Value = Option<i64>;
    const EXPECTED: &'static str = "an array";

    fn binary<D: Deserializer<'de>>(self, _: D) -> Result<Self::Value, D::Error> {
        Err(Self::wrong())
    }

    fn seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut p = Parts::default();
        let only = p.item(&mut a, int::<i64>())?;
        p.fixed(&mut a)?;
        Ok(only)
    }
}
