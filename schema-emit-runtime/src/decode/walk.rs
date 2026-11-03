//! Reading JSON text with serde_json's reader. A value is its own text: an array's items and an
//! object's entries come back as slices of the text they sit in, unparsed, so a reader decodes
//! each part from its slice, and a union tries each branch on the same one.

use std::borrow::Cow;
use std::fmt;
use std::string::String;

use serde::de::{self, Deserialize, Deserializer as _, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;

use super::Unfit;

/// Why text fails where it isn't one JSON value.
const NOT_JSON: &str = "text that isn't one JSON value";
/// Why text fails where it nests deeper than [`DEPTH`].
const TOO_DEEP: &str = "arrays and objects nested past the depth limit";
/// How deep arrays and objects in decoded text may nest, as serde_json allows.
const DEPTH: u8 = 128;

/// The first byte of the value `v` holds, past whitespace.
pub(super) fn first(v: &str) -> Option<u8> {
    v.bytes().find(|b| !b" \t\n\r".contains(b))
}

/// `v` read as one `T`, where its first byte says it is of `T`'s kind.
pub(super) fn parse<'a, T: Deserialize<'a>>(v: &'a str) -> Result<T, Unfit> {
    serde_json::from_str(v).map_err(|_| Unfit::other(NOT_JSON))
}

/// The text of a JSON string, borrowed where it has no escapes.
pub(super) fn string<'a>(v: &'a str, expected: &'static str) -> Result<Cow<'a, str>, Unfit> {
    match first(v) {
        Some(b'"') if !v.contains('\\') => parse::<&str>(v).map(Cow::Borrowed),
        Some(b'"') => parse::<String>(v).map(Cow::Owned),
        _ => Err(Unfit::kind(expected)),
    }
}

/// Each item of the array (`open` `[`) or entry of the object (`{`) `v`, in order, to `each`: an
/// entry's key, none for an item, and the part's text. The first failure of `each` ends the walk.
pub(super) fn walk<'a>(
    v: &'a str,
    open: u8,
    expected: &'static str,
    each: impl FnMut(Option<&str>, &'a str) -> Result<(), Unfit>,
) -> Result<(), Unfit> {
    if first(v) != Some(open) {
        return Err(Unfit::kind(expected));
    }
    let mut walk = Walk { each, unfit: None };
    let mut reader = serde_json::Deserializer::from_str(v);
    let read = reader
        .deserialize_any(&mut walk)
        .and_then(|()| reader.end());
    match (walk.unfit, read) {
        (Some(unfit), _) => Err(unfit),
        (None, Ok(())) => Ok(()),
        (None, Err(_)) => Err(Unfit::other(NOT_JSON)),
    }
}

/// Text whose arrays and objects nest no deeper than [`DEPTH`], so the readers, which go one
/// call deeper for each, stay within the stack.
pub(super) fn shallow(v: &str) -> Result<(), Unfit> {
    let (mut depth, mut text, mut escaped) = (0_u8, false, false);
    for b in v.bytes() {
        match (text, escaped, b) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') | (false, _, b'"') => text = !text,
            (false, _, b'[' | b'{') => {
                depth = depth.saturating_add(1);
                if depth > DEPTH {
                    return Err(Unfit::other(TOO_DEEP));
                }
            }
            (false, _, b']' | b'}') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

/// What walks a structure's parts: `each` takes each, and `unfit` keeps the failure that ended
/// the walk.
struct Walk<F> {
    each: F,
    unfit: Option<Unfit>,
}

impl<'a, F: FnMut(Option<&str>, &'a str) -> Result<(), Unfit>> Walk<F> {
    fn step<E: de::Error>(&mut self, key: Option<&str>, part: &'a RawValue) -> Result<(), E> {
        (self.each)(key, part.get()).map_err(|unfit| {
            self.unfit = Some(unfit);
            E::custom("")
        })
    }
}

impl<'a, F: FnMut(Option<&str>, &'a str) -> Result<(), Unfit>> Visitor<'a> for &mut Walk<F> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array or an object")
    }

    fn visit_seq<A: SeqAccess<'a>>(self, mut items: A) -> Result<(), A::Error> {
        while let Some(item) = items.next_element::<&RawValue>()? {
            self.step(None, item)?;
        }
        Ok(())
    }

    fn visit_map<A: MapAccess<'a>>(self, mut entries: A) -> Result<(), A::Error> {
        while let Some(Key(key)) = entries.next_key()? {
            let value = entries.next_value::<&RawValue>()?;
            self.step(Some(&key), value)?;
        }
        Ok(())
    }
}

/// An object's key, borrowed from the text where it has no escapes.
struct Key<'a>(Cow<'a, str>);

impl<'a> Deserialize<'a> for Key<'a> {
    fn deserialize<D: de::Deserializer<'a>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_str(KeyText)
    }
}

struct KeyText;

impl<'a> Visitor<'a> for KeyText {
    type Value = Key<'a>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a key")
    }

    fn visit_borrowed_str<E>(self, key: &'a str) -> Result<Key<'a>, E> {
        Ok(Key(Cow::Borrowed(key)))
    }

    fn visit_str<E>(self, key: &str) -> Result<Key<'a>, E> {
        Ok(Key(Cow::Owned(key.into())))
    }
}
