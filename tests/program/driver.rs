//! The cost program: each case's decode, the derive reference's, and `serde_json::Value`'s,
//! run once on the case's text, each counted. The harness adds after this each case's module and
//! `CASES`. With no argument it prints a line per case and kind (case, kind, allocation calls,
//! bytes, status); `--list` names the cases; `<case> <kind>` runs one, for callgrind.

mod count;

use std::fmt::Debug;
use std::hint::black_box;

#[global_allocator]
static COUNTING: count::Counting = count::Counting;

/// What one decode cost, and its value's `Debug` text or its failure.
pub struct Row {
    calls: usize,
    bytes: usize,
    shown: Result<String, String>,
}

/// `text` decoded by `decode`; the function callgrind collects inside.
#[inline(never)]
pub fn measured_decode<T, E: Debug>(
    decode: fn(&str) -> Result<T, E>,
    text: &str,
) -> Result<T, String> {
    decode(black_box(text)).map_err(|e| format!("{e:?}"))
}

/// `f`, counted; its value is shown once the count has stopped.
pub fn row<T: Debug>(f: impl FnOnce() -> Result<T, String>) -> Row {
    let (out, calls, bytes) = count::measure(f);
    Row {
        calls,
        bytes,
        shown: out.map(|v| format!("{v:?}")),
    }
}

const KINDS: [&str; 3] = ["decode", "derive", "value"];

/// The line of a row; derive says whether it read what the decode read.
fn line(case: &str, kind: &str, r: &Row, decoded: Option<&Result<String, String>>) -> String {
    let status = match (&r.shown, decoded) {
        (Ok(s), Some(Ok(d))) if kind == "derive" => if s == d { "same" } else { "differs" }.into(),
        (Ok(_), Some(Err(_))) if kind == "derive" => "differs".to_string(),
        (Ok(_), _) => "ok".to_string(),
        (Err(e), _) => format!("fails: {}", e.replace(['\n', '\t'], " ")),
    };
    format!("{case}\t{kind}\t{}\t{}\t{status}", r.calls, r.bytes)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [list] if list == "--list" => CASES.iter().for_each(|(name, _)| println!("{name}")),
        [case, kind] => {
            let (_, run) = CASES.iter().find(|(n, _)| n == case).expect("a case");
            println!("{}", line(case, kind, &run(kind), None));
        }
        _ => {
            for (case, run) in CASES {
                let rows = KINDS.map(run);
                for (kind, r) in KINDS.iter().zip(&rows) {
                    println!("{}", line(case, kind, r, rows.first().map(|d| &d.shown)));
                }
            }
        }
    }
}
