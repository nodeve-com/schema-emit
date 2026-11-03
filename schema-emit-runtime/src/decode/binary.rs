//! The binary form: how a format that isn't self-describing (postcard) holds a value, which the
//! seeds read by position. A record is its fields in order, an optional field an option, a map a
//! list of key and value, a timestamp whole seconds then nanoseconds, bytes raw, a union its
//! branch's index then the branch's value. Nothing in it is text, bar a value of any kind.

use std::fmt;
use std::string::String;
use std::vec::Vec;

use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, SeqAccess, VariantAccess, Visitor,
};

use super::Unfit;

/// The failure of a position or a branch that the value lacks.
fn cut_short<E: de::Error>() -> E {
    Unfit::other("a position is missing").raise()
}

/// The next position of `a`, read by `seed`.
pub fn next<'de, A: SeqAccess<'de>, S: DeserializeSeed<'de>>(
    a: &mut A,
    seed: S,
) -> Result<S::Value, A::Error> {
    a.next_element_seed(seed)?.ok_or_else(cut_short)
}

/// The failure of a union value whose index is no branch's.
pub fn no_branch<E: de::Error>() -> E {
    Unfit::other("an index no branch has").raise()
}

/// A fixed run of positions: a record's fields or a tuple's items.
pub trait Positions<'de>: Sized {
    type Value;
    /// How many positions it reads.
    const LEN: usize;
    fn read<A: SeqAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error>;
}

/// The seed and visitor of a fixed run of positions.
pub struct Tuple<P>(pub P);

impl<'de, P: Positions<'de>> DeserializeSeed<'de> for Tuple<P> {
    type Value = P::Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<P::Value, D::Error> {
        d.deserialize_tuple(P::LEN, self)
    }
}

impl<'de, P: Positions<'de>> Visitor<'de> for Tuple<P> {
    type Value = P::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a run of positions")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<P::Value, A::Error> {
        self.0.read(a)
    }
}

/// A list: its length, then each item read by `S`.
#[derive(Clone, Copy)]
pub struct Items<S>(pub S);

impl<'de, S: DeserializeSeed<'de> + Copy> Visitor<'de> for Items<S> {
    type Value = Vec<S::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::with_capacity(a.size_hint().unwrap_or(0).min(4096));
        while let Some(item) = a.next_element_seed(self.0)? {
            out.push(item);
        }
        Ok(out)
    }
}

/// A map's entry: its key, then its value read by `S`.
#[derive(Clone, Copy)]
pub struct Pair<S>(pub S);

impl<'de, S: DeserializeSeed<'de> + Copy> DeserializeSeed<'de> for Pair<S> {
    type Value = (String, S::Value);

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_tuple(2, self)
    }
}

impl<'de, S: DeserializeSeed<'de> + Copy> Visitor<'de> for Pair<S> {
    type Value = (String, S::Value);

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a key and a value")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let key = next(&mut a, super::text())?;
        Ok((key, next(&mut a, self.0)?))
    }
}

/// The visitor of bytes.
pub struct Raw;

impl Visitor<'_> for Raw {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bytes")
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
        Ok(v.to_vec())
    }

    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
        Ok(v)
    }
}

/// A union's branches, read by their index.
pub trait Branches<'de>: Sized {
    type Value;
    /// The union's name, and its branches' names, in index order.
    const NAME: &'static str;
    const NAMES: &'static [&'static str];
    /// The value of branch `index`, which `v` holds.
    fn read<V: VariantAccess<'de>>(self, index: u32, v: V) -> Result<Self::Value, V::Error>;
}

/// The seed and visitor of a union: its branch's index, then the branch's value.
pub struct Choice<B>(pub B);

impl<'de, B: Branches<'de>> DeserializeSeed<'de> for Choice<B> {
    type Value = B::Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<B::Value, D::Error> {
        d.deserialize_enum(B::NAME, B::NAMES, self)
    }
}

impl<'de, B: Branches<'de>> Visitor<'de> for Choice<B> {
    type Value = B::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a branch's index and its value")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, a: A) -> Result<B::Value, A::Error> {
        let (index, v) = a.variant::<u32>()?;
        self.0.read(index, v)
    }
}
