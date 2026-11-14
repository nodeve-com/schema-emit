//! The runtime's own `any-value` dependency, so a test program links the same version.

/// The value of the runtime manifest's `any-value` line, without `optional`.
pub fn any_value_dep() -> String {
    let manifest = include_str!("../../schema-emit-runtime/Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.starts_with("any-value ="))
        .unwrap();
    line.trim_start_matches("any-value =")
        .replace(", optional = true", "")
        .trim()
        .to_string()
}
