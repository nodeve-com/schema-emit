//! What the decode code that schema-emit writes links (feature `decode`). Each model gets a serde
//! `Deserialize` impl, written out by the emitter; it reads a field's value through a seed built
//! from the parts here, and builds no document. One impl reads JSON text, a parsed
//! `serde_json::Value`, and any other self-describing format.
//!
//! - [`Kinds`] and [`Shape`] read a value by its JSON kind; `scalars` and `numbers` hold those of
//!   the scalars.
//! - `shapes` holds the seeds of a list, a map, a nullable or boxed value, and a type that
//!   deserializes itself, and [`Parts`], which a model, a list and a tuple read their parts with.
//! - `union` reads a union's value as text and tries each branch.
//! - `rest` keeps the keys of an open model that nothing reads as text and writes them back; `ser`
//!   holds the writers the emitted `Serialize` impls compose.
//! - `binary` reads a value by position where the format isn't self-describing (postcard): each
//!   [`Kinds`] reads its own binary form, so the same emitted impl reads both.
//! - `skip` and `skips` read an optional field's value and keep what it skipped.
//! - [`Unfit`] says where a value fails and why, and carries a failure out of the deserializer
//!   that reads the value.
//!
//! The first part that fails fails the decode, naming its place. An optional field whose value
//! doesn't fit is absent and a [`Skips`] entry instead ([`Skip`]); the decode that returns them
//! is the one that collects.

pub mod binary;
#[macro_use]
mod refuse;
mod any;
mod kinds;
mod numbers;
mod parts;
mod rest;
mod scalars;
pub mod ser;
mod shapes;
mod skip;
mod skips;
mod unfit;
mod union;
mod why;

pub use any::{any, Any};
pub use kinds::{Kinds, Shape};
pub use numbers::*;
pub use parts::Parts;
pub use rest::{members, Unread};
pub use scalars::*;
pub use serde::de::{Deserialize, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess};
pub use serde::de::{EnumAccess, VariantAccess};
pub use serde::ser::{Serialize, SerializeMap, SerializeTuple, Serializer};
pub use shapes::*;
pub use skip::{skip, Skip};
pub use skips::Skips;
pub use unfit::Unfit;
pub use union::{kind, Pick, Raw};
pub use why::Why;

/// Whatever `read` decodes from the JSON text `text`, which holds nothing past one value. The
/// text reads bare first; where an optional value failed, it reads again so each such value is
/// skipped.
pub fn json<'a, T>(
    text: &'a str,
    mut read: impl FnMut(
        &mut serde_json::Deserializer<serde_json::de::StrRead<'a>>,
    ) -> Result<(T, Skips), Unfit>,
) -> Result<(T, Skips), Unfit> {
    let mut once = || {
        let mut de = serde_json::Deserializer::from_str(text);
        let out = read(&mut de)?;
        de.end().map_err(|e| Unfit::from_json(&e))?;
        Ok(out)
    };
    match skip::bare(&mut once) {
        (Err(_), true) => skip::whole(once),
        (done, _) => done,
    }
}

/// What the decode of model `name` reads, with the skips it notes, each led by the model. A
/// failure names the model at the root of its path.
pub fn root<T, E>(read: impl FnOnce() -> Result<T, E>, name: &str) -> Result<(T, Skips), Unfit> {
    let _previous = unfit::Previous::empty();
    skips::collect(
        || {
            read().map_err(|_| match skip::bare_failed() {
                true => Unfit::absent(),
                false => Unfit::raised().map_or_else(
                    || Unfit::other("a value the deserializer can't read").model(name),
                    |unfit| unfit.model(name),
                ),
            })
        },
        |u| u.model(name),
    )
}

#[cfg(test)]
mod binary_tests;
#[cfg(test)]
mod skip_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tuple_tests;
