//! The derive attribute of each struct and enum.

use super::StdForm;
use crate::ir::Base;

impl StdForm<'_> {
    /// The derive attribute of a struct or an enum, which also derives `Default` where `default`.
    pub(super) fn derives(&self, default: bool) -> String {
        let extra: String = self.derive.iter().map(|d| format!(", {d}")).collect();
        let default = if default { ", Default" } else { "" };
        format!("#[derive(Debug, Clone, PartialEq{default}{extra})]")
    }

    /// Whether every field of model `m` has a default value: an optional or nullable one always
    /// does, as does a required one of a type that does. A schema `default` is the field's value.
    /// A union and a value of any kind have none, nor has a model written elsewhere or one that
    /// holds itself.
    pub(super) fn defaults(&self, m: usize, seen: &mut Vec<usize>) -> bool {
        if seen.contains(&m) || (self.external)(m).is_some() {
            return false;
        }
        seen.push(m);
        let fields = self.schema.models.get(m).map(|m| m.fields.as_slice());
        let ok = fields.unwrap_or_default().iter().all(|f| {
            f.ty.nullable
                || !f.required
                || match &f.ty.base {
                    Base::Union(_) | Base::Any => false,
                    Base::Model(n) => self.defaults(*n, seen),
                    _ => true,
                }
        });
        seen.pop();
        ok
    }
}
