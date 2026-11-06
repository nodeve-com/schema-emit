//! The [`Kinds`] of the scalars: a boolean, null, a whole number of each width, a float of each
//! width, text, a timestamp, a duration, and bytes. Each has a constructor returning its [`Shape`],
//! which emitted code passes as the seed of a field.

use std::string::String;
use std::vec::Vec;

use base64::Engine as _;
use chrono::{DateTime, TimeDelta, Utc};
use serde::de::{self, Deserialize, Deserializer};

use super::{binary, Kinds, Shape, Unfit};

/// A boolean.
#[derive(Clone, Copy)]
pub struct Bool;

impl<'de> Kinds<'de> for Bool {
    type Value = bool;
    const EXPECTED: &'static str = "a boolean";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
        bool::deserialize(d)
    }

    fn bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
        Ok(v)
    }
}

/// Only null.
#[derive(Clone, Copy)]
pub struct Null;

impl<'de> Kinds<'de> for Null {
    type Value = any_value::Null;
    const EXPECTED: &'static str = "null";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<any_value::Null, D::Error> {
        <()>::deserialize(d).map(|()| any_value::Null)
    }

    fn null<E: de::Error>(self) -> Result<any_value::Null, E> {
        Ok(any_value::Null)
    }
}

/// Text.
#[derive(Clone, Copy)]
pub struct Text;

impl<'de> Kinds<'de> for Text {
    type Value = String;
    const EXPECTED: &'static str = "text";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<String, D::Error> {
        String::deserialize(d)
    }

    fn text<E: de::Error>(self, v: &str) -> Result<String, E> {
        Ok(v.into())
    }
}

/// RFC 3339 `date-time`, at UTC, as `any_value::parse_timestamp` reads it.
#[derive(Clone, Copy)]
pub struct Timestamp;

impl<'de> Kinds<'de> for Timestamp {
    type Value = DateTime<Utc>;
    const EXPECTED: &'static str = "an RFC 3339 timestamp";

    /// Whole seconds, then the nanoseconds past them.
    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<DateTime<Utc>, D::Error> {
        let (seconds, nanos) = <(i64, u32)>::deserialize(d)?;
        DateTime::from_timestamp(seconds, nanos).ok_or_else(past_range)
    }

    fn text<E: de::Error>(self, v: &str) -> Result<DateTime<Utc>, E> {
        any_value::parse_timestamp(v).map_err(|e| time_error(e, Self::wrong::<E>))
    }
}

/// The failure of text that any-value's reader refused: `wrong` where it is no instant or span,
/// else the range.
fn time_error<E: de::Error>(e: any_value::Error, wrong: fn() -> E) -> E {
    match e {
        any_value::Error::Range => past_range(),
        _ => wrong(),
    }
}

/// Why an instant or span that reads has no value.
fn past_range<E: de::Error>() -> E {
    Unfit::range("an instant or span past the range").raise()
}

/// A duration, `format: human-duration`, as `any_value::parse_duration` reads it.
#[derive(Clone, Copy)]
pub struct Duration;

impl<'de> Kinds<'de> for Duration {
    type Value = TimeDelta;
    const EXPECTED: &'static str = "a duration";

    /// As chrono writes one: whole seconds, then the nanoseconds past them.
    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<TimeDelta, D::Error> {
        TimeDelta::deserialize(d)
    }

    fn text<E: de::Error>(self, v: &str) -> Result<TimeDelta, E> {
        any_value::parse_duration(v).map_err(|e| time_error(e, Self::wrong::<E>))
    }
}

/// Padded base64 of the standard alphabet.
#[derive(Clone, Copy)]
pub struct Bytes;

impl<'de> Kinds<'de> for Bytes {
    type Value = Vec<u8>;
    const EXPECTED: &'static str = "padded base64";

    /// The raw bytes.
    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<Vec<u8>, D::Error> {
        d.deserialize_byte_buf(binary::Raw)
    }

    fn text<E: de::Error>(self, v: &str) -> Result<Vec<u8>, E> {
        base64::engine::general_purpose::STANDARD
            .decode(v)
            .map_err(|_| Self::wrong())
    }
}

pub fn boolean() -> Shape<Bool> {
    Shape(Bool)
}

pub fn null() -> Shape<Null> {
    Shape(Null)
}

pub fn text() -> Shape<Text> {
    Shape(Text)
}

pub fn timestamp() -> Shape<Timestamp> {
    Shape(Timestamp)
}

pub fn duration() -> Shape<Duration> {
    Shape(Duration)
}

pub fn bytes() -> Shape<Bytes> {
    Shape(Bytes)
}
