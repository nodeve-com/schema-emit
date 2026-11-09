The standard form: values as standard Rust, for use anywhere. A struct per object model, named by a function you pass to [`StdForm`], such as [`names`]; a clash adds `_`. [`fields`] gives each schema name's Rust name, in snake case.

- A required field is `T`, an optional or nullable one `Option<T>`, an optional nullable one `Option<Option<T>>`: absent `None`, null `Some(None)`. An optional field with a `default` is `T` ([defaults](#defaults)).
- A value of any kind is `::any_value::Value`, a null `::any_value::Null`, both from the `any-value` crate.
- A number keeps its width: an integer is the Rust integer of its name, a float `f32` or `f64`. Text is `String`, bytes `Vec<u8>`, a timestamp `chrono::DateTime<chrono::Utc>`, a duration `chrono::TimeDelta`, a list `Vec`, a map a `Vec` of pairs.
- A `Box` wraps only a field whose type holds its own model.
- A model whose root is no object, such as one number or a list, is its value's own Rust type, named by an alias: an `int8` root is `i8`. One that holds itself through a `$ref` is a struct of one field instead, as Rust has no alias of itself.
- A union is an enum, a variant per branch, named by its `$defs` key, else its branches' types joined by `Or`.

[`emit::Emitter`](super::Emitter) writes its checks over these types, taking a [`StdForm`]. Beside each struct it writes a serde `Deserialize` and `Serialize` impl and the function `decode_<name>` (snake case, same rule). An alias has no impl, as Rust allows none on `i8` or `Vec`, so it gets `decode_<name>` and `encode_<name>(&value, serializer)`, and `default_<name>()` where its root has a [default](#defaults).

## Extra keys

A key no field names, in a model's input:

- **Dropped**, under `x-strip-additional: true`: no field holds it, and `Serialize` writes none. This is schema-emit's own keyword, so `notes` needn't name it.
- **A failure**, under `additionalProperties: false` (a closed model): it fails the decode. A schema giving it `x-strip-additional` too is invalid.
- **Kept**, otherwise (an open model): as text, or as a typed value if a check reads it, and `Serialize` writes it back.

## Defaults

Each `default` is a function returning it: `<Model>::default_<field>()` for a property's, and `default_<name>()` (the same name rule as `decode_<name>`) for an alias model's root. Anything needing a value it has none of calls it. The decode does so when it finds no value for a field: the key is absent, or its value is bad and skipped. A defaulted field always holds a value, so the field's checks and the model's counts of keys present read it as present, and `Serialize` writes it.

A default is a boolean, a number, or plain text. Lowering judges it by its type, and refuses one of another kind, past the number's width, or failing the type's checks. [Missing](missing.md#smaller-gaps) has the defaults schema-emit can't hold. A struct holding a default derives no `Default`, since Rust's would differ from it.

## Decode

`decode_<name>` takes any serde deserializer (`schema_emit_runtime::decode::json` for text) and returns `(value, skips)`. The first failing required value fails the decode, as does a key a closed model doesn't name ([extra keys](#extra-keys)). A bad optional value is absent instead, returned as a skip beside the value (schema-emit-runtime's README has which deserializers skip, and how many skips it keeps). `Unfit` names a failure's or a skip's place (`path`, as `Model.field[2].key`) and its `Why`.

A union tries each branch on its value's text: the first whose check passes, else the first that decodes, else the one the value fit furthest.

## Encode

`Serialize` skips an absent optional field, writes a union as its branch's value, and writes unread keys back. A timestamp and a duration write in any-value's text forms ([README](https://github.com/nodeve-com/any-value#timestamps-and-durations-as-text)).

The same impls read and write a binary format such as postcard, by position (schema-emit-runtime's README has the layout).
