//! [`Skip`]: the seed of an optional field. A value that doesn't fit is absent and noted as a
//! skip, and the decode reads on. In JSON text the value is taken as text first, so a failure
//! leaves the deserializer past it; a deserializer of a tree answers with the value's own
//! deserializer. A format that isn't human-readable has no skips.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::thread_local;

use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor};

use super::skips::{mark, note, take, Mark};
use super::union::{Text, RAW};
use super::Unfit;

/// Why a value fails that no `Unfit` was raised for.
const UNREAD: &str = "a value the deserializer can't read";

/// How an optional field reads now.
#[derive(Clone, Copy)]
struct Trial {
    /// Inside a bare read: a field fails with its value.
    bare: bool,
    /// Whether a field met that read.
    met: bool,
}

const IDLE: Trial = Trial {
    bare: false,
    met: false,
};

thread_local! {
    static TRIAL: Cell<Trial> = const { Cell::new(IDLE) };
}

/// How an optional field reads now.
fn trial() -> Trial {
    TRIAL.try_with(Cell::get).unwrap_or(IDLE)
}

/// Sets how optional fields read, giving the trial it replaces.
fn exchange(next: Trial) -> Trial {
    TRIAL.try_with(|t| t.replace(next)).unwrap_or(IDLE)
}

/// Puts the trial back when a read ends or unwinds.
struct Restore(Trial);

impl Drop for Restore {
    fn drop(&mut self) {
        exchange(self.0);
    }
}

/// What `read` gives with `bare` set, and whether a field met a bare read.
fn scoped<T>(bare: bool, read: impl FnOnce() -> T) -> (T, bool) {
    let _restore = Restore(exchange(Trial { bare, met: false }));
    let out = read();
    (out, trial().met)
}

/// What `read` gives read bare, and whether a field failed in it.
pub(super) fn bare<T>(read: impl FnOnce() -> T) -> (T, bool) {
    scoped(true, read)
}

/// Whether a bare read has failed through an optional value, so its failure is read again and
/// its own text says nothing.
pub(super) fn bare_failed() -> bool {
    let trial = trial();
    trial.bare && trial.met
}

/// What `read` gives, its fields taking their own text whatever read holds it.
pub(super) fn whole<T>(read: impl FnOnce() -> T) -> T {
    scoped(false, read).0
}

/// The value `S` reads, or nothing where the value doesn't fit.
pub struct Skip<S, V>(S, PhantomData<fn() -> V>);

impl<S: Clone, V> Clone for Skip<S, V> {
    fn clone(&self) -> Self {
        Skip(self.0.clone(), PhantomData)
    }
}

pub fn skip<S, V>(seed: S) -> Skip<S, V>
where
    S: for<'a> DeserializeSeed<'a, Value = V> + Clone,
{
    Skip(seed, PhantomData)
}

impl<'de, S, V> DeserializeSeed<'de> for Skip<S, V>
where
    S: for<'a> DeserializeSeed<'a, Value = V> + Clone,
{
    type Value = Option<V>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<V>, D::Error> {
        match (d.is_human_readable(), trial().bare) {
            (true, false) => d.deserialize_newtype_struct(RAW, self),
            (true, true) => self.0.deserialize(d).map(Some).map_err(|_| {
                drop(Unfit::raised());
                exchange(Trial {
                    met: true,
                    bare: true,
                });
                de::Error::custom("an optional value failed")
            }),
            (false, _) => self.0.deserialize(d).map(Some),
        }
    }
}

impl<'de, S, V> Visitor<'de> for Skip<S, V>
where
    S: for<'a> DeserializeSeed<'a, Value = V> + Clone,
{
    type Value = Option<V>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a value")
    }

    /// Where the deserializer holds the value's text: its one entry.
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Option<V>, A::Error> {
        a.next_key::<IgnoredAny>()?;
        let text = a.next_value_seed(Text)?;
        let at = mark();
        let (read, met) = bare(|| read_text(&self.0, &text));
        let read = match read {
            Err(_) if met => {
                drop(take(at));
                read_text(&self.0, &text)
            }
            read => read,
        };
        settle(read, at)
    }

    /// Where it holds a tree: the value's deserializer.
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Option<V>, D::Error> {
        let at = mark();
        settle(self.0.deserialize(d), at)
    }
}

/// The value `seed` reads from `text`, which holds nothing past it.
fn read_text<S, V>(seed: &S, text: &str) -> Result<V, serde_json::Error>
where
    S: for<'a> DeserializeSeed<'a, Value = V> + Clone,
{
    let mut de = serde_json::Deserializer::from_str(text);
    let v = seed.clone().deserialize(&mut de)?;
    de.end()?;
    Ok(v)
}

/// The value, or absent with a skip where the failure is an unfit value. Any other failure
/// fails the decode. Skips noted inside a value that failed go with it.
fn settle<V, F, E: de::Error>(read: Result<V, F>, at: Mark) -> Result<Option<V>, E> {
    match read {
        Ok(v) => Ok(Some(v)),
        Err(_) => {
            drop(take(at));
            let unfit = Unfit::raised().ok_or_else(|| Unfit::other(UNREAD).raise::<E>())?;
            note(unfit);
            Ok(None)
        }
    }
}
