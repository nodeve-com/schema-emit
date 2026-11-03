//! Conformance: every case of the JSON-Schema-Test-Suite's draft 2020-12 files (vendored at
//! `tests/fixture/JSON-Schema-Test-Suite/` from json-schema-org's commit 5b0ee16, its LICENSE
//! beside them) and of `tests/fixture/cases/`, lowered, emitted, built, and run over the std
//! form. Each instance passes, is unsupported (lowering or emitting refuses it), is wrong (its
//! check disagrees with the case), or is n/a (no std value is that instance, `suite/typed.rs`).
//! The gate: none wrong, and exactly the instances `tests/fixture/pass/<file>.txt` names pass.
//! `SUITE=<file>` runs one file; `SUITE_BLESS=1` rewrites pass lists; `SUITE_SHOW=1` names misses.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

#[path = "suite/files.rs"]
mod files;
#[path = "suite/judge.rs"]
mod judge;
#[path = "suite/run.rs"]
mod run;
#[path = "suite/typed.rs"]
mod typed;

use std::collections::BTreeMap;
use std::path::PathBuf;

use files::{fixture, stem};
use judge::judge;

use schema_emit::ir::{Field, Model, Schema};
use schema_emit::json_schema;
use serde_json::{json, Map, Value};

/// The annotation keywords the fixture cases carry.
const NOTES: &[&str] = &["unit", "quantityKind"];

enum Verdict {
    Unsupported(String),
    NotApplicable(String),
    /// No typed value of the schema's type is the instance: the decode refuses it.
    Refused(String),
    /// Built: its number in the program.
    Built(usize),
}

/// One instance: its name, what the case expects, and how it went.
struct Instance {
    file: String,
    name: String,
    valid: bool,
    failures: Option<Vec<String>>,
    verdict: Verdict,
}

#[test]
fn the_suite_passes_its_baseline() {
    let only = std::env::var("SUITE").ok();
    let files = suite_files(only.as_deref());
    assert!(!files.is_empty(), "no suite file named {only:?}");
    let mut instances = Vec::new();
    let mut modules = Vec::new();
    for path in &files {
        instances.extend(file_instances(path, &mut modules));
    }
    let outcomes = run::run(&modules);
    let checked = settle(&mut instances, &outcomes);
    judge(&instances, &checked, &files);
}

/// The suite's case files, `only` the one it names, in order.
fn suite_files(only: Option<&str>) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ["JSON-Schema-Test-Suite", "cases"]
        .iter()
        .flat_map(|d| std::fs::read_dir(fixture().join(d)).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| only.is_none_or(|o| stem(p) == o))
        .collect();
    files.sort();
    files
}

/// Each instance of the case file `path`; those that build add their module to `modules`.
fn file_instances(path: &PathBuf, modules: &mut Vec<(usize, String)>) -> Vec<Instance> {
    let groups: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut seen = BTreeMap::new();
    let mut out = Vec::new();
    for group in &groups {
        for test in group["tests"].as_array().unwrap() {
            let mut name = format!(
                "{}/{}",
                group["description"].as_str().unwrap(),
                test["description"].as_str().unwrap()
            );
            let n = seen.entry(name.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                name = format!("{name}#{n}");
            }
            let failures = test.get("failures").map(|f| {
                let names = f.as_array().unwrap().iter();
                names.map(|x| x.as_str().unwrap().to_string()).collect()
            });
            out.push(Instance {
                file: stem(path),
                name,
                valid: test["valid"].as_bool().unwrap(),
                failures,
                verdict: build(&group["schema"], &test["data"], modules),
            });
        }
    }
    out
}

/// The verdict of building one instance's module, added to `modules` where it builds.
fn build(schema: &Value, data: &Value, modules: &mut Vec<(usize, String)>) -> Verdict {
    let case = match prepare(schema, data) {
        Ok(case) => case,
        Err(v) => return v,
    };
    match run::module(modules.len(), &case) {
        Ok(m) => {
            modules.push((modules.len(), m));
            Verdict::Built(modules.len() - 1)
        }
        Err(e) => Verdict::Unsupported(e.to_string()),
    }
}

/// Turns each built instance that didn't decode into its verdict; gives those that did.
fn settle(instances: &mut [Instance], outcomes: &BTreeMap<usize, run::Outcome>) -> Checked {
    let mut checked = Checked::new();
    for i in instances {
        let Verdict::Built(n) = i.verdict else {
            continue;
        };
        i.verdict = match &outcomes[&n] {
            run::Outcome::Checked(valid, failures) => {
                checked.insert(n, (*valid, failures.clone()));
                continue;
            }
            run::Outcome::Kind => Verdict::Refused("a kind the schema's type doesn't take".into()),
            run::Outcome::Other => Verdict::NotApplicable("no std value holds it".into()),
        };
    }
    checked
}

/// The case for one instance, or why it has none.
fn prepare(schema: &Value, data: &Value) -> Result<run::Case, Verdict> {
    let (schema, data) = typed::fit(schema, data).map_err(Verdict::NotApplicable)?;
    let none = Map::new();
    let unsupported = |e: schema_emit::Error| Verdict::Unsupported(e.to_string());
    let object =
        schema.get("type") == Some(&Value::String("object".into())) && !typed::map(&schema);
    let (schema, model, data) = if object {
        let (s, m) = json_schema::lower("Value", &schema, &none, NOTES).map_err(unsupported)?;
        (s, m, data)
    } else {
        // Anything else is the one field of a model around it.
        let doc = json_schema::Document {
            schema: &schema,
            shared: &none,
            notes: NOTES,
        };
        let (mut s, ty) =
            json_schema::lower_ty("Value.value", &doc, Schema::default()).map_err(unsupported)?;
        s.models.push(Model {
            name: "Value".into(),
            fields: vec![Field {
                name: "value".into(),
                ty,
                required: true,
            }],
            ..Model::default()
        });
        let m = s.models.len() - 1;
        (s, m, json!({ "value": data }))
    };
    Ok(run::Case {
        schema,
        model,
        json: data.to_string(),
    })
}

/// Each built case's verdict and failures, where it decoded.
type Checked = BTreeMap<usize, (bool, Vec<String>)>;
