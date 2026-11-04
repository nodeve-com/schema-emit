# One decode

`decode_<name>` becomes the only reader of JSON into a model. cel-emit and loom call it. cel-emit's typed `json.decode(text, Message)` goes.

## Shape

- Each model gets an emitted serde `Deserialize` impl: written out by the emitter, not `#[derive]`. It visits tokens as serde reads them, building no `serde_json::Value` tree.
- One impl reads JSON text, a parsed `serde_json::Value`, and YAML. Every caller uses the same function.
- Types stay strong: a required field is `T`, an optional field is `Option<T>`.
- The decode skips a bad optional value: the field is absent and the decode reads on. A bad required value, or an unknown key in a closed model, fails the decode at the first one.
- A decode that succeeds returns its value and each skip, an `Unfit` with its place and reason, so a caller can act on it: a file loader tells the user which line to fix. The list keeps the first skips up to a cap, counting the rest; the cap is in schema-emit-runtime's README.
- A union keeps its value as a raw slice (`RawValue`) and tries each branch on it, using the README's union rule. When the input isn't text, serde_json writes only that value back to text. Checked in a throwaway prototype.
- Unknown keys follow the schema, as the IR says (`Model.others`, `Model.additional`). If a check reads them, they stay typed values. If the model is open and nothing reads them, the model keeps them unparsed, as one raw JSON text of the keys, which its emitted `Serialize` writes back as they arrived; with `x-strip-additional: true` the decode drops them, keeping nothing. In a closed model, an unknown key fails the decode, and lowering refuses `x-strip-additional` beside it.
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

- Loom adds one function to cel-emit, `json.decode(text, Model)`, an overload beside cel-emit's `json.decode(text)`, that calls the model's `decode_<name>`. Loom's definitions keep their spelling.
- Loom's three file loaders (host config, the MQTT adapter's settings, definitions) call `decode_<name>` on their parsed value, unchanged.
- A bundle's record is the model's std-form type, replacing cel-emit's `Declared` struct for models. Std form gains general options as this needs (`Default`, `Serialize`, extra derives); nothing loom-specific.
- Loom's sinks write an open model's unread keys back through the model's `Serialize`, replacing cel-emit's deleted rest-key carrier.
- Records cross the bundle boundary in postcard, positional by type, through the std form's own serde in its binary form (`is_human_readable()` false): numbers as numbers, a timestamp as seconds and nanoseconds, no text. `bs::message!`'s impls go. Text is only JSON's.
- A stored number is its field's width; a step's result is never held wide. Narrowing past the width fails as `range` at the write, as loom `docs/steps.md` says.
- A model filled from a dynamic value stays cel-emit's dyn fill, its undeclared keys kept in the model's raw-others field; `json.encode` of a model writes through the model's `Serialize`.
- Loom pins schema-emit and cel-emit at the landed revs.

## Measure

The plan lands only if decode does no worse than today's reader, in both measures below.

### schema-emit's own

Built first, at today's reader, so its figures are the baseline.

- `tests/cost.rs`, beside `suite.rs`: lowers each fixture, emits its std form and `decode_<name>`, and builds one program that counts allocations around each decode, as cel-emit's `references/count.rs` does.
- Fixtures in `tests/fixture/cost/`, a schema and an instance per kind the plan touches: number widths (128-bit included), nested models, arrays of models, optional, nullable, and optional nullable fields, a union, a timestamp, open models with unknown keys read and skipped, escaped strings, a bad optional value skipped, and nesting near the depth limit.
- The table shows each case beside two references built from the same text:
  - `serde_json::from_str::<Value>`.
  - Derive: the same emitted types with `#[derive(serde::Deserialize)]`, in a second module. No hand-written Rust per fixture.
- The std form gains an option: its types without the decode, with extra derives an embedder names (`Serialize`, say).
- Where derive reads differently from the plan's rules, the table shows it: a union as `untagged`, an optional nullable field without a helper, and a skipped bad value that derive refuses.
- Allocation calls gate: each case at or under its baseline. Instructions, by the same callgrind toggle on `measured_decode`, only show; schema-emit gets its own `linux.sh` and `instructions.sh`.
- Hand-written check mirrors stay out: this plan leaves checks unchanged.

