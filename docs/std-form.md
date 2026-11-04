Values as standard Rust, for use anywhere: a struct per model, named by a function you pass to [`StdForm`], such as [`names`]; a clash adds `_`. [`fields`] gives each schema name's Rust name, in snake case.

- A required field is `T`, an optional or nullable one `Option<T>`, an optional nullable one `Option<Option<T>>`: absent `None`, null `Some(None)`.
- A number keeps its width: an integer is the Rust integer of its name, a float `f32` or `f64`. Text is `String`, bytes `Vec<u8>`, a timestamp `chrono::DateTime<chrono::Utc>`, a duration `chrono::TimeDelta`, a list `Vec`, a map a `Vec` of pairs. A value of any kind is a `serde_json::Value`, a null `()`.
- A `Box` wraps only a field whose type holds its own model.
- A union is an enum, a variant per branch, named by its `$defs` key, else its branches' types joined by `Or`.
- Keys no field names become typed values if a check reads them, else text if the model is open, else a failure. An open model with `x-strip-additional: true` drops those keys instead: it has no field for them, and `Serialize` writes none. The keyword beside a closed model is invalid.

[`StdForm`] is the [`Access`](super::Access) to these types. Beside each struct it writes a serde `Deserialize` and `Serialize` impl and the function `decode_<name>` (snake case, same rule).

## Decode

`decode_<name>` takes any serde deserializer (`schema_emit_runtime::decode::json` for text) and returns `(value, skips)`. The first failing required value, or a key a closed model doesn't name, fails the decode. A bad optional value is absent instead, returned as a skip beside the value (schema-emit-runtime's README has which deserializers skip, and how many skips it keeps). `Unfit` names a failure's or a skip's place (`path`, as `Model.field[2].key`) and its `Why`.

A union tries each branch on its value's text: the first whose check passes, else the first that decodes, else the one the value fit furthest.

## Encode

`Serialize` skips an absent optional field, writes a union as its branch's value, and writes unread keys back. A duration writes as seconds, such as `1.5s`.

The same impls read and write a binary format such as postcard, by position (schema-emit-runtime's README has the layout).
