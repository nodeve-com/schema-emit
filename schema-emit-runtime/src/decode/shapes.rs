//! The readers of structured values: a list's items, a map's entries, a tuple's positions, a
//! model's fields and the keys none names. Each reads its parts' text through a reader the
//! emitted code passes and puts its own step on the path of a part that fails.

use std::collections::BTreeMap;
use std::string::String;
use std::vec::Vec;

use super::walk::{first, shallow, walk};
use super::{null, Unfit};

/// The text `v` of a whole value of model `name`, read by `read`: refused where it nests too
/// deep, and a failure's path led by `name`.
pub fn root<T>(
    v: &str,
    name: &str,
    read: impl FnOnce(&str) -> Result<T, Unfit>,
) -> Result<T, Unfit> {
    shallow(v).and_then(|()| read(v)).map_err(|e| e.model(name))
}

/// A value that is null or what `item` reads: `None` for null.
pub fn nullable<T>(
    v: &str,
    item: impl FnOnce(&str) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    match first(v) {
        Some(b'n') => null(v).map(|()| None),
        _ => item(v).map(Some),
    }
}

/// An array, each item read by `item`.
pub fn list<T>(v: &str, item: impl Fn(&str) -> Result<T, Unfit>) -> Result<Vec<T>, Unfit> {
    tail(v, 0, item)
}

/// The items of the array `v` from position `from`, each read by `item`.
pub fn tail<T>(
    v: &str,
    from: usize,
    item: impl Fn(&str) -> Result<T, Unfit>,
) -> Result<Vec<T>, Unfit> {
    let (mut out, mut i) = (Vec::new(), 0_usize);
    walk(v, b'[', "an array", |_, x| {
        if i >= from {
            out.push(item(x).map_err(|e| e.index(i))?);
        }
        i = i.saturating_add(1);
        Ok(())
    })?;
    Ok(out)
}

/// The first `N` items of the array `v`, a tuple's positions: `None` past its end.
pub fn tuple<const N: usize>(v: &str) -> Result<[Option<&str>; N], Unfit> {
    let (mut out, mut i) = ([None; N], 0_usize);
    walk(v, b'[', "an array", |_, x| {
        if let Some(slot) = out.get_mut(i) {
            *slot = Some(x);
        }
        i = i.saturating_add(1);
        Ok(())
    })?;
    Ok(out)
}

/// Position `index` of a tuple, read by `item`: `None` where the array ends before it.
pub fn element<T>(
    x: Option<&str>,
    index: usize,
    item: impl FnOnce(&str) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    x.map(|x| item(x).map_err(|e| e.index(index))).transpose()
}

/// An array of no items past `n`.
pub fn fixed(v: &str, n: usize) -> Result<(), Unfit> {
    let mut i = 0_usize;
    walk(v, b'[', "an array", |_, _| {
        if i == n {
            return Err(Unfit::kind("no more items than the tuple holds").index(n));
        }
        i = i.saturating_add(1);
        Ok(())
    })
}

/// An object's values under `names`, a model's fields: `None` where it lacks the key, the last
/// value where it repeats one.
pub fn object<'a, const N: usize>(
    v: &'a str,
    names: &[&str; N],
) -> Result<[Option<&'a str>; N], Unfit> {
    let mut out = [None; N];
    walk(v, b'{', "an object", |k, x| {
        let at = names.iter().position(|n| Some(*n) == k);
        if let Some(slot) = at.and_then(|at| out.get_mut(at)) {
            *slot = Some(x);
        }
        Ok(())
    })?;
    Ok(out)
}

/// A required field `name`, its value `x` where the object has one, read by `item`.
pub fn field<T>(
    x: Option<&str>,
    name: &str,
    item: impl FnOnce(&str) -> Result<T, Unfit>,
) -> Result<T, Unfit> {
    match x {
        Some(x) => item(x).map_err(|e| e.field(name)),
        None => Err(Unfit::other(super::ABSENT).field(name)),
    }
}

/// An optional field `name`, its value `x` where the object has one: `None` where absent.
pub fn optional<T>(
    x: Option<&str>,
    name: &str,
    item: impl FnOnce(&str) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    x.map(|x| item(x).map_err(|e| e.field(name))).transpose()
}

/// An object, each value read by `item`, under its key.
pub fn map<T>(
    v: &str,
    item: impl Fn(&str) -> Result<T, Unfit>,
) -> Result<BTreeMap<String, T>, Unfit> {
    rest(v, &[], item)
}

/// The entries of the object `v` under keys not in `named`, each read by `item`.
pub fn rest<T>(
    v: &str,
    named: &[&str],
    item: impl Fn(&str) -> Result<T, Unfit>,
) -> Result<BTreeMap<String, T>, Unfit> {
    let mut out = BTreeMap::new();
    walk(v, b'{', "an object", |k, x| {
        let k = k.unwrap_or_default();
        if !named.contains(&k) {
            out.insert(k.into(), item(x).map_err(|e| e.field(k))?);
        }
        Ok(())
    })?;
    Ok(out)
}

/// An object of no key outside `named`.
pub fn closed(v: &str, named: &[&str]) -> Result<(), Unfit> {
    walk(v, b'{', "an object", |k, _| {
        let k = k.unwrap_or_default();
        match named.contains(&k) {
            true => Ok(()),
            false => Err(Unfit::other(super::UNNAMED).field(k)),
        }
    })
}
