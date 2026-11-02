//! Whether a pattern finds itself in a text, for a front end or a test that must know at
//! lowering which names a pattern reaches. The emitted check uses the same ECMA-262 table.

use crate::Error;

/// Whether `pattern`, as ECMA-262 reads it, matches somewhere in `text`.
pub fn matches(pattern: &str, text: &str) -> Result<bool, Error> {
    pattern_emit::matches_ecma(pattern, text)
        .map_err(|why| Error::unsupported(format!("pattern {pattern:?}: {why}")))
}
