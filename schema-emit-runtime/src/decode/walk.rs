//! Reading JSON text in one pass. A [`Reader`] moves through the text value by value: it reads an
//! array's and an object's brackets, commas and colons itself, and hands each string, number and
//! literal to serde_json, which reads that one token in place.

use std::borrow::Cow;
use std::fmt;
use std::string::String;

use serde::de::{self, Deserialize, Visitor};

use super::Unfit;

/// Why text fails where it isn't one JSON value.
const NOT_JSON: &str = "text that isn't one JSON value";
/// Why text fails where it nests deeper than [`DEPTH`].
const TOO_DEEP: &str = "arrays and objects nested past the depth limit";
/// How deep arrays and objects in decoded text may nest, as serde_json allows.
const DEPTH: u8 = 128;

/// JSON text, read from a place in it: the next value to read starts there.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    text: &'a str,
    at: usize,
    /// How many arrays and objects the place is inside.
    depth: u8,
}

impl<'a> Reader<'a> {
    pub(super) fn new(text: &'a str) -> Reader<'a> {
        Reader {
            text,
            at: 0,
            depth: 0,
        }
    }

    /// The next byte past whitespace, the reader moved to it.
    pub(super) fn peek(&mut self) -> Option<u8> {
        let bytes = self.text.as_bytes();
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = bytes.get(self.at) {
            self.at = self.at.saturating_add(1);
        }
        bytes.get(self.at).copied()
    }

    /// Past the byte `b`, where the next byte past whitespace is `b`.
    fn eat(&mut self, b: u8) -> Result<(), Unfit> {
        match self.peek() == Some(b) {
            true => {
                self.at = self.at.saturating_add(1);
                Ok(())
            }
            false => Err(Unfit::other(NOT_JSON)),
        }
    }

    /// The string, number or literal here, read by serde_json as one `T`.
    pub(super) fn token<T: Deserialize<'a>>(&mut self) -> Result<T, Unfit> {
        let rest = self.text.get(self.at..).unwrap_or_default();
        let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<T>();
        let token = stream.next().and_then(Result::ok);
        let token = token.ok_or_else(|| Unfit::other(NOT_JSON))?;
        self.at = self.at.saturating_add(stream.byte_offset());
        Ok(token)
    }

    /// Nothing but whitespace past here.
    pub(super) fn end(&mut self) -> Result<(), Unfit> {
        match self.peek() {
            None => Ok(()),
            Some(_) => Err(Unfit::other(NOT_JSON)),
        }
    }
}

/// An array or an object being read, part by part: its reader, the byte that closes it, how many
/// parts it has begun, and whether it has closed.
pub struct Parts<'r, 'a> {
    pub(super) r: &'r mut Reader<'a>,
    close: u8,
    begun: usize,
    closed: bool,
}

impl<'r, 'a> Parts<'r, 'a> {
    /// The array (`open` `[`) or object (`{`) here, or `expected` where another kind is.
    pub(super) fn open(
        r: &'r mut Reader<'a>,
        open: u8,
        expected: &'static str,
    ) -> Result<Parts<'r, 'a>, Unfit> {
        if r.peek() != Some(open) {
            return Err(Unfit::kind(expected));
        }
        r.eat(open)?;
        r.depth = r.depth.saturating_add(1);
        if r.depth > DEPTH {
            return Err(Unfit::other(TOO_DEEP));
        }
        let close = if open == b'[' { b']' } else { b'}' };
        Ok(Parts {
            r,
            close,
            begun: 0,
            closed: false,
        })
    }

    /// The position of the next part, the reader moved to it: `None` once the structure closes.
    pub(super) fn next(&mut self) -> Result<Option<usize>, Unfit> {
        if self.closed {
            return Ok(None);
        }
        if self.r.peek() == Some(self.close) {
            self.r.eat(self.close)?;
            self.r.depth = self.r.depth.saturating_sub(1);
            self.closed = true;
            return Ok(None);
        }
        if self.begun > 0 {
            self.r.eat(b',')?;
        }
        if matches!(self.r.peek(), None | Some(b']' | b'}')) {
            return Err(Unfit::other(NOT_JSON));
        }
        let at = self.begun;
        self.begun = self.begun.saturating_add(1);
        Ok(Some(at))
    }

    /// An object entry's key, the reader moved past its colon to the value.
    pub(super) fn key(&mut self) -> Result<Str<'a>, Unfit> {
        if self.r.peek() != Some(b'"') {
            return Err(Unfit::other(NOT_JSON));
        }
        let key = self.r.token::<Str>()?;
        self.r.eat(b':')?;
        Ok(key)
    }
}

/// A JSON string's text, borrowed from the text where it has no escapes.
pub(super) struct Str<'a>(pub(super) Cow<'a, str>);

impl<'a> Deserialize<'a> for Str<'a> {
    fn deserialize<D: de::Deserializer<'a>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_str(StrText)
    }
}

struct StrText;

impl<'a> Visitor<'a> for StrText {
    type Value = Str<'a>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string")
    }

    fn visit_borrowed_str<E>(self, text: &'a str) -> Result<Str<'a>, E> {
        Ok(Str(Cow::Borrowed(text)))
    }

    fn visit_str<E>(self, text: &str) -> Result<Str<'a>, E> {
        Ok(Str(Cow::Owned(String::from(text))))
    }
}
