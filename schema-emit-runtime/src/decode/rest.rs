//! The keys of an open model that no field names and no check reads: kept as the text of their
//! members (`"k":v,"j":w`) as they arrived, and written back by [`members`].

use std::fmt;
use std::string::String;
use std::vec::Vec;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde_json::value::RawValue;

use super::union::Text;
use super::{Raw, Unfit};

/// The members read so far.
#[derive(Default)]
pub struct Unread(Vec<u8>);

impl Unread {
    /// The member under `key`, whose value `a` is at, read as text.
    pub fn push<'de, A: MapAccess<'de>>(&mut self, key: &str, a: &mut A) -> Result<(), A::Error> {
        let value = a.next_value::<Raw>()?;
        match self.0.is_empty() {
            true => self.0.reserve(
                key.len()
                    .saturating_add(value.get().len())
                    .saturating_add(8)
                    .max(64),
            ),
            false => self.0.push(b','),
        }
        serde_json::to_writer(&mut self.0, key).map_err(|e| Unfit::from_json(&e).raise())?;
        self.0.push(b':');
        self.0.extend_from_slice(value.get().as_bytes());
        Ok(())
    }

    /// The members as text, none as an empty string.
    pub fn text(self) -> String {
        String::from_utf8(self.0).unwrap_or_default()
    }
}

/// Writes each member of `text`, as [`Unread::text`] gave it, into `out`.
pub fn members<M: SerializeMap>(out: &mut M, text: &str) -> Result<(), M::Error> {
    if text.is_empty() {
        return Ok(());
    }
    let object = std::format!("{{{text}}}");
    let mut de = serde_json::Deserializer::from_str(&object);
    let mut each = Each { out, failed: None };
    let read = de.deserialize_any(&mut each);
    match each.failed {
        Some(e) => Err(e),
        None => read.map_err(|e| Unfit::from_json(&e).refuse()),
    }
}

struct Each<'m, M: SerializeMap> {
    out: &'m mut M,
    failed: Option<M::Error>,
}

impl<'de, M: SerializeMap> Visitor<'de> for &mut Each<'_, M> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("members")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        while let Some(k) = a.next_key_seed(Text)? {
            let v = a.next_value::<&RawValue>()?;
            if let Err(e) = self.out.serialize_entry(&*k, v) {
                self.failed = Some(e);
                return Err(de::Error::custom("not written"));
            }
        }
        Ok(())
    }

    refuse!("members"; unit none some bool i64 u64 f64 str bytes seq enum newtype);
}
