//! A union reads its value once as text ([`Raw`]), then tries each branch on that text through
//! [`Pick`]: the first branch that decodes and whose check passes is the value; else the first
//! that decodes, so its check says why; else the failure of the branch the value fit furthest.

use std::borrow::Cow;
use std::fmt;
use std::string::String;

use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor};

use super::skip::whole;
use super::skips::{mark, restore, take, Taken};
use super::Any;
use super::Unfit;

/// The name under which serde_json hands a deserializer the text of one value.
pub(super) const RAW: &str = "$serde_json::private::RawValue";

/// The text of one JSON value: a slice of the input where the input is JSON text, else the value
/// written back to text.
pub struct Raw<'de>(Cow<'de, str>);

impl Raw<'_> {
    pub fn get(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Raw<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_newtype_struct(RAW, RawText)
    }
}

struct RawText;

impl<'de> Visitor<'de> for RawText {
    type Value = Raw<'de>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a value")
    }

    /// Where the deserializer holds the text: its one entry.
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Raw<'de>, A::Error> {
        a.next_key::<IgnoredAny>()?;
        a.next_value_seed(Text).map(Raw)
    }

    /// Where it doesn't: the value, written as text.
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Raw<'de>, D::Error> {
        let text = serde_json::to_string(&Any.deserialize(d)?)
            .map_err(|e| Unfit::from_json(&e).raise())?;
        Ok(Raw(Cow::Owned(text)))
    }
}

/// A string, borrowed from the input where it can be.
pub(super) struct Text;

impl<'de> DeserializeSeed<'de> for Text {
    type Value = Cow<'de, str>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Text {
    type Value = Cow<'de, str>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("text")
    }

    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        Ok(Cow::Borrowed(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Cow::Owned(v.into()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(Cow::Owned(v))
    }

    refuse!("text"; unit none some bool i64 u64 f64 bytes seq map enum newtype);
}

/// The JSON kinds a branch takes, as the bits of a mask.
pub mod kind {
    pub const NULL: u8 = 1;
    pub const BOOL: u8 = 2;
    pub const NUMBER: u8 = 4;
    pub const TEXT: u8 = 8;
    pub const ARRAY: u8 = 16;
    pub const OBJECT: u8 = 32;
    pub const ANY: u8 = 63;

    /// The kind of the value that starts with `byte`.
    pub(super) fn of(byte: Option<u8>) -> u8 {
        match byte {
            Some(b'{') => OBJECT,
            Some(b'[') => ARRAY,
            Some(b'"') => TEXT,
            Some(b't' | b'f') => BOOL,
            Some(b'n') => NULL,
            Some(b'-' | b'0'..=b'9') => NUMBER,
            _ => ANY,
        }
    }

    /// What a branch of the kinds in `mask` takes, for the failure of a value of another kind.
    pub(super) fn expected(mask: u8) -> &'static str {
        match mask {
            m if m & OBJECT != 0 => "an object",
            m if m & ARRAY != 0 => "an array",
            m if m & TEXT != 0 => "text",
            m if m & NUMBER != 0 => "a number",
            m if m & BOOL != 0 => "a boolean",
            _ => "null",
        }
    }
}

/// The choice among a union's branches, made as each is tried in order on the value's text.
pub struct Pick<'t, T> {
    text: &'t str,
    kind: u8,
    held: Option<T>,
    first: Option<T>,
    /// The skips of the branch that holds, and of the first that decoded.
    skips: (Taken, Taken),
    why: Option<Unfit>,
}

impl<'t, T> Pick<'t, T> {
    pub fn new(text: &'t str) -> Self {
        Pick {
            text,
            kind: kind::of(text.trim_start().bytes().next()),
            held: None,
            first: None,
            skips: Default::default(),
            why: None,
        }
    }

    /// Tries a branch that takes the kinds in `mask`: `seed` reads the text, and `wrap` makes
    /// the union's value of it with whether that value passes the union's check. A branch the
    /// value's kind doesn't fit isn't read, and once a branch holds, the rest aren't tried.
    pub fn branch<S: DeserializeSeed<'t>>(
        &mut self,
        mask: u8,
        seed: S,
        wrap: impl FnOnce(S::Value) -> (T, bool),
    ) {
        if self.held.is_some() {
            return;
        }
        if mask & self.kind == 0 {
            self.fail(Unfit::kind(kind::expected(mask)));
            return;
        }
        let at = mark();
        let mut de = serde_json::Deserializer::from_str(self.text);
        let read = whole(|| seed.deserialize(&mut de).and_then(|v| de.end().map(|()| v)));
        let noted = take(at);
        match read {
            Ok(v) => {
                let (value, holds) = wrap(v);
                match (holds, self.first.is_some()) {
                    (true, _) => (self.held, self.skips.0) = (Some(value), noted),
                    (false, false) => (self.first, self.skips.1) = (Some(value), noted),
                    (false, true) => {}
                }
            }
            Err(e) => self.fail(Unfit::raised().unwrap_or_else(|| Unfit::from_json(&e))),
        }
    }

    /// Keeps `failure`, or the one of it and the earlier that says more.
    fn fail(&mut self, failure: Unfit) {
        self.why = Some(Unfit::either(self.why.take(), failure));
    }

    /// The branch that holds, else the first that decoded, else the failure.
    pub fn finish<E: de::Error>(self) -> Result<T, E> {
        let (value, noted) = match self.held {
            Some(v) => (Some(v), self.skips.0),
            None => (self.first, self.skips.1),
        };
        match value {
            Some(v) => {
                restore(noted);
                Ok(v)
            }
            None => Err(self
                .why
                .unwrap_or_else(|| Unfit::kind("one of the union's branches"))
                .raise()),
        }
    }
}
