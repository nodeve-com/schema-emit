//! What the decode functions that schema-emit writes link (feature `decode`): readers of the JSON
//! value at a [`Reader`]'s place as a std value of one kind, and [`Unfit`], which says where a
//! value fails to decode and why. The structured readers (list, map, tuple, model fields) are in
//! `shapes`; each adds its step to the path of a failure inside it. `walk` reads the text.

use std::borrow::Cow;
use std::fmt;
use std::string::String;
use std::vec::Vec;

use base64::Engine as _;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use serde_json::Number;

mod shapes;
mod walk;

pub use shapes::*;
pub use walk::{Parts, Reader};

/// Why a value has no std form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The value is another kind than the type takes; the name says which kind it takes.
    Kind(&'static str),
    /// The value is that kind but no std value holds it: an integer past 64 bits, a key no field
    /// names, a field it lacks, or an instant or span past chrono's range.
    Other(&'static str),
}

/// Why a model's field fails where the value lacks it.
pub(crate) const ABSENT: &str = "a required field is absent";
/// Why a key fails where no field of the model names it.
pub(crate) const UNNAMED: &str = "a key no field names";

/// A value that doesn't decode: its [`Why`], and where, as `Model.field[2].key`. A path built
/// outward, one step at a time, as the failure returns through the readers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unfit {
    pub path: String,
    pub why: Why,
}

impl Unfit {
    /// A value of another kind than `expected`, at the place the reader is.
    pub fn kind(expected: &'static str) -> Unfit {
        Unfit {
            path: String::new(),
            why: Why::Kind(expected),
        }
    }

    /// A value of the right kind no std value holds, at the place the reader is.
    pub fn other(reason: &'static str) -> Unfit {
        Unfit {
            path: String::new(),
            why: Why::Other(reason),
        }
    }

    /// Whether the value is of another kind than the type takes.
    pub fn is_kind(&self) -> bool {
        matches!(self.why, Why::Kind(_))
    }

    /// The failure of field or key `name` of the value this failure is in.
    pub fn field(mut self, name: &str) -> Unfit {
        let dot = if name.is_empty() { "" } else { "." };
        self.path = std::format!("{dot}{name}{}", self.path);
        self
    }

    /// The failure of item `index` of the list this failure is in.
    pub fn index(mut self, index: usize) -> Unfit {
        self.path = std::format!("[{index}]{}", self.path);
        self
    }

    /// The failure of the model `name`, a value's root.
    pub fn model(mut self, name: &str) -> Unfit {
        self.path = std::format!("{name}{}", self.path);
        self
    }

    /// The failure of two that say more: the one whose value fit further, the first of equals.
    pub fn either(first: Option<Unfit>, next: Unfit) -> Unfit {
        match first {
            Some(f) if f.reach() >= next.reach() => f,
            _ => next,
        }
    }

    /// How far the value fit before failing: least, another kind at its root; then the right
    /// kind at its root, or a field it lacks, or a key no field names; most, inside what it holds.
    fn reach(&self) -> u8 {
        let one_step = self.path.starts_with('.')
            && !self.path.get(1..).unwrap_or_default().contains(['.', '[']);
        match (&self.why, self.path.is_empty()) {
            (Why::Kind(_), true) => 0,
            (Why::Other(_), true) => 1,
            (Why::Other(r), false) if one_step && [ABSENT, UNNAMED].contains(r) => 1,
            _ => 2,
        }
    }
}

impl fmt::Display for Unfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (what, why) = match self.why {
            Why::Kind(expected) => ("expected ", expected),
            Why::Other(reason) => ("", reason),
        };
        match self.path.as_str() {
            "" => write!(f, "{what}{why}"),
            path => write!(f, "{path}: {what}{why}"),
        }
    }
}

impl std::error::Error for Unfit {}

pub fn null(r: &mut Reader<'_>) -> Result<(), Unfit> {
    match r.peek() {
        Some(b'n') => r.token(),
        _ => Err(Unfit::kind("null")),
    }
}

pub fn boolean(r: &mut Reader<'_>) -> Result<bool, Unfit> {
    match r.peek() {
        Some(b't' | b'f') => r.token(),
        _ => Err(Unfit::kind("a boolean")),
    }
}

pub fn text(r: &mut Reader<'_>) -> Result<String, Unfit> {
    string(r, "text").map(Cow::into_owned)
}

