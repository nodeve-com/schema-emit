//! A union decodes branch by branch, in order: the first that decodes and whose check passes is
//! the value. Where none passes its check, the first that decodes is, so its check reports why.
//! Where none decodes, the failure is the branch's the value fit furthest (`Unfit::either`). The check
//! is the union's own, over its enum (`Emitter::check_union`), so a branch of any type has one.

use super::d;
use crate::emit::{Emitter, STD};
use crate::ir::Union;
use crate::Error;

use super::super::{idents, StdForm};

impl StdForm<'_> {
    /// The function `read_<enum>`, decoding `union`, and the check it calls.
    pub(super) fn union_decoder(&self, union: &Union) -> Result<String, Error> {
        let (name, unfit) = (self.union_name(union), d("Unfit"));
        let id = idents::snake(&name);
        let ok = format!("ok_{id}");
        let mut out = Emitter::new(self.schema, self).check_union(union, &ok)?;
        let mut arms = String::new();
        for (i, variant) in union.variants.iter().enumerate() {
            let read = self.dec(variant, None, "v", 0);
            let held = self.variant_of(union, i, "x");
            arms.push_str(&format!(
                "match {read} {{ Ok(x) => {{ let c = {held}; \
                 let mut h = {STD}::Holds::default(); {ok}(&c, &mut h); \
                 if h.held() {{ return Ok(c); }} if first.is_none() {{ first = Some(c); }} }} \
                 Err(e) => {{ why = Some({}(why.take(), e)); }} }}\n",
                d("Unfit::either")
            ));
        }
        out.push_str(&format!(
            "#[allow(dead_code, unused_variables)] fn read_{id}(v: &::serde_json::Value) \
             -> Result<{name}, {unfit}> {{ \
             let mut first: Option<{name}> = None; let mut why: Option<{unfit}> = None;\n{arms}\
             match first {{ Some(c) => Ok(c), \
             None => Err(why.unwrap_or_else(|| {unfit}::kind(\"one of the union's branches\"))) }} }}\n"
        ));
        Ok(out)
    }
}
