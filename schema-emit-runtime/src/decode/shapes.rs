//! The structured seeds: a list, a map, a nullable value, a boxed one, any value, and a type
//! that deserializes itself. They compose, and the emitted code passes them as a field's seed.

use std::borrow::Cow;
use std::boxed::Box;
use std::fmt;
use std::marker::PhantomData;
use std::string::String;
use std::vec::Vec;

use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

use super::union::Text;
use super::{binary, Kinds, Parts, Shape};

/// A model's field names, and whether a key none of them is keeps its text.
#[derive(Clone, Copy)]
pub struct Names {
    names: &'static [&'static str],
    keep: bool,
}

impl Names {
    pub const fn new(names: &'static [&'static str], keep: bool) -> Names {
        Names { names, keep }
    }

    fn slot<'de>(&self, key: &str, text: impl FnOnce() -> Cow<'de, str>) -> Slot<'de> {
        match self.names.iter().position(|n| *n == key) {
            Some(i) => Slot::Field(i),
            None if self.keep => Slot::Other(text()),
            None => Slot::Other(Cow::Borrowed("")),
        }
    }
}

/// Which field a key names, else the key.
pub enum Slot<'de> {
    Field(usize),
    Other(Cow<'de, str>),
}

impl<'de> DeserializeSeed<'de> for Names {
    type Value = Slot<'de>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Slot<'de>, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Names {
    type Value = Slot<'de>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a key")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Slot<'de>, E> {
        Ok(self.slot(v, || Cow::Owned(v.into())))
    }

    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Slot<'de>, E> {
        Ok(self.slot(v, || Cow::Borrowed(v)))
    }

    refuse!("a key"; unit none some bool i64 u64 f64 bytes seq map enum newtype);
}

/// A type that deserializes itself: a model or a union.
pub struct Of<T>(PhantomData<fn() -> T>);

impl<T> Clone for Of<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Of<T> {}

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for Of<T> {
    type Value = T;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::deserialize(d)
    }
}

pub fn of<T>() -> Of<T> {
    Of(PhantomData)
}

/// The value `S` reads, boxed.
#[derive(Clone, Copy)]
pub struct Boxed<S>(S);

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Boxed<S> {
    type Value = Box<S::Value>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(d).map(Box::new)
    }
}

pub fn boxed<S>(seed: S) -> Boxed<S> {
    Boxed(seed)
}

/// Null, else the value `S` reads.
#[derive(Clone, Copy)]
pub struct Nullable<S>(S);

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Nullable<S> {
    type Value = Option<S::Value>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_option(self)
    }
}

impl<'de, S: DeserializeSeed<'de>> Visitor<'de> for Nullable<S> {
    type Value = Option<S::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a value or null")
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(d).map(Some)
    }
}

pub fn nullable<S>(seed: S) -> Nullable<S> {
    Nullable(seed)
}

/// An array, each item read by `S`.
#[derive(Clone, Copy)]
pub struct List<S>(S);

impl<'de, S: DeserializeSeed<'de> + Copy> Kinds<'de> for List<S> {
    type Value = Vec<S::Value>;
    const EXPECTED: &'static str = "an array";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(binary::Items(self.0))
    }

    fn seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        Parts::default().tail(&mut a, self.0)
    }
}

pub fn list<S>(seed: S) -> Shape<List<S>> {
    Shape(List(seed))
}

/// An object, each value read by `S`, under its key, as pairs in the order they arrived; a
/// repeated key takes the later value. In binary, a list of key and value.
#[derive(Clone, Copy)]
pub struct Map<S>(S);

impl<'de, S: DeserializeSeed<'de> + Copy> Kinds<'de> for Map<S> {
    type Value = Vec<(String, S::Value)>;
    const EXPECTED: &'static str = "an object";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(binary::Items(binary::Pair(self.0)))
    }

    fn map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        while let Some(key) = a.next_key_seed(Text)? {
            Parts::other(&mut out, key.into_owned(), || a.next_value_seed(self.0))?;
        }
        Ok(out)
    }
}

pub fn map<S>(seed: S) -> Shape<Map<S>> {
    Shape(Map(seed))
}
