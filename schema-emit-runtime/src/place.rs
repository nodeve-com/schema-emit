//! A failure's path as text: marks expanded, `[]` filled; and the `at` a check builds.

use core::fmt;

use crate::{Failure, DEPTH};

impl fmt::Display for Failure {
    /// Its path, the marks expanded and the `[]` filled.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        expand(self.path, &self.at, f).map_err(|()| fmt::Error)
    }
}

/// Where [`expand`] writes a path: a failure prints through `core::fmt`, while [`under`] reads
/// it through its own [`Prefix`], so a check's module links no formatting.
trait Sink {
    /// Writes `s`; an error stops the expansion.
    fn text(&mut self, s: &str) -> Result<(), ()>;
    /// Writes a `[]`, filled with `entry` or left empty.
    fn hole(&mut self, entry: u32) -> Result<(), ()>;
}

impl Sink for fmt::Formatter<'_> {
    fn text(&mut self, s: &str) -> Result<(), ()> {
        self.write_str(s).map_err(drop)
    }

    fn hole(&mut self, entry: u32) -> Result<(), ()> {
        write!(self, "[{entry}]").map_err(drop)
    }
}

/// The `at` of a failure being built: up to [`DEPTH`] entries, a push past them dropped.
#[derive(Clone, Copy, Debug, Default)]
pub struct Trail {
    at: [u32; DEPTH],
    len: usize,
}

impl Trail {
    pub const fn new() -> Trail {
        Trail {
            at: [0; DEPTH],
            len: 0,
        }
    }

    pub fn push(mut self, entry: u32) -> Trail {
        if let Some(slot) = self.at.get_mut(self.len) {
            *slot = entry;
            self.len = self.len.saturating_add(1);
        }
        self
    }

    /// This, then each entry of `other`.
    pub fn join(self, other: &Trail) -> Trail {
        other.as_slice().iter().fold(self, |t, e| t.push(*e))
    }

    pub fn as_slice(&self) -> &[u32] {
        self.at.get(..self.len).unwrap_or_default()
    }
}

/// Writes `path` to `w`, each mark expanded by `at`, and each `[]` given the next entry.
fn expand(path: &str, at: &[u32], w: &mut impl Sink) -> Result<(), ()> {
    let mut next = at.iter().copied();
    let mut rest = path;
    while !rest.is_empty() {
        let (head, mark, tail) =
            match cut(rest, "(").and_then(|(h, a)| cut(a, ")*").map(|(m, t)| (h, m, t))) {
                Some((head, mark, tail)) => (head, Some(mark), tail),
                None => (rest, None, ""),
            };
        holes(head, &mut next, w)?;
        if let Some(steps) = mark {
            let several = cut(steps, "|").is_some();
            for _ in 0..next.next().unwrap_or(0) {
                let pick = if several { next.next().unwrap_or(0) } else { 0 };
                let step = Split::new(steps, "|")
                    .nth(pick as usize)
                    .unwrap_or_default();
                holes(step, &mut next, w)?;
            }
        }
        rest = tail;
    }
    Ok(())
}

/// Writes `text`, each `[]` given the next entry.
fn holes(text: &str, next: &mut impl Iterator<Item = u32>, w: &mut impl Sink) -> Result<(), ()> {
    let mut parts = Split::new(text, "[]");
    w.text(parts.next().unwrap_or_default())?;
    for part in parts {
        w.hole(next.next().unwrap_or(0))?;
        w.text(part)?;
    }
    Ok(())
}

/// `s` around the first `pat` in it, compared byte by byte: each pattern here is short ASCII,
/// and `str`'s substring search would link its two-way searcher into every check.
fn cut<'s>(s: &'s str, pat: &str) -> Option<(&'s str, &'s str)> {
    let at = s
        .as_bytes()
        .windows(pat.len().max(1))
        .position(|w| w == pat.as_bytes())?;
    Some((s.get(..at)?, s.get(at.checked_add(pat.len())?..)?))
}

/// The parts of a text between each `pat`, as `str::split` gives them, through [`cut`].
struct Split<'s> {
    rest: Option<&'s str>,
    pat: &'static str,
}

impl<'s> Split<'s> {
    fn new(s: &'s str, pat: &'static str) -> Split<'s> {
        Split { rest: Some(s), pat }
    }
}

impl<'s> Iterator for Split<'s> {
    type Item = &'s str;

    fn next(&mut self) -> Option<&'s str> {
        let s = self.rest?;
        let (part, rest) = cut(s, self.pat).map_or((s, None), |(h, t)| (h, Some(t)));
        self.rest = rest;
        Some(part)
    }
}

