//! [`Any`]: a value of any kind, as `any_value::Value` holds one.

use std::string::String;

use any_value::Value;
use serde::de::{Deserialize, DeserializeSeed, Deserializer};

use super::Unfit;

/// A value of any kind. A format that isn't self-describing holds it as the text of its JSON, the
/// one place a binary form has text.
#[derive(Clone, Copy)]
pub struct Any;

impl<'de> DeserializeSeed<'de> for Any {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        match d.is_human_readable() {
            true => Value::deserialize(d),
            false => {
                let text = String::deserialize(d)?;
                serde_json::from_str(&text).map_err(|e| Unfit::from_json(&e).raise())
            }
        }
    }
}

pub fn any() -> Any {
    Any
}
