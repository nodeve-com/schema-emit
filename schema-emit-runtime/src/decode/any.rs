//! [`Any`]: a value of any kind, as serde_json holds one. Its visitor takes every kind itself, so
//! the deserializer never formats a failure of a kind it doesn't take.

use std::fmt;
use std::string::String;
use std::vec::Vec;

use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use super::union::Text;
use super::Unfit;

/// A value of any kind, as serde_json holds one. A format that isn't self-describing holds it
/// as the text of its JSON, the one place a binary form has text.
#[derive(Clone, Copy)]
pub struct Any;

impl<'de> DeserializeSeed<'de> for Any {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        match d.is_human_readable() {
            true => d.deserialize_any(self),
            false => {
                let text = String::deserialize(d)?;
                serde_json::from_str(&text).map_err(|e| Unfit::from_json(&e).raise())
            }
        }
    }
}

impl<'de> Visitor<'de> for Any {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Ok(Number::from_f64(v).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = a.next_element_seed(Any)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut members = Map::new();
        while let Some(key) = a.next_key_seed(Text)? {
            members.insert(key.into_owned(), a.next_value_seed(Any)?);
        }
        Ok(Value::Object(members))
    }

    refuse!("a JSON value"; bytes enum newtype);
}

pub fn any() -> Any {
    Any
}
