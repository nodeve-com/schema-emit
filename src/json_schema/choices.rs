//! Area: choices and enums. `enum` and `const`, each a `OneOf` of the members a value of the base
//! can equal: `1.0` equals the integer 1, and a member of another kind is dropped, since no value
//! of the base equals it. Null is a member of a nullable type, and of the null type.
//!
//! A list or object member is read item by item and key by key against the base's types, as
//! JSON Schema equality compares them, so `1` equals `1.0` and key order is free. It is dropped
//! where no value of the base equals it: a member of another kind, or an extra item. A key no
//! field holds is refused, since a value may carry keys its model lacks, as are a map and a
//! tuple with trailing items, which have no reader.

use serde_json::Value;

use super::{Done, Key};
use crate::ir::{Base, Check, Lit, Num, Schema, Ty};
use crate::Error;

pub(super) fn keyword(k: &mut Key) -> Done {
    let members = match (k.key, k.value) {
        ("enum", Value::Array(xs)) => xs.as_slice(),
        ("const", v) => std::slice::from_ref(v),
        ("enum", _) => return k.refuse(),
        _ => return None,
    };
    let mut lits = Vec::new();
    for m in members {
        match member(&k.lower.out, k.ty, m) {
            Ok(Some(lit)) => lits.push(lit),
            Ok(None) => {}
            Err(e) => return Some(Err(e)),
        }
    }
    k.ty.checks.push(Check::OneOf(lits));
    Some(Ok(()))
}

/// `m` as a literal of `ty`, `None` where no value of `ty` equals it.
pub(super) fn member(schema: &Schema, ty: &Ty, m: &Value) -> Result<Option<Lit>, Error> {
    let base = &ty.base;
    Ok(match (base, m) {
        (_, Value::Null) if ty.nullable || *base == Base::Null => Some(Lit::Null),
        (Base::Any, _) => return Err(Error::unsupported("a choice of any value")),
        (Base::List(item), Value::Array(xs)) => {
            return all(schema, xs, xs.iter().map(|_| &**item)).map(|l| l.map(Lit::List))
        }
        (Base::Tuple { prefix, rest }, Value::Array(xs)) => {
            if rest.is_some() {
                return Err(Error::unsupported("a tuple choice with trailing items"));
            }
            return all(schema, xs, prefix.iter())
                .map(|l| l.filter(|_| xs.len() <= prefix.len()).map(Lit::List));
        }
        (Base::Model(i), Value::Object(kv)) => return object(schema, *i, kv),
        (Base::Map(_), Value::Object(_)) => {
            return Err(Error::unsupported("a map choice, which has no key reader"))
        }
        (Base::Bool, Value::Bool(b)) => Some(Lit::Bool(*b)),
        (Base::Int(_), Value::Number(_)) => match super::bounds::num(m) {
            Some(Num::Int(i)) => Some(Lit::Int(i)),
            Some(Num::Float(f)) if f == f.trunc() && f.abs() < 1.7e38 => Some(Lit::Int(f as i128)),
            _ => None,
        },
        (Base::Float(_), Value::Number(n)) => n.as_f64().map(Lit::Float),
        (Base::Text, Value::String(s)) => Some(Lit::Text(s.clone())),
        _ => None,
    })
}

/// Each of `xs` as a literal of the type paired with it; `None` where one has no equal.
fn all<'a>(
    schema: &Schema,
    xs: &[Value],
    tys: impl Iterator<Item = &'a Ty>,
) -> Result<Option<Vec<Lit>>, Error> {
    let mut lits = Vec::new();
    for (x, ty) in xs.iter().zip(tys) {
        match member(schema, ty, x)? {
            Some(lit) => lits.push(lit),
            None => return Ok(None),
        }
    }
    Ok(Some(lits))
}

/// An object as a literal of model `index`.
fn object(
    schema: &Schema,
    index: usize,
    kv: &serde_json::Map<String, Value>,
) -> Result<Option<Lit>, Error> {
    let Some(model) = schema.models.get(index) else {
        return Err(Error::invalid(format!("no model {index}")));
    };
    let mut fields = Vec::new();
    for (key, v) in kv {
        let Some(field) = model.fields.iter().find(|f| f.name == *key) else {
            return Err(Error::unsupported(
                "an object choice with a key no field holds",
            ));
        };
        match member(schema, &field.ty, v)? {
            Some(lit) => fields.push((key.clone(), lit)),
            None => return Ok(None),
        }
    }
    Ok(Some(Lit::Object(fields)))
}
