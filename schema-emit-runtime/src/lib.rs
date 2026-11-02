#![doc = include_str!("../README.md")]
#![no_std]

#[cfg(feature = "decode")]
extern crate std;

#[cfg(feature = "decode")]
pub mod decode;
mod place;

pub use pattern_emit::{Pattern, Unusable};
pub use place::{under, Trail};

/// Why a value fails its check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    /// A required field holds nothing.
    Absent,
    /// A value past its width or bounds, outside its choices, or of a length or count past its
    /// limits.
    Range,
    /// A list with two items equal by JSON Schema's rule.
    Duplicate,
    /// A value that isn't what its type says: text that isn't UTF-8, a list the embedder can't
    /// read.
    Cast,
}

impl Reason {
    /// Its name in a failure.
    pub const fn name(self) -> &'static str {
        match self {
            Reason::Absent => "absent",
            Reason::Range => "range",
            Reason::Duplicate => "duplicate",
            Reason::Cast => "cast",
        }
    }
}

/// Where a check sends each failure.
///
/// `path` names the field: `<Model>.<field>`, a nested model's field after it
/// (`Value.climate.temp_c`), `[]` for each list item, filled from `at` in order
/// (`Value.readings[]` with `at` `[1]` is `Value.readings[1]`).
///
/// A model that reaches itself marks its steps in `path` as `(<step>)*`: `Value(.next)*.n`; with
/// several, `(.left|.right)*`. In `at` the mark takes its repeat count, then per repeat the
/// index of the step taken (only with several steps) and the indices of the `[]` inside it. The
/// `[]` before the mark come first in `at`, those after it last. `Value(.next)*.n` with `at`
/// `[2]` is `Value.next.next.n`. `Value.root(.children[])*.leaf[]` with `at` `[2, 4, 1, 7]` is
/// `Value.root.children[4].children[1].leaf[7]`. `Value(.left|.right)*.n` with `at` `[2, 0, 1]`
/// is `Value.left.right.n`. A count of 0 drops the step.
pub trait Report {
    fn fail(&mut self, path: &'static str, at: &[u32], reason: Reason);
}

/// One failure, as [`Failures`] keeps it: up to [`DEPTH`] list indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub path: &'static str,
    pub at: [u32; DEPTH],
    pub reason: Reason,
}

/// Entries of `at` a [`Failure`] keeps; deeper ones read as 0.
pub const DEPTH: usize = 4;

/// The first `N` failures, and how many more were dropped: a value's failures are capped.
#[derive(Clone, Copy, Debug)]
pub struct Failures<const N: usize> {
    list: [Option<Failure>; N],
    len: usize,
    pub dropped: u32,
}

impl<const N: usize> Default for Failures<N> {
    fn default() -> Self {
        Failures {
            list: [None; N],
            len: 0,
            dropped: 0,
        }
    }
}

impl<const N: usize> Failures<N> {
    /// Each kept failure, in the order reported.
    pub fn iter(&self) -> impl Iterator<Item = &Failure> {
        self.list.iter().flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0 && self.dropped == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl<const N: usize> Report for Failures<N> {
    fn fail(&mut self, path: &'static str, at: &[u32], reason: Reason) {
        let mut kept = [0; DEPTH];
        for (k, a) in kept.iter_mut().zip(at) {
            *k = *a;
        }
        match self.list.get_mut(self.len) {
            Some(slot) => {
                *slot = Some(Failure {
                    path,
                    at: kept,
                    reason,
                });
                self.len = self.len.saturating_add(1);
            }
            None => self.dropped = self.dropped.saturating_add(1),
        }
    }
}

/// Whether every check reported to it held: a branch of `anyOf`, `oneOf`, or `if` over an object
/// or a list checks into one, so its verdict is one condition.
#[derive(Clone, Copy, Debug, Default)]
pub struct Holds {
    failed: bool,
}

impl Holds {
    pub fn held(&self) -> bool {
        !self.failed
    }
}

impl Report for Holds {
    fn fail(&mut self, _: &'static str, _: &[u32], _: Reason) {
        self.failed = true;
    }
}

/// Keeps the first failure at or under one path, and ignores the rest.
#[derive(Clone, Copy, Debug)]
pub struct Under {
    path: &'static str,
    pub reason: Option<Reason>,
}

impl Under {
    pub const fn new(path: &'static str) -> Under {
        Under { path, reason: None }
    }
}

impl Report for Under {
    fn fail(&mut self, path: &'static str, at: &[u32], reason: Reason) {
        if self.reason.is_none() && under(self.path, path, at) {
            self.reason = Some(reason);
        }
    }
}

/// Whether `x` is a whole multiple of `m`, as JSON Schema's `multipleOf` reads it: the quotient
/// is an integer, and a quotient past `f64`'s range is not.
pub fn multiple_of(x: f64, m: f64) -> bool {
    let q = x / m;
    q.is_finite() && q == trunc(q)
}

/// `f64::trunc`, which `core` lacks: a finite value at or past 2^52 is already whole.
fn trunc(x: f64) -> f64 {
    const WHOLE: f64 = 4_503_599_627_370_496.0;
    if x.is_nan() || x.abs() >= WHOLE {
        return x;
    }
    // In range of i64 here: |x| < 2^52.
    #[allow(clippy::cast_possible_truncation)]
    let t = x as i64;
    t as f64
}

/// Text length in Unicode scalar values, as JSON Schema counts it.
pub fn chars(s: &str) -> u64 {
    s.chars().fold(0u64, |n, _| n.saturating_add(1))
}

#[cfg(test)]
mod tests;
