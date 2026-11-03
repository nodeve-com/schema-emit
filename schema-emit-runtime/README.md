# schema-emit-runtime

What the Rust that `schema-emit` writes links: the [`Reason`] a failure has, the [`Report`] failures go to, [`Failures`], which keeps the first of them, and [`Holds`], which keeps only whether any came. Being `no_std`, with one dependency, pattern-emit's search side, and no allocator of its own, it builds into whatever module or binary the emitted check lands in. The `decode` feature adds `decode`, which needs `std`: the seeds and writers that the `Deserialize` and `Serialize` impls schema-emit writes compose, and `Unfit`, naming where a decode fails and why, as a `Why` to match on. Each seed also reads the binary form by position (`decode::binary`), chosen by the format's `is_human_readable`.

## Why

Emitted checks link this crate, not schema-emit, which depends on serde_json and the JSON Schema front end. This crate is `no_std` and depends only on pattern-emit's search side (no parser), so a check's module brings in none of that. Built for wasm32, a module with emitted checks has no imports, and the first model costs about 3.5 KB.

## Conventions

- **`no_std`, one dependency**, bar the `decode` feature. New code uses `core` only, and adds no crate. [`Pattern`] is pattern-emit's, re-exported so emitted code names only `s::`.
- **Never panics.** Pure crates deny `unwrap`, `expect`, indexing, and unchecked arithmetic in its lints. A panic there would take down the process.
- **Failures go to [`Report::fail`]**, with the path as a string literal and its list indices in `at`. A recursive model marks its steps back to itself, `Value(.left|.right)*.n`, and `at` leads with the repeat count, then each repeat's step and indices ([`Report`] gives the order). `Failure` prints the expanded path. Entries past [`DEPTH`] read as 0. A new reason is a new [`Reason`] variant with its name in `Reason::name`. Those names are what the suite compares against each case's expected failures, so renaming one changes the expected output.
- **Helpers.** A check's arithmetic too long to write inline goes here as a function with a unit test beside it, like [`multiple_of`](fn@multiple_of) and [`chars`](fn@chars).
- **Testing.** Unit tests sit beside the code and run with `cargo nextest run -p schema-emit-runtime`. schema-emit's suite tests the emitted checks.

## Binary form

A format that is not human-readable gets a positional form, with no intermediate value:

- A model is its fields in order, an optional one an option, then its kept keys: a list of key and value, else their text.
- A map is a list of key and value; a repeated key in text keeps its place and takes the later value.
- A timestamp is whole seconds then nanoseconds, a duration as chrono writes it, bytes raw.
- A tuple is an option per position, then the list of the rest.
- A union is its branch's index, then the branch's value.
- A value of any kind is the text of its JSON, the one text.
- A binary failure is `Why::Other`, with a fixed reason.

## Failures

- A failure is an `Unfit`, held in a slot of the thread while it crosses the deserializer. `Unfit::raise` fills the slot and returns a custom error of one fixed message; no failure becomes text or comes back from it.
- Each level the error returns through takes the `Unfit` from the slot, leads its path with its step, and raises it again. An error of the text or the deserializer passes through unchanged.
- `decode::root` empties the slot on entry, and takes the `Unfit` on failure. It makes an error that isn't a raised failure an `Unfit` there, once, with a fixed reason, formatting no error. `Unfit::from_json` reads a `serde_json::Error` by its category.
- An embedder that fills a model through another deserializer calls `Unfit::raised` after the fill fails, for the last `Unfit` raised in the thread.

## Skips

- A bad optional value is absent, and the decode notes it as a skip: an `Unfit` with its full place. `decode_<name>` returns the value with its `Skips`; a `Deserialize` used directly notes nothing.
- `Skips` keeps the first 15 and counts the rest in `dropped`.
- A skip names its place, as a failure does: each part that returns leads the skips noted inside it with its step.
- A union keeps the skips of the branch it chooses and drops those of the branches it tries.
- JSON text and a `serde_json::Value` skip. A tree deserializer skips if it answers a newtype struct with `visit_newtype_struct`. A streaming deserializer other than serde_json's text can't skip, and the binary form has no skips: the bad value fails the decode.
- `decode::json` reads the text without copying any value first, and reads it again only where an optional value failed.
