//! What decoding costs. Each fixture in `tests/fixture/cost/` (`<name>.schema.json` and the
//! instance `<name>.json`) is lowered and emitted twice: the std types with their `decode_`
//! function, and the same types deriving serde's `Deserialize`. One program holds every case and
//! counts the allocation calls and bytes of three reads of the instance text: the decode, the
//! derive, and `serde_json::Value`. A table of them prints (`--no-capture`); the derive column says
//! whether it read what the decode read. Only the decode's allocation calls gate, at the figure
//! `tests/fixture/cost/baseline.txt` records for the case. `COST_BLESS=1` rewrites the baseline.
//! `scripts/instructions.sh` counts instructions of the same program under callgrind.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use schema_emit::ir::Schema;
use serde_json::{Map, Value};

use schema_emit::emit::std_form::StdForm;
use schema_emit::json_schema;

/// The cost program's allocator and its `main`, before the cases.
const COUNT: &str = include_str!("program/count.rs");
const DRIVER: &str = include_str!("program/driver.rs");

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixture/cost");

/// What a derive needs: serde's macros, and chrono's serde support for its time types.
const MANIFEST: &str = "[package]\nname = \"schema-emit-cost\"\nversion = \"0.0.0\"\n\
    edition = \"2021\"\n[workspace]\n[dependencies]\n\
    schema-emit-runtime = { path = RUNTIME, features = [\"decode\"] }\n\
    any-value = { path = ANYVALUE }\n\
    serde = { version = \"1\", features = [\"derive\"] }\n\
    serde_json = { version = \"1\", features = [\"float_roundtrip\"] }\n\
    chrono = { version = \"0.4\", default-features = false, features = [\"serde\"] }\n\
    [profile.release]\ndebug = false\nstrip = false\n";

