//! A second document's definitions, joined among a first's.

use serde_json::{Map, Value};

use super::defs;
use crate::Error;

/// `schema` with `other`'s definitions among its own `$defs`: `other`'s root as `name`, and each
/// of `other`'s own definitions by its name. A name both define is an error naming it.
pub fn join(schema: &Value, name: &str, other: &Value) -> Result<Value, Error> {
    let mut document = schema.clone();
    let object = document
        .as_object_mut()
        .ok_or_else(|| Error::invalid("a schema is an object"))?;
    let ours = object
        .entry("$defs")
        .or_insert_with(|| Value::Object(Map::new()));
    let ours = ours
        .as_object_mut()
        .ok_or_else(|| Error::invalid("`$defs` is an object"))?;
    let mut root = other.clone();
    if let Some(object) = root.as_object_mut() {
        object.remove("$defs");
    }
    for (key, value) in defs(other).into_iter().chain([(name.to_string(), root)]) {
        if ours.contains_key(&key) {
            return Err(Error::invalid(format!("both documents define {key}")));
        }
        ours.insert(key, value);
    }
    Ok(document)
}
