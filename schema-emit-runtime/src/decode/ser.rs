//! Writers: how a std value of each type serializes, composed the way the seeds are. A [`Write`]
//! says how to write a `T`; [`With`] pairs a value with its writer so it serializes. Emitted
//! `Serialize` impls build these from a field's type: an optional field that is absent is skipped
//! by the impl, `None` of a nullable type is null, a timestamp is RFC 3339, a duration is
//! seconds with a fraction and an `s`, bytes are padded base64. A format that isn't human-readable gets the
//! binary form (`decode::binary`): a timestamp is its seconds then nanoseconds, a duration as
//! chrono writes it, bytes raw, a map a list of key and value.

use std::string::String;
use std::vec::Vec;

use base64::Engine as _;
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde::ser::{Serialize, SerializeTuple, Serializer};

use super::Unfit;

/// How to write a `T`.
pub trait Write<T: ?Sized> {
    fn write<S: Serializer>(&self, v: &T, s: S) -> Result<S::Ok, S::Error>;
}

impl<T: ?Sized, W: Write<T>> Write<T> for &W {
    fn write<S: Serializer>(&self, v: &T, s: S) -> Result<S::Ok, S::Error> {
        (**self).write(v, s)
    }
}

/// A value and the writer that writes it.
pub struct With<'a, T: ?Sized, W>(pub &'a T, pub W);

impl<T: ?Sized, W: Write<T>> Serialize for With<'_, T, W> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.1.write(self.0, s)
    }
}

/// A type that serializes itself.
pub struct Plain;

impl<T: Serialize + ?Sized> Write<T> for Plain {
    fn write<S: Serializer>(&self, v: &T, s: S) -> Result<S::Ok, S::Error> {
        v.serialize(s)
    }
}

/// A timestamp, as RFC 3339 at UTC.
pub struct Stamp;

impl Write<DateTime<Utc>> for Stamp {
    fn write<S: Serializer>(&self, v: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        if !s.is_human_readable() {
            return (v.timestamp(), v.timestamp_subsec_nanos()).serialize(s);
        }
        s.serialize_str(&v.to_rfc3339_opts(SecondsFormat::AutoSi, true))
    }
}

/// A duration, as seconds with the fraction only where there is one (`1.5s`, `-2s`).
pub struct Span;

impl Write<TimeDelta> for Span {
    fn write<S: Serializer>(&self, v: &TimeDelta, s: S) -> Result<S::Ok, S::Error> {
        if !s.is_human_readable() {
            return v.serialize(s);
        }
        let (secs, frac) = (v.num_seconds(), v.subsec_nanos());
        let sign = if secs < 0 || frac < 0 { "-" } else { "" };
        let text = match frac {
            0 => std::format!("{sign}{}s", secs.unsigned_abs()),
            _ => {
                let digits = std::format!("{:09}", frac.unsigned_abs());
                let digits = digits.trim_end_matches('0');
                std::format!("{sign}{}.{digits}s", secs.unsigned_abs())
            }
        };
        s.serialize_str(&text)
    }
}

/// Bytes, as padded base64.
pub struct Base64;

impl Write<Vec<u8>> for Base64 {
    fn write<S: Serializer>(&self, v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        if !s.is_human_readable() {
            return s.serialize_bytes(v);
        }
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }
}

/// A list, each item written by `W`.
pub struct Seq<W>(pub W);

impl<T, W: Write<T>> Write<Vec<T>> for Seq<W> {
    fn write<S: Serializer>(&self, v: &Vec<T>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(|x| With(x, &self.0)))
    }
}

/// A map, each value written by `W`: an object, else a list of key and value.
pub struct Entries<W>(pub W);

impl<T, W: Write<T>> Write<Vec<(String, T)>> for Entries<W> {
    fn write<S: Serializer>(&self, v: &Vec<(String, T)>, s: S) -> Result<S::Ok, S::Error> {
        let entries = v.iter().map(|(k, x)| (k, With(x, &self.0)));
        match s.is_human_readable() {
            true => s.collect_map(entries),
            false => s.collect_seq(entries),
        }
    }
}

