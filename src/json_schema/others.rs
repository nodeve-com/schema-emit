//! Area: keys no field names. An object may hold keys its `properties` doesn't list, and these
//! keywords judge them: `patternProperties`, `additionalProperties` as a schema, `propertyNames`,
//! `dependentSchemas`, a count of properties on an open object, and `required` or
//! `dependentRequired` naming a key `properties` lacks. Such an object's model holds
//! [`Model::others`], the type of those keys' values.
//!
//! The values of keys no field names share one type, so every rule giving them a value check
//! must agree on a base: `patternProperties` and `additionalProperties` state it. A boolean
//! schema fits any. A rule judging a value under a key of no stated type, as a `dependentSchemas`
//! subschema does, is refused.
//!
//! `patternProperties` is refused where a pattern matches a field's name: the field's type and
//! the pattern's would both hold. `additionalProperties` reaches the keys no field and no
//! pattern names. `propertyNames` reads each key present, a field's too.
//!
//! On a value not an object these keywords check nothing.

use serde_json::{json, Value};

use super::{boolean, branches, Done, Key, Lower};
use crate::ir::{Base, Check, Model, Ty};
use crate::Error;

mod needed;

pub use needed::branch_names;
use needed::patterns;
pub(super) use needed::{closed, counted, needed, reads};

pub(super) fn keyword(k: &mut Key) -> Done {
    let named = matches!(
        k.key,
        "patternProperties" | "propertyNames" | "dependentSchemas" | "required"
    );
    let Base::Model(m) = k.ty.base else {
        return named.then_some(Ok(()));
    };
    match k.key {
        "required" => Some(required(k, m)),
        "patternProperties" => Some(pattern_properties(k, m)),
        "additionalProperties" => additional(k, m),
        "propertyNames" => Some(names(k)),
        "dependentSchemas" => Some(dependent_schemas(k)),
        _ => None,
    }
}

/// The model `m`.
fn model(lower: &Lower, m: usize) -> Result<&Model, Error> {
    lower
        .out
        .models
        .get(m)
        .ok_or_else(|| Error::invalid(format!("no model {m}")))
}

/// Whether model `m` holds keys no field names, else why a rule for them is refused.
fn held(lower: &Lower, m: usize, key: &str) -> Result<(), Error> {
    match model(lower, m)?.others {
        Some(_) => Ok(()),
        None => Err(Error::unsupported(format!(
            "`{key}` beside `$ref`: keys no field names"
        ))),
    }
}

/// Lets the model's other keys' value type be `ty`'s: the first base a rule states, which every
/// other must equal. A boolean schema's base is any.
pub(super) fn adopt(lower: &mut Lower, m: usize, ty: &Ty) -> Result<(), Error> {
    if ty.base == Base::Any {
        return Ok(());
    }
    let slot = lower
        .out
        .models
        .get_mut(m)
        .and_then(|x| x.others.as_mut())
        .ok_or_else(|| Error::unsupported("a rule for keys no field names, with none held"))?;
    if slot.base == Base::Any {
        slot.base = ty.base.clone();
        slot.nullable = ty.nullable;
        return Ok(());
    }
    if slot.base == ty.base && slot.nullable == ty.nullable {
        return Ok(());
    }
    Err(Error::unsupported(
        "keys no field names, whose values are of two types",
    ))
}

/// `required` names no field holds: present, as a key no field names.
fn required(k: &mut Key, m: usize) -> Result<(), Error> {
    let Value::Array(names) = k.value else {
        return Ok(());
    };
    let fields = &model(k.lower, m)?.fields;
    let lacking: Vec<String> = names
        .iter()
        .filter_map(Value::as_str)
        .filter(|n| !fields.iter().any(|f| f.name == *n))
        .map(str::to_string)
        .collect();
    if !lacking.is_empty() {
        k.ty.checks.push(Check::Present(lacking));
    }
    Ok(())
}

