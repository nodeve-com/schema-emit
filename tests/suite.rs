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
#[path = "suite/run.rs"]
mod run;
#[path = "suite/typed.rs"]
mod typed;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use files::{departures, fixture, stem};

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
    let mut files: Vec<PathBuf> = ["JSON-Schema-Test-Suite", "cases"]
        .iter()
        .flat_map(|d| std::fs::read_dir(fixture().join(d)).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| only.as_deref().is_none_or(|o| stem(p) == o))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no suite file named {only:?}");

    let mut instances = Vec::new();
    let mut modules = Vec::new();
    for path in &files {
        let groups: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut seen = BTreeMap::new();
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
                    f.as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_str().unwrap().to_string())
                        .collect()
                });
                let verdict = match prepare(&group["schema"], &test["data"]) {
                    Ok(case) => match run::module(modules.len(), &case) {
                        Ok(m) => {
                            modules.push((modules.len(), m));
                            Verdict::Built(modules.len() - 1)
                        }
                        Err(e) => Verdict::Unsupported(e.to_string()),
                    },
                    Err(v) => v,
                };
                instances.push(Instance {
                    file: stem(path),
                    name,
                    valid: test["valid"].as_bool().unwrap(),
                    failures,
                    verdict,
                });
            }
        }
    }
    let outcomes = run::run(&modules);
    let mut checked = Checked::new();
    for i in &mut instances {
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
    judge(&instances, &checked, &files);
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
        let (mut s, ty) =
            json_schema::lower_ty("Value.value", &schema, &none, NOTES, Schema::default())
                .map_err(unsupported)?;
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

fn judge(instances: &[Instance], outcomes: &Checked, files: &[PathBuf]) {
    let bless = std::env::var("SUITE_BLESS").is_ok();
    let show = std::env::var("SUITE_SHOW").is_ok();
    let departures = departures();
    let mut passed: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut counts: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut wrong = Vec::new();
    for i in instances {
        let slot = match &i.verdict {
            Verdict::Built(n) => {
                let (valid, failures) = &outcomes[n];
                let departs = departures.contains(&format!("{}/{}", i.file, i.name));
                let same = departs || i.failures.as_ref().is_none_or(|f| f == failures);
                let right = (*valid != i.valid) == departs && same;
                if right {
                    passed.entry(&i.file).or_default().insert(&i.name);
                    0
                } else {
                    wrong.push(format!(
                        "{}/{}: expected {}{}, got {} {failures:?}",
                        i.file,
                        i.name,
                        if i.valid { "valid" } else { "invalid" },
                        i.failures
                            .as_ref()
                            .map_or(String::new(), |f| format!(" {f:?}")),
                        if *valid { "valid" } else { "invalid" },
                    ));
                    2
                }
            }
            // A refusal is invalid and lists no check failures: right only where the case says so.
            Verdict::Refused(why)
                if (!i.valid && i.failures.as_ref().is_none_or(Vec::is_empty))
                    || departures.contains(&format!("{}/{}", i.file, i.name)) =>
            {
                passed.entry(&i.file).or_default().insert(&i.name);
                *reasons.entry(format!("refused: {why}")).or_default() += 1;
                0
            }
            Verdict::Refused(why) => {
                wrong.push(format!(
                    "{}/{}: expected {}{:?}, refused: {why}",
                    i.file,
                    i.name,
                    if i.valid { "valid" } else { "invalid" },
                    i.failures
                ));
                2
            }
            Verdict::Unsupported(why) => {
                *reasons.entry(short(why)).or_default() += 1;
                1
            }
            Verdict::NotApplicable(why) => {
                *reasons.entry(format!("n/a: {why}")).or_default() += 1;
                3
            }
        };
        if show {
            match &i.verdict {
                Verdict::Unsupported(why) => println!("unsupported: {}/{}: {why}", i.file, i.name),
                Verdict::NotApplicable(why) => println!("n/a: {}/{}: {why}", i.file, i.name),
                Verdict::Refused(why) => println!("refused: {}/{}: {why}", i.file, i.name),
                Verdict::Built(_) => {}
            }
        }
        counts.entry(&i.file).or_default()[slot] += 1;
    }
    println!(
        "{:<28} {:>5} {:>5} {:>5} {:>5}",
        "file", "pass", "unsup", "wrong", "n/a"
    );
    for (file, [p, u, w, n]) in &counts {
        println!("{file:<28} {p:>5} {u:>5} {w:>5} {n:>5}");
    }
    let mut top: Vec<(usize, String)> = reasons.into_iter().map(|(w, n)| (n, w)).collect();
    top.sort_unstable_by(|a, b| b.cmp(a));
    println!("reasons, most common first:");
    for (n, why) in top.iter().take(30) {
        println!("{n:>5} {why}");
    }
    for w in &wrong {
        println!("wrong: {w}");
    }

    let mut lost = Vec::new();
    let mut gained = Vec::new();
    for path in files {
        let file = stem(path);
        let list = fixture().join("pass").join(format!("{file}.txt"));
        let now: BTreeSet<&str> = passed.get(file.as_str()).cloned().unwrap_or_default();
        if bless {
            let text: String = now.iter().map(|n| format!("{n}\n")).collect();
            if text.is_empty() {
                let _ = std::fs::remove_file(&list);
            } else {
                std::fs::write(&list, text).unwrap();
            }
            continue;
        }
        let before: BTreeSet<String> = std::fs::read_to_string(&list)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        lost.extend(
            before
                .iter()
                .filter(|b| !now.contains(b.as_str()))
                .map(|b| format!("{file}/{b}")),
        );
        gained.extend(
            now.iter()
                .filter(|n| !before.contains(**n))
                .map(|n| format!("{file}/{n}")),
        );
    }
    assert!(wrong.is_empty(), "wrong, worse than refused: {wrong:#?}");
    assert!(lost.is_empty(), "stopped passing:\n{lost:#?}");
    assert!(
        gained.is_empty(),
        "newly passing; add them with SUITE_BLESS=1:\n{gained:#?}"
    );
}

/// A reason without the schema path that starts it, so like reasons group.
fn short(why: &str) -> String {
    let kind = why.split_once(": ").map_or(why, |(k, _)| k);
    let last = why.rsplit(": ").next().unwrap_or(why);
    format!("{kind}: {last}")
}
