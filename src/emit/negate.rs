//! Area: negation. `Not` holds where its type's checks don't all hold on the view: their
//! conditions, conjoined, negated. A type with none holds for every value, so its `Not` fails
//! each.

use super::Emitter;
use crate::ir::Check;
use crate::Error;

pub(super) fn cond(e: &Emitter, check: &Check, view: &str) -> Option<Result<String, Error>> {
    let Check::Not(inner) = check else {
        return None;
    };
    Some(
        e.conds(&inner.base, &inner.checks, view)
            .map(|conds| match conds.is_empty() {
                true => "!(true)".into(),
                false => format!("!({})", conds.join(" && ")),
            }),
    )
}