/// Whether `failed`, a failure's path with its `at`, is `path` or a field or item under it. Both
/// keep `[]` as the template: `path` `A.xs[]` holds `A.xs[].v`.
pub fn under(path: &str, failed: &str, at: &[u32]) -> bool {
    let mut prefix = Prefix {
        want: path.as_bytes(),
        seen: 0,
        ok: true,
    };
    let _ = expand(failed, at, &mut prefix);
    prefix.ok && prefix.seen >= path.len()
}

/// Checks the text written to it starts with `want`, then ends or goes on with `.` or `[`.
struct Prefix<'a> {
    want: &'a [u8],
    seen: usize,
    ok: bool,
}

impl Sink for Prefix<'_> {
    fn text(&mut self, s: &str) -> Result<(), ()> {
        for b in s.bytes() {
            match self.want.get(self.seen) {
                Some(w) if *w == b => {}
                Some(_) => self.ok = false,
                None => self.ok = b == b'.' || b == b'[',
            }
            if !self.ok || self.seen >= self.want.len() {
                return Err(());
            }
            self.seen = self.seen.saturating_add(1);
        }
        Ok(())
    }

    /// Keeps `[]` as the template has it.
    fn hole(&mut self, _: u32) -> Result<(), ()> {
        self.text("[]")
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use crate::{Failures, Reason, Report};

    use super::*;

    fn failure(path: &'static str, at: &[u32]) -> Failure {
        let mut f = Failures::<1>::default();
        f.fail(path, at, Reason::Range);
        let kept = *f.iter().next().unwrap();
        kept
    }

    #[test]
    fn marks_expand_by_their_count_and_indices() {
        let chain = failure("Value(.next)*.n", &[2]);
        assert_eq!(chain.to_string(), "Value.next.next.n");
        assert_eq!(failure("Value(.next)*.n", &[0]).to_string(), "Value.n");
        let tree = failure("Value.root(.children[])*.leaf[]", &[2, 4, 1, 7]);
        assert_eq!(
            tree.to_string(),
            "Value.root.children[4].children[1].leaf[7]"
        );
        let past = failure("Value(.c[])*.leaf", &[5, 1, 1, 1, 1, 1]);
        assert_eq!(past.to_string(), "Value.c[1].c[1].c[1].c[0].c[0].leaf");
    }

    #[test]
    fn alternating_marks_read_a_step_index_per_repeat() {
        let tree = failure("Value(.left|.right)*.n", &[3, 0, 1, 1]);
        assert_eq!(tree.to_string(), "Value.left.right.right.n");
        let mixed = failure("Value(.kids[]|.next)*.n", &[2, 0, 2, 1]);
        assert_eq!(mixed.to_string(), "Value.kids[2].next.n");
        assert_eq!(failure("Value(.l|.r)*.n", &[0]).to_string(), "Value.n");
        let past = failure("Value(.l|.r)*.n", &[3, 1, 1, 1]);
        assert_eq!(past.to_string(), "Value.r.r.r.n");
        let cut = failure("Value(.l|.r)*.n", &[3, 1, 1, 1, 1, 1]);
        assert_eq!(cut.to_string(), "Value.r.r.r.n");
    }

    #[test]
    fn under_reads_alternating_marks() {
        assert!(under("V.left.right", "V(.left|.right)*.n", &[2, 0, 1]));
        assert!(!under("V.left.left", "V(.left|.right)*.n", &[2, 0, 1]));
        assert!(under("V.kids[].next", "V(.kids[]|.next)*.n", &[2, 0, 4, 1]));
        assert!(under("V.n", "V(.left|.right)*.n", &[0]));
    }

    #[test]
    fn split_parts_as_str_split_does() {
        use std::vec::Vec;
        for text in ["", "[]", "a[]", "[]b", "a[]b[][]c", "a[b]", "é[]ü[", "[[]]"] {
            let ours: Vec<_> = Split::new(text, "[]").collect();
            assert_eq!(ours, text.split("[]").collect::<Vec<_>>(), "{text}");
        }
        assert_eq!(cut("a)*b)*", ")*"), Some(("a", "b)*")));
        assert_eq!(cut("a)", ")*"), None);
        assert_eq!(cut("abc", ""), None);
    }

    #[test]
    fn a_trail_keeps_its_first_entries() {
        let rest = Trail::new().push(2).push(3);
        let t = Trail::new().push(1).join(&rest).push(4).push(5);
        assert_eq!(t.as_slice(), &[1, 2, 3, 4]);
    }

    #[test]
    fn under_reads_marks_as_expanded() {
        assert!(under("V.next", "V(.next)*.n", &[1]));
        assert!(under("V.next.next", "V(.next)*.n", &[2]));
        assert!(!under("V.next", "V(.next)*.n", &[0]));
        assert!(under("V.n", "V(.next)*.n", &[0]));
        assert!(under("V.c[]", "V(.c[])*.n", &[1, 3]));
    }
}
