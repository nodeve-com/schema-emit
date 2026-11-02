//! Where the suite's fixtures are and what they list.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The instances `tests/fixture/departures.txt` lists, `<file>/<name>` and a reason after a tab:
/// where schema-emit deliberately disagrees with the suite (it asserts `format`, `contentEncoding`), a
/// verdict is right only where it differs from the case's, so a stale entry fails.
pub fn departures() -> BTreeSet<String> {
    let text = std::fs::read_to_string(fixture().join("departures.txt")).unwrap_or_default();
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(name, _)| name.to_string())
        .collect()
}

pub fn stem(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().into_owned()
}

pub fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture")
}
