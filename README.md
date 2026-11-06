# schema-emit

A **schema** describes what valid data looks like: which fields it has and which values it allows. schema-emit reads a [JSON Schema](https://json-schema.org) and writes Rust, for any caller:

- **Standard Rust types.** A struct per object model, and a value's own type for any other ([`emit::std_form::StdForm`]), its fields named from the schema, with a serde `Deserialize` and `Serialize` it writes out.
- **Extra keys dropped, kept, or refused.** `x-strip-additional: true` drops keys no field names ([`emit::std_form`] § Extra keys lists all three).
- **Models written elsewhere.** You can say another module holds a model ([`emit::std_form::StdForm::with_external`]).
- **Common value types.** Timestamps and durations are `chrono`'s. `format: human-duration` is a duration. Their text forms are [any-value's](https://github.com/nodeve-com/any-value#timestamps-and-durations-as-text).
- **Annotations you name.** A keyword such as a unit rides along when you ask for it. schema-emit refuses any other.

Anything else on a type, such as how it crosses a boundary or its storage, you write beside the type. The generated code uses schema-emit-runtime.

JSON Schema is the baseline. Another schema language would lower to the same IR, adding only what JSON Schema can't say.

## In short

A model with one required number, `temp_c`, from -40 to 85:

```json
{
  "type": "object",
  "required": ["temp_c"],
  "properties": {
    "temp_c": { "type": "number", "minimum": -40, "maximum": 85 }
  }
}
```

The emitted check, function header left out:

```rust,ignore
match Some(&v.temp_c) { Some(f0) => { { let n1: f64 = (*f0) as f64;
if !(n1.is_finite() && n1 >= f64::from_bits(0xc044000000000000) && n1 <= f64::from_bits(0x4055400000000000)) { r.fail("Reading.temp_c", &[], __s::Reason::Range); }
}
} None => { r.fail("Reading.temp_c", &[], __s::Reason::Absent); } }
```

A missing field fails `absent`; a value not finite or outside -40 to 85 fails `range`.

## Why

- **A validator over a parsed JSON document**, as the `jsonschema` crate generates, costs in wasm32 about 110 KB for the first model and 10 KB for each one after.
- **CDDL** has no corpus of verdicts, and each Rust reader refuses part of it.
- **JSON Schema** has a published conformance corpus, and editors already check YAML against it.

Why an emitted `Deserialize`, not `#[derive(Deserialize)]`: a union takes its first branch whose check passes, and `1.0` reads as a whole number. A failure names its place, inside a union too. It builds no document and reads JSON text, a `serde_json::Value`, or YAML alike.

## Features

- **Checks only what types can't promise.** The embedder's types guarantee each value's base. The check tests the rest: a width, a bound, a length, a choice, a field's presence. It never converts a value.
- **Reports, then goes on.** Each failing value goes to a `Report` with its path and `Reason`. `Failures` keeps the first N and counts the rest.
- **Refuses what it can't check**, naming the keyword.
- **Exact bounds.** A fractional bound on an integer rounds inward, so a comparison stays exact past 2^53.
- **Widths from `format`.** `int8` through `uint128`, `float`, and `double`, as OpenAPI's format registry names them. A float fails NaN and infinities.
- **Never panics.** Emitted code has no indexing, unchecked arithmetic, or `unwrap`.
- **Tested against the standard.** JSON-Schema-Test-Suite's draft 2020-12 files and the crate's own cases run through emitted code; none is wrong. Gaps: [`missing`].

## Use

1. [`json_schema::lower`] takes the keywords to carry (`notes`) and returns an [`ir::Schema`], the IR the emitter reads, and the named model's index. [`json_schema::lower_defs`] takes every `$defs` entry of a rootless document. A definition named twice is one model. The default `json-schema` feature holds it.
2. [`emit::Emitter`] takes a [`emit::std_form::StdForm`] and writes one function per model, checking a value of its std form.
3. The embedder writes the crate around it: `use ::schema_emit_runtime as __s;`, the model types, and a call passing a `Report`. schema-emit builds nothing.

[`emit::std_form`] has the standard form: each Rust type, `decode_<name>`, and `Serialize`.

## Conventions

- **Areas.** Each kind of constraint is one area: a module in the front end (`src/json_schema/<area>.rs`, lowering keywords to [`ir::Check`]s) and one in the emitter (`src/emit/<area>.rs`, writing them).
- **A front-end area** answers each keyword from its `keyword` function: `Some(Ok(()))` lowered (or, as JSON Schema reads a keyword on another type, nothing to check), `Some(Err(…))` refused, `None` not its keyword. A keyword no area owns counts as unsupported.
- **An emitter area** turns a check into a condition on the value's view (see the [`emit`] module docs), or walks a value it holds, as lists and models do. Literals are data: a bound is a literal of its exact value, choices an array. The emitter generates every name; a path is a string literal.
- **[`Error`]** is `unsupported` where no area handles a keyword yet, and `invalid` where the schema is wrong.
- **Adding a case**: see `tests/suite.rs`'s docs.
