//! A schema with no `type` whose `enum` or `const` members share one JSON kind takes that kind.
//! Under `allOf`, the schema and its subschemas that list members must name the same kind, and
//! null is allowed only where every list holds it.

use serde_json::{Map, Value};

use crate::Error;

/// The one kind of a schema's choices, and whether null is among them.
pub(super) struct Kind {
    name: &'static str,
    nullable: bool,
}

impl Kind {
    /// The `type` keyword's value stating this kind.
    pub(super) fn type_value(&self) -> Value {
        if self.nullable {
            Value::from(vec![self.name, "null"])
        } else {
            Value::from(self.name)
        }
    }
}

/// The kind of the members `s` lists in `enum` or `const`; `None` where it lists none.
fn of_members(name: &str, s: &Map<String, Value>) -> Result<Option<Kind>, Error> {
    let members: Vec<&Value> = match (s.get("enum"), s.get("const")) {
        (Some(Value::Array(xs)), None) => xs.iter().collect(),
        (None, Some(v)) => vec![v],
        _ => return Ok(None),
    };
    let mut kind: Option<&'static str> = None;
    let mut nullable = false;
    for m in members {
        let k = match m {
            Value::Null => {
                nullable = true;
                continue;
            }
            Value::Bool(_) => "boolean",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Number(n) if n.is_f64() => "number",
            Value::Number(_) => "integer",
        };
        kind = match (kind, k) {
            (None, k) => Some(k),
            (Some(a), b) if a == b => Some(a),
            (Some("integer" | "number"), "integer" | "number") => Some("number"),
            _ => return Err(mixed(name)),
        };
    }
    Ok(Some(Kind {
        name: kind.unwrap_or("null"),
        nullable: nullable && kind.is_some(),
    }))
}

fn mixed(name: &str) -> Error {
    Error::unsupported(format!(
        "{name}: no type, and its choices are of more than one kind"
    ))
}

/// The kind `s` and `subs` agree on through their choices; `None` where none lists any.
pub(super) fn of(
    name: &str,
    s: &Map<String, Value>,
    subs: &[&Value],
) -> Result<Option<Kind>, Error> {
    let mut found: Option<Kind> = None;
    let own = std::iter::once(s).chain(subs.iter().filter_map(|v| v.as_object()));
    for m in own {
        let Some(k) = of_members(name, m)? else {
            continue;
        };
        found = Some(match found {
            None => k,
            Some(f) => merge(name, f, k)?,
        });
    }
    Ok(found)
}

fn merge(name: &str, a: Kind, b: Kind) -> Result<Kind, Error> {
    let name_of = match (a.name, b.name) {
        (x, y) if x == y => x,
        ("integer" | "number", "integer" | "number") => "number",
        _ => return Err(mixed(name)),
    };
    Ok(Kind {
        name: name_of,
        nullable: a.nullable && b.nullable,
    })
}
