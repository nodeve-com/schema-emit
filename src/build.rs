//! A build script's reads and writes: the JSON Schema it lowers, and the source it emits.

use std::error::Error;
use std::path::PathBuf;
use std::{env, fs};

/// The JSON at `path`, from the package root, rereading it when it changes.
pub fn read(path: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    println!("cargo:rerun-if-changed={path}");
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

/// `source` as `$OUT_DIR/<file>`.
pub fn write(file: &str, source: &str) -> Result<(), Box<dyn Error>> {
    fs::write(PathBuf::from(env::var("OUT_DIR")?).join(file), source)?;
    Ok(())
}
