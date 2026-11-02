//! The keys a schema's scope evaluates, from its `properties` and the schemas applied to the same
//! object.

use serde_json::{Map, Value};

use super::{all_of, branches, Evaluated};
use crate::json_schema::Lower;
use crate::Error;

/// How many `$ref`s deep a scope reads before it counts as recursive.
const DEPTH: usize = 16;

pub(super) fn evaluated(lower: &Lower, s: &Map<String, Value>) -> Result<Evaluated, Error> {
    scope(lower, s, 0)
}

/// The names `sub` evaluates, `None` for every key.
fn names_of(
    lower: &Lower,
    sub: &Map<String, Value>,
    depth: usize,
) -> Result<Option<Vec<String>>, Error> {
    Ok(match scope(lower, sub, depth.saturating_add(1))? {
        Evaluated::Everything => None,
        Evaluated::Names(n) => Some(n),
    })
}

fn scope(lower: &Lower, s: &Map<String, Value>, depth: usize) -> Result<Evaluated, Error> {
    if s.contains_key("patternProperties") || s.contains_key("$dynamicRef") {
        return Err(Error::unsupported(
            "`unevaluatedProperties` beside `patternProperties` or `$dynamicRef`",
        ));
    }
    if (depth > 0 && s.contains_key("unevaluatedProperties"))
        || s.contains_key("additionalProperties")
    {
        return Ok(Evaluated::Everything);
    }
    let mut names: Vec<String> = match s.get("properties") {
        Some(Value::Object(p)) => p.keys().cloned().collect(),
        _ => Vec::new(),
    };
    if let Some(target) = s.get("$ref") {
        let def = target
            .as_str()
            .and_then(|t| lower.definition(t))
            .and_then(Value::as_object)
            .filter(|_| depth < DEPTH)
            .ok_or_else(|| Error::unsupported("`unevaluatedProperties` beside this `$ref`"))?;
        match names_of(lower, def, depth)? {
            None => return Ok(Evaluated::Everything),
            Some(n) => names.extend(n),
        }
    }
    for sub in all_of(s) {
        match names_of(lower, sub, depth)? {
            None => return Ok(Evaluated::Everything),
            Some(n) => names.extend(n),
        }
    }
    // Which branch passes isn't known here, so one may only name keys already read.
    for sub in branches(s) {
        match names_of(lower, sub, depth)? {
            Some(n) if n.iter().all(|x| names.contains(x)) => {}
            _ => {
                return Err(Error::unsupported(
                    "`unevaluatedProperties` beside a branch that evaluates keys",
                ))
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(Evaluated::Names(names))
}