/// One fixture: its name, its lowered schema, and its instance text.
struct Case {
    name: String,
    schema: Schema,
    model: usize,
    text: String,
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every fixture, by name.
fn cases() -> Vec<Case> {
    let mut names: Vec<String> = std::fs::read_dir(FIXTURES)
        .unwrap()
        .filter_map(|e| {
            let file = e.unwrap().file_name().to_string_lossy().into_owned();
            file.strip_suffix(".schema.json").map(str::to_string)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no cost fixture");
    let dir = Path::new(FIXTURES);
    let lowered = |name: &str| {
        let schema: Value =
            serde_json::from_str(&read(&dir.join(format!("{name}.schema.json")))).unwrap();
        let (schema, model) = json_schema::lower("Root", &schema, &Map::new(), &[])
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let text = read(&dir.join(format!("{name}.json")))
            .trim_end()
            .to_string();
        Case {
            name: name.into(),
            schema,
            model,
            text,
        }
    };
    names.iter().map(|n| lowered(n)).collect()
}

/// The name of model `i`'s struct.
fn name(i: usize) -> String {
    format!("M{i}")
}

/// Case `n`'s module: the emitted types and decode, the derived types, and the three reads.
fn module(n: usize, case: &Case) -> String {
    let allow = "#![allow(unused_imports, clippy::all)]";
    let emitted = StdForm::new(&case.schema, &name).items().unwrap();
    let derive = ["::serde::Deserialize"];
    let derived = StdForm::new(&case.schema, &name)
        .with_derive(&derive)
        .types()
        .replace("pub others: String", "#[serde(default)] pub others: String");
    let m = case.model;
    let ty = name(m);
    format!(
        "mod c{n} {{ pub mod e {{ {allow} use ::schema_emit_runtime as __s; {emitted} }}\n\
         pub mod d {{ {allow} {derived} }}\n\
         const TEXT: &str = {:?};\n\
         fn decoded(t: &str) -> Result<e::{ty}, ::schema_emit_runtime::decode::Unfit> \
         {{ ::schema_emit_runtime::decode::json(t, |d| e::decode_m{m}(d)).map(|r| r.0) }}\n\
         fn derived(t: &str) -> Result<d::{ty}, ::serde_json::Error> {{ ::serde_json::from_str(t) }}\n\
         fn value(t: &str) -> Result<::serde_json::Value, ::serde_json::Error> \
         {{ ::serde_json::from_str(t) }}\n\
         pub fn run(kind: &str) -> super::Row {{ match kind {{\n\
         \"decode\" => super::row(|| super::measured_decode(decoded, TEXT)),\n\
         \"derive\" => super::row(|| super::measured_decode(derived, TEXT)),\n\
         _ => super::row(|| super::measured_decode(value, TEXT)) }} }} }}\n",
        case.text,
    )
}

/// The program's source: its driver, each case's module, and the list of them.
fn source(cases: &[Case]) -> String {
    let mut out = String::from(DRIVER);
    let mut list = String::from("const CASES: &[(&str, fn(&str) -> Row)] = &[\n");
    for (n, case) in cases.iter().enumerate() {
        out.push_str(&module(n, case));
        let _ = writeln!(list, "({:?}, c{n}::run),", case.name);
    }
    out + &list + "];\n"
}

/// Builds the program in release against this repository's lock; its executable.
fn build(cases: &[Case]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("schema-emit-cost");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema-emit-runtime");
    let any = format!(
        "{:?}",
        runtime.join("../../any-value").display().to_string()
    );
    let manifest = MANIFEST
        .replace("RUNTIME", &format!("{:?}", runtime.display().to_string()))
        .replace("ANYVALUE", &any);
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
        dir.join("Cargo.lock"),
    )
    .unwrap();
    std::fs::write(dir.join("src/count.rs"), COUNT).unwrap();
    std::fs::write(dir.join("src/main.rs"), source(cases)).unwrap();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = Command::new(cargo)
        .args(["build", "--release", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&built.stderr);
    assert!(
        built.status.success(),
        "the cost program doesn't build:\n{said}"
    );
    dir.join("target/release/schema-emit-cost")
}

/// One printed line of the program: a case's kind, allocation calls, bytes, and status.
struct Line {
    kind: String,
    calls: usize,
    bytes: usize,
    status: String,
}

/// The program's lines, by case.
fn run(exe: &Path) -> BTreeMap<String, Vec<Line>> {
    let out = Command::new(exe).output().unwrap();
    assert!(out.status.success(), "the cost program failed to run");
    let mut lines: BTreeMap<String, Vec<Line>> = BTreeMap::new();
    for l in String::from_utf8(out.stdout).unwrap().lines() {
        let f: Vec<&str> = l.split('\t').collect();
        lines.entry(f[0].into()).or_default().push(Line {
            kind: f[1].into(),
            calls: f[2].parse().unwrap(),
            bytes: f[3].parse().unwrap(),
            status: f[4].into(),
        });
    }
    lines
}

/// The table of every case's three reads.
fn table(lines: &BTreeMap<String, Vec<Line>>) -> String {
    let mut out =
        String::from("| Case | Read | Allocations | Bytes | Result |\n| - | - | - | - | - |\n");
    for (case, rows) in lines {
        for r in rows {
            let _ = writeln!(
                out,
                "| {case} | {} | {} | {} | {} |",
                r.kind, r.calls, r.bytes, r.status
            );
        }
    }
    out
}

/// Each case's decode allocation calls past the baseline's, or missing from it.
fn past(lines: &BTreeMap<String, Vec<Line>>, baseline: &BTreeMap<String, usize>) -> Vec<String> {
    let mut over = Vec::new();
    for (case, rows) in lines {
        let calls = rows.iter().find(|r| r.kind == "decode").unwrap().calls;
        match baseline.get(case) {
            Some(&allowed) if calls <= allowed => {}
            Some(allowed) => over.push(format!("{case}: {calls} past {allowed}")),
            None => over.push(format!("{case}: {calls}, no baseline")),
        }
    }
    over
}

#[test]
fn each_decode_allocates_no_more_than_its_baseline() {
    let exe = build(&cases());
    let lines = run(&exe);
    println!("{}", table(&lines));
    let file = Path::new(FIXTURES).join("baseline.txt");
    if std::env::var("COST_BLESS").is_ok() {
        let mut text = String::new();
        for (case, rows) in &lines {
            let calls = rows.iter().find(|r| r.kind == "decode").unwrap().calls;
            let _ = writeln!(text, "{case}\t{calls}");
        }
        std::fs::write(&file, text).unwrap();
    }
    let baseline: BTreeMap<String, usize> = read(&file)
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(c, n)| {
            (
                c.to_string(),
                n.split('\t').next().unwrap().parse().unwrap(),
            )
        })
        .collect();
    let over = past(&lines, &baseline);
    assert!(
        over.is_empty(),
        "allocation calls past the baseline: {over:?}"
    );
}
