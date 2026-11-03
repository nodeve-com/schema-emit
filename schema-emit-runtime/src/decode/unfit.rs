//! [`Unfit`]: where a value fails to decode and why. A decode runs inside serde, whose errors
//! are the deserializer's own, so a failure crosses it out of band: [`Unfit::raise`] holds the
//! failure in a slot of the thread and returns a custom error of one fixed marker. Each level
//! the error returns through takes the failure from the slot, leads its path with its own step,
//! and raises it again; [`Unfit::raised`] takes it. An error that isn't the marker, one of the
//! text or the deserializer, passes through as it is. The root of a decode (`decode::root`)
//! empties the slot on entry and makes the last error of the other kind an `Unfit` once.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::string::{String, ToString as _};
use std::thread_local;

use serde::{de, ser};
use serde_json::error::Category;

use super::why::Why;

/// The message of every failure that [`Unfit::raise`] returns; the failure is in the slot.
const MARKER: &str = "an unfit value";

thread_local! {
    static RAISED: Cell<Option<Unfit>> = const { Cell::new(None) };
}

/// Replaces the slot's failure, which is lost where the slot is gone (a thread ending).
fn exchange(next: Option<Unfit>) -> Option<Unfit> {
    RAISED.try_with(|slot| slot.replace(next)).ok().flatten()
}

/// Puts back the failure that was in the slot when a decode began, when it ends or unwinds.
pub(super) struct Previous(Option<Unfit>);

impl Previous {
    /// Empties the slot, keeping what it held.
    pub(super) fn empty() -> Previous {
        Previous(exchange(None))
    }
}

impl Drop for Previous {
    fn drop(&mut self) {
        exchange(self.0.take());
    }
}

/// A value that doesn't decode: its first failing place, where (`path`, as `Model.field[2].key`)
/// and why. A path builds outward, one step at a time, as the failure returns through the values
/// that hold it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unfit {
    pub path: Cow<'static, str>,
    pub why: Why,
}

impl Unfit {
    /// A value of another kind than `expected`, at the place the failure is raised.
    pub fn kind(expected: &'static str) -> Unfit {
        Unfit::at_root(Why::Kind(expected.into()))
    }

    /// A value of the right kind no std value holds, at the place the failure is raised.
    pub fn other(reason: &'static str) -> Unfit {
        Unfit::at_root(Why::Other(reason.into()))
    }

    /// A value past what its type holds, at the place the failure is raised.
    pub fn range(reason: &'static str) -> Unfit {
        Unfit::at_root(Why::Range(reason.into()))
    }

    /// A required field that the value lacks, at the place the failure is raised.
    pub fn absent() -> Unfit {
        Unfit::at_root(Why::Absent)
    }

    /// A key that no field of the closed model names, at the place the failure is raised.
    pub fn unnamed() -> Unfit {
        Unfit::at_root(Why::Unnamed)
    }

    fn at_root(why: Why) -> Unfit {
        Unfit {
            path: Cow::Borrowed(""),
            why,
        }
    }

    /// Leads the path by `steps`.
    fn lead(&mut self, steps: &[&str]) {
        lead(&mut self.path, steps);
    }

    /// Whether the value is of another kind than the type takes.
    pub fn is_kind(&self) -> bool {
        matches!(self.why, Why::Kind(_))
    }

    /// The failure of field or key `name` of the value this failure is in.
    pub fn field(mut self, name: &str) -> Unfit {
        let dot = if name.is_empty() { "" } else { "." };
        self.lead(&[dot, name]);
        self
    }

    /// The failure of field `step`, which is written as a path step (`.name`), of the value this
    /// failure is in. A step that is static allocates nothing where it leads the path.
    pub fn step(mut self, step: &'static str) -> Unfit {
        match self.path.is_empty() {
            true => self.path = Cow::Borrowed(step),
            false => lead(&mut self.path, &[step]),
        }
        self
    }

    /// The failure of item `index` of the list this failure is in.
    pub fn index(mut self, index: usize) -> Unfit {
        self.lead(&["[", &index.to_string(), "]"]);
        self
    }

    /// The failure of the model `name`, a value's root.
    pub fn model(mut self, name: &str) -> Unfit {
        self.lead(&[name]);
        self
    }

    /// The failure of two that say more: the one whose value fit further, the first of equals.
    pub fn either(first: Option<Unfit>, next: Unfit) -> Unfit {
        match first {
            Some(f) if f.reach() >= next.reach() => f,
            _ => next,
        }
    }

    /// How far the value fit before failing: least, another kind at its root; then the right
    /// kind at its root, or a field it lacks, or a key no field names; most, inside what it holds.
    fn reach(&self) -> u8 {
        let one_step = self.path.starts_with('.')
            && !self.path.get(1..).unwrap_or_default().contains(['.', '[']);
        match &self.why {
            Why::Kind(_) if self.path.is_empty() => 0,
            Why::Kind(_) => 2,
            _ if self.path.is_empty() => 1,
            Why::Absent | Why::Unnamed if one_step => 1,
            _ => 2,
        }
    }

    /// This failure as the error of the deserializer `E`: the failure goes in the slot of the
    /// thread, and the error is the marker.
    pub fn raise<E: de::Error>(self) -> E {
        exchange(Some(self));
        E::custom(MARKER)
    }

    /// [`raise`](Self::raise) for the error of a serializer.
    pub fn refuse<E: ser::Error>(self) -> E {
        exchange(Some(self));
        E::custom(MARKER)
    }

    /// The failure last raised in this thread and not taken yet: where a read that failed through
    /// a deserializer of another crate left it.
    pub fn raised() -> Option<Unfit> {
        exchange(None)
    }

    /// The failure of text that serde_json can't read, by the kind of its error.
    pub fn from_json(e: &serde_json::Error) -> Unfit {
        Unfit::other(match e.classify() {
            Category::Io => "text that can't be read",
            Category::Syntax => "text that isn't JSON",
            Category::Data => "JSON that doesn't fit",
            Category::Eof => "JSON that ends too soon",
        })
    }
}

/// Leads `path` by `steps`, built in one allocation.
fn lead(path: &mut Cow<'static, str>, steps: &[&str]) {
    let mut led = String::with_capacity(
        steps
            .iter()
            .map(|s| s.len())
            .fold(path.len(), usize::saturating_add),
    );
    steps.iter().for_each(|s| led.push_str(s));
    led.push_str(path);
    *path = Cow::Owned(led);
}

/// The place, then why.
impl fmt::Display for Unfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.why {
            Why::Kind(_) => "expected ",
            _ => "",
        };
        let why = self.why.text();
        match &*self.path {
            "" => write!(f, "{what}{why}"),
            path => write!(f, "{path}: {what}{why}"),
        }
    }
}

impl std::error::Error for Unfit {}
