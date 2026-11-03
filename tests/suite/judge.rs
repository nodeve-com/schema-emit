//! The suite's verdict on every instance: tallies, a report, and the pass lists they must match.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::files::{departures, fixture, stem};
use super::{Checked, Instance, Verdict};

/// Where an instance falls in the table: passes, is unsupported, is wrong, or is n/a.
#[derive(Clone, Copy)]
enum Slot {
    Pass = 0,
    Unsupported = 1,
    Wrong = 2,
    NotApplicable = 3,
}

/// What the instances came to.
#[derive(Default)]
struct Tally<'a> {
    passed: BTreeMap<&'a str, BTreeSet<&'a str>>,
    counts: BTreeMap<&'a str, [usize; 4]>,
    reasons: BTreeMap<String, usize>,
    wrong: Vec<String>,
}

impl<'a> Tally<'a> {
    /// Counts instance `i`.
    fn record(&mut self, i: &'a Instance, outcomes: &Checked, departures: &BTreeSet<String>) {
        let departs = departures.contains(&format!("{}/{}", i.file, i.name));
        let slot = match &i.verdict {
            Verdict::Built(n) => self.built(i, &outcomes[n], departs),
            // A refusal is invalid and lists no check failures: right only where the case says so.
            Verdict::Refused(why) => self.refused(i, why, departs),
            Verdict::Unsupported(why) => {
                *self.reasons.entry(short(why)).or_default() += 1;
                Slot::Unsupported
            }
            Verdict::NotApplicable(why) => {
                *self.reasons.entry(format!("n/a: {why}")).or_default() += 1;
                Slot::NotApplicable
            }
        };
        self.counts.entry(&i.file).or_default()[slot as usize] += 1;
    }

    /// An instance that decoded and checked, as `(valid, failures)`.
    fn built(&mut self, i: &'a Instance, got: &(bool, Vec<String>), departs: bool) -> Slot {
        let (valid, failures) = got;
        let same = departs || i.failures.as_ref().is_none_or(|f| f == failures);
        if (*valid != i.valid) == departs && same {
            self.pass(i);
            return Slot::Pass;
        }
        self.wrong.push(format!(
            "{}/{}: expected {}{}, got {} {failures:?}",
            i.file,
            i.name,
            if i.valid { "valid" } else { "invalid" },
            i.failures
                .as_ref()
                .map_or(String::new(), |f| format!(" {f:?}")),
            if *valid { "valid" } else { "invalid" },
        ));
        Slot::Wrong
    }

    /// An instance no std value holds.
    fn refused(&mut self, i: &'a Instance, why: &str, departs: bool) -> Slot {
        if (!i.valid && i.failures.as_ref().is_none_or(Vec::is_empty)) || departs {
            self.pass(i);
            *self.reasons.entry(format!("refused: {why}")).or_default() += 1;
            return Slot::Pass;
        }
        self.wrong.push(format!(
            "{}/{}: expected {}{:?}, refused: {why}",
            i.file,
            i.name,
            if i.valid { "valid" } else { "invalid" },
            i.failures
        ));
        Slot::Wrong
    }

    fn pass(&mut self, i: &'a Instance) {
        self.passed.entry(&i.file).or_default().insert(&i.name);
    }

    /// Prints the table, the commonest reasons, and each wrong instance.
    fn report(&self) {
        println!(
            "{:<28} {:>5} {:>5} {:>5} {:>5}",
            "file", "pass", "unsup", "wrong", "n/a"
        );
        for (file, [p, u, w, n]) in &self.counts {
            println!("{file:<28} {p:>5} {u:>5} {w:>5} {n:>5}");
        }
        let mut top: Vec<(usize, &String)> = self.reasons.iter().map(|(w, n)| (*n, w)).collect();
        top.sort_unstable_by(|a, b| b.cmp(a));
        println!("reasons, most common first:");
        for (n, why) in top.iter().take(30) {
            println!("{n:>5} {why}");
        }
        for w in &self.wrong {
            println!("wrong: {w}");
        }
    }
}

/// Names each instance that isn't built, for `SUITE_SHOW`.
fn show(instances: &[Instance]) {
    for i in instances {
        match &i.verdict {
            Verdict::Unsupported(why) => println!("unsupported: {}/{}: {why}", i.file, i.name),
            Verdict::NotApplicable(why) => println!("n/a: {}/{}: {why}", i.file, i.name),
            Verdict::Refused(why) => println!("refused: {}/{}: {why}", i.file, i.name),
            Verdict::Built(_) => {}
        }
    }
}

/// Rewrites each file's pass list, or gives the instances that stopped passing and those that
/// newly pass against it.
fn compare_lists(
    passed: &BTreeMap<&str, BTreeSet<&str>>,
    files: &[PathBuf],
    bless: bool,
) -> (Vec<String>, Vec<String>) {
    let (mut lost, mut gained) = (Vec::new(), Vec::new());
    for path in files {
        let file = stem(path);
        let list = fixture().join("pass").join(format!("{file}.txt"));
        let now: BTreeSet<&str> = passed.get(file.as_str()).cloned().unwrap_or_default();
        if bless {
            bless_list(&list, &now);
            continue;
        }
        let text = std::fs::read_to_string(&list).unwrap_or_default();
        let before: BTreeSet<&str> = text.lines().collect();
        lost.extend(
            before
                .iter()
                .filter(|b| !now.contains(**b))
                .map(|b| format!("{file}/{b}")),
        );
        gained.extend(
            now.iter()
                .filter(|n| !before.contains(**n))
                .map(|n| format!("{file}/{n}")),
        );
    }
    (lost, gained)
}

/// Writes `now`, one name a line, to the pass list `list`, removing it where `now` is empty.
fn bless_list(list: &std::path::Path, now: &BTreeSet<&str>) {
    let text: String = now.iter().map(|n| format!("{n}\n")).collect();
    if text.is_empty() {
        let _ = std::fs::remove_file(list);
    } else {
        std::fs::write(list, text).unwrap();
    }
}

pub fn judge(instances: &[Instance], outcomes: &Checked, files: &[PathBuf]) {
    let departures = departures();
    let mut tally = Tally::default();
    for i in instances {
        tally.record(i, outcomes, &departures);
    }
    if std::env::var("SUITE_SHOW").is_ok() {
        show(instances);
    }
    tally.report();
    let bless = std::env::var("SUITE_BLESS").is_ok();
    let (lost, gained) = compare_lists(&tally.passed, files, bless);
    assert!(
        tally.wrong.is_empty(),
        "wrong, worse than refused: {:#?}",
        tally.wrong
    );
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
