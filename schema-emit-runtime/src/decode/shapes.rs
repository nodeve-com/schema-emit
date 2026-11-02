//! The readers of structured values: a list's items, a map's entries, a tuple's positions, a
//! model's fields and the keys none names. Each reads its parts through a reader the emitted code
//! passes and puts its own step on the path of a part that fails.

use std::collections::BTreeMap;
use std::string::String;
use std::vec::Vec;

use serde_json::{Map, Value};

use super::{array, object, Unfit};

/// A value that is null or what `item` reads: `None` for null.
pub fn nullable<T>(
    v: &Value,
    item: impl FnOnce(&Value) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    match v {
        Value::Null => Ok(None),
        v => item(v).map(Some),
    }
}

/// An array, each item read by `item`.
pub fn list<T>(v: &Value, item: impl Fn(&Value) -> Result<T, Unfit>) -> Result<Vec<T>, Unfit> {
    items(array(v)?, 0, item)
}

/// An object, each value read by `item`, under its key.
pub fn map<T>(
    v: &Value,
    item: impl Fn(&Value) -> Result<T, Unfit>,
) -> Result<BTreeMap<String, T>, Unfit> {
    entries(object(v)?.iter(), item)
}

/// The items of `xs` from position `from`, each read by `item`.
pub fn tail<T>(
    xs: &[Value],
    from: usize,
    item: impl Fn(&Value) -> Result<T, Unfit>,
) -> Result<Vec<T>, Unfit> {
    items(xs.get(from..).unwrap_or_default(), from, item)
}

fn items<T>(
    xs: &[Value],
    from: usize,
    item: impl Fn(&Value) -> Result<T, Unfit>,
) -> Result<Vec<T>, Unfit> {
    let mut out = Vec::with_capacity(xs.len());
    for (i, x) in xs.iter().enumerate() {
        out.push(item(x).map_err(|e| e.index(from.saturating_add(i)))?);
    }
    Ok(out)
}

fn entries<'a, T>(
    entries: impl Iterator<Item = (&'a String, &'a Value)>,
    item: impl Fn(&Value) -> Result<T, Unfit>,
) -> Result<BTreeMap<String, T>, Unfit> {
    let mut out = BTreeMap::new();
    for (k, x) in entries {
        out.insert(k.clone(), item(x).map_err(|e| e.field(k))?);
    }
    Ok(out)
}

/// Position `index` of a tuple: `None` where `xs` ends before it.
pub fn element<T>(
    xs: &[Value],
    index: usize,
    item: impl FnOnce(&Value) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    match xs.get(index) {
        Some(x) => item(x).map(Some).map_err(|e| e.index(index)),
        None => Ok(None),
    }
}

/// A tuple of no items past `n`.
pub fn fixed(xs: &[Value], n: usize) -> Result<(), Unfit> {
    match xs.get(n) {
        Some(_) => Err(Unfit::kind("no more items than the tuple holds").index(n)),
        None => Ok(()),
    }
}

/// A required field `name` of `o`, read by `item`.
pub fn field<T>(
    o: &Map<String, Value>,
    name: &str,
    item: impl FnOnce(&Value) -> Result<T, Unfit>,
) -> Result<T, Unfit> {
    match o.get(name) {
        Some(v) => item(v).map_err(|e| e.field(name)),
        None => Err(Unfit::other(super::ABSENT).field(name)),
    }
}

/// An optional field `name` of `o`: `None` where absent.
pub fn optional<T>(
    o: &Map<String, Value>,
    name: &str,
    item: impl FnOnce(&Value) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    match o.get(name) {
        Some(v) => item(v).map(Some).map_err(|e| e.field(name)),
        None => Ok(None),
    }
}

/// The entries of `o` under keys not in `named`, each read by `item`.
pub fn rest<T>(
    o: &Map<String, Value>,
    named: &[&str],
    item: impl Fn(&Value) -> Result<T, Unfit>,
) -> Result<BTreeMap<String, T>, Unfit> {
    entries(o.iter().filter(|(k, _)| !named.contains(&k.as_str())), item)
}

/// An object of no key outside `named`.
pub fn closed(o: &Map<String, Value>, named: &[&str]) -> Result<(), Unfit> {
    match o.keys().find(|k| !named.contains(&k.as_str())) {
        Some(k) => Err(Unfit::other(super::UNNAMED).field(k)),
        None => Ok(()),
    }
}