/// The text of a JSON string, borrowed where it has no escapes.
fn string<'a>(r: &mut Reader<'a>, expected: &'static str) -> Result<Cow<'a, str>, Unfit> {
    match r.peek() {
        Some(b'"') => r.token::<walk::Str>().map(|s| s.0),
        _ => Err(Unfit::kind(expected)),
    }
}

/// A JSON number, or `expected` where another kind is here.
fn number(r: &mut Reader<'_>, expected: &'static str) -> Result<Number, Unfit> {
    match r.peek() {
        Some(b'-' | b'0'..=b'9') => r.token(),
        _ => Err(Unfit::kind(expected)),
    }
}

pub fn float(r: &mut Reader<'_>) -> Result<f64, Unfit> {
    let n = number(r, "a number")?;
    n.as_f64().ok_or(Unfit::other("a number no f64 holds"))
}

pub fn int(r: &mut Reader<'_>) -> Result<i64, Unfit> {
    let whole = whole(r)?;
    i64::try_from(whole).map_err(|_| Unfit::other("an integer past 64 bits"))
}

pub fn uint(r: &mut Reader<'_>) -> Result<u64, Unfit> {
    let whole = whole(r)?;
    u64::try_from(whole).map_err(|_| Unfit::other("an integer past 64 bits"))
}

/// A whole number, which may be written as a float, such as `1.0`.
fn whole(r: &mut Reader<'_>) -> Result<i128, Unfit> {
    let n = number(r, "an integer")?;
    if let Some(i) = n.as_i64() {
        return Ok(i128::from(i));
    }
    if let Some(u) = n.as_u64() {
        return Ok(i128::from(u));
    }
    match n.as_f64() {
        Some(f) if f != f.trunc() => Err(Unfit::kind("an integer")),
        // Whole, and past 64 bits where it isn't below 2^64 in magnitude.
        Some(f) if f.abs() < 1.8e19 => Ok(f as i128),
        _ => Err(Unfit::other("an integer past 64 bits")),
    }
}

/// RFC 3339 `date-time`, at UTC. A leap second reads as the next second, and a `:60` outside
/// 23:59 UTC fails.
pub fn timestamp(r: &mut Reader<'_>) -> Result<DateTime<Utc>, Unfit> {
    let text = string(r, "an RFC 3339 timestamp")?;
    let at = DateTime::parse_from_rfc3339(&text)
        .map_err(|_| Unfit::kind("an RFC 3339 timestamp"))?
        .with_timezone(&Utc);
    let (seconds, nanos) = (at.timestamp(), at.timestamp_subsec_nanos());
    let (seconds, nanos) = match nanos.checked_sub(1_000_000_000) {
        Some(past) if (at.hour(), at.minute()) == (23, 59) => (seconds.saturating_add(1), past),
        Some(_) => return Err(Unfit::kind("an RFC 3339 timestamp")),
        None => (seconds, nanos),
    };
    DateTime::from_timestamp(seconds, nanos).ok_or(Unfit::other(PAST_RANGE))
}

/// A duration, `format: human-duration`: an optional sign, then `humantime`'s text (`1.5s`,
/// `90m`).
#[allow(clippy::disallowed_methods)]
pub fn duration(r: &mut Reader<'_>) -> Result<TimeDelta, Unfit> {
    let text = string(r, "a duration")?;
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, text.get(1..).unwrap_or_default()),
        Some(b'+') => (false, text.get(1..).unwrap_or_default()),
        _ => (false, &*text),
    };
    let span = humantime::parse_duration(unsigned).map_err(|_| Unfit::kind("a duration"))?;
    let delta = TimeDelta::from_std(span).map_err(|_| Unfit::other(PAST_RANGE))?;
    let signed = match negative {
        true => TimeDelta::zero().checked_sub(&delta),
        false => Some(delta),
    };
    signed.ok_or(Unfit::other(PAST_RANGE))
}

/// Why an instant or span that reads has no value.
const PAST_RANGE: &str = "an instant or span past the range";

/// Padded base64 of the standard alphabet.
pub fn bytes(r: &mut Reader<'_>) -> Result<Vec<u8>, Unfit> {
    base64::engine::general_purpose::STANDARD
        .decode(&*string(r, "padded base64")?)
        .map_err(|_| Unfit::kind("padded base64"))
}

#[cfg(test)]
mod tests;