/// A rule's type: `schema` lowered, its base adopted. `None` where it admits every value.
fn rule(lower: &mut Lower, m: usize, schema: &Value) -> Result<Option<Ty>, Error> {
    let ty = lower.ty("patternProperties", schema)?;
    if ty.base == Base::Any && ty.checks.is_empty() {
        return Ok(None);
    }
    adopt(lower, m, &ty)?;
    Ok(Some(ty))
}

fn pattern_properties(k: &mut Key, m: usize) -> Result<(), Error> {
    let Value::Object(rules) = k.value else {
        return Err(Error::invalid(format!(
            "`patternProperties` of {}",
            k.value
        )));
    };
    if rules.is_empty() {
        return Ok(());
    }
    held(k.lower, m, k.key)?;
    for (pattern, schema) in rules {
        for f in &model(k.lower, m)?.fields {
            if crate::matching::matches(pattern, &f.name)? {
                return Err(Error::unsupported(format!(
                    "`patternProperties` {pattern:?} matches the field `{}`",
                    f.name
                )));
            }
        }
        if let Some(ty) = rule(k.lower, m, schema)? {
            k.ty.checks.push(Check::Matching {
                pattern: pattern.clone(),
                ty: Box::new(ty),
            });
        }
    }
    Ok(())
}

/// `additionalProperties` beside `patternProperties` or as a schema; `None` for the models area
/// where it is `false` with no pattern: that closes the model, which holds only its fields.
fn additional(k: &mut Key, m: usize) -> Done {
    let skip = patterns(k.siblings);
    let ty = match k.value {
        Value::Bool(true) => return Some(Ok(())),
        Value::Bool(false) if skip.is_empty() => return None,
        Value::Bool(false) => boolean::ty(false),
        schema @ Value::Object(_) => {
            match held(k.lower, m, k.key).and_then(|()| rule(k.lower, m, schema)) {
                Ok(Some(ty)) => ty,
                Ok(None) => return Some(Ok(())),
                Err(e) => return Some(Err(e)),
            }
        }
        _ => return k.refuse(),
    };
    k.ty.checks.push(Check::Unmatched {
        patterns: skip,
        ty: Box::new(ty),
    });
    Some(Ok(()))
}

/// `propertyNames`: a schema of the key as text.
fn names(k: &mut Key) -> Result<(), Error> {
    let mut schema = match k.value {
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => json!({ "type": "string", "enum": [] }),
        Value::Object(s) => Value::Object(s.clone()),
        other => return Err(Error::invalid(format!("`propertyNames` of {other}"))),
    };
    if let Value::Object(s) = &mut schema {
        if !s.contains_key("$ref") {
            s.entry("type")
                .or_insert_with(|| Value::String("string".into()));
        }
    }
    let ty = k.lower.ty("propertyNames", &schema)?;
    if ty.base != Base::Text {
        return Err(Error::unsupported("`propertyNames` of a type but text"));
    }
    if !ty.checks.is_empty() {
        k.ty.checks.push(Check::Names(Box::new(ty)));
    }
    Ok(())
}

/// `dependentSchemas`: where a key is present, the model meets its subschema. Each is a
/// conditional branch that holds where the key is absent.
fn dependent_schemas(k: &mut Key) -> Result<(), Error> {
    let Value::Object(deps) = k.value else {
        return Err(Error::invalid(format!("`dependentSchemas` of {}", k.value)));
    };
    let parent = branches::parent(k.ty);
    for (key, sub) in deps {
        let present = json!({ "required": [key] });
        let mut when = k.lower.onto("dependentSchemas", &parent, &present)?;
        // A null has no key to be present.
        when.nullable = false;
        let then = k.lower.onto("dependentSchemas", &parent, sub)?;
        k.ty.checks.push(Check::Conditional {
            when: Box::new(when),
            then: Some(Box::new(then)),
            otherwise: None,
        });
    }
    Ok(())
}