/// A value of any kind: as it is, else as the text of its JSON.
pub struct Dynamic;

impl Write<serde_json::Value> for Dynamic {
    fn write<S: Serializer>(&self, v: &serde_json::Value, s: S) -> Result<S::Ok, S::Error> {
        match s.is_human_readable() {
            true => v.serialize(s),
            false => match serde_json::to_string(v) {
                Ok(text) => s.serialize_str(&text),
                Err(e) => Err(Unfit::from_json(&e).refuse()),
            },
        }
    }
}

/// A value or null, the value written by `W`; in binary, an option.
pub struct Maybe<W>(pub W);

impl<T, W: Write<T>> Write<Option<T>> for Maybe<W> {
    fn write<S: Serializer>(&self, v: &Option<T>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(x) => s.serialize_some(&With(x, &self.0)),
            None => s.serialize_none(),
        }
    }
}

/// How a tuple writes one position, a [`Maybe`] prefix item or the [`Seq`] of the rest. The
/// rest's items join the array flat in text; in binary it is one element, a list.
pub trait Slot<T> {
    fn put<S: SerializeTuple>(&self, v: &T, human: bool, t: &mut S) -> Result<(), S::Error>;
}

impl<T, W: Write<T>> Slot<Option<T>> for Maybe<W> {
    fn put<S: SerializeTuple>(&self, v: &Option<T>, _: bool, t: &mut S) -> Result<(), S::Error> {
        t.serialize_element(&With(v, self))
    }
}

impl<T, W: Write<T>> Slot<Vec<T>> for Seq<W> {
    fn put<S: SerializeTuple>(&self, v: &Vec<T>, human: bool, t: &mut S) -> Result<(), S::Error> {
        match human {
            true => v
                .iter()
                .try_for_each(|x| t.serialize_element(&With(x, &self.0))),
            false => t.serialize_element(&With(v, self)),
        }
    }
}

/// A tuple, each position written by the slot at its place in `W`: an array, else its positions
/// in order.
pub struct Positional<W>(pub W);

impl Write<()> for Positional<()> {
    fn write<S: Serializer>(&self, _: &(), s: S) -> Result<S::Ok, S::Error> {
        s.serialize_tuple(0)?.end()
    }
}

macro_rules! positional {
    ($len:literal: $($a:ident $w:ident $i:tt),+) => {
        impl<$($a, $w: Slot<$a>),+> Write<($($a,)+)> for Positional<($($w,)+)> {
            fn write<S: Serializer>(&self, v: &($($a,)+), s: S) -> Result<S::Ok, S::Error> {
                let human = s.is_human_readable();
                let mut t = s.serialize_tuple($len)?;
                $(self.0.$i.put(&v.$i, human, &mut t)?;)+
                t.end()
            }
        }
    };
}

positional!(1: A0 W0 0);
positional!(2: A0 W0 0, A1 W1 1);
positional!(3: A0 W0 0, A1 W1 1, A2 W2 2);
positional!(4: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3);
positional!(5: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4);
positional!(6: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5);
positional!(7: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5, A6 W6 6);
positional!(8: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5, A6 W6 6, A7 W7 7);
positional!(9: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5, A6 W6 6, A7 W7 7, A8 W8 8);
positional!(10: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5, A6 W6 6, A7 W7 7, A8 W8 8, A9 W9 9);
positional!(11: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5, A6 W6 6, A7 W7 7, A8 W8 8, A9 W9 9, A10 W10 10);
positional!(12: A0 W0 0, A1 W1 1, A2 W2 2, A3 W3 3, A4 W4 4, A5 W5 5, A6 W6 6, A7 W7 7, A8 W8 8, A9 W9 9, A10 W10 10, A11 W11 11);
