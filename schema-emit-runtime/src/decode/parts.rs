//! [`Parts`]: how a model, a list, a map and a tuple read their parts. The first part that fails
//! fails the decode at once, its failure led by the part's place.

use std::string::String;
use std::vec::Vec;

use serde::de::{self, DeserializeSeed, IgnoredAny, SeqAccess};

use super::skips::{self, mark};
use super::Unfit;

/// The index of an array's next item. The functions without it read an object's parts.
#[derive(Default)]
pub struct Parts {
    at: usize,
}

/// The failure raised under `e`, as `step` makes it of the part; an error of the text as it is.
fn fail<E: de::Error>(e: E, step: impl FnOnce(Unfit) -> Unfit) -> E {
    match Unfit::raised() {
        Some(unfit) => step(unfit).raise(),
        None => e,
    }
}

/// What `read` gives, led by `step`: a failure made of the part, the skips it noted as they are.
fn part<T, E: de::Error>(
    read: impl FnOnce() -> Result<T, E>,
    step: impl Fn(Unfit) -> Unfit,
) -> Result<T, E> {
    let at = mark();
    match read() {
        Ok(v) => {
            skips::lead(at, &step);
            Ok(v)
        }
        Err(e) => Err(fail(e, step)),
    }
}

impl Parts {
    /// The field named `name` (a path step, `.name`) has the value `read` gives, into `slot`.
    /// The skips noted while it reads are led by the field.
    pub fn field<T, E: de::Error>(
        slot: &mut Option<T>,
        name: &'static str,
        read: impl FnOnce() -> Result<T, E>,
    ) -> Result<(), E> {
        *slot = Some(part(read, |u| u.step(name))?);
        Ok(())
    }

    /// The optional field `name`, whose `read` gives nothing where the value was skipped: the
    /// slot keeps what it had.
    pub fn optional<T, E: de::Error>(
        slot: &mut Option<T>,
        name: &'static str,
        read: impl FnOnce() -> Result<Option<T>, E>,
    ) -> Result<(), E> {
        if let Some(v) = part(read, |u| u.step(name))? {
            *slot = Some(v);
        }
        Ok(())
    }

    /// The value `read` gives under `key`, which no field names, into `out`; a repeated key keeps
    /// its place and takes the later value.
    pub fn other<T, K: AsRef<str> + Into<String>, E: de::Error>(
        out: &mut Vec<(String, T)>,
        key: K,
        read: impl FnOnce() -> Result<T, E>,
    ) -> Result<(), E> {
        let v = part(read, |u| u.field(key.as_ref()))?;
        put(out, key.into(), v);
        Ok(())
    }

    /// Fails the model where the required field `name` has no `value`.
    pub fn require<T, E: de::Error>(value: &Option<T>, name: &'static str) -> Result<(), E> {
        match value {
            Some(_) => Ok(()),
            None => Err(Unfit::absent().step(name).raise()),
        }
    }

    /// The failure of a closed model on `key`, which no field names.
    pub fn unnamed<E: de::Error>(key: &str) -> E {
        Unfit::unnamed().field(key).raise()
    }

    /// The next item of `a` read by `seed`, `None` at the end.
    pub fn item<'de, A: SeqAccess<'de>, S: DeserializeSeed<'de>>(
        &mut self,
        a: &mut A,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        let at = self.at;
        self.at = at.saturating_add(1);
        part(|| a.next_element_seed(seed), |u| u.index(at))
    }

    /// The items of `a` not yet read, each read by `seed`.
    pub fn tail<'de, A: SeqAccess<'de>, S: DeserializeSeed<'de> + Copy>(
        &mut self,
        a: &mut A,
        seed: S,
    ) -> Result<Vec<S::Value>, A::Error> {
        let mut out = Vec::new();
        while let Some(item) = self.item(a, seed)? {
            out.push(item);
        }
        Ok(out)
    }

    /// The end of `a`, a tuple of no items past those read.
    pub fn fixed<'de, A: SeqAccess<'de>>(&mut self, a: &mut A) -> Result<(), A::Error> {
        match a.next_element::<IgnoredAny>()? {
            Some(_) => {
                let extra = Unfit::kind("no more items than the tuple holds").index(self.at);
                Err(extra.raise())
            }
            None => Ok(()),
        }
    }

    /// `out` as the decode of the value: a part `out` lacks fails as absent.
    pub fn finish<T, E: de::Error>(out: Option<T>) -> Result<T, E> {
        out.ok_or_else(|| Unfit::absent().raise())
    }
}

/// `value` under `key` in `out`: a key already there takes the value in its place.
pub(super) fn put<T>(out: &mut Vec<(String, T)>, key: String, value: T) {
    match out.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = value,
        None => out.push((key, value)),
    }
}
