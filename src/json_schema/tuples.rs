//! Area: tuples. `prefixItems` states a type per position; `items` applies past them: a schema,
//! `true` (any kind, unchecked), or `false` (no items past the prefix, so the array is
//! fixed-length). Positions and past-prefix items all of one type are one list. Otherwise the
//! array is a tuple of its positions' types, then a trailing list of `items` where it may hold
//! more. A tuple whose positions differ in kind is mixed-kind, which no expression reads whole,
//! so lowering warns of it.

use std::mem::discriminant;

use serde_json::{Map, Value};

use super::Lower;
use crate::ir::{Base, Ty};
use crate::Error;

pub(super) fn base(lower: &mut Lower, name: &str, s: &Map<String, Value>) -> Result<Base, Error> {
    let Some(Value::Array(prefix)) = s.get("prefixItems") else {
        return Err(Error::invalid(format!(
            "{name}: `prefixItems` is not an array"
        )));
    };
    let mut tys = Vec::new();
    for (i, p) in prefix.iter().enumerate() {
        tys.push(lower.ty(&format!("{name}[{i}]"), p)?);
    }
    let rest = match s.get("items") {
        None | Some(Value::Bool(true)) => Some(Ty::of(Base::Any)),
        Some(Value::Bool(false)) => None,
        Some(items) => Some(lower.ty(&format!("{name}[]"), items)?),
    };
    if let Some(item) = &rest {
        if tys.iter().all(|t| t == item) {
            return Ok(Base::List(Box::new(item.clone())));
        }
    }
    // `Any` is every kind at once, so it differs from no kind of value.
    let kinds: Vec<_> = tys
        .iter()
        .chain(rest.iter())
        .filter(|t| t.base != Base::Any)
        .map(|t| discriminant(&t.base))
        .collect();
    if kinds.first().is_some_and(|f| kinds.iter().any(|k| k != f)) {
        lower.out.warnings.push(format!(
            "{name}: a mixed-kind tuple; no expression reads it whole"
        ));
    }
    Ok(Base::Tuple {
        prefix: tys,
        rest: rest.map(Box::new),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use crate::json_schema::lower;

    /// The warnings lowering a model with one field of `schema` gives.
    fn warnings(schema: serde_json::Value) -> Vec<String> {
        let model = json!({"type": "object", "properties": {"a": schema}});
        let (s, _) = lower("Value", &model, &Map::new(), &[]).unwrap();
        s.warnings
    }

    #[test]
    fn a_mixed_kind_tuple_warns() {
        let w = warnings(json!({"type": "array",
            "prefixItems": [{"type": "integer"}, {"type": "string"}], "items": false}));
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn one_kind_warns_not() {
        let w = warnings(json!({"type": "array",
            "prefixItems": [{"type": "integer"}, {"type": "integer", "minimum": 2}],
            "items": false}));
        assert!(w.is_empty(), "{w:?}");
    }
}
