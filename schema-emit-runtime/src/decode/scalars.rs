//! The [`Kinds`] of the scalars: a boolean, null, a whole number of each width, a float of each
//! width, text, a timestamp, a duration, and bytes. Each has a constructor returning its [`Shape`],
//! which emitted code passes as the seed of a field.

use std::string::String;
use std::vec::Vec;

use base64::Engine as _;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
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
    type Value = ();
    const EXPECTED: &'static str = "null";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        <()>::deserialize(d)
    }

    fn null<E: de::Error>(self) -> Result<(), E> {
        Ok(())
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

/// RFC 3339 `date-time`, at UTC. A leap second reads as the next second, and a `:60` outside
/// 23:59 UTC fails.
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
        let at = DateTime::parse_from_rfc3339(v)
            .map_err(|_| Self::wrong::<E>())?
            .with_timezone(&Utc);
        let (seconds, nanos) = (at.timestamp(), at.timestamp_subsec_nanos());
        let (seconds, nanos) = match nanos.checked_sub(1_000_000_000) {
            Some(past) if (at.hour(), at.minute()) == (23, 59) => (seconds.saturating_add(1), past),
            Some(_) => return Err(Self::wrong()),
            None => (seconds, nanos),
        };
        DateTime::from_timestamp(seconds, nanos).ok_or_else(past_range)
    }
}

/// Why an instant or span that reads has no value.
fn past_range<E: de::Error>() -> E {
    Unfit::range("an instant or span past the range").raise()
}

/// A duration, `format: human-duration`: an optional sign, then `humantime`'s text (`1.5s`,
/// `90m`).
#[derive(Clone, Copy)]
pub struct Duration;

impl<'de> Kinds<'de> for Duration {
    type Value = TimeDelta;
    const EXPECTED: &'static str = "a duration";

    /// As chrono writes one: whole seconds, then the nanoseconds past them.
    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<TimeDelta, D::Error> {
        TimeDelta::deserialize(d)
    }

    #[allow(clippy::disallowed_methods)]
    fn text<E: de::Error>(self, v: &str) -> Result<TimeDelta, E> {
        let (negative, unsigned) = match v.as_bytes().first() {
            Some(b'-') => (true, v.get(1..).unwrap_or_default()),
            Some(b'+') => (false, v.get(1..).unwrap_or_default()),
            _ => (false, v),
        };
        let span = humantime::parse_duration(unsigned).map_err(|_| Self::wrong::<E>())?;
        let delta = TimeDelta::from_std(span).map_err(|_| past_range::<E>())?;
        let signed = match negative {
            true => TimeDelta::zero().checked_sub(&delta),
            false => Some(delta),
        };
        signed.ok_or_else(past_range)
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
