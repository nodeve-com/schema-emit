//! Runs emitted checks, whatever front end lowered them: each case a module of its std types,
//! its decode, its check, and its instance's JSON, all in one program cargo builds against
//! schema-emit-runtime and runs once. It decodes each instance, checks the value, and prints each
//! case's verdict and failures, or why the instance doesn't decode.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use schema_emit::emit::std_form::StdForm;
use schema_emit::emit::Emitter;
use schema_emit::ir::Schema;
use schema_emit::Error;

/// One instance to check: its schema, the model to check it as, and its value as JSON text.
pub struct Case {
    pub schema: Schema,
    pub model: usize,
    pub json: String,
}

/// What a case gave.
pub enum Outcome {
    /// Decoded and checked: valid, and each failure as `<path>: <reason>`.
    Checked(bool, Vec<String>),
    /// The instance is another kind than the model's types take, at this place.
    Kind,
    /// The instance is of the right kinds but no value of the types holds it.
    Other,
}

/// The name of model `i`'s struct.
pub fn name(i: usize) -> String {
    format!("M{i}")
}

/// Each case's module, or why it doesn't emit.
pub fn module(n: usize, case: &Case) -> Result<String, Error> {
    let std = StdForm::new(&case.schema, &name);
    let check = Emitter::new(&case.schema, &std).check(case.model, "check")?;
    Ok(format!(
        "mod c{n} {{ #![allow(unused_imports, clippy::all)] use ::schema_emit_runtime as s;\n{}{check}\
         pub fn run() -> super::Outcome {{ let json = ::serde_json::from_str({:?}).unwrap_or_default(); \
         match decode_m{}(&json) {{ Ok(v) => {{ let mut f = s::Failures::<16>::default(); \
         check(&v, &mut f); super::outcome(&f) }} Err(e) => super::refused(&e) }} }} }}\n",
        std.items()?,
        case.json,
        case.model
    ))
}

/// Builds and runs `modules`, each `(n, source)`, returning each one's outcome by `n`.
pub fn run(modules: &[(usize, String)]) -> BTreeMap<usize, Outcome> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("schema-emit-suite");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    write_project(&dir, modules);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = Command::new(&cargo)
        .args(["build", "--quiet", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "the emitted checks don't build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let out = Command::new(dir.join("target/debug/schema-emit-suite"))
        .output()
        .unwrap();
    assert!(out.status.success(), "the emitted checks failed to run");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(outcome_of)
        .collect()
}

/// Writes the program's manifest and source under `dir`.
fn write_project(dir: &Path, modules: &[(usize, String)]) {
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema-emit-runtime");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"schema-emit-suite\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\
             [dependencies]\nschema-emit-runtime = {{ path = {:?}, features = [\"decode\"] }}\n\
             serde_json = {{ version = \"1\", features = [\"float_roundtrip\"] }}\n\
             chrono = {{ version = \"0.4\", default-features = false }}\n[workspace]\n\
             [profile.dev]\ndebug = false\n",
            runtime.display().to_string()
        ),
    )
    .unwrap();
    let mut main = String::from(MAIN);
    for (_, m) in modules {
        main.push_str(m);
    }
    main.push_str("fn main() {\n");
    for (n, _) in modules {
        let _ = writeln!(main, "print({n}, c{n}::run());");
    }
    main.push_str("}\n");
    std::fs::write(dir.join("src/main.rs"), main).unwrap();
}

/// The case number and outcome one line of the program's output gives.
fn outcome_of(line: &str) -> (usize, Outcome) {
    let mut parts = line.split('\t');
    let n = parts.next().unwrap().parse().unwrap();
    let verdict = parts.next();
    let text = parts.next().unwrap_or_default();
    let failures = text
        .split(';')
        .filter(|f| !f.is_empty())
        .map(str::to_string)
        .collect();
    let outcome = match verdict {
        Some("kind") => Outcome::Kind,
        Some("other") => Outcome::Other,
        verdict => Outcome::Checked(verdict == Some("valid"), failures),
    };
    (n, outcome)
}

/// What every program carries: an outcome from failures, and printing it.
const MAIN: &str = r#"
pub type Outcome = (&'static str, String);
pub fn outcome<const N: usize>(f: &::schema_emit_runtime::Failures<N>) -> Outcome {
    let mut out = String::new();
    for x in f.iter() {
        out.push_str(&format!("{x}: {};", x.reason.name()));
    }
    (if f.is_empty() { "valid" } else { "invalid" }, out)
}
pub fn refused(e: &::schema_emit_runtime::decode::Unfit) -> Outcome {
    (if e.is_kind() { "kind" } else { "other" }, e.to_string().replace(['\n', '\t'], " "))
}
fn print(n: usize, (verdict, text): Outcome) {
    println!("{n}\t{verdict}\t{text}");
}
"#;
