# One decode

`decode_<name>` becomes the only reader of JSON into a model. cel-emit and loom call it. cel-emit's typed `json.decode(text, Message)` goes.

## Shape

- Each model gets an emitted serde `Deserialize` impl: written out by the emitter, not `#[derive]`. It visits tokens as serde reads them, building no `serde_json::Value` tree.
- One impl reads JSON text, a parsed `serde_json::Value`, and YAML. Every caller uses the same function.
- Types stay strong: a required field is `T`, an optional field is `Option<T>`.
- The decode reads past a failing optional field and leaves it absent. A failing required field fails the decode. The model's check then judges the result.
- A union keeps its value as a raw slice (`RawValue`) and tries each branch on it, using the README's union rule. When the input isn't text, serde_json writes only that value back to text. Checked in a throwaway prototype.
- Unknown keys follow the schema, as the IR already says (`Model.others`, `Model.open`). If a check reads them, they're kept as typed values. If the model is open and nothing reads them, they're skipped. If it's closed, an unknown key is a failure.
- An integral float such as `1.0` fills a whole-number field. The visitor accepts it.
- Depth: serde_json's own limit of 128 nested levels on text, skipped values included. Parsing a value or YAML already applied a limit. schema-emit keeps no limit of its own.
- Each number keeps its width (`i8`, `u16`, `f32`, ...), as loom `docs/steps.md` requires. The IR gains 128-bit integers (`i128`, `u128`), named as cel-emit's widths. serde rejects a value past the width while decoding.
- An optional nullable field is `Option<Option<T>>`: absent is the outer `None`, `null` is `Some(None)`. A skipped bad value is absent, never `null`.
- A non-object where a field takes a model is a bad value, so the rules above apply.
- A timestamp is RFC 3339 at UTC, read by chrono; a leap second is the next second.
- A failure still names its place (`Model.field[2].key`). Each level adds its own part to the error.

## Deletions first

- schema-emit: the hand-written `decode::Reader` and its walk (`schema-emit-runtime/src/decode/walk.rs`), plus the emitter code that targets it (`src/emit/std_form/decode.rs`, `decode/union.rs`), replaced by the `Deserialize` emitter.
- cel-emit: its typed decode, through cel-emit `PLANS/type-arguments.md`. That plan also lets an embedder's function take a message type as an argument, so loom can add its own.
- README "Why a written decode function" line: rewritten for emitted `Deserialize`.

## Loom

- Loom adds one function to cel-emit that calls the model's `decode_<name>`. Its name is still to choose (`decode(text, Model)`?).
- Loom's three file loaders (host config, the MQTT adapter's settings, definitions) call `decode_<name>` on their parsed value, unchanged.
- Loom pins schema-emit and cel-emit at the landed revs.

## Measure

- Allocation calls and callgrind instructions for `answer_rows`. The plan lands only if decode does no worse than today's reader. Today, at schema-emit 80aa62e:
  - Allocation calls: generated decode 50 vs reference 51.
  - callgrind instructions: decode 392k vs derive 242k vs interpreter 875k.
  - Command: in cel-emit, `cargo test -p cel-emit-cli --test references -- --no-capture`; instructions via `sh scripts/linux.sh sh scripts/instructions.sh`.
