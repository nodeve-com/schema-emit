//! Meeting two lowered types: each describes one value, so the base is one kind (integers and
//! floats narrow to the narrower width), the checks of both apply, nullable only where both are,
//! and notes must agree.

use super::Meet;
use crate::ir::{Base, Check, Field, Float, Int, Model, Ty};
use crate::json_schema::Lower;
use crate::Error;

impl Lower {
    /// Meets `st`, a subschema's type, into `meet`. Two objects merge their fields, by `closed`.
    pub(super) fn into(&mut self, meet: &mut Meet, st: Ty, sub_closed: bool) -> Result<(), Error> {
        let objects = match (&meet.ty.base, &st.base) {
            (Base::Model(_), Base::Model(j)) => Some(*j),
            _ => None,
        };
        match objects {
            Some(j) => {
                self.merge_fields(meet, j, sub_closed)?;
                absorb(meet.name, meet.ty, st)
            }
            None => ty_meet(meet.name, meet.ty, st),
        }
    }

    /// Merges model `j`'s fields into the parent's model.
    fn merge_fields(&mut self, meet: &Meet, j: usize, sub_closed: bool) -> Result<(), Error> {
        let name = meet.name;
        let Base::Model(i) = meet.ty.base else {
            return Err(Error::invalid(format!("{name}: no parent model")));
        };
        let Some(theirs) = self.out.models.get(j).map(|m| m.fields.clone()) else {
            return Err(Error::invalid(format!("{name}: no model {j}")));
        };
        let Some(model) = self.out.models.get_mut(i) else {
            return Err(Error::invalid(format!("{name}: no model {i}")));
        };
        let has = |fields: &[Field], n: &str| fields.iter().any(|f| f.name == n);
        if meet.closed {
            if let Some(f) = theirs.iter().find(|f| !has(&model.fields, &f.name)) {
                return Err(Error::unsupported(format!(
                    "{name}: `allOf` adds `{}`, which a closed object doesn't name",
                    f.name
                )));
            }
        }
        if sub_closed {
            if let Some(f) = model.fields.iter().find(|f| !has(&theirs, &f.name)) {
                return Err(Error::unsupported(format!(
                    "{name}: a closed subschema doesn't name `{}`, which another subschema gives",
                    f.name
                )));
            }
        }
        merge_into(name, model, theirs)?;
        Ok(())
    }
}

/// Adds each of `theirs` to `model`: a field both name meets its type and requirement.
fn merge_into(name: &str, model: &mut Model, theirs: Vec<Field>) -> Result<(), Error> {
    for f in theirs {
        match model.fields.iter_mut().find(|g| g.name == f.name) {
            Some(g) => {
                ty_meet(&format!("{name}.{}", f.name), &mut g.ty, f.ty)?;
                g.required |= f.required;
            }
            None => model.fields.push(f),
        }
    }
    Ok(())
}

/// Meets `st` into `ty` where neither is an object's fields: the same kind, the narrower width,
/// the checks of both.
pub(in crate::json_schema) fn ty_meet(name: &str, ty: &mut Ty, st: Ty) -> Result<(), Error> {
    // A boolean schema admits every kind, so the other subschema's kind is the type.
    if ty.base == Base::Any {
        ty.base = st.base.clone();
    }
    match (&mut ty.base, &st.base) {
        (Base::Null, Base::Null) | (_, Base::Any) | (Base::Bool, Base::Bool) => {}
        (a, b) if a.same_text(b) => {}
        (Base::Int(a), Base::Int(b)) => *a = int_meet(name, *a, *b)?,
        // Every integer is a `number` of double width, so that subschema adds no kind.
        (Base::Int(_), Base::Float(Float::F64)) => {}
        // An integer on a number's view: whole, with the integer's own checks. Its width has no
        // place on a float, so only the default one, which JSON Schema's integer lacks, is taken.
        (Base::Float(_), Base::Int(Int::I64)) => ty.checks.push(Check::Whole),
        (Base::Float(_), Base::Int(w)) => {
            return Err(Error::unsupported(format!(
                "{name}: `allOf` gives a number an integer of width {w:?}"
            )));
        }
        (Base::Float(a), Base::Float(b)) => *a = float_meet(*a, *b),
        (Base::List(a), Base::List(b)) | (Base::Map(a), Base::Map(b)) => {
            ty_meet(name, a, (**b).clone())?;
        }
        (Base::Model(_), Base::Model(_)) => {
            return Err(Error::unsupported(format!(
                "{name}: `allOf` gives an object nested in a list, a map, or a property both name"
            )));
        }
        (a, b) => {
            return Err(Error::unsupported(format!(
                "{name}: `allOf` subschemas disagree on the type: {} and {}",
                kind(a),
                kind(b)
            )));
        }
    }
    absorb(name, ty, st)
}

/// Adds `st`'s checks to `ty`, and its notes where they agree. A value is nullable only where
/// both admit null.
pub(super) fn absorb(name: &str, ty: &mut Ty, st: Ty) -> Result<(), Error> {
    ty.nullable &= st.nullable;
    ty.checks.extend(st.checks);
    for (what, theirs) in st.notes.0 {
        agree(name, &what, &mut ty.notes.0, theirs)?;
    }
    Ok(())
}

fn agree(
    name: &str,
    what: &str,
    mine: &mut std::collections::BTreeMap<String, String>,
    theirs: String,
) -> Result<(), Error> {
    if let Some(m) = mine.get(what) {
        if *m != theirs {
            return Err(Error::unsupported(format!(
                "{name}: `allOf` gives `{what}` as {m} and {theirs}"
            )));
        }
    }
    mine.insert(what.into(), theirs);
    Ok(())
}

/// The narrowest integer width both bounds admit, if one width is exactly that range.
fn int_meet(name: &str, a: Int, b: Int) -> Result<Int, Error> {
    let lo = a.bounds().0.max(b.bounds().0);
    let hi = a.bounds().1.min(b.bounds().1);
    [
        Int::I8,
        Int::I16,
        Int::I32,
        Int::I64,
        Int::I128,
        Int::U8,
        Int::U16,
        Int::U32,
        Int::U64,
        Int::U128,
    ]
    .into_iter()
    .find(|w| w.bounds() == (lo, hi))
    .ok_or_else(|| {
        Error::unsupported(format!(
            "{name}: `allOf` integers of widths {a:?} and {b:?}, which no one width holds"
        ))
    })
}

fn float_meet(a: Float, b: Float) -> Float {
    if a == Float::F32 || b == Float::F32 {
        Float::F32
    } else {
        Float::F64
    }
}

pub(super) fn kind(base: &Base) -> &'static str {
    match base {
        Base::Null => "null",
        Base::Bool => "a boolean",
        Base::Int(_) => "an integer",
        Base::Float(_) => "a number",
        Base::Text => "a string",
        Base::Timestamp => "a timestamp",
        Base::Duration => "a duration",
        Base::Bytes => "bytes",
        Base::List(_) | Base::Tuple { .. } => "an array",
        Base::Map(_) => "a map",
        Base::Model(_) | Base::Union(_) => "an object",
        Base::Any => "any value",
    }
}
