//! The walk: the emitter and the place in the code being written, which every check on a value
//! carries together.

use super::{at::At, guard, scalars, Emitter};
use crate::ir::{Base, Check, Ty};
use crate::Error;

/// The code being written for one value: the emitter writing it and where the value sits.
pub(super) struct Walk<'a, 'e> {
    pub e: &'a Emitter<'e>,
    pub at: At,
}

impl<'a, 'e> Walk<'a, 'e> {
    /// A walk whose failures' paths start at `path`.
    pub(super) fn new(e: &'a Emitter<'e>, path: &str) -> Self {
        Walk {
            e,
            at: At::new(path),
        }
    }

    /// `f` with `step` added to the path, then removed.
    pub(super) fn within<T>(&mut self, step: &str, f: impl FnOnce(&mut Self) -> T) -> T {
        let outer = self.at.path.len();
        self.at.path.push_str(step);
        let out = f(self);
        self.at.path.truncate(outer);
        out
    }

    /// `f` with field or key `name` added to the path as `.name`, then removed. A name that is
    /// empty adds nothing.
    pub(super) fn within_field<T>(&mut self, name: &str, f: impl FnOnce(&mut Self) -> T) -> T {
        let step = if name.is_empty() {
            String::new()
        } else {
            format!(".{name}")
        };
        self.within(&step, f)
    }

    /// The checks on `value`, a `&` of `ty`'s Rust. A nullable value is an `Option`: a present
    /// value meets the checks, and null meets only the choices that admit it.
    pub(super) fn value(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        if !ty.nullable {
            return self.present(value, ty);
        }
        let inner = self.at.local("y");
        let present = self.present(&inner, ty)?;
        let absent = self.null(ty);
        Ok(some_none(value, &inner, &present, &absent))
    }

    /// The checks on a value that isn't null, `value` a `&` of `ty`'s base.
    pub(super) fn present(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        if let Base::Union(u) = &ty.base {
            return self.union(value, ty, u);
        }
        // A structured value's branches each walk it, so they check apart from its own checks.
        if ty.base.is_structured() && ty.checks.iter().any(Check::is_branch) {
            let (branches, own): (Vec<Check>, Vec<Check>) =
                ty.checks.iter().cloned().partition(Check::is_branch);
            let own = Ty {
                checks: own,
                ..ty.clone()
            };
            let own = self.present(value, &own)?;
            return Ok(own + &self.branches(value, &branches)?);
        }
        let choice = ty.checks.iter().any(|c| matches!(c, Check::OneOf(_)));
        match &ty.base {
            // A map has no reader, so a choice of one holds only null.
            Base::Map(_) if choice => Ok(self.at.fail("Range")),
            Base::Model(_) | Base::List(_) | Base::Tuple { .. } if choice => {
                self.choices(value, ty)
            }
            Base::Any => self.any(ty),
            // Checked above, before its branches.
            Base::Union(_) => Err(Error::unsupported("a union beside branches")),
            Base::Model(m) => self.model(value, *m, &ty.checks),
            Base::Map(_) => self.map(value, ty),
            Base::List(item) => self.list(value, ty, item),
            Base::Tuple { .. } => self.tuple(value, ty),
            Base::Text => self.text(value, ty),
            // Held in the embedder's own type, which no check reads.
            Base::Timestamp | Base::Duration | Base::Bytes => {
                let conds = self.e.conds(&ty.base, &ty.checks, value)?;
                Ok(guard(&self.at, &conds))
            }
            // Null's only value is null, which `null` checks.
            Base::Null => Ok(self.null(ty)),
            Base::Bool | Base::Int(_) | Base::Float(_) => self.scalar(value, ty),
        }
    }

    /// A scalar's checks on `value`, a `&` of its base, through its view.
    fn scalar(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        let view = self.at.local("n");
        let bind = scalars::view(&ty.base, value, &view);
        let conds = self.e.conds(&ty.base, &ty.checks, &view)?;
        if conds.is_empty() {
            return Ok(String::new());
        }
        Ok(format!("{{ {bind}\n{}}}\n", guard(&self.at, &conds)))
    }

    /// The checks of branch `ty` on `value`, a `&` of a present value of its base: as
    /// [`Walk::present`], but a model checks only the fields the branch names.
    pub(super) fn branch(&mut self, value: &str, ty: &Ty) -> Result<String, Error> {
        match ty.base {
            Base::Model(m) => self.view(value, m, ty),
            _ => self.present(value, ty),
        }
    }

    /// Branch `ty` on `value`, a `&` of the value's Rust: an `Option` where `option` says. Null
    /// holds where the branch admits it.
    pub(super) fn branch_value(
        &mut self,
        value: &str,
        ty: &Ty,
        option: bool,
    ) -> Result<String, Error> {
        if !option {
            return self.branch(value, ty);
        }
        let inner = self.at.local("y");
        let present = self.branch(&inner, ty)?;
        let absent = guard(&self.at, &super::admits(super::admits_null(ty)));
        Ok(some_none(value, &inner, &present, &absent))
    }

    /// The checks on null, judged as the code is written: only choices and branches judge it.
    pub(super) fn null(&self, ty: &Ty) -> String {
        guard(&self.at, &super::admits(super::holds_of_null(&ty.checks)))
    }
}

/// A `match` of the `Option` `value`: `present` binds `inner`, `absent` is null's.
fn some_none(value: &str, inner: &str, present: &str, absent: &str) -> String {
    format!("match {value} {{ Some({inner}) => {{ {present}}} None => {{ {absent}}} }}\n")
}
