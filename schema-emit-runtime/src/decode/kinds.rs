//! [`Kinds`]: what a type reads of each kind of JSON value, and [`Shape`], the serde visitor and
//! seed that applies it. A kind a type doesn't read fails as [`Unfit::kind`] after the value
//! has been read through, so the deserializer stands past it and the value around it can go on.

use std::fmt;

use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, IgnoredAny, MapAccess, SeqAccess, Visitor,
};

use super::Unfit;

/// What a type reads. Each method takes the value of one kind and gives the std value or a
/// failure; the defaults fail as the wrong kind, so a type writes only the kinds it reads.
pub trait Kinds<'de>: Sized {
    type Value;
    /// What the type takes, for the failure of a value of another kind.
    const EXPECTED: &'static str;

    /// The failure of a value of a kind the type doesn't read.
    fn wrong<E: de::Error>() -> E {
        Unfit::kind(Self::EXPECTED).raise()
    }

    /// The value at `d` in a format that isn't self-describing, read by position: a record is
    /// its fields in order, an optional one an option, a list its length then its items.
    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error>;

    /// The value at `d`, which the type reads by its kind.
    fn read<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(Shape(self))
    }

    fn null<E: de::Error>(self) -> Result<Self::Value, E> {
        Err(Self::wrong())
    }

    fn bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Err(Self::wrong())
    }

    /// An integer, signed 128-bit holding every one of them but those past it.
    fn int<E: de::Error>(self, _: i128) -> Result<Self::Value, E> {
        Err(Self::wrong())
    }

    /// An integer past `i128::MAX`.
    fn huge<E: de::Error>(self, v: u128) -> Result<Self::Value, E> {
        #[allow(clippy::cast_precision_loss)]
        self.float(v as f64)
    }

    fn float<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Err(Self::wrong())
    }

    fn text<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Err(Self::wrong())
    }

    /// An array, whose items the type reads or ignores before it returns.
    fn seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        while a.next_element::<IgnoredAny>()?.is_some() {}
        Err(Self::wrong())
    }

    /// An object, whose entries the type reads or ignores before it returns.
    fn map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        while a.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Err(Self::wrong())
    }
}

/// A type's [`Kinds`] as the visitor and the seed that read it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shape<K>(pub K);

impl<'de, K: Kinds<'de>> DeserializeSeed<'de> for Shape<K> {
    type Value = K::Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<K::Value, D::Error> {
        match d.is_human_readable() {
            true => self.0.read(d),
            false => self.0.binary(d),
        }
    }
}

impl<'de, K: Kinds<'de>> Visitor<'de> for Shape<K> {
    type Value = K::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(K::EXPECTED)
    }

    fn visit_unit<E: de::Error>(self) -> Result<K::Value, E> {
        self.0.null()
    }

    fn visit_none<E: de::Error>(self) -> Result<K::Value, E> {
        self.0.null()
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<K::Value, D::Error> {
        d.deserialize_any(self)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<K::Value, D::Error> {
        d.deserialize_any(self)
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<K::Value, E> {
        self.0.bool(v)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<K::Value, E> {
        self.0.int(v.into())
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<K::Value, E> {
        self.0.int(v.into())
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<K::Value, E> {
        self.0.int(v)
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<K::Value, E> {
        match i128::try_from(v) {
            Ok(v) => self.0.int(v),
            Err(_) => self.0.huge(v),
        }
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<K::Value, E> {
        self.0.float(v)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<K::Value, E> {
        self.0.text(v)
    }

    fn visit_bytes<E: de::Error>(self, _: &[u8]) -> Result<K::Value, E> {
        Err(K::wrong())
    }

    fn visit_enum<A: EnumAccess<'de>>(self, _: A) -> Result<K::Value, A::Error> {
        Err(K::wrong())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<K::Value, A::Error> {
        self.0.seq(a)
    }

    fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<K::Value, A::Error> {
        self.0.map(a)
    }
}
