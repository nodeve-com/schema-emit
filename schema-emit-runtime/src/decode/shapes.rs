//! The readers of structured values: a list's items, a map's entries, a tuple's positions, a
//! model's fields and the keys none names. Each reads its parts in text order through a reader
//! the emitted code passes, and puts its own step on the path of a part that fails.

use std::collections::BTreeMap;
use std::string::String;
use std::vec::Vec;

use serde::de::IgnoredAny;
use serde_json::{Map, Value};

use super::walk::{Parts, Reader};
use super::{null, Unfit};

/// The text `v` of a whole value of model `name`, read by `read`. Where the read fails, text that
/// isn't one JSON value, or nests too deep, fails as that; a failure's path is led by `name`.
pub fn root<'a, T>(
    v: &'a str,
    name: &str,
    read: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<T, Unfit> {
    let mut r = Reader::new(v);
    let read = read(&mut r).and_then(|out| r.end().map(|()| out));
    read.map_err(|e| {
        let mut whole = Reader::new(v);
        let broken = skip(&mut whole).and_then(|()| whole.end()).err();
        broken.unwrap_or(e).model(name)
    })
}

/// The value here, read and dropped.
pub fn skip(r: &mut Reader<'_>) -> Result<(), Unfit> {
    match r.peek() {
        Some(b'[') => list(r, skip).map(drop),
        Some(b'{') => object(r, |_, r| skip(r)),
        _ => r.token::<IgnoredAny>().map(drop),
    }
}

/// The value here, the reader moved past it: a reader of it, for each branch of a union.
pub fn raw<'a>(r: &mut Reader<'a>) -> Result<Reader<'a>, Unfit> {
    let at = r.clone();
    skip(r)?;
    Ok(at)
}

/// The value here read by `read`, or its failure with the reader moved past it. Fails where the
/// text holds no value there.
fn settle<'a, T>(
    r: &mut Reader<'a>,
    read: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<Result<T, Unfit>, Unfit> {
    let at = r.clone();
    match read(r) {
        Ok(value) => Ok(Ok(value)),
        Err(e) => {
            *r = at;
            skip(r).map(|()| Err(e))
        }
    }
}

/// A value of any kind, as serde_json holds one.
pub fn any(r: &mut Reader<'_>) -> Result<Value, Unfit> {
    match r.peek() {
        Some(b'[') => list(r, any).map(Value::Array),
        Some(b'{') => {
            let mut out = Map::new();
            object(r, |k, r| {
                out.insert(k.into(), any(r)?);
                Ok(())
            })?;
            Ok(Value::Object(out))
        }
        _ => r.token(),
    }
}

/// A value that is null or what `item` reads: `None` for null.
pub fn nullable<'a, T>(
    r: &mut Reader<'a>,
    item: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    match r.peek() {
        Some(b'n') => null(r).map(|()| None),
        _ => item(r).map(Some),
    }
}

/// The array here, read item by item.
pub fn array<'r, 'a>(r: &'r mut Reader<'a>) -> Result<Parts<'r, 'a>, Unfit> {
    Parts::open(r, b'[', "an array")
}

/// An array, each item read by `item`.
pub fn list<'a, T>(
    r: &mut Reader<'a>,
    item: impl FnMut(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<Vec<T>, Unfit> {
    tail(array(r)?, item)
}

/// The items of `a` not yet read, each read by `item`.
pub fn tail<'a, T>(
    mut a: Parts<'_, 'a>,
    mut item: impl FnMut(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<Vec<T>, Unfit> {
    let mut out = Vec::new();
    while let Some(i) = a.next()? {
        out.push(item(a.r).map_err(|e| e.index(i))?);
    }
    Ok(out)
}

/// The next item of `a`, a tuple's position, read by `item`: `None` where the array has ended.
pub fn element<'a, T>(
    a: &mut Parts<'_, 'a>,
    item: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<Option<T>, Unfit> {
    let Some(i) = a.next()? else {
        return Ok(None);
    };
    item(a.r).map(Some).map_err(|e| e.index(i))
}

/// The end of `a`, an array of no items past those read.
pub fn fixed(mut a: Parts<'_, '_>) -> Result<(), Unfit> {
    match a.next()? {
        Some(i) => Err(Unfit::kind("no more items than the tuple holds").index(i)),
        None => Ok(()),
    }
}

/// An object, each entry to `each`: its key, and the reader at its value, which `each` reads.
pub fn object<'a>(
    r: &mut Reader<'a>,
    mut each: impl FnMut(&str, &mut Reader<'a>) -> Result<(), Unfit>,
) -> Result<(), Unfit> {
    let mut o = Parts::open(r, b'{', "an object")?;
    while o.next()?.is_some() {
        let key = o.key()?;
        each(&key.0, o.r)?;
    }
    Ok(())
}

/// An object, each value read by `item`, under its key.
pub fn map<'a, T>(
    r: &mut Reader<'a>,
    mut item: impl FnMut(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<BTreeMap<String, T>, Unfit> {
    let mut out = BTreeMap::new();
    object(r, |k, r| {
        out.insert(k.into(), item(r).map_err(|e| e.field(k))?);
        Ok(())
    })?;
    Ok(out)
}

/// A model field's value here, read by `item` into `slot`, its failure kept there: a later value
/// under the same key replaces it. Fails where the text holds no value here.
pub fn put<'a, T>(
    slot: &mut Option<Result<T, Unfit>>,
    r: &mut Reader<'a>,
    item: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<(), Unfit> {
    *slot = Some(settle(r, item)?);
    Ok(())
}

/// A required field `name`, from its `slot`: absent where the object had no value for it.
pub fn field<T>(slot: Option<Result<T, Unfit>>, name: &str) -> Result<T, Unfit> {
    match slot {
        Some(read) => read.map_err(|e| e.field(name)),
        None => Err(Unfit::other(super::ABSENT).field(name)),
    }
}

/// An optional field `name`, from its `slot`: `None` where absent.
pub fn optional<T>(slot: Option<Result<T, Unfit>>, name: &str) -> Result<Option<T>, Unfit> {
    slot.transpose().map_err(|e| e.field(name))
}

/// A model's entries under keys no field names, and the first of them that failed.
pub struct Rest<T> {
    entries: BTreeMap<String, T>,
    unfit: Option<Unfit>,
}

impl<T> Default for Rest<T> {
    fn default() -> Self {
        Rest {
            entries: BTreeMap::new(),
            unfit: None,
        }
    }
}

impl<T> Rest<T> {
    /// The entries, or the first failure among them.
    pub fn done(self) -> Result<BTreeMap<String, T>, Unfit> {
        match self.unfit {
            Some(unfit) => Err(unfit),
            None => Ok(self.entries),
        }
    }
}

/// The value here under `key`, which no field names, read by `item` into `rest`.
pub fn rest<'a, T>(
    rest: &mut Rest<T>,
    key: &str,
    r: &mut Reader<'a>,
    item: impl FnOnce(&mut Reader<'a>) -> Result<T, Unfit>,
) -> Result<(), Unfit> {
    match settle(r, item)? {
        Ok(value) => {
            rest.entries.insert(key.into(), value);
        }
        Err(e) => {
            rest.unfit.get_or_insert(e.field(key));
        }
    }
    Ok(())
}

/// The value here under `key`, which no field of a closed model names: a failure kept in `rest`.
pub fn unnamed(rest: &mut Rest<()>, key: &str, r: &mut Reader<'_>) -> Result<(), Unfit> {
    rest.unfit
        .get_or_insert_with(|| Unfit::other(super::UNNAMED).field(key));
    skip(r)
}