### cel-emit's `answer_rows`

- Allocation calls and callgrind instructions. Today, at schema-emit 80aa62e:
  - Allocation calls: generated decode 50 vs reference 51.
  - callgrind instructions: decode 392k vs derive 242k vs interpreter 875k.
  - Command: in cel-emit, `cargo test -p cel-emit-cli --test references -- --no-capture`; instructions via `sh scripts/linux.sh sh scripts/instructions.sh`.
- Before landing, cel-emit reads the local tree through `--config 'patch."https://github.com/nodeve-com/schema-emit".<crate>.path=...'` for `schema-emit` and `schema-emit-runtime`, its own code unchanged. This measures schema-emit's change apart from cel-emit's `PLANS/type-arguments.md`.

## Where it stands

- schema-emit: on main and pushed at db0551f: the cost harness, the emitted `Deserialize` with skips, and their docs.
- cel-emit: worktree `../cel-emit-wt-type-args`, branch `worktree-agent-type-args`: type arguments (the typed decode and `Decls.fill`/`Decls.rest` go), `Messages::serializes`, a dyn filling a message or list through its `Deserialize`, 128-bit widths, a model field converting a number when it can (past width `range`). In its working tree, not yet in a commit: the pin at schema-emit db0551f, the program's `__s` alias and its decode through `decode::json`, `json::own` writing a self-serializing message's text, and a number-rule test's stale case. All tests pass but the `answer_rows` allocation gate. Squash its `docs/extras.md` edits into one markdown commit at landing.
- loom: worktree `/Users/kaicurry/dev/loom/.claude/worktrees/one-decode`, branch `worktree-agent-one-decode`, most of it in the working tree, not in a commit: the loom `[patch]` block pointing at both worktrees blocks commits by the guard; it goes for rev pins at landing. Two commits on the branch (`f8a9b89b`, `17c5b165`) moved the wire to JSON and are undone in the working tree; drop them when squashing.

Open in loom:

- A bundle drops a decode's skips: a missing optional value is silence, as docs/steps.md says. A file loader whose decode skips refuses, naming each skip's place.
- `cargo test -p loom-compiler --test all --no-fail-fast`, shell-tokio with e2e, and adapter-mqtt pass. `module`'s `ALLOWED` holds `TwoWaySearcher::next_back`, reached only from serde_json's `make_error`.
- A foreign deserializer's own error (not a raised `Unfit`, not serde_json's) reads as one fixed reason, its message dropped, so no failure path formats.
- 2-models' metered module: 672 KB against main's 499 KB; cause and cuts tried in loom research/2026-11-03-23-emitted-decode-size.md (in the loom worktree, not in a commit).
- `decode_fuel::a_key_the_model_leaves_out_is_kept_at_a_cost_per_byte`: an open model's unread keys, kept and written back, under 120 fuel a byte. Branch with skips: 111 compact, 97 indented. Loom main's reader on the same pass: 343 (173 keeping them, unwritten). Main's bound of 25 was a closed model skipping its unread keys, which now fail it. For loom's commit body.
- Not yet run: cold test time against main, the workspace tests, clippy, the lefthook checks.

Landing order: schema-emit to main and pushed; cel-emit pins it, to main and pushed; loom pins both revs, drops `[patch]`, to main. Each markdown commit shown to the user first; pushing asked first.

## What's left before landing

1. schema-emit: `x-strip-additional` (IR `Model.additional`: fail, keep, strip), to main and pushed. cel-emit's `answer_rows` schema strips, so its decode does the reference's work: 129 allocation calls against 51 today, 50 against 50 without the extra keys.
2. Loom: whether `docs/steps.md` offers strip as a model's third choice, or loom refuses the keyword. Loom's `models/extra.rs` and `forms.rs` read `Model.open`.
3. The landing checks: cold test time against main, workspace tests, clippy, lefthook. Then squash, and show you the markdown commits.
